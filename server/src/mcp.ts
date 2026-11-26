import {
  BUCKET_LABELS,
  MAX_TEXT_LENGTH,
  TodoDB,
  isValidBucket,
  isValidPriority,
  isValidStatus,
  type ResolutionSummary,
  type TodoPriority,
  type TodoStatus,
} from "./db";

interface JsonRpcRequest {
  jsonrpc?: string;
  id?: string | number | null;
  method?: string;
  params?: Record<string, unknown>;
}

const PROTOCOL_VERSION = "2025-03-26";

const TOOLS = [
  {
    name: "list_todos",
    description: "List tasks on the shared Kanban & todo board along with active prediction pools.",
    inputSchema: {
      type: "object",
      properties: {
        status: {
          type: "string",
          enum: ["all", "todo", "in_progress", "done", "active", "completed"],
          description: "Filter by Kanban column or completion state (default: all).",
        },
        priority: {
          type: "string",
          enum: ["low", "medium", "high"],
          description: "Optional priority filter.",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "add_todo",
    description: "Add a new task card to the shared Kanban board.",
    inputSchema: {
      type: "object",
      properties: {
        text: {
          type: "string",
          minLength: 1,
          maxLength: MAX_TEXT_LENGTH,
          description: "Task title/description.",
        },
        status: {
          type: "string",
          enum: ["todo", "in_progress", "done"],
          description: "Target Kanban column (default: todo).",
        },
        priority: {
          type: "string",
          enum: ["low", "medium", "high"],
          description: "Task priority (default: medium).",
        },
      },
      required: ["text"],
      additionalProperties: false,
    },
  },
  {
    name: "move_todo",
    description:
      "Move a task card to another Kanban column (moving to done settles prediction bets!), update priority, or edit text.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Todo ID." },
        status: {
          type: "string",
          enum: ["todo", "in_progress", "done"],
          description: "Destination Kanban column.",
        },
        priority: {
          type: "string",
          enum: ["low", "medium", "high"],
          description: "Updated priority.",
        },
        text: {
          type: "string",
          minLength: 1,
          maxLength: MAX_TEXT_LENGTH,
          description: "Updated task text.",
        },
        done: { type: "boolean", description: "Optional boolean completion flag." },
      },
      required: ["id"],
      additionalProperties: false,
    },
  },
  {
    name: "toggle_todo",
    description: "Toggle a task between done and todo (or set done explicitly).",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Todo ID." },
        done: { type: "boolean", description: "Optional explicit completion state." },
      },
      required: ["id"],
      additionalProperties: false,
    },
  },
  {
    name: "delete_todo",
    description: "Delete a single task card from the shared board by ID.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Todo ID to delete." },
      },
      required: ["id"],
      additionalProperties: false,
    },
  },
  {
    name: "clear_completed",
    description: "Delete all tasks in the Done column.",
    inputSchema: {
      type: "object",
      properties: {},
      additionalProperties: false,
    },
  },
  {
    name: "place_bet",
    description:
      "Place an Aura Coin prediction bet on how fast a card will be completed (speedrun=<5m, today=5m-1h, eventually=1h-24h, never=24h+/deleted).",
    inputSchema: {
      type: "object",
      properties: {
        cardId: { type: "string", description: "ID of the incomplete card to bet on." },
        bucket: {
          type: "string",
          enum: ["speedrun", "today", "eventually", "never"],
          description: "Predicted completion timeframe.",
        },
        amount: { type: "integer", minimum: 10, description: "Amount of Aura Coins to wager." },
        playerName: { type: "string", description: "Optional display name (default: MCP Bot)." },
      },
      required: ["cardId", "bucket", "amount"],
      additionalProperties: false,
    },
  },
  {
    name: "get_leaderboard",
    description: "Get the top prediction gamblers leaderboard and open card betting markets.",
    inputSchema: {
      type: "object",
      properties: {},
      additionalProperties: false,
    },
  },
];

function rpcResult(id: string | number | null, result: unknown) {
  return { jsonrpc: "2.0", id, result };
}

function rpcError(id: string | number | null, code: number, message: string) {
  return { jsonrpc: "2.0", id, error: { code, message } };
}

function toolSuccess(data: unknown) {
  return {
    content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
  };
}

function toolFailure(message: string) {
  return {
    isError: true,
    content: [{ type: "text", text: message }],
  };
}

