# Frontend Prototype

Mobile-first web UI prototype intended for Tauri Mobile embedding.

## Features

- Tailwind CSS + Catppuccin Mocha palette
- WebSocket connection to `/room/{id}`
- `WireMessage`-compatible payload format

## Run locally

```bash
cd FRONTEND
python -m http.server 3000
```

Open `http://127.0.0.1:3000`.

## Notes

- `payload_cipher` currently uses UTF-8 bytes to match backend schema.
- Replace JS byte conversion with shared Rust E2EE integration in full Tauri setup.
