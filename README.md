# Todo

A shared HTML todo app with a Dockerized TypeScript backend (SQLite + live WebSockets + MCP server), exposed via Cloudflare Tunnel.

## Endpoints

| Endpoint | Purpose |
| --- | --- |
| `GET /api/health` | Health check |
| `GET /api/todos` | List all shared tasks |
| `POST /api/todos` | Add task (`{ "text": "..." }`) |
| `PATCH /api/todos/:id` | Toggle/edit task (`{ "done": true }`) |
| `DELETE /api/todos/:id` | Delete a task |
| `DELETE /api/todos?completed=true` | Clear completed tasks |
| `GET /ws` | Live WebSocket sync (`{ "type": "sync", "todos": [...] }`) |
| `POST /mcp` | Streamable HTTP MCP server (`list_todos`, `add_todo`, `toggle_todo`, `delete_todo`, `clear_completed`) |

## Deploy on a VPS (Docker + Cloudflare Tunnel)

### Option A: Permanent URL with a Named Cloudflare Tunnel
1. Create a tunnel in Cloudflare Zero Trust → Networks → Tunnels, point a public hostname to `http://server:3000`, and copy the tunnel token.
2. On your VPS:
   ```bash
   git clone https://github.com/tassak-dozu/todo-ultra-max.git
   cd todo-ultra-max
   cp .env.example .env
   # set CLOUDFLARE_TUNNEL_TOKEN=... in .env
   docker compose up -d --build
   ```
3. Point `DEFAULT_API_BASE` in `index.html` to your tunnel hostname (or test immediately via `?api=https://your-domain.com`).

### Option B: Zero-config Quick Tunnel
```bash
docker compose --profile quick up -d --build server cloudflared-quick
docker compose logs -f cloudflared-quick
```
Copy the printed `https://*.trycloudflare.com` URL.

## MCP Client Configuration

Point any Streamable HTTP MCP client at `<your-api-url>/mcp`:

```json
{
  "mcpServers": {
    "todo": {
      "url": "https://your-domain.com/mcp"
    }
  }
}
```
