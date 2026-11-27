# AGENTS.md

Instructions for AI coding agents working on `todo-ultra-max`.

## Project Overview

`todo-ultra-max` is a shared real-time Kanban/todo board with a single-file vanilla frontend (`index.html`, hosted on GitHub Pages) and a Rust backend (`server/`, built with `axum`, `tokio`, and `rusqlite` in WAL mode) deployed behind a Cloudflare Tunnel.

Core features:
- Shared Kanban (`todo`, `in_progress`, `done`) and List views with real-time WebSocket sync, presence orbs, typing indicators, remote-drag highlights, and RTT telemetry.
- Parimutuel prediction gambling on card completion times (`speedrun`, `today`, `eventually`, `never`) with player wallets, bailouts, and a global leaderboard.
- Adaptive Cloudflare Turnstile gate (`interaction-only`, server-side `siteverify`, unlocking 5 moves for 5 minutes).
- Password-protected Admin Aura Mode (Turnstile bypass + nuclear board/bet wipes).
- Optional API key gatekeeper (`writes` or `all` mode) and a Streamable HTTP MCP server (`POST /mcp`).

## Repository Structure

```text
.
├── docker-compose.yml   # Rust server + cloudflared (named & quick-profile tunnels)
├── index.html           # Self-contained frontend (HTML/CSS/JS + WebGPU/WebGL2 background)
├── README.md            # Feature overview, API reference, MCP & self-host docs
└── server/
    ├── Cargo.toml       # Rust dependencies & release profile
    ├── Dockerfile       # Container build for the Rust backend
    └── src/
        ├── main.rs      # Axum router, REST handlers, WebSocket (/ws), auth, admin & rate limits
        ├── db.rs        # SQLite (WAL) schema & queries for todos, wallets, bets, leaderboard
        ├── captcha.rs   # Cloudflare Turnstile siteverify & move-credit tracking
        └── mcp.rs       # Streamable HTTP MCP endpoint (/mcp) and tool handlers
