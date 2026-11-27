# Project Rules — todo-ultra-max

## Overview

`todo-ultra-max` is a real-time shared Kanban/todo board with a single-file vanilla frontend (`index.html`, hosted on GitHub Pages) and a Rust backend (`server/`, using `axum`, `tokio`, and bundled `rusqlite` in WAL mode) deployed via Docker and Cloudflare Tunnel.

## Repository Structure

- `index.html` — Self-contained frontend (HTML/CSS/JS, Kanban/List views, WebSocket live sync & presence, prediction gambling UI, Admin Aura Mode, Turnstile modal, Live Support widget, WebGPU/WebGL2 background).
- `docker-compose.yml` — Services for `server` (bound strictly to `127.0.0.1:3000:3000` with `todo-data:/data`), `cloudflared`, and `cloudflared-quick`.
- `server/src/main.rs` — Axum HTTP router, REST API (`/api/*`), WebSocket hub (`/ws`), presence/typing/dragging, rate limiting, API key auth, and Admin Aura Mode (`/api/admin/*`).
- `server/src/db.rs` — SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
- `server/src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
- `server/src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) exposing `list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, and `get_leaderboard`.

## Build, Run & Test Commands

- **Backend (`server/`):** `cargo check`, `cargo run`, `cargo build --release`, `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`
- **Frontend:** No build step. Serve or open `index.html` directly and append `?api=http://127.0.0.1:3000` to target a local backend.
- **Docker (root):** `docker compose up -d --build server cloudflared`

## Critical Rules

- **Zero-build frontend:** Never introduce bundlers, frameworks, or npm dependencies into `index.html`. Preserve the `?api=` query override and dark/light themes.
- **Frontend/backend contract:** Any change to REST payloads, WebSocket message types (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), or auth headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) must be updated in both `server/src/` and `index.html`.
- **Bet settlement integrity:** Valid statuses are `todo`, `in_progress`, `done`; priorities are `low`, `medium`, `high`; bet buckets are `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted). Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.
- **MCP parity:** Keep `server/src/mcp.rs` tools aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
- **Security & migrations:** Never weaken constant-time API key verification (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, or Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`). Keep SQLite schema changes backward-compatible with existing `/data/todos.db` volumes, and keep `server` bound to `127.0.0.1:3000:3000`.
- **Scoped edits:** Do not modify the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly asked.

## Verification

Before completing any Rust task, run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` inside `server/`.
