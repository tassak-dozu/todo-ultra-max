import { createHash, timingSafeEqual } from "node:crypto";
import { TodoDB, isValidPriority, isValidStatus } from "./db";
import { handleMcpPayload } from "./mcp";

const PORT = Number(process.env.PORT || 3000);
const DB_PATH = process.env.DB_PATH || "/data/todos.db";
const WRITE_LIMIT_PER_MIN = Number(process.env.WRITE_LIMIT_PER_MIN || 90);
const API_KEY = (process.env.API_KEY || "").trim();
const API_KEY_MODE: "none" | "writes" | "all" = !API_KEY
  ? "none"
  : process.env.API_KEY_MODE === "all"
    ? "all"
    : "writes";

const CORS_HEADERS: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PATCH, DELETE, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Accept, Authorization, X-API-Key, Mcp-Session-Id",
  "Access-Control-Expose-Headers": "Mcp-Session-Id",
};

const WS_TOPIC = "todos";
const PEER_COLORS = ["#a78bfa", "#22d3ee", "#34d399", "#fbbf24", "#fb7185", "#60a5fa", "#f472b6"];

interface ActivityEvent {
  id: string;
  action: "added" | "moved" | "edited" | "deleted" | "cleared";
  source: "web" | "mcp";
  summary: string;
  cardId?: string;
  at: number;
}

interface PeerState {
  id: string;
  color: string;
  typing: boolean;
  draggingId: string | null;
}

const db = new TodoDB(DB_PATH);
const peers = new Map<string, PeerState>();
const activityLog: ActivityEvent[] = [];
let mcpCalls = 0;

function sha256(val: string): Buffer {
  return createHash("sha256").update(val).digest();
}

const API_KEY_HASH = API_KEY ? sha256(API_KEY) : null;

function extractApiKey(req: Request, url: URL): string {
  const auth = req.headers.get("authorization") || "";
  if (auth.toLowerCase().startsWith("bearer ")) {
    return auth.slice(7).trim();
  }
  const headerKey = req.headers.get("x-api-key");
  if (headerKey) return headerKey.trim();
  const queryKey = url.searchParams.get("api_key");
  if (queryKey) return queryKey.trim();
  return "";
}

function isKeyValid(candidate: string): boolean {
  if (!API_KEY_HASH) return true;
  if (!candidate) return false;
  return timingSafeEqual(sha256(candidate), API_KEY_HASH);
}

function authorize(req: Request, url: URL, isWrite: boolean): boolean {
  if (API_KEY_MODE === "none") return true;
  if (API_KEY_MODE === "writes" && !isWrite) return true;
  return isKeyValid(extractApiKey(req, url));
}

function recordActivity(event: Omit<ActivityEvent, "id" | "at">): ActivityEvent {
  const full: ActivityEvent = {
    id: crypto.randomUUID(),
    at: Date.now(),
    ...event,
  };
  activityLog.unshift(full);
  if (activityLog.length > 20) activityLog.length = 20;
  return full;
}

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

let serverRef: ReturnType<typeof Bun.serve<PeerState>> | null = null;

function getPresencePayload() {
  const list = Array.from(peers.values());
  return {
    online: list.length,
    peers: list.slice(0, 12).map((p) => ({
      id: p.id,
      color: p.color,
      typing: p.typing,
      draggingId: p.draggingId,
    })),
    typingCount: list.filter((p) => p.typing).length,
    draggingIds: list.map((p) => p.draggingId).filter(Boolean) as string[],
  };
}

function broadcastSync(lastEvent?: ActivityEvent) {
  if (!serverRef) return;
  const payload = JSON.stringify({
    type: "sync",
    todos: db.list(),
    ...getPresencePayload(),
    authMode: API_KEY_MODE,
    mcpCalls,
    activity: activityLog.slice(0, 12),
    lastEvent: lastEvent ?? null,
  });
  serverRef.publish(WS_TOPIC, payload);
}

function broadcastPresence() {
  if (!serverRef) return;
  const payload = JSON.stringify({
    type: "presence",
    ...getPresencePayload(),
  });
  serverRef.publish(WS_TOPIC, payload);
}

