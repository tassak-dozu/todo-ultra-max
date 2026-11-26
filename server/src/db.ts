import { Database } from "bun:sqlite";
import { mkdirSync } from "node:fs";
import { dirname } from "node:path";

export const MAX_TEXT_LENGTH = 280;
export const MAX_TODOS = Number(process.env.MAX_TODOS || 500);
export const STARTING_BALANCE = 1000;
export const BAILOUT_THRESHOLD = 100;
export const BAILOUT_AMOUNT = 500;

export type TodoStatus = "todo" | "in_progress" | "done";
export type TodoPriority = "low" | "medium" | "high";
export type BetBucket = "speedrun" | "today" | "eventually" | "never";

export const VALID_STATUSES: readonly TodoStatus[] = ["todo", "in_progress", "done"];
export const VALID_PRIORITIES: readonly TodoPriority[] = ["low", "medium", "high"];
export const VALID_BUCKETS: readonly BetBucket[] = ["speedrun", "today", "eventually", "never"];

export const BUCKET_LABELS: Record<BetBucket, string> = {
  speedrun: "⚡ < 5 min",
  today: "🔥 5m – 1h",
  eventually: "🐢 1h – 24h",
  never: "💀 24h+ / Nuked",
};

export const BUCKET_MAX_MS: Record<BetBucket, number> = {
  speedrun: 5 * 60 * 1000,
  today: 60 * 60 * 1000,
  eventually: 24 * 60 * 60 * 1000,
  never: Number.POSITIVE_INFINITY,
};

export interface Todo {
  id: string;
  text: string;
  status: TodoStatus;
  priority: TodoPriority;
  done: boolean;
  position: number;
  createdAt: number;
  completedAt: number | null;
}

export interface Wallet {
  playerId: string;
  name: string;
  balance: number;
  wins: number;
  losses: number;
  totalWon: number;
}

export interface Bet {
  id: string;
  cardId: string;
  cardText: string;
  playerId: string;
  playerName: string;
  bucket: BetBucket;
  amount: number;
  status: "open" | "won" | "lost";
  payout: number;
  createdAt: number;
  resolvedAt: number | null;
}

export interface MarketSummary {
  cardId: string;
  totalPool: number;
  betCount: number;
  pools: Record<BetBucket, number>;
  recentBets: Array<{
    id: string;
    playerName: string;
    bucket: BetBucket;
    amount: number;
    createdAt: number;
  }>;
}

export interface ResolutionSummary {
  cardId: string;
  cardText: string;
  winningBucket: BetBucket;
  totalPool: number;
  totalPaid: number;
  winnerCount: number;
  loserCount: number;
}

interface TodoRow {
  id: string;
  text: string;
  status: string;
  priority: string;
  done: number;
  position: number;
  created_at: number;
  completed_at: number | null;
}

interface WalletRow {
  player_id: string;
  name: string;
  balance: number;
  wins: number;
  losses: number;
  total_won: number;
}

interface BetRow {
  id: string;
  card_id: string;
  card_text: string;
  player_id: string;
  player_name: string;
  bucket: string;
  amount: number;
  status: string;
  payout: number;
  created_at: number;
  resolved_at: number | null;
}

export function isValidStatus(v: unknown): v is TodoStatus {
  return typeof v === "string" && (VALID_STATUSES as readonly string[]).includes(v);
}

export function isValidPriority(v: unknown): v is TodoPriority {
  return typeof v === "string" && (VALID_PRIORITIES as readonly string[]).includes(v);
}

export function isValidBucket(v: unknown): v is BetBucket {
  return typeof v === "string" && (VALID_BUCKETS as readonly string[]).includes(v);
}

export function bucketForElapsedMs(elapsedMs: number): BetBucket {
  if (elapsedMs < BUCKET_MAX_MS.speedrun) return "speedrun";
  if (elapsedMs < BUCKET_MAX_MS.today) return "today";
  if (elapsedMs < BUCKET_MAX_MS.eventually) return "eventually";
  return "never";
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
    completedAt: row.completed_at ?? null,
  };
}

function rowToWallet(row: WalletRow): Wallet {
  return {
    playerId: row.player_id,
    name: row.name,
    balance: row.balance,
    wins: row.wins,
    losses: row.losses,
    totalWon: row.total_won,
  };
}

