# Backend Worker

Cloudflare Worker Durable Object backend for Oxide Chat.

## Endpoint

- WebSocket upgrade at `/room/{room_id}`

## Storage

Messages are persisted in Durable Object SQLite storage with schema:

- `sender_id` (`TEXT`)
- `payload_cipher` (`BLOB`)
- `created_at` (`INTEGER`)

## Run

```bash
cd BACKEND
wrangler dev
```