export function handleMcpPayload(
  payload: unknown,
  db: TodoDB,
  onMutate: (info?: { resolution?: ResolutionSummary | null; customSummary?: string; cardId?: string }) => void
): { status: number; body: unknown | null } {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return { status: 400, body: rpcError(null, -32600, "Invalid JSON-RPC request") };
  }

  const req = payload as JsonRpcRequest;
  const id = req.id ?? null;

  if (req.jsonrpc !== "2.0" || typeof req.method !== "string") {
    return { status: 400, body: rpcError(id, -32600, "Invalid JSON-RPC 2.0 payload") };
  }

  if (req.method === "notifications/initialized" || req.method.startsWith("notifications/")) {
    return { status: 202, body: null };
  }

  switch (req.method) {
    case "initialize": {
      return {
        status: 200,
        body: rpcResult(id, {
          protocolVersion: PROTOCOL_VERSION,
          capabilities: { tools: {} },
          serverInfo: {
            name: "todo-kanban-mcp",
            version: "1.2.0",
          },
        }),
      };
    }

    case "ping": {
      return { status: 200, body: rpcResult(id, {}) };
    }

    case "tools/list": {
      return { status: 200, body: rpcResult(id, { tools: TOOLS }) };
    }

    case "tools/call": {
      const params = req.params ?? {};
      const name = typeof params.name === "string" ? params.name : "";
      const args =
        params.arguments && typeof params.arguments === "object"
          ? (params.arguments as Record<string, unknown>)
          : {};

      try {
        switch (name) {
          case "list_todos": {
            const rawStatus = args.status ?? args.filter;
            const status =
              rawStatus === "active" ||
              rawStatus === "completed" ||
              rawStatus === "all" ||
              isValidStatus(rawStatus)
                ? rawStatus
                : "all";
            const priority = isValidPriority(args.priority) ? args.priority : undefined;
            const todos = db.list({ status, priority });
            const markets = db.getMarketsMap();
            return {
              status: 200,
              body: rpcResult(id, toolSuccess({ count: todos.length, todos, markets })),
            };
          }

          case "add_todo": {
            if (typeof args.text !== "string") {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: text")) };
            }
            const status: TodoStatus | undefined = isValidStatus(args.status)
              ? args.status
              : undefined;
            const priority: TodoPriority | undefined = isValidPriority(args.priority)
              ? args.priority
              : undefined;
            const todo = db.add(args.text, { status, priority });
            onMutate({ cardId: todo.id });
            return { status: 200, body: rpcResult(id, toolSuccess({ todo })) };
          }

          case "move_todo": {
            if (typeof args.id !== "string" || !args.id) {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: id")) };
            }
            const res = db.update(args.id, {
              status: isValidStatus(args.status) ? args.status : undefined,
              priority: isValidPriority(args.priority) ? args.priority : undefined,
              text: typeof args.text === "string" ? args.text : undefined,
              done: typeof args.done === "boolean" ? args.done : undefined,
            });
            if (!res) {
              return { status: 200, body: rpcResult(id, toolFailure(`Todo not found: ${args.id}`)) };
            }
            onMutate({ resolution: res.resolution, cardId: res.todo.id });
            return { status: 200, body: rpcResult(id, toolSuccess(res)) };
          }

          case "toggle_todo": {
            if (typeof args.id !== "string" || !args.id) {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: id")) };
            }
            const existing = db.get(args.id);
            if (!existing) {
              return { status: 200, body: rpcResult(id, toolFailure(`Todo not found: ${args.id}`)) };
            }
            const nextDone = typeof args.done === "boolean" ? args.done : !existing.done;
            const res = db.update(args.id, { done: nextDone });
            onMutate({ resolution: res?.resolution, cardId: args.id });
            return { status: 200, body: rpcResult(id, toolSuccess(res)) };
          }

          case "delete_todo": {
            if (typeof args.id !== "string" || !args.id) {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: id")) };
            }
            const res = db.delete(args.id);
            if (!res.deleted) {
              return { status: 200, body: rpcResult(id, toolFailure(`Todo not found: ${args.id}`)) };
            }
            onMutate({ resolution: res.resolution, cardId: args.id });
            return {
              status: 200,
              body: rpcResult(id, toolSuccess({ deleted: args.id, resolution: res.resolution })),
            };
          }

          case "clear_completed": {
            const count = db.clearCompleted();
            if (count > 0) onMutate();
            return { status: 200, body: rpcResult(id, toolSuccess({ deleted: count })) };
          }

          case "place_bet": {
            if (typeof args.cardId !== "string" || !isValidBucket(args.bucket) || typeof args.amount !== "number") {
              return {
                status: 200,
                body: rpcResult(id, toolFailure("Required: cardId (string), bucket, amount (number)")),
              };
            }
            const playerName =
              typeof args.playerName === "string" && args.playerName.trim()
                ? args.playerName.trim()
                : "MCP Degen 🤖";
            const playerId = `mcp-${playerName.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 24) || "bot"}`;
            const placed = db.placeBet({
              playerId,
              playerName,
              cardId: args.cardId,
              bucket: args.bucket,
              amount: args.amount,
            });
            onMutate({
              cardId: args.cardId,
              customSummary: `🎰 ${placed.wallet.name} bet ${placed.bet.amount} 🪙 on ${BUCKET_LABELS[placed.bet.bucket]}`,
            });
            return { status: 200, body: rpcResult(id, toolSuccess(placed)) };
          }

          case "get_leaderboard": {
            return {
              status: 200,
              body: rpcResult(
                id,
                toolSuccess({
                  leaderboard: db.getLeaderboard(10),
                  markets: db.getMarketsMap(),
                })
              ),
            };
          }

          default:
            return { status: 200, body: rpcError(id, -32601, `Unknown tool: ${name}`) };
        }
      } catch (err) {
        const msg = err instanceof Error ? err.message : "Tool execution failed";
        return { status: 200, body: rpcResult(id, toolFailure(msg)) };
      }
    }

    default:
      return { status: 200, body: rpcError(id, -32601, `Method not found: ${req.method}`) };
  }
}
