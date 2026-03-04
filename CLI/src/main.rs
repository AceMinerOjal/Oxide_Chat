use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::Utc;
use crossterm::{
    event::{Event as CEvent, EventStream, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{SinkExt, StreamExt};
use percent_encoding::percent_decode_str;
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    env,
    fmt::Write as _,
    hash::{Hash, Hasher},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
use tokio::{fs, select, sync::mpsc, time};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

const BLUE: Color = Color::Rgb(137, 180, 250);
const GREEN: Color = Color::Rgb(166, 227, 161);
const RED: Color = Color::Rgb(243, 139, 168);
const TEXT: Color = Color::Rgb(205, 214, 244);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireMessage {
    sender_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    payload_cipher: Vec<u8>,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresenceEvent {
    kind: String,
    sender_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    avatar_url: Option<String>,
    created_at: i64,
}

#[derive(Debug, Clone)]
enum OutboundFrame {
    Chat(WireMessage),
    Presence(PresenceEvent),
}

#[derive(Debug, Clone)]
enum IncomingFrame {
    Chat(WireMessage),
    Presence(PresenceEvent),
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Normal,
    Insert,
}

#[derive(Debug, Clone)]
enum Status {
    Connected,
    Connecting,
    Error,
    Disconnected,
}

#[derive(Debug, Clone)]
struct ChatLine {
    sender_id: String,
    avatar_url: Option<String>,
    body: String,
    created_at: i64,
}

#[derive(Debug)]
struct State {
    ws_base: String,
    room_id: String,
    connected_room_id: Option<String>,
    mode: Mode,
    input: String,
    messages: Vec<ChatLine>,
    status_text: String,
    status: Status,
    sender_id: String,
    avatar_url: Option<String>,
    avatar_cache: HashMap<String, PathBuf>,
    should_quit: bool,
}

#[derive(Debug)]
enum NetEvent {
    Incoming(WireMessage),
    Presence(PresenceEvent),
    Error(String),
    Disconnected(String),
}

#[derive(Debug, Clone)]
enum UiAction {
    Reconnect { clear_history: bool },
    ShowAvatar(Option<String>),
    ShowSelfAvatar,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (ws_base, room_id, sender_id, avatar_url) = parse_cli_args()?;

    let ws_url = room_ws_url(&ws_base, &room_id)?;

    let mut state = State {
        ws_base,
        room_id,
        connected_room_id: None,
        mode: Mode::Normal,
        input: String::new(),
        messages: Vec::new(),
        status_text: format!("connecting {}", ws_url),
        status: Status::Connecting,
        sender_id,
        avatar_url,
        avatar_cache: HashMap::new(),
        should_quit: false,
    };

    let (mut outbound_tx, mut net_rx) = connect_room(&ws_url).await?;
    send_presence_join(&outbound_tx, &state.sender_id, state.avatar_url.clone());
    state.status = Status::Connected;
    state.connected_room_id = Some(state.room_id.clone());
    state.status_text = format!("connected to {} as {}", state.room_id, state.sender_id);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut events = EventStream::new();
    let mut tick = time::interval(Duration::from_millis(50));

    loop {
        terminal.draw(|f| ui(f, &state))?;

        if state.should_quit {
            break;
        }

        select! {
            maybe_event = events.next() => {
                if let Some(Ok(event)) = maybe_event {
                    if let Some(action) = handle_event(event, &mut state, &outbound_tx) {
                        match action {
                            UiAction::Reconnect { clear_history } => {
                                reconnect(
                                    &mut state,
                                    &mut outbound_tx,
                                    &mut net_rx,
                                    clear_history,
                                )
                                .await;
                            }
                            UiAction::ShowAvatar(sender_filter) => {
                                let preview_result =
                                    show_avatar_with_kitty(&mut terminal, &mut state, sender_filter)
                                        .await;
                                apply_status_result(
                                    &mut state,
                                    preview_result,
                                    "avatar preview done",
                                );
                            }
                            UiAction::ShowSelfAvatar => {
                                let preview_result =
                                    show_self_avatar_with_kitty(&mut terminal, &mut state).await;
                                apply_status_result(
                                    &mut state,
                                    preview_result,
                                    "self avatar preview done",
                                );
                            }
                        }
                    }
                }
            }
            maybe_net = net_rx.recv() => {
                if let Some(evt) = maybe_net {
                    match evt {
                        NetEvent::Incoming(msg) => {
                            state.messages.push(ChatLine {
                                sender_id: msg.sender_id,
                                avatar_url: msg.avatar_url,
                                body: decode_payload(&msg.payload_cipher),
                                created_at: msg.created_at,
                            });
                        }
                        NetEvent::Presence(event) => {
                            let body = match event.kind.as_str() {
                                "presence.join" => format!("{} joined room", event.sender_id),
                                "presence.leave" => format!("{} left room", event.sender_id),
                                _ => format!("presence event from {}", event.sender_id),
                            };
                            state.messages.push(ChatLine {
                                sender_id: "system".to_string(),
                                avatar_url: None,
                                body,
                                created_at: event.created_at,
                            });
                        }
                        NetEvent::Error(err) => {
                            state.status = Status::Error;
                            state.status_text = err;
                        }
                        NetEvent::Disconnected(reason) => {
                            state.status = Status::Disconnected;
                            state.status_text = reason;
                            state.connected_room_id = None;
                        }
                    }
                } else {
                    state.status = Status::Disconnected;
                    state.status_text = "network task ended".to_string();
                    state.connected_room_id = None;
                }
            }
            _ = tick.tick() => {}
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

async fn reconnect(
    state: &mut State,
    outbound_tx: &mut mpsc::UnboundedSender<OutboundFrame>,
    net_rx: &mut mpsc::UnboundedReceiver<NetEvent>,
    clear_history: bool,
) {
    let next_url = match room_ws_url(&state.ws_base, &state.room_id) {
        Ok(url) => url,
        Err(err) => {
            state.status = Status::Error;
            state.status_text = err.to_string();
            return;
        }
    };

    state.status = Status::Connecting;
    state.status_text = format!("connecting {}", next_url);
    state.connected_room_id = None;
    if clear_history {
        state.messages.clear();
    }

    match connect_room(&next_url).await {
        Ok((new_tx, new_rx)) => {
            *outbound_tx = new_tx;
            *net_rx = new_rx;
            send_presence_join(outbound_tx, &state.sender_id, state.avatar_url.clone());
            state.status = Status::Connected;
            state.connected_room_id = Some(state.room_id.clone());
            state.status_text = format!("connected to {} as {}", state.room_id, state.sender_id);
        }
        Err(err) => {
            state.status = Status::Error;
            state.status_text = err.to_string();
        }
    }
}

fn apply_status_result(state: &mut State, result: Result<()>, success_status: &str) {
    match result {
        Ok(()) => {
            state.status = Status::Connected;
            state.status_text = success_status.to_string();
        }
        Err(err) => {
            state.status = Status::Error;
            state.status_text = err.to_string();
        }
    }
}

fn parse_cli_args() -> Result<(String, String, String, Option<String>)> {
    let mut positional: Vec<String> = Vec::new();
    let mut username: Option<String> = None;
    let mut avatar_url: Option<String> = None;
    let mut args = env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--username" | "-u" => {
                let value = args
                    .next()
                    .context("missing value for --username; usage: --username <name>")?;
                if value.trim().is_empty() {
                    anyhow::bail!("username cannot be empty");
                }
                username = Some(value.trim().to_string());
            }
            "--avatar-url" => {
                let value = args
                    .next()
                    .context("missing value for --avatar-url; usage: --avatar-url <https://...>")?;
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    anyhow::bail!("avatar URL cannot be empty");
                }
                avatar_url = Some(trimmed.to_string());
            }
            _ if arg.starts_with('-') => {
                anyhow::bail!("unknown flag: {arg}");
            }
            _ => positional.push(arg),
        }
    }

    if positional.len() > 2 {
        anyhow::bail!("too many positional arguments; usage: cargo run -- [--username <name>] [--avatar-url <https://...>] <ws-base-url> [room-id]");
    }

    let ws_base = positional
        .first()
        .cloned()
        .or_else(|| env::var("OXIDE_WS_BASE").ok())
        .context("missing websocket base URL; pass <ws-base-url> or set OXIDE_WS_BASE")?;
    let room_id = positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| "general".to_string());
    let sender_id = username
        .or_else(|| env::var("OXIDE_USERNAME").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("cli-{}", Utc::now().timestamp_millis()));
    let avatar_url = avatar_url
        .or_else(|| env::var("OXIDE_AVATAR_URL").ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok((ws_base, room_id, sender_id, avatar_url))
}

fn room_ws_url(ws_base: &str, room_id: &str) -> Result<String> {
    let base = ws_base.trim();
    if !base.starts_with("ws://") && !base.starts_with("wss://") {
        anyhow::bail!("websocket base URL must start with ws:// or wss://");
    }
    let ws_url = format!("{}/room/{}", base.trim_end_matches('/'), room_id.trim());
    let _ = Url::parse(&ws_url).context("invalid websocket URL")?;
    Ok(ws_url)
}

async fn connect_room(
    ws_url: &str,
) -> Result<(
    mpsc::UnboundedSender<OutboundFrame>,
    mpsc::UnboundedReceiver<NetEvent>,
)> {
    let (stream, _) = connect_async(ws_url)
        .await
        .with_context(|| format!("failed to connect websocket: {ws_url}"))?;

    let (mut write, mut read) = stream.split();

    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<OutboundFrame>();
    let (net_tx, net_rx) = mpsc::unbounded_channel::<NetEvent>();

    let net_tx_writer = net_tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            let encoded = match msg {
                OutboundFrame::Chat(chat) => serde_json::to_string(&chat),
                OutboundFrame::Presence(presence) => serde_json::to_string(&presence),
            };

            match encoded {
                Ok(body) => {
                    if let Err(err) = write.send(Message::Text(body)).await {
                        let _ = net_tx_writer.send(NetEvent::Error(format!("send failed: {err}")));
                        break;
                    }
                }
                Err(err) => {
                    let _ = net_tx_writer.send(NetEvent::Error(format!("serialize failed: {err}")));
                }
            }
        }
    });

    tokio::spawn(async move {
        while let Some(next) = read.next().await {
            match next {
                Ok(Message::Text(text)) => {
                    dispatch_incoming_bytes(&net_tx, text.as_bytes(), "text");
                }
                Ok(Message::Binary(bin)) => {
                    dispatch_incoming_bytes(&net_tx, &bin, "binary");
                }
                Ok(Message::Close(frame)) => {
                    let reason = frame
                        .map(|f| format!("socket closed: {}", f.reason))
                        .unwrap_or_else(|| "socket closed".to_string());
                    let _ = net_tx.send(NetEvent::Disconnected(reason));
                    break;
                }
                Ok(_) => {}
                Err(err) => {
                    let _ = net_tx.send(NetEvent::Error(format!("receive failed: {err}")));
                    break;
                }
            }
        }
    });

    Ok((outbound_tx, net_rx))
}

