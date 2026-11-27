# Project Rules — todo-ultra-max

## Workspace Overview

`todo-ultra-max` is a real-time shared Kanban/todo board:
- **Frontend:** `index.html` (zero-build vanilla HTML/CSS/JS, hosted on GitHub Pages).
- **Backend:** `server/` (Rust with `axum`, `tokio`, bundled `rusqlite` in WAL mode).
- **Deployment:** `docker-compose.yml` (`server` bound to `127.0.0.1:3000:3000`, persistent `todo-data:/data` volume, `cloudflared` tunnel).

## Module Map

- `index.html` — Markup, styles, WebSocket client, prediction market UI, Admin Aura Mode, Turnstile modal, Live Support widget, WebGPU/WebGL2 shaders.
- `server/src/main.rs` — Axum router, REST handlers (`/api/*`), `/ws` hub, presence/typing/dragging, rate limits, API key auth, Admin Aura Mode.
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) and tool handlers.

## Commands

- **Backend (`server/`):** `cargo check`, `cargo run`, `cargo build --release`, `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`
- **Frontend:** Open/serve `index.html` with `?api=http://127.0.0.1:3000` for local testing.
- **Docker (root):** `docker compose up -d --build server cloudflared`

## Rules & Invariants

1. **Zero-build frontend:** Do not add bundlers or npm dependencies to `index.html`. Preserve `?api=` override and dark/light themes.
2. **Contract parity:** Update both `server/src/` and `index.html` whenever REST payloads, WebSocket messages (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), or headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) change.
3. **Domain & bet settlement:** Statuses are `todo`, `in_progress`, `done`; priorities are `low`, `medium`, `high`; bet buckets are `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted). Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.
4. **MCP parity:** Keep `server/src/mcp.rs` tools (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, `get_leaderboard`) aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
5. **Security & persistence:** Preserve constant-time API key verification, `ADMIN_KEY` auth, Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`), backward-compatible SQLite migrations for `/data/todos.db`, and `127.0.0.1:3000:3000` binding.
6. **Scoped edits:** Avoid touching the WebGPU/WebGL2 shaders or Live Support widget in `index.html` unless explicitly requested.

## Verification

Run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` inside `server/` before finishing Rust changes.
