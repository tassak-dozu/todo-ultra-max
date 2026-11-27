# GEMINI.md

Context and instructions for Gemini CLI working on `todo-ultra-max`.

## Project Overview

`todo-ultra-max` is a real-time shared Kanban/todo board with:
- **Frontend (`index.html`):** Single-file, zero-build vanilla HTML/CSS/JS app hosted on GitHub Pages, featuring Kanban/List views, WebSocket live sync & presence, prediction gambling UI, Admin Aura Mode, Cloudflare Turnstile modal, Live Support widget, and a WebGPU/WebGL2 background.
- **Backend (`server/`):** Rust service (`axum` + `tokio` + bundled `rusqlite` in WAL mode) exposing REST (`/api/*`), WebSocket (`/ws`), and Streamable HTTP MCP (`/mcp`) endpoints.
- **Deployment (`docker-compose.yml`):** Rust container bound to `127.0.0.1:3000:3000` with persistent SQLite volume (`todo-data:/data`), fronted by `cloudflared`.

## File Map

- `index.html` — Complete frontend (markup, styles, client logic, WebGPU/WebGL2 shaders).
- `docker-compose.yml` — Service definitions for `server`, `cloudflared`, and `cloudflared-quick`.
- `server/Cargo.toml` & `server/Dockerfile` — Rust crate config and container build.
- `server/src/main.rs` — Axum router, REST handlers, `/ws` hub, presence/typing/dragging, rate limiting, API key auth, and Admin Aura Mode.
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) and tool handlers.

## Build, Run & Test

### Backend (from `server/`)
```bash
cargo check
cargo run
cargo build --release
cargo test
cargo clippy --all-targets
cargo fmt --check

```

### Frontend

* Serve or open `index.html` directly (no npm/bundler).
* Append `?api=http://127.0.0.1:3000` to target a local backend instead of `https://todo-ultra-max.mantikafasi.dev`.

### Docker (from repo root)

```bash
docker compose up -d --build server cloudflared
# Or zero-config quick tunnel:
docker compose --profile quick up -d --build server cloudflared-quick

```

## Core Rules & Invariants

1. **Keep `index.html` zero-build:** Do not introduce bundlers, frameworks, or external build steps. Preserve `?api=` override and dark/light theme support.
2. **Maintain frontend/backend parity:** Any changes to REST payloads, WebSocket message types (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), or headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) must be updated in both `server/src/` and `index.html`.
3. **Preserve domain & bet settlement logic:**
* Todo statuses: `todo`, `in_progress`, `done`; priorities: `low`, `medium`, `high`.
* Bet buckets: `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted).
* Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.


4. **Keep MCP aligned:** `server/src/mcp.rs` tools (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, `get_leaderboard`) must stay consistent with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
5. **Security & persistence:**
* Do not weaken constant-time API key checks (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, or Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`).
* Keep SQLite schema changes backward-compatible with existing `/data/todos.db` volumes.
* Keep `server` bound to `127.0.0.1:3000:3000` in `docker-compose.yml`.


6. **Stay scoped:** Do not touch the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly requested.

## Verification Checklist

* After Rust changes: run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` in `server/`.
* After frontend changes: verify JS syntax and test against a local backend with `?api=http://127.0.0.1:3000`.


