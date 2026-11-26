import { TodoDB, isValidPriority, isValidStatus } from "./db";
import { handleMcpPayload } from "./mcp";

const PORT = Number(process.env.PORT || 3000);
const DB_PATH = process.env.DB_PATH || "/data/todos.db";
const WRITE_LIMIT_PER_MIN = Number(process.env.WRITE_LIMIT_PER_MIN || 90);

const CORS_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PATCH, DELETE, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Accept, Mcp-Session-Id",
  "Access-Control-Expose-Headers": "Mcp-Session-Id",
};

const WS_TOPIC = "todos";

const db = new TodoDB(DB_PATH);
let onlineCount = 0;

// Simple in-memory per-IP write rate limiter
const writeBuckets = new Map<string, { count: number; resetAt: number }>();

function checkWriteRateLimit(req: Request): boolean {
  const ip =
    req.headers.get("cf-connecting-ip") ||
    req.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ||
    "local";
  const now = Date.now();
  const bucket = writeBuckets.get(ip);
  if (!bucket || now >= bucket.resetAt) {
    writeBuckets.set(ip, { count: 1, resetAt: now + 60_000 });
    return true;
  }
  if (bucket.count >= WRITE_LIMIT_PER_MIN) {
    return false;
  }
  bucket.count++;
  return true;
}

function json(data: unknown, status = 200, extraHeaders?: Record<string, string>): Response {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "Content-Type": "application/json",
      ...CORS_HEADERS,
      ...(extraHeaders || {}),
    },
  });
}

let serverRef: ReturnType<typeof Bun.serve> | null = null;

function broadcastSync() {
  if (!serverRef) return;
  const payload = JSON.stringify({
    type: "sync",
    todos: db.list(),
    online: onlineCount,
  });
  serverRef.publish(WS_TOPIC, payload);
}

serverRef = Bun.serve({
  port: PORT,
  async fetch(req, server) {
    if (req.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    const url = new URL(req.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";

    // WebSocket endpoint for live sync
    if (path === "/ws") {
      if (server.upgrade(req)) {
        return undefined;
      }
      return json({ error: "Expected WebSocket upgrade" }, 426);
    }

    // Health check
    if (path === "/api/health" && req.method === "GET") {
      return json({ ok: true, transport: "http+ws+mcp", online: onlineCount });
    }

    // Streamable HTTP MCP endpoint
    if (path === "/mcp") {
      if (req.method === "GET") {
        return json({
          name: "todo-kanban-mcp",
          transport: "streamable-http",
          endpoint: "/mcp",
          hint: "Send JSON-RPC 2.0 POST requests to /mcp",
        });
      }
      if (req.method === "DELETE") {
        return new Response(null, { status: 204, headers: CORS_HEADERS });
      }
      if (req.method === "POST") {
        let payload: unknown;
        try {
          payload = await req.json();
        } catch {
          return json(
            { jsonrpc: "2.0", id: null, error: { code: -32700, message: "Parse error" } },
            400
          );
        }

        const isWriteTool =
          typeof payload === "object" &&
          payload !== null &&
          (payload as { method?: string }).method === "tools/call" &&
          (payload as { params?: { name?: string } }).params?.name !== "list_todos";

        if (isWriteTool && !checkWriteRateLimit(req)) {
          return json(
            {
              jsonrpc: "2.0",
              id: (payload as { id?: string | number | null }).id ?? null,
              error: { code: -32000, message: "Rate limit exceeded" },
            },
            429
          );
        }

        const { status, body } = handleMcpPayload(payload, db, broadcastSync);
        if (body === null) {
          return new Response(null, { status, headers: CORS_HEADERS });
        }
        return json(body, status);
      }
      return json({ error: "Method not allowed" }, 405);
    }

    // REST API: /api/todos
    if (path === "/api/todos") {
      if (req.method === "GET") {
        return json({ todos: db.list(), online: onlineCount });
      }

      if (req.method === "POST") {
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        let body: { text?: unknown; status?: unknown; priority?: unknown };
        try {
          body = (await req.json()) as { text?: unknown; status?: unknown; priority?: unknown };
        } catch {
          return json({ error: "Invalid JSON body" }, 400);
        }
        if (typeof body?.text !== "string") {
          return json({ error: "Task text is required" }, 400);
        }
        try {
          const todo = db.add(body.text, {
            status: isValidStatus(body.status) ? body.status : undefined,
            priority: isValidPriority(body.priority) ? body.priority : undefined,
          });
          broadcastSync();
          return json({ todo }, 201);
        } catch (err) {
          return json({ error: err instanceof Error ? err.message : "Bad request" }, 400);
        }
      }

      if (req.method === "DELETE") {
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        if (url.searchParams.get("completed") === "true") {
          const deleted = db.clearCompleted();
          if (deleted > 0) broadcastSync();
          return json({ deleted });
        }
        return json({ error: "Specify ?completed=true to clear completed tasks" }, 400);
      }
    }

    // REST API: /api/todos/:id
    const itemMatch = path.match(/^\/api\/todos\/([^/]+)$/);
    if (itemMatch) {
      const id = decodeURIComponent(itemMatch[1]);

      if (req.method === "PATCH") {
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        let body: {
          status?: unknown;
          priority?: unknown;
          done?: unknown;
          text?: unknown;
          position?: unknown;
        };
        try {
          body = (await req.json()) as typeof body;
        } catch {
          return json({ error: "Invalid JSON body" }, 400);
        }
        try {
          const updated = db.update(id, {
            status: isValidStatus(body.status) ? body.status : undefined,
            priority: isValidPriority(body.priority) ? body.priority : undefined,
            done: typeof body.done === "boolean" ? body.done : undefined,
            text: typeof body.text === "string" ? body.text : undefined,
            position: typeof body.position === "number" ? body.position : undefined,
          });
          if (!updated) return json({ error: "Todo not found" }, 404);
          broadcastSync();
          return json({ todo: updated });
        } catch (err) {
          return json({ error: err instanceof Error ? err.message : "Bad request" }, 400);
        }
      }

      if (req.method === "DELETE") {
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        const ok = db.delete(id);
        if (!ok) return json({ error: "Todo not found" }, 404);
        broadcastSync();
        return json({ deleted: id });
      }
    }

    return json({ error: "Not found" }, 404);
  },
  websocket: {
    open(ws) {
      onlineCount++;
      ws.subscribe(WS_TOPIC);
      broadcastSync();
    },
    message(ws, message) {
      if (message === "ping") {
        ws.send("pong");
      }
    },
    close(ws) {
      onlineCount = Math.max(0, onlineCount - 1);
      ws.unsubscribe(WS_TOPIC);
      broadcastSync();
    },
  },
});

console.log(`Shared Kanban API + WS + MCP listening on http://0.0.0.0:${serverRef.port} (db: ${DB_PATH})`);
