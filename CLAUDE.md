# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Summary

`todo-ultra-max` is a real-time shared Kanban/todo board with a single-file vanilla frontend (`index.html`, deployed to GitHub Pages) and a Rust backend (`server/`, using `axum`, `tokio`, and bundled `rusqlite` in WAL mode) deployed via Docker and Cloudflare Tunnel.

## Commands

### Backend (`server/`)
```bash
cd server
cargo check                     # Fast typecheck
cargo run                       # Run locally on PORT (default 3000)
cargo build --release           # Optimized release binary
cargo test                      # Run tests
cargo clippy --all-targets      # Lint
cargo fmt --check               # Format check

```

### Frontend

* No build step. Open or serve `index.html` directly.
* Point at a local backend with `?api=http://127.0.0.1:3000` (defaults to `https://todo-ultra-max.mantikafasi.dev`).

### Docker (repo root)

```bash
docker compose up -d --build server cloudflared                # Named tunnel
docker compose --profile quick up -d --build server cloudflared-quick  # Quick tunnel

```

## Architecture

* **`index.html`** — Entire frontend (CSS, markup, WebSocket client, prediction market UI, Admin Aura Mode, Turnstile modal, Live Support widget, and WebGPU/WebGL2 background shaders). Keep it zero-build with no external bundlers or npm dependencies.
* **`server/src/main.rs`** — Axum HTTP router, REST API (`/api/*`), WebSocket hub (`/ws`), presence/typing/dragging broadcasts, rate limiting, API key auth, and Admin Aura Mode (`/api/admin/*`).
* **`server/src/db.rs`** — SQLite (WAL) schema and data access for todos, player wallets, bets, and leaderboard.
* **`server/src/captcha.rs`** — Cloudflare Turnstile `siteverify` integration and 5-move credit tracking.
* **`server/src/mcp.rs`** — Streamable HTTP MCP endpoint (`POST /mcp`) exposing `list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, and `get_leaderboard`.
* **`docker-compose.yml`** — Runs `server` bound strictly to `127.0.0.1:3000:3000` with persistent `/data/todos.db` volume (`todo-data`), fronted by `cloudflared`.

## Key Invariants & Gotchas

* **Frontend/backend sync:** Changes to REST shapes, WebSocket events (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), or auth headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) must be updated in both `server/src/` and `index.html`.
* **Bet settlement rules:**
* Statuses: `todo`, `in_progress`, `done`. Priorities: `low`, `medium`, `high`.
* Buckets: `speedrun` (< 5m), `today` (5m–1h), `eventually` (1h–24h), `never` (24h+ / deleted).
* Moving a card to `done` settles its open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlement atomic.


* **Auth & Turnstile:**
* Preserve constant-time API key verification (`API_KEY`, `API_KEY_MODE=writes|all`) and `ADMIN_KEY` checks.
* Unverified writes return `428` with `{ captchaRequired: true, siteKey }` when Turnstile is active; `X-Admin-Token` bypasses Turnstile.


* **SQLite compatibility:** Existing deployments mount `todo-data:/data`. Keep schema changes additive or safely migrated on startup.
* **Scoped edits:** Do not modify the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly asked.

## Verification

Before finishing any change:

1. Run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` inside `server/`.
2. If `index.html` was modified, verify JS syntax and test against a local server using `?api=http://127.0.0.1:3000`.