serverRef = Bun.serve<PeerState>({
  port: PORT,
  async fetch(req, server) {
    if (req.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    const url = new URL(req.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";

    // Health check (always public)
    if (path === "/api/health" && req.method === "GET") {
      return json({
        ok: true,
        transport: "http+ws+mcp",
        authMode: API_KEY_MODE,
        online: peers.size,
        mcpCalls,
      });
    }

    // Verify API key status
    if (path === "/api/auth/verify" && req.method === "GET") {
      const provided = extractApiKey(req, url);
      const valid = isKeyValid(provided);
      return json(
        {
          authMode: API_KEY_MODE,
          hasKeyConfigured: API_KEY_MODE !== "none",
          keyProvided: Boolean(provided),
          valid,
        },
        valid ? 200 : 401
      );
    }

    // WebSocket endpoint for live sync
    if (path === "/ws") {
      if (!authorize(req, url, false)) {
        return json({ error: "API key required" }, 401);
      }
      const peer: PeerState = {
        id: crypto.randomUUID().slice(0, 8),
        color: PEER_COLORS[peers.size % PEER_COLORS.length],
        typing: false,
        draggingId: null,
      };
      if (server.upgrade(req, { data: peer })) {
        return undefined;
      }
      return json({ error: "Expected WebSocket upgrade" }, 426);
    }

    // Streamable HTTP MCP endpoint
    if (path === "/mcp") {
      if (req.method === "GET") {
        return json({
          name: "todo-kanban-mcp",
          transport: "streamable-http",
          endpoint: "/mcp",
          authMode: API_KEY_MODE,
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

        const method =
          typeof payload === "object" && payload !== null
            ? (payload as { method?: string }).method
            : "";
        const toolName =
          typeof payload === "object" && payload !== null
            ? (payload as { params?: { name?: string } }).params?.name
            : "";
        const toolArgs =
          typeof payload === "object" && payload !== null
            ? ((payload as { params?: { arguments?: Record<string, unknown> } }).params?.arguments ?? {})
            : {};

        const isWriteTool = method === "tools/call" && toolName !== "list_todos";

        if (!authorize(req, url, isWriteTool)) {
          return json(
            {
              jsonrpc: "2.0",
              id: (payload as { id?: string | number | null }).id ?? null,
              error: { code: -32001, message: "Unauthorized: valid API key required" },
            },
            401
          );
        }

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

        if (method === "tools/call") mcpCalls++;

        const { status, body } = handleMcpPayload(payload, db, () => {
          let action: ActivityEvent["action"] = "edited";
          let summary = `MCP ran ${toolName}`;
          let cardId = typeof toolArgs.id === "string" ? toolArgs.id : undefined;

          if (toolName === "add_todo") {
            action = "added";
            summary = `Robot spawned "${String(toolArgs.text || "").slice(0, 48)}"`;
          } else if (toolName === "move_todo" || toolName === "toggle_todo") {
            action = "moved";
            const dest = typeof toolArgs.status === "string" ? toolArgs.status.replace("_", " ") : "updated";
            summary = `Robot shoved a card → ${dest}`;
          } else if (toolName === "delete_todo") {
            action = "deleted";
            summary = "Robot yeeted a card into the void";
          } else if (toolName === "clear_completed") {
            action = "cleared";
            summary = "Robot nuked the Done column";
          }
          const ev = recordActivity({ action, source: "mcp", summary, cardId });
          broadcastSync(ev);
        });

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
        if (!authorize(req, url, false)) {
          return json({ error: "API key required" }, 401);
        }
        return json({
          todos: db.list(),
          ...getPresencePayload(),
          authMode: API_KEY_MODE,
          mcpCalls,
          activity: activityLog.slice(0, 12),
        });
      }

      if (req.method === "POST") {
        if (!authorize(req, url, true)) {
          return json({ error: "API key required to add tasks" }, 401);
        }
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
          const ev = recordActivity({
            action: "added",
            source: "web",
            summary: `Spawned "${todo.text.slice(0, 48)}"`,
            cardId: todo.id,
          });
          broadcastSync(ev);
          return json({ todo }, 201);
        } catch (err) {
          return json({ error: err instanceof Error ? err.message : "Bad request" }, 400);
        }
      }

      if (req.method === "DELETE") {
        if (!authorize(req, url, true)) {
          return json({ error: "API key required to clear tasks" }, 401);
        }
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        if (url.searchParams.get("completed") === "true") {
          const deleted = db.clearCompleted();
          if (deleted > 0) {
            const ev = recordActivity({
              action: "cleared",
              source: "web",
              summary: `Nuked ${deleted} finished card${deleted === 1 ? "" : "s"}`,
            });
            broadcastSync(ev);
          }
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
        if (!authorize(req, url, true)) {
          return json({ error: "API key required to edit tasks" }, 401);
        }
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
          const before = db.get(id);
          const updated = db.update(id, {
            status: isValidStatus(body.status) ? body.status : undefined,
            priority: isValidPriority(body.priority) ? body.priority : undefined,
            done: typeof body.done === "boolean" ? body.done : undefined,
            text: typeof body.text === "string" ? body.text : undefined,
            position: typeof body.position === "number" ? body.position : undefined,
          });
          if (!updated) return json({ error: "Todo not found" }, 404);

          const moved = before && before.status !== updated.status;
          const ev = recordActivity({
            action: moved ? "moved" : "edited",
            source: "web",
            summary: moved
              ? `Slid "${updated.text.slice(0, 42)}" → ${updated.status.replace("_", " ")}`
              : `Rewrote "${updated.text.slice(0, 44)}"`,
            cardId: updated.id,
          });
          broadcastSync(ev);
          return json({ todo: updated });
        } catch (err) {
          return json({ error: err instanceof Error ? err.message : "Bad request" }, 400);
        }
      }

      if (req.method === "DELETE") {
        if (!authorize(req, url, true)) {
          return json({ error: "API key required to delete tasks" }, 401);
        }
        if (!checkWriteRateLimit(req)) {
          return json({ error: "Too many requests, slow down" }, 429);
        }
        const before = db.get(id);
        const ok = db.delete(id);
        if (!ok) return json({ error: "Todo not found" }, 404);
        const ev = recordActivity({
          action: "deleted",
          source: "web",
          summary: before ? `Yeeted "${before.text.slice(0, 44)}"` : "Yeeted a card into the void",
          cardId: id,
        });
        broadcastSync(ev);
        return json({ deleted: id });
      }
    }

    return json({ error: "Not found" }, 404);
  },
  websocket: {
    open(ws) {
      peers.set(ws.data.id, ws.data);
      ws.subscribe(WS_TOPIC);
      ws.send(
        JSON.stringify({
          type: "welcome",
          peerId: ws.data.id,
          color: ws.data.color,
        })
      );
      broadcastSync();
    },
    message(ws, raw) {
      const text = typeof raw === "string" ? raw : new TextDecoder().decode(raw);
      if (text === "ping") {
        ws.send("pong");
        return;
      }
      try {
        const msg = JSON.parse(text) as { type?: string; t?: number; active?: boolean; cardId?: string | null };
        if (msg.type === "ping") {
          ws.send(JSON.stringify({ type: "pong", t: msg.t ?? Date.now() }));
          return;
        }
        if (msg.type === "typing") {
          const next = Boolean(msg.active);
          if (ws.data.typing !== next) {
            ws.data.typing = next;
            broadcastPresence();
          }
          return;
        }
        if (msg.type === "dragging") {
          const next = typeof msg.cardId === "string" ? msg.cardId : null;
          if (ws.data.draggingId !== next) {
            ws.data.draggingId = next;
            broadcastPresence();
          }
        }
      } catch {}
    },
    close(ws) {
      peers.delete(ws.data.id);
      ws.unsubscribe(WS_TOPIC);
      broadcastPresence();
    },
  },
});

console.log(
  `Shared Kanban API + WS + MCP listening on http://0.0.0.0:${serverRef.port} (db: ${DB_PATH}, auth: ${API_KEY_MODE})`
);
