# Oxide Chat Backend

Cloudflare Worker + Durable Object backend for room-based encrypted chat relay and persistence.

## What It Does

- WebSocket endpoint at `/room/{room_id}`
- Durable Object room isolation by `room_id`
- SQLite-backed message persistence inside each room object
- Presence relay (`presence.join`, `presence.leave`)

## Prerequisites

- Rust toolchain (stable) with `wasm32-unknown-unknown` target
- `cargo` available in PATH
- Node.js (for Wrangler)
- Cloudflare account and authenticated `wrangler` CLI

Install the wasm target:

```bash
rustup target add wasm32-unknown-unknown
```

## Project Layout

- `src/lib.rs`: worker and Durable Object implementation
- `wrangler.toml`: worker, build, and Durable Object binding config
- `build/`: generated worker artifacts (not source of truth)

## Route Contract

Client connects to:

```text
wss://<your-backend-domain>/room/<room_id>
```

Behavior:

- Non-matching path returns `400` (`Expected path format: /room/{room_id}`)
- Matching path without WebSocket upgrade returns `426`
- Valid WebSocket requests are attached to a room Durable Object by `room_id`

Important:

- There is no backend-side default domain fallback.
- Host/domain is whatever URL the client calls.

## Local Development

```bash
make dev
```

Equivalent direct command:

```bash
wrangler dev
```

Then connect a client to:

```text
ws://127.0.0.1:8787/room/local-test
```

## Build and Validate

Run a Rust type/check pass:

```bash
make check
```

Build worker artifacts:

```bash
make build
```

## Deploy

Deploy to Cloudflare:

```bash
make deploy
```

Before deploy, ensure:

- You are logged in: `wrangler login`
- `wrangler.toml` contains the correct worker name/bindings for your environment

## Wire Message Format

Expected payload JSON:

```json
{
  "sender_id": "string",
  "payload_cipher": [1, 2, 3],
  "created_at": 1700000000000
}
```

Notes:

- `payload_cipher` is relayed and stored as bytes.
- The backend does not decrypt payloads.
- `avatar_url` is optional and may be absent.

## Storage Schema

Persisted in Durable Object SQLite:

- `sender_id` (`TEXT`)
- `avatar_url` (`TEXT`, nullable)
- `payload_cipher` (`BLOB`)
- `created_at` (`INTEGER`)

## Troubleshooting

- `wasm target not found`: run `rustup target add wasm32-unknown-unknown`
- `wrangler not found`: install Wrangler CLI and confirm it is on PATH
- `authentication failed`: run `wrangler login`
- `durable object binding errors`: verify `CHATROOM` binding and migrations in `wrangler.toml`
