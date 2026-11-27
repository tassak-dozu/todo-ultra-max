# CODEX.md

Instructions for Codex working on `todo-ultra-max`.

## Project Overview

`todo-ultra-max` is a real-time shared Kanban/todo board consisting of:
- **Frontend (`index.html`):** Single-file, zero-build vanilla HTML/CSS/JS app hosted on GitHub Pages (Kanban & List views, WebSocket sync/presence, prediction gambling UI, Admin Aura Mode, Cloudflare Turnstile modal, Live Support widget, and WebGPU/WebGL2 background).
- **Backend (`server/`):** Rust service (`axum` + `tokio` + bundled `rusqlite` in WAL mode) serving REST (`/api/*`), WebSocket (`/ws`), and Streamable HTTP MCP (`/mcp`).
- **Deployment (`docker-compose.yml`):** Rust container bound to `127.0.0.1:3000:3000` with persistent SQLite volume (`todo-data:/data`), fronted by `cloudflared`.

## Repository Layout

- `index.html` — Complete frontend (markup, styles, client JS, WebGPU/WebGL2 shaders).
- `docker-compose.yml` — Services for `server`, `cloudflared`, and `cloudflared-quick`.
- `server/Cargo.toml` & `server/Dockerfile` — Rust crate config and container build.
- `server/src/main.rs` — Axum router, REST endpoints, `/ws` hub, presence/typing/dragging, rate limiting, API key auth, and Admin Aura Mode.
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) and tool handlers.

## Commands

### Backend (`server/`)
```bash
cd server
cargo check                     # Typecheck
cargo run                       # Run locally on PORT (default 3000)
cargo build --release           # Release build
cargo test                      # Run tests
cargo clippy --all-targets      # Lint
cargo fmt --check               # Format check

```

### Frontend

* No build step. Serve or open `index.html` directly.
* Use `?api=http://127.0.0.1:3000` to target a local backend instead of `https://todo-ultra-max.mantikafasi.dev`.

### Docker (repo root)

```bash
docker compose up -d --build server cloudflared
# Quick tunnel (no token):
docker compose --profile quick up -d --build server cloudflared-quick

```

## Working Rules & Invariants

1. **Zero-build frontend:** Keep `index.html` self-contained (vanilla HTML/CSS/JS, no bundlers or npm dependencies). Preserve `?api=` override and dark/light theme support.
2. **Frontend/backend contract:** Any changes to REST payloads, WebSocket messages (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), or headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) must be updated in both `server/src/` and `index.html`.
3. **Domain & bet settlement integrity:**
* Statuses: `todo`, `in_progress`, `done`. Priorities: `low`, `medium`, `high`.
* Bet buckets: `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted).
* Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.


4. **MCP parity:** Keep `server/src/mcp.rs` tools (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, `get_leaderboard`) aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
5. **Security & persistence:**
* Preserve constant-time API key verification (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, and Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`).
* Keep SQLite schema changes backward-compatible with existing `/data/todos.db` volumes.
* Keep `server` bound strictly to `127.0.0.1:3000:3000` in `docker-compose.yml`.


6. **Scoped changes:** Avoid editing the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly requested.

## Verification

Before concluding:

1. Run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` in `server/`.
2. If `index.html` changed, verify JS syntax and test against a local backend via `?api=http://127.0.0.1:3000`.

