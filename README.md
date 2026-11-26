<div align="center">

# ⚡ TODO ULTRA MAX ⚡

### 🌌 Real-Time Shared Kanban Board · 🔌 Native WebSockets · 🤖 Live MCP · 🛡️ Cloudflare Tunnel

[![Live App](https://img.shields.io/badge/🚀_LIVE_APP-GitHub_Pages-8b5cf6?style=for-the-badge)](https://tassak-dozu.github.io/todo-ultra-max/)
[![Live API](https://img.shields.io/badge/⚡_API-Cloudflare_Tunnel-22d3ee?style=for-the-badge)](https://todo-ultra-max.mantikafasi.dev/api/health)
[![MCP Ready](https://img.shields.io/badge/🤖_MCP-Streamable_HTTP-f472b6?style=for-the-badge)](https://todo-ultra-max.mantikafasi.dev/mcp)
[![Runtime](https://img.shields.io/badge/🥟_Bun-SQLite_WAL-10b981?style=for-the-badge)](https://bun.sh)
[![Docker](https://img.shields.io/badge/🐳_Docker-Zero_Exposed_Ports-0ea5e9?style=for-the-badge)](./docker-compose.yml)

**One shared board. Zero frameworks. Maximum aura. Pure velocity.** 🔥

</div>

---

## ✨ What's Inside

- 🗂️ **Kanban + List Views** — Silky drag-and-drop across **Procrastinating (`To Do`)**, **Actually Cooking (`In Progress`)**, and **Certified Done (`Done`)** with priority glow pills (`Low` / `Medium` / `High`) and double-click inline editing.
- 🌌 **Maximum Aura UI** — Multi-layer drifting aurora mesh, cursor-reactive spotlight cards, remote-drag glow rings, neon update flashes, and Dark/Light mode.
- 📡 **Live Telemetry & Presence** — Real-time WebSocket RTT (`ms`) + signal bars, stacked glowing online viewer orbs, live *"Someone is cooking..."* typing indicator, and a live activity ticker + stream drawer.
- 🤖 **Built-in MCP Server (`/mcp`)** — Plug in any Streamable HTTP MCP client and watch cards appear, move, and vanish live in every browser tab.
- 🔐 **API Key Gatekeeper** — Optional `API_KEY` protection (`writes` or `all` mode) with an in-app API Key modal, live verification, and automatic `Authorization: Bearer` injection into REST, WebSocket, and MCP snippets.
- 🛡️ **Zero-Exposed-Port VPS Deployment** — Runs inside Docker on a VPS with a permanent Cloudflare Tunnel sidecar (`cloudflared`) so your origin IP stays invisible.

---

## 🧭 Endpoints

| Method & Path | Description |
| --- | --- |
| `GET /api/health` | 💓 Health check, auth mode, live online count & MCP call counter |
| `GET /api/auth/verify` | 🔑 Verify an API key (`Authorization: Bearer` / `X-API-Key` / `?api_key=`) |
| `GET /api/todos` | 📋 Fetch all shared cards + live presence & activity feed |
| `POST /api/todos` | ➕ Spawn a card (`{ "text", "status"?, "priority"? }`) |
| `PATCH /api/todos/:id` | 🚚 Move column, flip priority, or edit text |
| `DELETE /api/todos/:id` | 🗑️ Yeet a single card into the void |
| `DELETE /api/todos?completed=true` | 🧹 Nuke everything in Done |
| `GET /ws` | ⚡ Live WebSocket stream (sync, presence, typing, remote drag, ping/pong RTT) |
| `POST /mcp` | 🤖 Streamable HTTP MCP (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`) |

---

## 🤖 Connect via MCP

Drop this straight into your MCP client config:

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

## 🐳 Self-Host on a VPS (Docker + Cloudflare Tunnel)

```bash
git clone https://github.com/tassak-dozu/todo-ultra-max.git
cd todo-ultra-max
cp .env.example .env
# Add CLOUDFLARE_TUNNEL_TOKEN=... (and optional API_KEY=... / API_KEY_MODE=writes)
docker compose up -d --build server cloudflared
```

> ⚡ Need a zero-config test URL without a tunnel token?
> ```bash
> docker compose --profile quick up -d --build server cloudflared-quick
> docker compose --profile quick logs -f cloudflared-quick
> ```

---

## 🔐 Environment Variables

| Variable | Default | Purpose |
| --- | --- | --- |
| `CLOUDFLARE_TUNNEL_TOKEN` | *(none)* | 🛡️ Permanent Cloudflare Zero Trust Tunnel token |
| `API_KEY` | *(empty = open)* | 🔑 Secret key required for writes (or all requests) |
| `API_KEY_MODE` | `writes` | 🚦 `writes` (public read, key to mutate) or `all` (key for everything) |
| `MAX_TODOS` | `500` | 📦 Max cards allowed on the board |
| `WRITE_LIMIT_PER_MIN` | `90` | 🧱 Per-IP writes/min rate limit (honors `CF-Connecting-IP`) |
