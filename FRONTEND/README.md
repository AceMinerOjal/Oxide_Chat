# Frontend

Mobile-first web UI for Oxide Chat, also bundled into the Android WebView app.

## Features

- Tailwind CSS + Catppuccin Mocha palette
- WebSocket connection to `/room/{id}`
- `WireMessage`-compatible payload format
- Firebase Google sign-in before chat connect

## WebSocket Base URL

The app expects a WebSocket base URL (host only), then appends `/room/{room_id}`.

Examples:

- local: `ws://127.0.0.1:8787`
- deployed: `wss://chat.example.com`

The default shown in the input is a placeholder:

```text
wss://your-host.example.com
```

It is not auto-populated from your deployed backend domain.

## Run locally

```bash
cd FRONTEND/web
python -m http.server 3000
```

Open `http://127.0.0.1:3000`.

After sign-in:

1. Set base URL to `ws://127.0.0.1:8787` (or your deployed `wss://...` host)
2. Choose room ID
3. Connect

## Android

Android project is in `FRONTEND/android`.

Build debug APK:

```bash
cd FRONTEND/android
./gradlew :app:assembleDebug
```

## Notes

- `payload_cipher` currently uses UTF-8 bytes to match backend schema.
- For Android emulator + local backend, use `ws://10.0.2.2:<port>`.
- Base URL and room ID are persisted in local storage keys:
  - `oxide.baseUrl`
  - `oxide.roomId`
- Fill in Firebase config placeholders in:
  - `FRONTEND/web/assets/js/firebase-config.js`
  - `FRONTEND/android/app/src/main/assets/assets/js/firebase-config.js`