fn dispatch_incoming_bytes(
    net_tx: &mpsc::UnboundedSender<NetEvent>,
    bytes: &[u8],
    frame_type: &str,
) {
    match parse_incoming_frame(bytes) {
        Ok(IncomingFrame::Chat(msg)) => {
            let _ = net_tx.send(NetEvent::Incoming(msg));
        }
        Ok(IncomingFrame::Presence(event)) => {
            let _ = net_tx.send(NetEvent::Presence(event));
        }
        Err(err) => {
            let _ = net_tx.send(NetEvent::Error(format!("invalid {frame_type} JSON: {err}")));
        }
    }
}

fn handle_event(
    event: CEvent,
    state: &mut State,
    outbound_tx: &mpsc::UnboundedSender<OutboundFrame>,
) -> Option<UiAction> {
    match event {
        CEvent::Key(key) if key.kind == KeyEventKind::Press => handle_key(key, state, outbound_tx),
        CEvent::Resize(_, _) => None,
        _ => None,
    }
}

fn handle_key(
    key: KeyEvent,
    state: &mut State,
    outbound_tx: &mpsc::UnboundedSender<OutboundFrame>,
) -> Option<UiAction> {
    match state.mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => state.should_quit = true,
            KeyCode::Char('i') => state.mode = Mode::Insert,
            _ => {}
        },
        Mode::Insert => match key.code {
            KeyCode::Esc => state.mode = Mode::Normal,
            KeyCode::Enter => {
                let text = state.input.trim().to_string();
                if text.is_empty() {
                    return None;
                }

                if let Some(next_base) = text.strip_prefix("/ws ").map(str::trim) {
                    if next_base.is_empty() {
                        state.status = Status::Error;
                        state.status_text = "usage: /ws <ws://host:port>".to_string();
                    } else if next_base == state.ws_base {
                        state.status = Status::Connected;
                        state.status_text =
                            format!("already using websocket base {}", state.ws_base);
                    } else {
                        state.ws_base = next_base.to_string();
                        state.status = Status::Connecting;
                        state.status_text =
                            format!("switching websocket base to {}", state.ws_base);
                        state.input.clear();
                        return Some(UiAction::Reconnect {
                            clear_history: false,
                        });
                    }
                    state.input.clear();
                    return None;
                }

                if let Some(next_room) = text.strip_prefix("/room ").map(str::trim) {
                    if next_room.is_empty() {
                        state.status = Status::Error;
                        state.status_text = "usage: /room <room-id>".to_string();
                    } else if state.connected_room_id.as_deref() == Some(next_room)
                        && matches!(state.status, Status::Connected)
                    {
                        state.status = Status::Connected;
                        state.status_text = format!("already in room {}", next_room);
                    } else {
                        state.room_id = next_room.to_string();
                        state.status = Status::Connecting;
                        state.status_text = format!("switching room to {}", state.room_id);
                        state.input.clear();
                        return Some(UiAction::Reconnect {
                            clear_history: true,
                        });
                    }
                    state.input.clear();
                    return None;
                }

                if text == "/reconnect" {
                    state.status = Status::Connecting;
                    state.status_text = "reconnecting".to_string();
                    state.input.clear();
                    return Some(UiAction::Reconnect {
                        clear_history: false,
                    });
                }

                if text == "/clear" {
                    state.messages.clear();
                    state.status = Status::Connected;
                    state.status_text = "chat history cleared".to_string();
                    state.input.clear();
                    return None;
                }

                if let Some(filter) = text.strip_prefix("/icat").map(str::trim) {
                    state.input.clear();
                    if filter == "self" {
                        return Some(UiAction::ShowSelfAvatar);
                    }
                    if filter.is_empty() {
                        return Some(UiAction::ShowAvatar(None));
                    }
                    return Some(UiAction::ShowAvatar(Some(filter.to_string())));
                }

                if let Some(next_name) = text.strip_prefix("/name ").map(str::trim) {
                    if next_name.is_empty() {
                        state.status = Status::Error;
                        state.status_text = "usage: /name <username>".to_string();
                    } else {
                        state.sender_id = next_name.to_string();
                        state.status = Status::Connected;
                        state.status_text = format!("username set to {}", state.sender_id);
                    }
                    state.input.clear();
                    return None;
                }

                if let Some(next_avatar) = text.strip_prefix("/avatar ").map(str::trim) {
                    if next_avatar.is_empty() {
                        state.status = Status::Error;
                        state.status_text = "usage: /avatar <https://image-url>".to_string();
                    } else {
                        state.avatar_url = Some(next_avatar.to_string());
                        state.status = Status::Connected;
                        state.status_text = "avatar URL updated".to_string();
                    }
                    state.input.clear();
                    return None;
                }

                let msg = WireMessage {
                    sender_id: state.sender_id.clone(),
                    avatar_url: state.avatar_url.clone(),
                    payload_cipher: text.as_bytes().to_vec(),
                    created_at: Utc::now().timestamp_millis(),
                };

                if let Err(err) = outbound_tx.send(OutboundFrame::Chat(msg.clone())) {
                    state.status = Status::Error;
                    state.status_text = format!("send queue error: {err}");
                    return None;
                }

                state.messages.push(ChatLine {
                    sender_id: msg.sender_id,
                    avatar_url: msg.avatar_url,
                    body: decode_payload(&msg.payload_cipher),
                    created_at: msg.created_at,
                });
                state.input.clear();
            }
            KeyCode::Backspace => {
                state.input.pop();
            }
            KeyCode::Char(c) => {
                state.input.push(c);
            }
            _ => {}
        },
    }
    None
}

