<div align="center">

# ⚡ TODO ULTRA MAX ⚡

### ✨ Powered by AI™ · 🦀 Blazingly Fast Rust Backend · 🔌 Native WebSockets · 🎰 Prediction Gambling · 🤖 Live MCP · 🛡️ Adaptive Aura Captcha

[![Live App](https://img.shields.io/badge/🚀_LIVE_APP-GitHub_Pages-8b5cf6?style=for-the-badge)](https://tassak-dozu.github.io/todo-ultra-max/)
[![Powered by AI](https://img.shields.io/badge/✨_POWERED_BY-AI™-f472b6?style=for-the-badge)](https://tassak-dozu.github.io/todo-ultra-max/)
[![Blazingly Fast Rust](https://img.shields.io/badge/🦀_BACKEND-BLAZINGLY_FAST_RUST-f97316?style=for-the-badge)](./server/src/main.rs)
[![Live API](https://img.shields.io/badge/⚡_API-Cloudflare_Tunnel-22d3ee?style=for-the-badge)](https://todo-ultra-max.mantikafasi.dev/api/health)
[![MCP Ready](https://img.shields.io/badge/🤖_MCP-Streamable_HTTP-10b981?style=for-the-badge)](https://todo-ultra-max.mantikafasi.dev/mcp)
[![Docker](https://img.shields.io/badge/🐳_Docker-Zero_Exposed_Ports-0ea5e9?style=for-the-badge)](./docker-compose.yml)

**One shared board. Powered by AI™. Rewritten in Blazingly Fast Rust (`axum` + `tokio` + bundled SQLite). Zero GC pauses. Maximum aura.** 🔥🦀✨

</div>

---

## ✨ What's Inside

- 🦀 **Blazingly Fast Rust Backend** — Built with `axum` + `tokio` + `rusqlite` (WAL mode) compiled to a tiny stripped release binary. Sub-millisecond responses, single-digit MB RAM, zero garbage collection.
- ✨ **Powered by AI™ Everywhere** — Iridescent AI aura badges, Agent Kevin's Live Support™ garbage-English engine, and native MCP robot control.
- 🗂️ **Kanban + List Views** — Drag-and-drop across **Procrastinating (`To Do`)**, **Actually Cooking (`In Progress`)**, and **Certified Done (`Done`)** with priority glow pills and double-click inline editing.
- 🎰 **Card Completion Prediction Gambling** — Start with `1,000 🪙` Aura Coins and bet on when any open card finishes (`⚡ < 5 min`, `🔥 5m – 1h`, `🐢 1h – 24h`, `💀 24h+ / Nuked`) with live parimutuel odds, auto-settlement, emergency bailouts, global leaderboard, and MCP `place_bet` / `get_leaderboard` tools.
- 🛡️ **Cloudflare Turnstile Gate** — Zero free unverified adds/deletes: any unverified IP must pass **Cloudflare Turnstile** (`interaction-only` — silent for trusted IPs/browsers, interactive only when Cloudflare flags risk) verified via server-side `siteverify`, unlocking 5 smooth moves for 5 minutes.
- 📡 **Live Telemetry & Presence** — Real-time WebSocket RTT (`ms`), signal bars, stacked online viewer orbs, typing indicator, remote-drag neon glow, and live board gossip feed.
- 🔐 **API Key Gatekeeper** — Optional `API_KEY` protection (`writes` or `all` mode) with constant-time SHA-256 verification and an in-app **🔑 VIP Pass** modal.

---

## 🧭 Endpoints

| Method & Path | Description |
| --- | --- |
| `GET /api/health` | 💓 Health check, Rust engine telemetry, auth mode, online count & MCP calls |
| `GET /api/auth/verify` | 🔑 Verify an API key (`Authorization: Bearer` / `X-API-Key` / `?api_key=`) |
| `POST /api/captcha/verify` | ✅ Verify Cloudflare Turnstile token via `siteverify` & unlock 5 moves |
| `GET /api/todos` | 📋 Fetch all shared cards, open betting markets, leaderboard & activity |
| `POST /api/todos` | ➕ Spawn a card (`{ "text", "status"?, "priority"? }`) |
| `PATCH /api/todos/:id` | 🚚 Move column (moving to `done` settles bets!), edit priority/text |
| `DELETE /api/todos/:id` | 🗑️ Yeet a card (deleting an open card settles `never` as winner!) |
| `DELETE /api/todos?completed=true` | 🧹 Nuke everything in Done |
| `GET /api/wallet` | 🪙 Fetch/create player wallet, bet history & leaderboard |
| `POST /api/wallet/bailout` | 💸 Emergency `+500 🪙` bailout when balance `< 100` |
| `POST /api/bets` | 🎰 Place a prediction wager on an open card |
| `GET /api/leaderboard` | 👑 Global richest degens leaderboard |
| `GET /ws` | ⚡ Live WebSocket stream (sync, presence, typing, remote drag, ping/pong RTT) |
| `POST /mcp` | 🤖 Streamable HTTP MCP (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`, `place_bet`, `get_leaderboard`) |

---

## 🤖 Connect via MCP

```json
{
  "mcpServers": {
    "todo-ultra-max": {
      "url": "https://todo-ultra-max.mantikafasi.dev/mcp",
      "headers": {
        "Authorization": "Bearer YOUR_API_KEY_IF_ENABLED"
      }
    }
  }
}
```

---

## 🐳 Self-Host on a VPS (Rust Docker + Cloudflare Tunnel)

```bash
git clone https://github.com/tassak-dozu/todo-ultra-max.git
cd todo-ultra-max
cp .env.example .env
# Add CLOUDFLARE_TUNNEL_TOKEN=... (and optional API_KEY=... / API_KEY_MODE=writes)
docker compose up -d --build server cloudflared
```
