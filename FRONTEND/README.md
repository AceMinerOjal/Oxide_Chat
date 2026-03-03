# Frontend

Mobile-first web UI for Oxide Chat, also bundled into the Android WebView app.

## Features

- Tailwind CSS + Catppuccin Mocha palette
- WebSocket connection to `/room/{id}`
- `WireMessage`-compatible payload format
- Firebase Google/GitHub sign-in before chat connect

## Run locally

```bash
cd FRONTEND
python -m http.server 3000
```

Open `http://127.0.0.1:3000`.

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
- Fill in Firebase config placeholders in:
  - `FRONTEND/assets/js/firebase-config.js`
  - `FRONTEND/android/app/src/main/assets/assets/js/firebase-config.js`