fn decode_payload(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                let _ = write!(&mut hex, "{byte:02x}");
            }
            hex
        }
    }
}

fn parse_incoming_frame(bytes: &[u8]) -> Result<IncomingFrame> {
    if let Ok(event) = serde_json::from_slice::<PresenceEvent>(bytes) {
        if event.kind == "presence.join" || event.kind == "presence.leave" {
            return Ok(IncomingFrame::Presence(event));
        }
    }

    let chat = serde_json::from_slice::<WireMessage>(bytes)
        .context("expected WireMessage or presence event payload")?;
    Ok(IncomingFrame::Chat(chat))
}

fn send_presence_join(
    outbound_tx: &mpsc::UnboundedSender<OutboundFrame>,
    sender_id: &str,
    avatar_url: Option<String>,
) {
    let event = PresenceEvent {
        kind: "presence.join".to_string(),
        sender_id: sender_id.to_string(),
        avatar_url,
        created_at: Utc::now().timestamp_millis(),
    };
    let _ = outbound_tx.send(OutboundFrame::Presence(event));
}

fn ui(frame: &mut Frame, state: &State) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(frame.size());

    let status_color = match state.status {
        Status::Connected => GREEN,
        Status::Connecting => BLUE,
        Status::Error => RED,
        Status::Disconnected => RED,
    };

    let mode_label = match state.mode {
        Mode::Normal => "NORMAL",
        Mode::Insert => "INSERT",
    };

    let header = Paragraph::new(Line::from(vec![
        Span::styled("Base: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(
            state.ws_base.as_str(),
            Style::default().fg(TEXT).bg(Color::Reset),
        ),
        Span::styled(" | ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled("User: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(
            state.sender_id.as_str(),
            Style::default().fg(TEXT).bg(Color::Reset),
        ),
        Span::styled(" | ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled("Avatar: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(
            if state.avatar_url.is_some() {
                "set"
            } else {
                "none"
            },
            Style::default().fg(TEXT).bg(Color::Reset),
        ),
        Span::styled(" | ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled("Room: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(
            state.room_id.as_str(),
            Style::default().fg(TEXT).bg(Color::Reset),
        ),
        Span::styled(" | Status: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(
            state.status_text.as_str(),
            Style::default().fg(status_color).bg(Color::Reset),
        ),
        Span::styled(" | Mode: ", Style::default().fg(TEXT).bg(Color::Reset)),
        Span::styled(mode_label, Style::default().fg(TEXT).bg(Color::Reset)),
    ]))
    .style(Style::default().fg(TEXT).bg(Color::Reset))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BLUE).bg(Color::Reset))
            .style(Style::default().bg(Color::Reset)),
    );
    frame.render_widget(header, layout[0]);

    let items: Vec<ListItem> = state
        .messages
        .iter()
        .map(|msg| {
            let ts = chrono::DateTime::from_timestamp_millis(msg.created_at)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "--:--:--".to_string());
            ListItem::new(Text::from(vec![
                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", ts),
                        Style::default().fg(BLUE).bg(Color::Reset),
                    ),
                    Span::styled(
                        format!("{}:", msg.sender_id),
                        Style::default().fg(GREEN).bg(Color::Reset),
                    ),
                ]),
                Line::from(Span::styled(
                    msg.body.clone(),
                    Style::default().fg(TEXT).bg(Color::Reset),
                )),
            ]))
        })
        .collect();

    let messages = List::new(items)
        .style(Style::default().fg(TEXT).bg(Color::Reset))
        .block(
            Block::default()
                .title("Messages")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE).bg(Color::Reset))
                .style(Style::default().bg(Color::Reset)),
        );

    let mut list_state = ListState::default();
    if !state.messages.is_empty() {
        // Keep the viewport following the latest message.
        list_state.select(Some(state.messages.len() - 1));
    }
    frame.render_stateful_widget(messages, layout[1], &mut list_state);

    let footer = Paragraph::new(state.input.as_str())
        .style(Style::default().fg(TEXT).bg(Color::Reset))
        .block(
            Block::default()
                .title("Input (i: insert, Esc: normal, q: quit, /ws, /room, /name, /avatar, /icat, /reconnect, /clear)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE).bg(Color::Reset))
                .style(Style::default().bg(Color::Reset)),
        );
    frame.render_widget(footer, layout[2]);
}

