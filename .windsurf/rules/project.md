---
trigger: always_on
description: "Core architecture, commands, and invariants for todo-ultra-max"
---

# todo-ultra-max — Windsurf Rules

## Architecture

`todo-ultra-max` is a real-time shared Kanban/todo board with a zero-build vanilla frontend (`index.html`, hosted on GitHub Pages) and a Rust backend (`server/`, using `axum`, `tokio`, and bundled `rusqlite` in WAL mode) behind a Cloudflare Tunnel (`docker-compose.yml`).

- `index.html` — Complete frontend (HTML/CSS/JS, Kanban/List views, WebSocket live sync & presence, prediction gambling UI, Admin Aura Mode, Turnstile modal, Live Support widget, WebGPU/WebGL2 background).
- `server/src/main.rs` — Axum router, REST API (`/api/*`), WebSocket hub (`/ws`), presence/typing/dragging, rate limits, API key auth, and Admin Aura Mode (`/api/admin/*`).
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` integration and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) exposing `list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, and `get_leaderboard`.
- `docker-compose.yml` — Runs `server` bound strictly to `127.0.0.1:3000:3000` with persistent `/data/todos.db` (`todo-data`), fronted by `cloudflared`.

## Commands

- **Backend (`server/`):** `cargo check`, `cargo run`, `cargo build --release`, `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`
- **Frontend:** Open/serve `index.html` directly; append `?api=http://127.0.0.1:3000` to target a local backend.
- **Docker (root):** `docker compose up -d --build server cloudflared`

## Rules & Invariants

- **Zero-build frontend:** Keep `index.html` self-contained (vanilla HTML/CSS/JS, no bundlers or npm dependencies). Preserve `?api=` override and dark/light themes.
- **Contract parity:** Keep REST payloads, WebSocket events (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), and headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) aligned between `server/src/` and `index.html`.
- **Domain & bet settlement:** Statuses are `todo`, `in_progress`, `done`; priorities are `low`, `medium`, `high`; bet buckets are `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted). Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.
- **MCP parity:** Keep `server/src/mcp.rs` tools aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
- **Security & persistence:** Preserve constant-time API key verification (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`), backward-compatible SQLite migrations for `/data/todos.db`, and `127.0.0.1:3000:3000` binding.
- **Scoped edits:** Do not modify the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly requested.

## Verification

After Rust changes, run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` in `server/`.
