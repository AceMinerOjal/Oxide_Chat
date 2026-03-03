# Oxide Chat

Rust-based encrypted chat project with:

- `BACKEND/`: Cloudflare Worker + Durable Object WebSocket room backend
- `CLI/`: Ratatui terminal chat client (Vim-style controls)
- `FRONTEND/`: Mobile-first web UI prototype for Tauri Mobile integration

## Protocol

All clients use the same JSON `WireMessage` payload:

```json
{
  "sender_id": "string",
  "payload_cipher": [1, 2, 3],
  "created_at": 1700000000000
}
```

WebSocket endpoint format:

- `/room/{id}`

## Quick Start

### 1. Run backend

```bash
cd BACKEND
# configure wrangler auth/bindings first
wrangler dev
```

### 2. Run CLI client

```bash
cd CLI
cargo run -- ws://127.0.0.1:8787 general
```

Controls:

- `i` enter insert mode
- `Esc` return to normal mode
- `q` quit

### 3. Run web frontend (optional)

```bash
cd FRONTEND
python -m http.server 3000
```

Open `http://127.0.0.1:3000`.

## Notes

- `payload_cipher` is currently plain UTF-8 bytes until the shared E2EE core is integrated.
- Backend stores encrypted payload bytes in Durable Object SQLite storage.