async fn show_avatar_with_kitty(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut State,
    sender_filter: Option<String>,
) -> Result<()> {
    let selected = select_avatar_message(&state.messages, sender_filter.as_deref())
        .context("no matching message with avatar_url found")?;
    let avatar_url = selected
        .avatar_url
        .as_deref()
        .context("selected message has no avatar_url")?;
    let image_path = ensure_cached_avatar(avatar_url, &mut state.avatar_cache).await?;

    preview_image_with_kitty(terminal, &image_path)
}

async fn show_self_avatar_with_kitty(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut State,
) -> Result<()> {
    let avatar_url = state
        .avatar_url
        .as_deref()
        .context("self avatar URL is not set; use /avatar <https://image-url>")?;
    let image_path = ensure_cached_avatar(avatar_url, &mut state.avatar_cache).await?;
    preview_image_with_kitty(terminal, &image_path)
}

fn select_avatar_message<'a>(
    messages: &'a [ChatLine],
    sender_filter: Option<&str>,
) -> Option<&'a ChatLine> {
    match sender_filter {
        Some(sender) => messages
            .iter()
            .rev()
            .find(|m| m.sender_id == sender && m.avatar_url.is_some()),
        None => messages.iter().rev().find(|m| m.avatar_url.is_some()),
    }
}

