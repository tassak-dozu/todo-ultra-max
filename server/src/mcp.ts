import { MAX_TEXT_LENGTH, TodoDB } from "./db";

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
    description: "List tasks in the shared todo list.",
    inputSchema: {
      type: "object",
      properties: {
        filter: {
          type: "string",
          enum: ["all", "active", "completed"],
          description: "Filter tasks by completion state (default: all).",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "add_todo",
    description: "Add a new task to the shared todo list.",
    inputSchema: {
      type: "object",
      properties: {
        text: {
          type: "string",
          minLength: 1,
          maxLength: MAX_TEXT_LENGTH,
          description: "Task description.",
        },
      },
      required: ["text"],
      additionalProperties: false,
    },
  },
  {
    name: "toggle_todo",
    description: "Mark a task as completed or active, or edit its text.",
    inputSchema: {
      type: "object",
      properties: {
        id: { type: "string", description: "Todo ID." },
        done: { type: "boolean", description: "New completion state." },
        text: {
          type: "string",
          minLength: 1,
          maxLength: MAX_TEXT_LENGTH,
          description: "Optional updated task text.",
        },
      },
      required: ["id"],
      additionalProperties: false,
    },
  },
  {
    name: "delete_todo",
    description: "Delete a single task from the shared todo list by ID.",
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
    description: "Delete all completed tasks from the shared todo list.",
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
  onMutate: () => void
): { status: number; body: unknown | null } {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) {
    return { status: 400, body: rpcError(null, -32600, "Invalid JSON-RPC request") };
  }

  const req = payload as JsonRpcRequest;
  const id = req.id ?? null;

  if (req.jsonrpc !== "2.0" || typeof req.method !== "string") {
    return { status: 400, body: rpcError(id, -32600, "Invalid JSON-RPC 2.0 payload") };
  }

  // Notifications have no id and require 202 Accepted with empty body
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
            name: "todo-shared-mcp",
            version: "1.0.0",
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
            const f = args.filter;
            const filter = f === "active" || f === "completed" ? f : "all";
            const todos = db.list(filter);
            return { status: 200, body: rpcResult(id, toolSuccess({ count: todos.length, todos })) };
          }

          case "add_todo": {
            if (typeof args.text !== "string") {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: text")) };
            }
            const todo = db.add(args.text);
            onMutate();
            return { status: 200, body: rpcResult(id, toolSuccess({ todo })) };
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
            const nextText = typeof args.text === "string" ? args.text : undefined;
            const updated = db.update(args.id, { done: nextDone, text: nextText });
            onMutate();
            return { status: 200, body: rpcResult(id, toolSuccess({ todo: updated })) };
          }

          case "delete_todo": {
            if (typeof args.id !== "string" || !args.id) {
              return { status: 200, body: rpcResult(id, toolFailure("Missing string argument: id")) };
            }
            const deleted = db.delete(args.id);
            if (!deleted) {
              return { status: 200, body: rpcResult(id, toolFailure(`Todo not found: ${args.id}`)) };
            }
            onMutate();
            return { status: 200, body: rpcResult(id, toolSuccess({ deleted: args.id })) };
          }

          case "clear_completed": {
            const count = db.clearCompleted();
            if (count > 0) onMutate();
            return { status: 200, body: rpcResult(id, toolSuccess({ deleted: count })) };
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
