use anyhow::{anyhow, Context as AnyhowContext};
use serde::{Deserialize, Serialize};
use worker::*;

const CREATE_MESSAGES_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sender_id TEXT NOT NULL,
    payload_cipher BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireMessage {
    sender_id: String,
    payload_cipher: Vec<u8>,
    created_at: i64,
}

#[event(fetch)]
pub async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let url = req.url()?;
    let mut segments = url
        .path_segments()
        .ok_or_else(|| Error::RustError("invalid URL path".into()))?;

    let first = segments.next().unwrap_or_default();
    let room_id = segments.next().unwrap_or_default();

    if first != "room" || room_id.is_empty() {
        return Response::error("Expected path format: /room/{room_id}", 400);
    }

    let namespace = env.durable_object("CHATROOM")?;
    let id = namespace.id_from_name(room_id)?;
    let stub = id.get_stub()?;
    stub.fetch_with_request(req).await
}

#[durable_object]
pub struct ChatRoom {
    state: State,
    env: Env,
}

#[durable_object]
impl DurableObject for ChatRoom {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&mut self, req: Request) -> Result<Response> {
        self.ensure_schema()
            .await
            .map_err(|e| Error::RustError(e.to_string()))?;

        let upgrade = req.headers().get("Upgrade")?.unwrap_or_default();
        if !upgrade.eq_ignore_ascii_case("websocket") {
            return Response::error("WebSocket upgrade required", 426);
        }

        let pair = WebSocketPair::new()?;
        let client = pair.client;
        let server = pair.server;

        // Durable Object WebSocket hibernation acceptance.
        self.state.accept_websocket(&server)?;

        Response::from_websocket(client)
    }

    async fn websocket_message(
        &mut self,
        ws: WebSocket,
        message: WebSocketIncomingMessage,
    ) -> Result<()> {
        let bytes = match message {
            WebSocketIncomingMessage::String(text) => text.into_bytes(),
            WebSocketIncomingMessage::Binary(data) => data,
        };

        let parsed = parse_wire_message(&bytes).map_err(|e| Error::RustError(e.to_string()))?;
        self.persist_message(&parsed)
            .await
            .map_err(|e| Error::RustError(e.to_string()))?;

        // Relay encrypted payload as-is to all connected peers in the room.
        for peer in self.state.get_websockets() {
            if peer.serialize_attachment().ok() != ws.serialize_attachment().ok() {
                let _ = peer.send_with_bytes(&bytes);
            }
        }

        Ok(())
    }
}

impl ChatRoom {
    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let sql = self
            .state
            .storage()
            .sql()
            .context("failed to get SQLite handle from durable object state")?;
        sql.exec(CREATE_MESSAGES_TABLE_SQL)
            .context("failed to apply messages schema migration")?;
        Ok(())
    }

    async fn persist_message(&self, message: &WireMessage) -> anyhow::Result<()> {
        let sql = self
            .state
            .storage()
            .sql()
            .context("failed to get SQLite handle from durable object state")?;

        sql.exec_with_bindings(
            "INSERT INTO messages (sender_id, payload_cipher, created_at) VALUES (?, ?, ?)",
            vec![
                message.sender_id.clone().into(),
                message.payload_cipher.clone().into(),
                message.created_at.into(),
            ],
        )
        .context("failed to insert encrypted message into messages table")?;

        Ok(())
    }
}

fn parse_wire_message(bytes: &[u8]) -> anyhow::Result<WireMessage> {
    if let Ok(v) = serde_json::from_slice::<WireMessage>(bytes) {
        return Ok(v);
    }

    Err(anyhow!(
        "message payload must be JSON encoded WireMessage for persistence"
    ))
}