async fn ensure_cached_avatar(
    avatar_url: &str,
    cache: &mut HashMap<String, PathBuf>,
) -> Result<PathBuf> {
    if let Some(existing) = cache.get(avatar_url).filter(|path| path.exists()) {
        return Ok(existing.clone());
    }

    let mut hasher = DefaultHasher::new();
    avatar_url.hash(&mut hasher);
    let hashed = hasher.finish();
    let (bytes, extension) = if let Some((data, ext)) = decode_data_avatar_url(avatar_url)? {
        (data, ext.to_string())
    } else {
        let response = reqwest::get(avatar_url)
            .await
            .with_context(|| format!("failed to fetch avatar URL: {avatar_url}"))?
            .error_for_status()
            .with_context(|| format!("avatar URL returned non-success status: {avatar_url}"))?;
        let data = response
            .bytes()
            .await
            .context("failed to read avatar bytes")?
            .to_vec();
        let ext = infer_extension_from_url(avatar_url)
            .unwrap_or("img")
            .to_string();
        (data, ext)
    };

    let cache_dir = env::temp_dir().join("oxide_chat_cli_avatars");
    fs::create_dir_all(&cache_dir).await?;
    let path = cache_dir.join(format!("{hashed:016x}.{extension}"));
    fs::write(&path, &bytes).await?;

    cache.insert(avatar_url.to_string(), path.clone());
    Ok(path)
}

