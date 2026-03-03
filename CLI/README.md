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
cargo run -- ws://127.0.0.1:8787 general
```

Arguments:

1. WebSocket base URL (default: `ws://127.0.0.1:8787`)
2. Room ID (default: `general`)
