import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export const MAX_TEXT_LENGTH = 280;
export const MAX_TODOS = Number(process.env.MAX_TODOS || 500);

export type TodoStatus = "todo" | "in_progress" | "done";
export type TodoPriority = "low" | "medium" | "high";

export const VALID_STATUSES: readonly TodoStatus[] = ["todo", "in_progress", "done"];
export const VALID_PRIORITIES: readonly TodoPriority[] = ["low", "medium", "high"];

export interface Todo {
  id: string;
  text: string;
  status: TodoStatus;
  priority: TodoPriority;
  done: boolean;
  position: number;
  createdAt: number;
}

interface TodoRow {
  id: string;
  text: string;
  status: string;
  priority: string;
  done: number;
  position: number;
  created_at: number;
}

export function isValidStatus(v: unknown): v is TodoStatus {
  return typeof v === "string" && (VALID_STATUSES as readonly string[]).includes(v);
}

export function isValidPriority(v: unknown): v is TodoPriority {
  return typeof v === "string" && (VALID_PRIORITIES as readonly string[]).includes(v);
}

function rowToTodo(row: TodoRow): Todo {
  const status: TodoStatus = isValidStatus(row.status)
    ? row.status
    : row.done
      ? "done"
      : "todo";
  const priority: TodoPriority = isValidPriority(row.priority) ? row.priority : "medium";
  return {
    id: row.id,
    text: row.text,
    status,
    priority,
    done: status === "done",
    position: row.position ?? 0,
    createdAt: row.created_at,
  };
}

export class TodoDB {
  private db: Database;