fn decode_data_avatar_url(avatar_url: &str) -> Result<Option<(Vec<u8>, &'static str)>> {
    if !avatar_url.starts_with("data:") {
        return Ok(None);
    }

    let (meta, data) = avatar_url
        .split_once(',')
        .context("invalid data URL avatar: missing comma separator")?;
    let mime_and_flags = meta.trim_start_matches("data:");
    let is_base64 = mime_and_flags.split(';').any(|p| p == "base64");
    let mime = mime_and_flags
        .split(';')
        .next()
        .filter(|m| !m.is_empty())
        .unwrap_or("application/octet-stream");
    let ext = extension_from_mime(mime).unwrap_or("img");

    let decoded = if is_base64 {
        BASE64_STANDARD
            .decode(data)
            .context("invalid base64 data URL payload")?
    } else {
        percent_decode_str(data).collect::<Vec<u8>>()
    };

    Ok(Some((decoded, ext)))
}

fn extension_from_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/webp" => Some("webp"),
        "image/gif" => Some("gif"),
        "image/svg+xml" => Some("svg"),
        _ => None,
    }
}

fn preview_image_with_kitty(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    image_path: &Path,
) -> Result<()> {
    suspend_tui(terminal)?;
    let preview_result = (|| -> Result<()> {
        run_kitty_icat(image_path)?;
        println!();
        println!("Press Enter to return to chat...");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(())
    })();
    resume_tui(terminal)?;
    preview_result
}

fn infer_extension_from_url(url: &str) -> Option<&'static str> {
    let lower = url.to_ascii_lowercase();
    if lower.ends_with(".png") {
        return Some("png");
    }
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        return Some("jpg");
    }
    if lower.ends_with(".webp") {
        return Some("webp");
    }
    if lower.ends_with(".gif") {
        return Some("gif");
    }
    None
}

fn run_kitty_icat(image_path: &Path) -> Result<()> {
    if env::var_os("KITTY_WINDOW_ID").is_none() {
        anyhow::bail!("not running inside Kitty terminal (KITTY_WINDOW_ID not set)");
    }

    let attempts: [(&str, &[&str]); 2] = [("kitten", &["icat"]), ("kitty", &["+kitten", "icat"])];

    let mut errors: Vec<String> = Vec::new();
    for (bin, args) in attempts {
        let mut cmd = Command::new(bin);
        cmd.args(args).arg(image_path);
        match cmd.status() {
            Ok(status) if status.success() => return Ok(()),
            Ok(status) => errors.push(format!("{bin} exited with status {status}")),
            Err(err) => errors.push(format!("{bin} failed: {err}")),
        }
    }

    anyhow::bail!("failed to run Kitty icat: {}", errors.join("; "))
}

fn suspend_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn resume_tui(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}
