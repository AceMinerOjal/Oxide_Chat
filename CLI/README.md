# CLI Client

Terminal chat client for Oxide Chat using Ratatui.

## Features

- Vim-style controls (`i`, `Esc`, `q`)
- Async WebSocket networking via Tokio + tokio-tungstenite
- Catppuccin Mocha colors
- Transparent-friendly rendering (`Color::Reset` background)
- `List`-based message view for better wrapping behavior

## Run

```bash
cd CLI
cargo run -- --username alice <ws-base-url> general
```

Arguments:

1. `--username <name>` or `-u <name>` (optional, or set `OXIDE_USERNAME`)
2. WebSocket base URL (required, or set `OXIDE_WS_BASE`)
3. Room ID (optional, default: `general`)

If no username is provided, CLI uses `cli-<timestamp>`.

Runtime commands (type in insert mode, then `Enter`):

- `/ws <ws://host:port>` change WebSocket base URL and reconnect
- `/room <room-id>` change room and reconnect
- `/name <username>` change sender username
- `/reconnect` reconnect using current base URL + room
