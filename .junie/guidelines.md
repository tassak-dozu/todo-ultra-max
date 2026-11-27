# Junie Guidelines — todo-ultra-max

## Project Overview

`todo-ultra-max` is a real-time shared Kanban/todo board with:
- **Frontend (`index.html`):** Single-file, zero-build vanilla HTML/CSS/JS app (Kanban & List views, WebSocket live sync & presence, prediction gambling UI, Admin Aura Mode, Cloudflare Turnstile modal, Live Support widget, and WebGPU/WebGL2 background).
- **Backend (`server/`):** Rust service (`axum`, `tokio`, bundled `rusqlite` in WAL mode) serving REST (`/api/*`), WebSocket (`/ws`), and Streamable HTTP MCP (`/mcp`).
- **Deployment (`docker-compose.yml`):** Rust container bound to `127.0.0.1:3000:3000` with persistent `/data/todos.db` volume (`todo-data`), fronted by `cloudflared`.

## Repository Layout

- `index.html` — Complete frontend (markup, styles, client JS, WebGPU/WebGL2 shaders).
- `docker-compose.yml` — `server`, `cloudflared`, and `cloudflared-quick` services.
- `server/Cargo.toml` & `server/Dockerfile` — Rust crate config and container build.
- `server/src/main.rs` — Axum router, REST handlers, `/ws` hub, presence/typing/dragging, rate limits, API key auth, and Admin Aura Mode.
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) and tool handlers.

## Build, Run & Verification Commands

Run Rust commands from `server/`:
```bash
cargo check
cargo run
cargo build --release
cargo test
cargo clippy --all-targets
cargo fmt --check

```

Frontend testing:

* Serve or open `index.html` directly (no build step) and append `?api=http://127.0.0.1:3000` to point at a local backend.

Docker (from repo root):

```bash
docker compose up -d --build server cloudflared

```

## Coding Standards & Invariants

1. **Zero-build frontend:** Keep `index.html` self-contained (vanilla HTML/CSS/JS, no bundlers or npm dependencies). Preserve `?api=` override and dark/light themes.
2. **Contract parity:** Keep REST payloads, WebSocket messages (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), and headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) in sync between `server/src/` and `index.html`.
3. **Domain & bet settlement:**
* Statuses: `todo`, `in_progress`, `done`. Priorities: `low`, `medium`, `high`.
* Bet buckets: `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted).
* Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlement atomic.


4. **MCP parity:** Keep `server/src/mcp.rs` tools (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, `get_leaderboard`) aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
5. **Security & migrations:**
* Do not weaken constant-time API key checks (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, or Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`).
* Keep SQLite schema changes backward-compatible with existing `/data/todos.db` volumes.
* Keep `server` bound to `127.0.0.1:3000:3000` in `docker-compose.yml`.


6. **Scoped changes:** Avoid editing the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly requested.

## Before Finishing

Always run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` in `server/` after Rust edits.
