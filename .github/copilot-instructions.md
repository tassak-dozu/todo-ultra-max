# GitHub Copilot Instructions — todo-ultra-max

## Architecture

`todo-ultra-max` is a shared real-time Kanban/todo board with a single-file vanilla frontend and a Rust backend:
- **`index.html`** — Zero-build frontend (HTML/CSS/JS) hosted on GitHub Pages. Includes Kanban & List views, WebSocket live sync/presence, prediction gambling UI, Admin Aura Mode, Cloudflare Turnstile modal, Live Support widget, and WebGPU/WebGL2 background shaders.
- **`server/src/main.rs`** — Axum HTTP router, REST API (`/api/*`), WebSocket hub (`/ws`), presence/typing/dragging, rate limiting, API key auth, and Admin Aura Mode (`/api/admin/*`).
- **`server/src/db.rs`** — Bundled SQLite (`rusqlite`, WAL mode) schema and queries for todos, wallets, bets, and leaderboard.
- **`server/src/captcha.rs`** — Cloudflare Turnstile `siteverify` integration and 5-move credit tracking.
- **`server/src/mcp.rs`** — Streamable HTTP MCP endpoint (`POST /mcp`) exposing `list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, and `get_leaderboard`.
- **`docker-compose.yml`** — Runs `server` bound strictly to `127.0.0.1:3000:3000` with persistent `/data/todos.db` (`todo-data` volume), fronted by `cloudflared`.

## Development & Verification

- **Backend (`server/`):** `cargo check`, `cargo run`, `cargo test`, `cargo clippy --all-targets`, `cargo fmt --check`.
- **Frontend:** No build step. Open/serve `index.html` with `?api=http://127.0.0.1:3000` to target a local server.
- **Docker (root):** `docker compose up -d --build server cloudflared`.

## Rules & Conventions

- Keep `index.html` completely self-contained (vanilla HTML/CSS/JS, no bundlers or npm packages). Preserve the `?api=` query override and dark/light themes.
- Keep REST schemas, WebSocket message types (`welcome`, `presence`, `sync`, `ping`/`pong`, `typing`, `dragging`), and auth headers (`Authorization: Bearer`, `X-Admin-Token`, `X-Captcha-Token`) synchronized between `server/src/` and `index.html`.
- Preserve valid statuses (`todo`, `in_progress`, `done`), priorities (`low`, `medium`, `high`), and bet buckets (`speedrun`, `today`, `eventually`, `never`). Moving a card to `done` settles open bets; deleting an open card settles `never` as the winner.
- Keep MCP tools in `server/src/mcp.rs` consistent with REST mutations, WebSocket broadcasts, and `mcpCalls` telemetry.
- Never weaken constant-time API key verification, `ADMIN_KEY` checks, or Turnstile `siteverify` (`428` with `captchaRequired: true`; bypassed by `X-Admin-Token`).
- Keep SQLite schema changes additive or safely migrated on startup for existing `/data/todos.db` volumes.
- Do not modify the WebGPU/WebGL2 background shaders or Live Support widget in `index.html` unless specifically asked.