  constructor(dbPath: string) {
    if (dbPath !== ":memory:") {
      mkdirSync(dirname(dbPath), { recursive: true });
    }
    this.db = new Database(dbPath, { create: true });
    this.db.exec("PRAGMA journal_mode = WAL;");
    this.db.exec("PRAGMA busy_timeout = 5000;");
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS todos (
        id TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'todo',
        priority TEXT NOT NULL DEFAULT 'medium',
        done INTEGER NOT NULL DEFAULT 0,
        position INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_todos_created_at ON todos(created_at DESC);
    `);

    // Migrate older databases cleanly if needed
    const cols = this.db
      .query<{ name: string }, []>("PRAGMA table_info(todos)")
      .all()
      .map((c) => c.name);
    if (!cols.includes("status")) {
      this.db.exec("ALTER TABLE todos ADD COLUMN status TEXT NOT NULL DEFAULT 'todo';");
      this.db.exec("UPDATE todos SET status = CASE WHEN done = 1 THEN 'done' ELSE 'todo' END;");
    }
    if (!cols.includes("priority")) {
      this.db.exec("ALTER TABLE todos ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium';");
    }
    if (!cols.includes("position")) {
      this.db.exec("ALTER TABLE todos ADD COLUMN position INTEGER NOT NULL DEFAULT 0;");
    }
  }

  list(opts?: { status?: TodoStatus | "all" | "active" | "completed"; priority?: TodoPriority }): Todo[] {
    const clauses: string[] = [];
    const params: (string | number)[] = [];

    if (opts?.status && opts.status !== "all") {
      if (opts.status === "active") {
        clauses.push("status != 'done'");
      } else if (opts.status === "completed") {
        clauses.push("status = 'done'");
      } else if (isValidStatus(opts.status)) {
        clauses.push("status = ?");
        params.push(opts.status);
      }
    }

    if (opts?.priority && isValidPriority(opts.priority)) {
      clauses.push("priority = ?");
      params.push(opts.priority);
    }

    let sql = "SELECT id, text, status, priority, done, position, created_at FROM todos";
    if (clauses.length > 0) sql += ` WHERE ${clauses.join(" AND ")}`;
    sql += " ORDER BY position ASC, created_at DESC LIMIT ?";
    params.push(MAX_TODOS);

    const rows = this.db.query<TodoRow, (string | number)[]>(sql).all(...params);
    return rows.map(rowToTodo);
  }

  get(id: string): Todo | null {
    const row = this.db
      .query<TodoRow, [string]>(
        "SELECT id, text, status, priority, done, position, created_at FROM todos WHERE id = ?"
      )
      .get(id);
    return row ? rowToTodo(row) : null;
  }

  add(
    rawText: string,
    opts?: { status?: TodoStatus; priority?: TodoPriority }
  ): Todo {
    const text = rawText.trim();
    if (!text) throw new Error("Task text is required");
    if (text.length > MAX_TEXT_LENGTH) {
      throw new Error(`Task text must be <= ${MAX_TEXT_LENGTH} characters`);
    }

    const countRow = this.db.query<{ c: number }, []>("SELECT COUNT(*) as c FROM todos").get();
    if ((countRow?.c ?? 0) >= MAX_TODOS) {
      throw new Error(`Todo board is full (max ${MAX_TODOS} items)`);
    }

    const status: TodoStatus = isValidStatus(opts?.status) ? opts.status : "todo";
    const priority: TodoPriority = isValidPriority(opts?.priority) ? opts.priority : "medium";
    const done = status === "done" ? 1 : 0;
    const minPosRow = this.db
      .query<{ m: number | null }, [string]>("SELECT MIN(position) as m FROM todos WHERE status = ?")
      .get(status);
    const position = (minPosRow?.m ?? 1) - 1;
    const id = crypto.randomUUID();
    const createdAt = Date.now();

    this.db
      .query(
        "INSERT INTO todos (id, text, status, priority, done, position, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)"
      )
      .run(id, text, status, priority, done, position, createdAt);

    return { id, text, status, priority, done: Boolean(done), position, createdAt };
  }

  update(
    id: string,
    patch: {
      status?: TodoStatus;
      priority?: TodoPriority;
      done?: boolean;
      text?: string;
      position?: number;
    }
  ): Todo | null {
    const existing = this.get(id);
    if (!existing) return null;

    let nextStatus: TodoStatus = existing.status;
    if (isValidStatus(patch.status)) {
      nextStatus = patch.status;
    } else if (typeof patch.done === "boolean") {
      nextStatus = patch.done ? "done" : existing.status === "done" ? "todo" : existing.status;
    }

    const nextPriority: TodoPriority = isValidPriority(patch.priority)
      ? patch.priority
      : existing.priority;

    let nextText = existing.text;
    if (typeof patch.text === "string") {
      const trimmed = patch.text.trim();
      if (!trimmed || trimmed.length > MAX_TEXT_LENGTH) {
        throw new Error(`Task text must be 1..${MAX_TEXT_LENGTH} characters`);
      }
      nextText = trimmed;
    }

    let nextPosition = existing.position;
    if (typeof patch.position === "number" && Number.isFinite(patch.position)) {
      nextPosition = Math.trunc(patch.position);
    } else if (nextStatus !== existing.status) {
      const minPosRow = this.db
        .query<{ m: number | null }, [string]>(
          "SELECT MIN(position) as m FROM todos WHERE status = ?"
        )
        .get(nextStatus);
      nextPosition = (minPosRow?.m ?? 1) - 1;
    }

    const nextDone = nextStatus === "done" ? 1 : 0;

    this.db
      .query(
        "UPDATE todos SET text = ?, status = ?, priority = ?, done = ?, position = ? WHERE id = ?"
      )
      .run(nextText, nextStatus, nextPriority, nextDone, nextPosition, id);

    return {
      id: existing.id,
      text: nextText,
      status: nextStatus,
      priority: nextPriority,
      done: Boolean(nextDone),
      position: nextPosition,
      createdAt: existing.createdAt,
    };
  }

  delete(id: string): boolean {
    const result = this.db.query("DELETE FROM todos WHERE id = ?").run(id);
    return result.changes > 0;
  }

  clearCompleted(): number {
    const result = this.db.query("DELETE FROM todos WHERE status = 'done' OR done = 1").run();
    return result.changes;
  }

  close(): void {
    this.db.close();
  }
}
