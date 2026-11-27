# todo-ultra-max — Replit Agent Guidelines

## Project Overview

`todo-ultra-max` is a real-time shared Kanban/todo board with a single-file vanilla frontend (`index.html`, hosted on GitHub Pages) and a Rust backend (`server/`, using `axum`, `tokio`, and bundled `rusqlite` in WAL mode) deployed behind a Cloudflare Tunnel (`docker-compose.yml`).

## Architecture & Tech Stack

- **Frontend (`index.html`):** Zero-build vanilla HTML/CSS/JS (no bundler, no npm). Includes Kanban & List views, WebSocket live sync & presence, prediction gambling UI, Admin Aura Mode, Cloudflare Turnstile modal, Live Support widget, and WebGPU/WebGL2 background shaders.
- **Backend (`server/`):**
  - `src/main.rs` — Axum router, REST API (`/api/*`), WebSocket hub (`/ws`), presence/typing/dragging, rate limiting, API key auth, and Admin Aura Mode (`/api/admin/*`).
  - `src/db.rs` — Bundled SQLite (WAL) schema and queries for todos, wallets, bets, and leaderboard.
  - `src/captcha.rs` — Cloudflare Turnstile `siteverify` verification and 5-move credit tracking.
  - `src/mcp.rs` — Streamable HTTP MCP endpoint (`POST /mcp`) exposing `list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, and `get_leaderboard`.
- **Deployment (`docker-compose.yml`):** Rust container bound strictly to `127.0.0.1:3000:3000` with persistent `/data/todos.db` (`todo-data` volume), fronted by `cloudflared`.

## Commands

- **Backend (`server/`):** `cargo check`, `cargo run`, `cargo build --release`, `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`
- **Frontend:** Serve or open `index.html` directly; append `?api=http://127.0.0.1:3000` to target a local backend.
- **Docker (root):** `docker compose up -d --build server cloudflared`

## Coding Standards & Invariants

- Keep `index.html` completely self-contained (no bundlers or npm dependencies). Preserve the `?api=` override and dark/light themes.
- Keep REST payloads, WebSocket events (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), and headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) synchronized between `server/src/` and `index.html`.
- Preserve valid statuses (`todo`, `in_progress`, `done`), priorities (`low`, `medium`, `high`), and bet buckets (`speedrun`, `today`, `eventually`, `never`). Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner. Keep wallet balances and settlements atomic.
- Keep MCP tools in `server/src/mcp.rs` aligned with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
- Never weaken constant-time API key verification (`API_KEY`, `API_KEY_MODE=writes|all`), `ADMIN_KEY` auth, or Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`).
- Keep SQLite schema changes backward-compatible with existing `/data/todos.db` volumes.
- Do not modify the WebGPU/WebGL2 shader pipeline or Live Support widget in `index.html` unless explicitly requested.
- Always run `cargo fmt --check`, `cargo clippy --all-targets`, and `cargo test` in `server/` after Rust changes.
