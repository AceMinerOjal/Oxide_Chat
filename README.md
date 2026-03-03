# Oxide Chat

Rust-based chat project with a shared wire format across backend, CLI, and frontend.
This repository is a template: replace placeholder config and endpoints with your own infrastructure before use.

## Repository Layout

- `BACKEND/`: Cloudflare Worker + Durable Object WebSocket room backend
- `CLI/`: Ratatui terminal chat client (Vim-style controls)
- `FRONTEND/`: Mobile-first web UI and Android WebView wrapper

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

1. Run backend

```bash
cd BACKEND
# configure wrangler auth/bindings first
wrangler dev
```

2. Run CLI client

```bash
cd CLI
cargo run -- --username alice <ws-base-url> general
```

Controls:

- `i` enter insert mode
- `Esc` return to normal mode
- `q` quit

3. Run web frontend (optional)

```bash
cd FRONTEND
python -m http.server 3000
```

Open `http://127.0.0.1:3000`.

4. Build Android app (optional)

```bash
cd FRONTEND/android
./gradlew :app:assembleDebug
```

APK output:
`FRONTEND/android/app/build/outputs/apk/debug/app-debug.apk`

## Notes

- `payload_cipher` currently uses UTF-8 bytes until shared E2EE is integrated.
- Backend stores encrypted payload bytes in Durable Object SQLite storage.
- Android sign-in uses Firebase redirect flow inside WebView.
- Fill in your Firebase config placeholders in:
  - `FRONTEND/assets/js/firebase-config.js`
  - `FRONTEND/android/app/src/main/assets/assets/js/firebase-config.js`
