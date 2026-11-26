# Shared Kanban & Todo Board

A real-time shared Kanban & todo board with a Dockerized TypeScript backend (SQLite + live WebSockets + Streamable HTTP MCP server), exposed through Cloudflare Tunnel.

## Features
- **Kanban & List views** with drag-and-drop columns (`To Do`, `In Progress`, `Done`), priorities (`low`, `medium`, `high`), and inline editing
- **Instant WebSocket sync** (`/ws`) with live online viewer count
- **Streamable HTTP MCP server** (`/mcp`) so AI agents can read, add, move, toggle, delete, and clear cards live

## Endpoints

| Endpoint | Purpose |
| --- | --- |
| `GET /api/health` | Health check + online count |
| `GET /api/todos` | List all shared cards |
| `POST /api/todos` | Add card (`{ "text", "status"?, "priority"? }`) |
| `PATCH /api/todos/:id` | Move/edit card (`{ "status"?, "priority"?, "text"?, "done"? }`) |
| `DELETE /api/todos/:id` | Delete a card |
| `DELETE /api/todos?completed=true` | Clear all Done cards |
| `GET /ws` | Live WebSocket sync (`{ "type": "sync", "todos", "online" }`) |
| `POST /mcp` | MCP Streamable HTTP (`list_todos`, `add_todo`, `move_todo`, `toggle_todo`, `delete_todo`, `clear_completed`) |

## Deploy on a VPS (Docker + Cloudflare Tunnel)

### Named Cloudflare Tunnel (permanent URL)
```bash
git clone https://github.com/tassak-dozu/todo-ultra-max.git
cd todo-ultra-max
cp .env.example .env   # set CLOUDFLARE_TUNNEL_TOKEN=...
docker compose up -d --build server cloudflared
```

### Quick Cloudflare Tunnel (zero-config HTTPS URL)
```bash
docker compose --profile quick up -d --build server cloudflared-quick
docker compose --profile quick logs -f cloudflared-quick
```