function rowToBet(row: BetRow): Bet {
  return {
    id: row.id,
    cardId: row.card_id,
    cardText: row.card_text,
    playerId: row.player_id,
    playerName: row.player_name,
    bucket: isValidBucket(row.bucket) ? row.bucket : "never",
    amount: row.amount,
    status: row.status === "won" || row.status === "lost" ? row.status : "open",
    payout: row.payout,
    createdAt: row.created_at,
    resolvedAt: row.resolved_at,
  };
}

function sanitizeName(raw: string | undefined, fallbackId: string): string {
  const cleaned = (raw || "").trim().replace(/\s+/g, " ").slice(0, 24);
  return cleaned || `Degen#${fallbackId.slice(0, 4)}`;
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
        created_at INTEGER NOT NULL,
        completed_at INTEGER
      );
      CREATE INDEX IF NOT EXISTS idx_todos_created_at ON todos(created_at DESC);

      CREATE TABLE IF NOT EXISTS wallets (
        player_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        balance INTEGER NOT NULL DEFAULT 1000,
        wins INTEGER NOT NULL DEFAULT 0,
        losses INTEGER NOT NULL DEFAULT 0,
        total_won INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS bets (
        id TEXT PRIMARY KEY,
        card_id TEXT NOT NULL,
        card_text TEXT NOT NULL,
        player_id TEXT NOT NULL,
        player_name TEXT NOT NULL,
        bucket TEXT NOT NULL,
        amount INTEGER NOT NULL,
        status TEXT NOT NULL DEFAULT 'open',
        payout INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        resolved_at INTEGER
      );
      CREATE INDEX IF NOT EXISTS idx_bets_card_status ON bets(card_id, status);
      CREATE INDEX IF NOT EXISTS idx_bets_player ON bets(player_id, created_at DESC);
    `);

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
    if (!cols.includes("completed_at")) {
      this.db.exec("ALTER TABLE todos ADD COLUMN completed_at INTEGER;");
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

    let sql =
      "SELECT id, text, status, priority, done, position, created_at, completed_at FROM todos";
    if (clauses.length > 0) sql += ` WHERE ${clauses.join(" AND ")}`;
    sql += " ORDER BY position ASC, created_at DESC LIMIT ?";
    params.push(MAX_TODOS);

    const rows = this.db.query<TodoRow, (string | number)[]>(sql).all(...params);
    return rows.map(rowToTodo);
  }

  get(id: string): Todo | null {
    const row = this.db
      .query<TodoRow, [string]>(
        "SELECT id, text, status, priority, done, position, created_at, completed_at FROM todos WHERE id = ?"
      )
      .get(id);
    return row ? rowToTodo(row) : null;
  }

  add(rawText: string, opts?: { status?: TodoStatus; priority?: TodoPriority }): Todo {
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
    const completedAt = done ? createdAt : null;

    this.db
      .query(
        "INSERT INTO todos (id, text, status, priority, done, position, created_at, completed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
      )
      .run(id, text, status, priority, done, position, createdAt, completedAt);

    return { id, text, status, priority, done: Boolean(done), position, createdAt, completedAt };
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
  ): { todo: Todo; resolution: ResolutionSummary | null } | null {
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
    const newlyCompleted = existing.status !== "done" && nextStatus === "done";
    const nextCompletedAt =
      nextStatus === "done" ? existing.completedAt ?? Date.now() : null;

    this.db
      .query(
        "UPDATE todos SET text = ?, status = ?, priority = ?, done = ?, position = ?, completed_at = ? WHERE id = ?"
      )
      .run(nextText, nextStatus, nextPriority, nextDone, nextPosition, nextCompletedAt, id);

    let resolution: ResolutionSummary | null = null;
    if (newlyCompleted) {
      const elapsed = Math.max(0, (nextCompletedAt ?? Date.now()) - existing.createdAt);
      const winningBucket = bucketForElapsedMs(elapsed);
      resolution = this.resolveCardMarket(id, nextText, winningBucket);
    }

    return {
      todo: {
        id: existing.id,
        text: nextText,
        status: nextStatus,
        priority: nextPriority,
        done: Boolean(nextDone),
        position: nextPosition,
        createdAt: existing.createdAt,
        completedAt: nextCompletedAt,
      },
      resolution,
    };
  }

  delete(id: string): { deleted: boolean; resolution: ResolutionSummary | null } {
    const existing = this.get(id);
    const result = this.db.query("DELETE FROM todos WHERE id = ?").run(id);
    if (result.changes === 0) return { deleted: false, resolution: null };

    let resolution: ResolutionSummary | null = null;
    if (existing && existing.status !== "done") {
      resolution = this.resolveCardMarket(id, existing.text, "never");
    }
    return { deleted: true, resolution };
  }

  clearCompleted(): number {
    const result = this.db.query("DELETE FROM todos WHERE status = 'done' OR done = 1").run();
    return result.changes;
  }

  // --- Prediction Gambling ---

  getOrCreateWallet(rawPlayerId: string, rawName?: string): Wallet {
    const playerId = rawPlayerId.trim().slice(0, 64);
    if (!playerId) throw new Error("playerId is required");
    const now = Date.now();
    const existing = this.db
      .query<WalletRow, [string]>(
        "SELECT player_id, name, balance, wins, losses, total_won FROM wallets WHERE player_id = ?"
      )
      .get(playerId);

    if (existing) {
      const nextName = rawName ? sanitizeName(rawName, playerId) : existing.name;
      if (nextName !== existing.name) {
        this.db
          .query("UPDATE wallets SET name = ?, updated_at = ? WHERE player_id = ?")
          .run(nextName, now, playerId);
        return { ...rowToWallet(existing), name: nextName };
      }
      return rowToWallet(existing);
    }

    const name = sanitizeName(rawName, playerId);
    this.db
      .query(
        "INSERT INTO wallets (player_id, name, balance, wins, losses, total_won, updated_at) VALUES (?, ?, ?, 0, 0, 0, ?)"
      )
      .run(playerId, name, STARTING_BALANCE, now);

    return {
      playerId,
      name,
      balance: STARTING_BALANCE,
      wins: 0,
      losses: 0,
      totalWon: 0,
    };
  }

  bailoutWallet(playerId: string, rawName?: string): { wallet: Wallet; granted: number } {
    const wallet = this.getOrCreateWallet(playerId, rawName);
    if (wallet.balance >= BAILOUT_THRESHOLD) {
      throw new Error(`You still got ${wallet.balance} coins! Bailout unlocks below ${BAILOUT_THRESHOLD}.`);
    }
    const now = Date.now();
    const nextBalance = wallet.balance + BAILOUT_AMOUNT;
    this.db
      .query("UPDATE wallets SET balance = ?, updated_at = ? WHERE player_id = ?")
      .run(nextBalance, now, wallet.playerId);
    return { wallet: { ...wallet, balance: nextBalance }, granted: BAILOUT_AMOUNT };
  }

  placeBet(params: {
    playerId: string;
    playerName?: string;
    cardId: string;
    bucket: BetBucket;
    amount: number;
  }): { bet: Bet; wallet: Wallet } {
    const amount = Math.floor(params.amount);
    if (!Number.isFinite(amount) || amount < 10) {
      throw new Error("Minimum bet is 10 coins");
    }
    if (amount > 100_000) {
      throw new Error("Max single bet is 100,000 coins");
    }
    if (!isValidBucket(params.bucket)) {
      throw new Error("Invalid prediction bucket");
    }

    const card = this.get(params.cardId);
    if (!card) throw new Error("Card not found");
    if (card.status === "done") throw new Error("Market closed — card is already done");

    const elapsed = Date.now() - card.createdAt;
    if (elapsed >= BUCKET_MAX_MS[params.bucket]) {
      throw new Error(`Too late! "${BUCKET_LABELS[params.bucket]}" window already expired for this card`);
    }

    const wallet = this.getOrCreateWallet(params.playerId, params.playerName);
    if (wallet.balance < amount) {
      throw new Error(`Not enough Aura Coins (balance: ${wallet.balance})`);
    }

    const betId = crypto.randomUUID();
    const now = Date.now();
    const nextBalance = wallet.balance - amount;

    const tx = this.db.transaction(() => {
      this.db
        .query("UPDATE wallets SET balance = ?, updated_at = ? WHERE player_id = ?")
        .run(nextBalance, now, wallet.playerId);
      this.db
        .query(
          "INSERT INTO bets (id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'open', 0, ?, NULL)"
        )
        .run(betId, card.id, card.text, wallet.playerId, wallet.name, params.bucket, amount, now);
    });
    tx();

    return {
      bet: {
        id: betId,
        cardId: card.id,
        cardText: card.text,
        playerId: wallet.playerId,
        playerName: wallet.name,
        bucket: params.bucket,
        amount,
        status: "open",
        payout: 0,
        createdAt: now,
        resolvedAt: null,
      },
      wallet: { ...wallet, balance: nextBalance },
    };
  }

  resolveCardMarket(
    cardId: string,
    cardText: string,
    winningBucket: BetBucket
  ): ResolutionSummary | null {
    const openBets = this.db
      .query<BetRow, [string]>(
        "SELECT id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at FROM bets WHERE card_id = ? AND status = 'open'"
      )
      .all(cardId);

    if (openBets.length === 0) return null;

    const now = Date.now();
    const totalPool = openBets.reduce((sum, b) => sum + b.amount, 0);
    const winners = openBets.filter((b) => b.bucket === winningBucket);
    const losers = openBets.filter((b) => b.bucket !== winningBucket);
    const winningPool = winners.reduce((sum, b) => sum + b.amount, 0);

    let totalPaid = 0;

    const tx = this.db.transaction(() => {
      for (const b of winners) {
        // Parimutuel split of full pot, with a minimum 1.5x house bonus if everyone bet the same winner
        const rawShare = winningPool > 0 ? Math.round((b.amount / winningPool) * totalPool) : b.amount;
        const payout = Math.max(rawShare, Math.round(b.amount * 1.5));
        totalPaid += payout;

        this.db
          .query("UPDATE bets SET status = 'won', payout = ?, resolved_at = ? WHERE id = ?")
          .run(payout, now, b.id);
        this.db
          .query(
            "UPDATE wallets SET balance = balance + ?, wins = wins + 1, total_won = total_won + ?, updated_at = ? WHERE player_id = ?"
          )
          .run(payout, payout, now, b.player_id);
      }

      for (const b of losers) {
        this.db
          .query("UPDATE bets SET status = 'lost', payout = 0, resolved_at = ? WHERE id = ?")
          .run(now, b.id);
        this.db
          .query("UPDATE wallets SET losses = losses + 1, updated_at = ? WHERE player_id = ?")
          .run(now, b.player_id);
      }
    });
    tx();

    return {
      cardId,
      cardText,
      winningBucket,
      totalPool,
      totalPaid,
      winnerCount: winners.length,
      loserCount: losers.length,
    };
  }

  getMarketsMap(): Record<string, MarketSummary> {
    const openBets = this.db
      .query<BetRow, []>(
        "SELECT id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at FROM bets WHERE status = 'open' ORDER BY created_at DESC"
      )
      .all();

    const map: Record<string, MarketSummary> = {};
    for (const row of openBets) {
      const bucket: BetBucket = isValidBucket(row.bucket) ? row.bucket : "never";
      let m = map[row.card_id];
      if (!m) {
        m = {
          cardId: row.card_id,
          totalPool: 0,
          betCount: 0,
          pools: { speedrun: 0, today: 0, eventually: 0, never: 0 },
          recentBets: [],
        };
        map[row.card_id] = m;
      }
      m.totalPool += row.amount;
      m.betCount += 1;
      m.pools[bucket] += row.amount;
      if (m.recentBets.length < 8) {
        m.recentBets.push({
          id: row.id,
          playerName: row.player_name,
          bucket,
          amount: row.amount,
          createdAt: row.created_at,
        });
      }
    }
    return map;
  }

  getPlayerBets(playerId: string, limit = 20): Bet[] {
    const rows = this.db
      .query<BetRow, [string, number]>(
        "SELECT id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at FROM bets WHERE player_id = ? ORDER BY created_at DESC LIMIT ?"
      )
      .all(playerId, limit);
    return rows.map(rowToBet);
  }

  getLeaderboard(limit = 10): Wallet[] {
    const rows = this.db
      .query<WalletRow, [number]>(
        "SELECT player_id, name, balance, wins, losses, total_won FROM wallets ORDER BY balance DESC, wins DESC LIMIT ?"
      )
      .all(limit);
    return rows.map(rowToWallet);
  }

  close(): void {
    this.db.close();
  }
}
