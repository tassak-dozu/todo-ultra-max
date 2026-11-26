const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PATCH, DELETE, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

const MAX_TEXT_LENGTH = 280;

function json(data, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "Content-Type": "application/json",
      ...CORS_HEADERS,
    },
  });
}

function rowToTodo(row) {
  return {
    id: row.id,
    text: row.text,
    done: Boolean(row.done),
    createdAt: row.created_at,
  };
}

export default {
  async fetch(request, env) {
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: CORS_HEADERS });
    }

    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";

    try {
      if (path === "/api/health" && request.method === "GET") {
        return json({ ok: true });
      }

      if (path === "/api/todos" && request.method === "GET") {
        const { results } = await env.DB.prepare(
          "SELECT id, text, done, created_at FROM todos ORDER BY created_at DESC"
        ).all();
        return json({ todos: results.map(rowToTodo) });
      }

      if (path === "/api/todos" && request.method === "POST") {
        let body;
        try {
          body = await request.json();
        } catch {
          return json({ error: "Invalid JSON body" }, 400);
        }

        const text = typeof body?.text === "string" ? body.text.trim() : "";
        if (!text) {
          return json({ error: "Task text is required" }, 400);
        }
        if (text.length > MAX_TEXT_LENGTH) {
          return json({ error: `Task text must be <= ${MAX_TEXT_LENGTH} characters` }, 400);
        }

        const id = crypto.randomUUID();
        const createdAt = Date.now();

        await env.DB.prepare(
          "INSERT INTO todos (id, text, done, created_at) VALUES (?, ?, 0, ?)"
        )
          .bind(id, text, createdAt)
          .run();

        return json({ todo: { id, text, done: false, createdAt } }, 201);
      }

      if (path === "/api/todos" && request.method === "DELETE") {
        if (url.searchParams.get("completed") === "true") {
          const result = await env.DB.prepare("DELETE FROM todos WHERE done = 1").run();
          return json({ deleted: result.meta?.changes ?? 0 });
        }
        return json({ error: "Specify ?completed=true to clear completed tasks" }, 400);
      }

      const itemMatch = path.match(/^\/api\/todos\/([^/]+)$/);
      if (itemMatch) {
        const id = decodeURIComponent(itemMatch[1]);

        if (request.method === "PATCH") {
          let body;
          try {
            body = await request.json();
          } catch {
            return json({ error: "Invalid JSON body" }, 400);
          }

          const existing = await env.DB.prepare(
            "SELECT id, text, done, created_at FROM todos WHERE id = ?"
          )
            .bind(id)
            .first();

          if (!existing) {
            return json({ error: "Todo not found" }, 404);
          }

          const nextDone =
            typeof body?.done === "boolean" ? (body.done ? 1 : 0) : existing.done;
          let nextText = existing.text;
          if (typeof body?.text === "string") {
            const trimmed = body.text.trim();
            if (!trimmed || trimmed.length > MAX_TEXT_LENGTH) {
              return json({ error: "Invalid task text" }, 400);
            }
            nextText = trimmed;
          }

          await env.DB.prepare("UPDATE todos SET text = ?, done = ? WHERE id = ?")
            .bind(nextText, nextDone, id)
            .run();

          return json({
            todo: {
              id: existing.id,
              text: nextText,
              done: Boolean(nextDone),
              createdAt: existing.created_at,
            },
          });
        }

        if (request.method === "DELETE") {
          const result = await env.DB.prepare("DELETE FROM todos WHERE id = ?")
            .bind(id)
            .run();
          if ((result.meta?.changes ?? 0) === 0) {
            return json({ error: "Todo not found" }, 404);
          }
          return json({ deleted: id });
        }
      }

      return json({ error: "Not found" }, 404);
    } catch (err) {
      return json({ error: "Internal server error" }, 500);
    }
  },
};
