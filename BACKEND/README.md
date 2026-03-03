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

## Wire Message

Expected payload (JSON):

```json
{
  "sender_id": "string",
  "payload_cipher": [1, 2, 3],
  "created_at": 1700000000000
}
```
