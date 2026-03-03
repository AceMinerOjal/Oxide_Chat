use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::{
    event::{Event as CEvent, EventStream, KeyCode, KeyEvent, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::{SinkExt, StreamExt};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use serde::{Deserialize, Serialize};
use std::{env, io, time::Duration};
use tokio::{select, sync::mpsc, time};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

const BLUE: Color = Color::Rgb(137, 180, 250);
const GREEN: Color = Color::Rgb(166, 227, 161);
const RED: Color = Color::Rgb(243, 139, 168);
const TEXT: Color = Color::Rgb(205, 214, 244);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireMessage {
    sender_id: String,
    payload_cipher: Vec<u8>,
    created_at: i64,
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
    body: String,
    created_at: i64,
}

#[derive(Debug)]
struct State {
    ws_base: String,
    room_id: String,
    mode: Mode,
    input: String,
    messages: Vec<ChatLine>,
    status_text: String,
    status: Status,
    sender_id: String,
    should_quit: bool,
}

#[derive(Debug)]
enum NetEvent {
    Incoming(WireMessage),
    Error(String),
    Disconnected(String),
}

#[derive(Debug, Clone, Copy)]
enum UiAction {
    Reconnect,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (ws_base, room_id, sender_id) = parse_cli_args()?;

    let ws_url = room_ws_url(&ws_base, &room_id)?;

    let mut state = State {
        ws_base,
        room_id,
        mode: Mode::Normal,
        input: String::new(),
        messages: Vec::new(),
        status_text: format!("connecting {}", ws_url),
        status: Status::Connecting,
        sender_id,
        should_quit: false,
    };

    let (mut outbound_tx, mut net_rx) = connect_room(&ws_url).await?;
    state.status = Status::Connected;
    state.status_text = "connected".to_string();

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
                            UiAction::Reconnect => {
                                match room_ws_url(&state.ws_base, &state.room_id) {
                                    Ok(next_url) => {
                                        state.status = Status::Connecting;
                                        state.status_text = format!("connecting {}", next_url);
                                        match connect_room(&next_url).await {
                                            Ok((new_tx, new_rx)) => {
                                                outbound_tx = new_tx;
                                                net_rx = new_rx;
                                                state.status = Status::Connected;
                                                state.status_text = "connected".to_string();
                                            }
                                            Err(err) => {
                                                state.status = Status::Error;
                                                state.status_text = err.to_string();
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        state.status = Status::Error;
                                        state.status_text = err.to_string();
                                    }
                                }
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
                                body: decode_payload(&msg.payload_cipher),
                                created_at: msg.created_at,
                            });
                        }
                        NetEvent::Error(err) => {
                            state.status = Status::Error;
                            state.status_text = err;
                        }
                        NetEvent::Disconnected(reason) => {
                            state.status = Status::Disconnected;
                            state.status_text = reason;
                        }
                    }
                } else {
                    state.status = Status::Disconnected;
                    state.status_text = "network task ended".to_string();
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

fn parse_cli_args() -> Result<(String, String, String)> {
    let mut positional: Vec<String> = Vec::new();
    let mut username: Option<String> = None;
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
            _ if arg.starts_with('-') => {
                anyhow::bail!("unknown flag: {arg}");
            }
            _ => positional.push(arg),
        }
    }

    if positional.len() > 2 {
        anyhow::bail!("too many positional arguments; usage: cargo run -- [--username <name>] <ws-base-url> [room-id]");
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

    Ok((ws_base, room_id, sender_id))
}

fn room_ws_url(ws_base: &str, room_id: &str) -> Result<String> {
    let base = ws_base.trim();
    if !base.starts_with("ws://") && !base.starts_with("wss://") {
        anyhow::bail!("websocket base URL must start with ws:// or wss://");
    }
    let ws_url = format!(
        "{}/room/{}",
        base.trim_end_matches('/'),
        room_id.trim()
    );
    let _ = Url::parse(&ws_url).context("invalid websocket URL")?;
    Ok(ws_url)
}

async fn connect_room(
    ws_url: &str,
) -> Result<(
    mpsc::UnboundedSender<WireMessage>,
    mpsc::UnboundedReceiver<NetEvent>,
)> {
    let (stream, _) = connect_async(ws_url)
        .await
        .with_context(|| format!("failed to connect websocket: {ws_url}"))?;

    let (mut write, mut read) = stream.split();

    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<WireMessage>();
    let (net_tx, net_rx) = mpsc::unbounded_channel::<NetEvent>();

    let net_tx_writer = net_tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            match serde_json::to_string(&msg) {
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
                Ok(Message::Text(text)) => match serde_json::from_str::<WireMessage>(&text) {
                    Ok(msg) => {
                        let _ = net_tx.send(NetEvent::Incoming(msg));
                    }
                    Err(err) => {
                        let _ =
                            net_tx.send(NetEvent::Error(format!("invalid message JSON: {err}")));
                    }
                },
                Ok(Message::Binary(bin)) => match serde_json::from_slice::<WireMessage>(&bin) {
                    Ok(msg) => {
                        let _ = net_tx.send(NetEvent::Incoming(msg));
                    }
                    Err(err) => {
                        let _ = net_tx.send(NetEvent::Error(format!("invalid binary JSON: {err}")));
                    }
                },
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

fn handle_event(
    event: CEvent,
    state: &mut State,
    outbound_tx: &mpsc::UnboundedSender<WireMessage>,
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
    outbound_tx: &mpsc::UnboundedSender<WireMessage>,
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
                    } else {
                        state.ws_base = next_base.to_string();
                        state.status = Status::Connecting;
                        state.status_text = format!("switching websocket base to {}", state.ws_base);
                        state.input.clear();
                        return Some(UiAction::Reconnect);
                    }
                    state.input.clear();
                    return None;
                }

                if let Some(next_room) = text.strip_prefix("/room ").map(str::trim) {
                    if next_room.is_empty() {
                        state.status = Status::Error;
                        state.status_text = "usage: /room <room-id>".to_string();
                    } else {
                        state.room_id = next_room.to_string();
                        state.status = Status::Connecting;
                        state.status_text = format!("switching room to {}", state.room_id);
                        state.input.clear();
                        return Some(UiAction::Reconnect);
                    }
                    state.input.clear();
                    return None;
                }

                if text == "/reconnect" {
                    state.status = Status::Connecting;
                    state.status_text = "reconnecting".to_string();
                    state.input.clear();
                    return Some(UiAction::Reconnect);
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

                let msg = WireMessage {
                    sender_id: state.sender_id.clone(),
                    payload_cipher: text.as_bytes().to_vec(),
                    created_at: Utc::now().timestamp_millis(),
                };

                if let Err(err) = outbound_tx.send(msg.clone()) {
                    state.status = Status::Error;
                    state.status_text = format!("send queue error: {err}");
                    return None;
                }

                state.messages.push(ChatLine {
                    sender_id: msg.sender_id,
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
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(_) => bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(""),
    }
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
                .title("Input (i: insert, Esc: normal, q: quit, /ws, /room, /name, /reconnect)")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BLUE).bg(Color::Reset))
                .style(Style::default().bg(Color::Reset)),
        );
    frame.render_widget(footer, layout[2]);
}
