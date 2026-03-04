# CLI Client

Terminal chat client for Oxide Chat using Ratatui.

## Features

- Vim-style controls (`i`, `Esc`, `q`)
- Async WebSocket networking via Tokio + tokio-tungstenite
- Catppuccin Mocha colors
- Transparent-friendly rendering (`Color::Reset` background)
- `List`-based message view for better wrapping behavior

## Usage

```bash
cd CLI
cargo run -- [--username <name>] [--avatar-url <https://...>] <ws-base-url> [room-id]
```

Arguments:

1. `--username <name>` or `-u <name>` (optional, or set `OXIDE_USERNAME`)
2. `--avatar-url <https://...>` (optional, or set `OXIDE_AVATAR_URL`)
3. WebSocket base URL (required, or set `OXIDE_WS_BASE`)
4. Room ID (optional, default: `general`)

If no username is provided, CLI uses `cli-<timestamp>`.

WebSocket base URL must start with `ws://` or `wss://`.
The client builds `<base>/room/<room-id>` internally.

## Examples

Local backend:

```bash
cargo run -- --username alice ws://127.0.0.1:8787 general
```

Deployed backend:

```bash
cargo run -- --username alice wss://chat.example.com general
```

Runtime commands (type in insert mode, then `Enter`):

- `/ws <ws://host:port>` change WebSocket base URL and reconnect
- `/room <room-id>` change room and reconnect
- `/name <username>` change sender username
- `/avatar <https://...>` change sender avatar URL
- `/reconnect` reconnect using current base URL + room
