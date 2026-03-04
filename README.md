# Oxide Chat

Oxide Chat is a shared protocol chat stack with:

- a Cloudflare Worker backend (`BACKEND/`)
- a terminal client (`CLI/`)
- a web UI plus Android WebView shell (`FRONTEND/`)

All clients send the same wire payload so they can interoperate without translation.

## Repository Layout

- `BACKEND/`: room-based WebSocket relay + Durable Object persistence
- `CLI/`: Ratatui terminal client with reconnect/switch-room commands
- `FRONTEND/`: browser UI and Android wrapper using the same app assets

## Wire Protocol

Chat messages are JSON:

```json
{
  "sender_id": "string",
  "payload_cipher": [1, 2, 3],
  "created_at": 1700000000000
}
```

WebSocket route shape is always:

```text
/room/{room_id}
```

## Endpoint Configuration

Use a WebSocket base URL (host only), then clients append `/room/{room_id}`.

Examples:

- local dev base: `ws://127.0.0.1:8787`
- deployed base: `wss://chat.example.com`

Important:

- The backend does not auto-detect or default to your personal domain.
- Frontend defaults are placeholders (`wss://your-host.example.com`) until you set your own value.

## Quick Start (Local)

1. Start backend dev server.

```bash
cd BACKEND
make dev
```

2. Start CLI client.

```bash
cd CLI
cargo run -- --username alice ws://127.0.0.1:8787 general
```

CLI controls:

- `i`: insert mode
- `Esc`: normal mode
- `q`: quit

3. Start web frontend.

```bash
cd FRONTEND/web
python -m http.server 3000
```

Open `http://127.0.0.1:3000`, sign in, set base URL to `ws://127.0.0.1:8787`, then connect.

## Android Build (Optional)

```bash
cd FRONTEND/android
./gradlew :app:assembleDebug
```

APK output:

`FRONTEND/android/app/build/outputs/apk/debug/app-debug.apk`

## Firebase Config

Set Firebase values before running frontend auth:

- `FRONTEND/web/assets/js/firebase-config.js`
- `FRONTEND/android/app/src/main/assets/assets/js/firebase-config.js`

## Notes

- `payload_cipher` is currently UTF-8 bytes until shared E2EE is added.
- Backend stores encrypted payload bytes in Durable Object SQLite storage.
