import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export const MAX_TEXT_LENGTH = 280;
export const MAX_TODOS = Number(process.env.MAX_TODOS || 500);

export interface Todo {
  id: string;
  text: string;
  done: boolean;
  createdAt: number;
}

interface TodoRow {
  id: string;
  text: string;
  done: number;
  created_at: number;
}

function rowToTodo(row: TodoRow): Todo {
  return {
    id: row.id,
    text: row.text,
    done: Boolean(row.done),
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
        done INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
      );
      CREATE INDEX IF NOT EXISTS idx_todos_created_at ON todos(created_at DESC);
    `);
  }

  list(filter: "all" | "active" | "completed" = "all"): Todo[] {
    let sql = "SELECT id, text, done, created_at FROM todos";
    if (filter === "active") sql += " WHERE done = 0";
    else if (filter === "completed") sql += " WHERE done = 1";
    sql += " ORDER BY created_at DESC LIMIT ?";
    const rows = this.db.query<TodoRow, [number]>(sql).all(MAX_TODOS);
    return rows.map(rowToTodo);
  }

  get(id: string): Todo | null {
    const row = this.db
      .query<TodoRow, [string]>("SELECT id, text, done, created_at FROM todos WHERE id = ?")
      .get(id);
    return row ? rowToTodo(row) : null;
  }

  add(rawText: string): Todo {
    const text = rawText.trim();
    if (!text) throw new Error("Task text is required");
    if (text.length > MAX_TEXT_LENGTH) {
      throw new Error(`Task text must be <= ${MAX_TEXT_LENGTH} characters`);
    }

    const countRow = this.db.query<{ c: number }, []>("SELECT COUNT(*) as c FROM todos").get();
    if ((countRow?.c ?? 0) >= MAX_TODOS) {
      throw new Error(`Todo list is full (max ${MAX_TODOS} items)`);
    }

    const id = crypto.randomUUID();
    const createdAt = Date.now();
    this.db
      .query("INSERT INTO todos (id, text, done, created_at) VALUES (?, ?, 0, ?)")
      .run(id, text, createdAt);

    return { id, text, done: false, createdAt };
  }

  update(id: string, patch: { done?: boolean; text?: string }): Todo | null {
    const existing = this.get(id);
    if (!existing) return null;

    const nextDone = typeof patch.done === "boolean" ? patch.done : existing.done;
    let nextText = existing.text;
    if (typeof patch.text === "string") {
      const trimmed = patch.text.trim();
      if (!trimmed || trimmed.length > MAX_TEXT_LENGTH) {
        throw new Error(`Task text must be 1..${MAX_TEXT_LENGTH} characters`);
      }
      nextText = trimmed;
    }

    this.db
      .query("UPDATE todos SET text = ?, done = ? WHERE id = ?")
      .run(nextText, nextDone ? 1 : 0, id);

    return {
      id: existing.id,
      text: nextText,
      done: nextDone,
      createdAt: existing.createdAt,
    };
  }

  delete(id: string): boolean {
    const result = this.db.query("DELETE FROM todos WHERE id = ?").run(id);
    return result.changes > 0;
  }

  clearCompleted(): number {
    const result = this.db.query("DELETE FROM todos WHERE done = 1").run();
    return result.changes;
  }

  close(): void {
    this.db.close();
  }
}
