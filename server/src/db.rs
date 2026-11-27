use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const MAX_TEXT_LENGTH: usize = 280;
pub const STARTING_BALANCE: i64 = 1000;
pub const BAILOUT_THRESHOLD: i64 = 100;
pub const BAILOUT_AMOUNT: i64 = 500;

pub const VALID_STATUSES: [&str; 3] = ["todo", "in_progress", "done"];
pub const VALID_PRIORITIES: [&str; 3] = ["low", "medium", "high"];
pub const VALID_BUCKETS: [&str; 4] = ["speedrun", "today", "eventually", "never"];

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn is_valid_status(s: &str) -> bool {
    VALID_STATUSES.contains(&s)
}

pub fn is_valid_priority(p: &str) -> bool {
    VALID_PRIORITIES.contains(&p)
}

pub fn is_valid_bucket(b: &str) -> bool {
    VALID_BUCKETS.contains(&b)
}

pub fn bucket_label(bucket: &str) -> &'static str {
    match bucket {
        "speedrun" => "⚡ < 5 min",
        "today" => "🔥 5m – 1h",
        "eventually" => "🐢 1h – 24h",
        _ => "💀 24h+ / Nuked",
    }
}

pub fn bucket_max_ms(bucket: &str) -> Option<i64> {
    match bucket {
        "speedrun" => Some(5 * 60 * 1000),
        "today" => Some(60 * 60 * 1000),
        "eventually" => Some(24 * 60 * 60 * 1000),
        "never" => None,
        _ => Some(0),
    }
}

pub fn bucket_for_elapsed_ms(elapsed_ms: i64) -> &'static str {
    if elapsed_ms < 5 * 60 * 1000 {
        "speedrun"
    } else if elapsed_ms < 60 * 60 * 1000 {
        "today"
    } else if elapsed_ms < 24 * 60 * 60 * 1000 {
        "eventually"
    } else {
        "never"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: String,
    pub text: String,
    pub status: String,
    pub priority: String,
    pub done: bool,
    pub position: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallet {
    pub player_id: String,
    pub name: String,
    pub balance: i64,
    pub wins: i64,
    pub losses: i64,
    pub total_won: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bet {
    pub id: String,
    pub card_id: String,
    pub card_text: String,
    pub player_id: String,
    pub player_name: String,
    pub bucket: String,
    pub amount: i64,
    pub status: String,
    pub payout: i64,
    pub created_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentBet {
    pub id: String,
    pub player_name: String,
    pub bucket: String,
    pub amount: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketSummary {
    pub card_id: String,
    pub total_pool: i64,
    pub bet_count: i64,
    pub pools: HashMap<String, i64>,
    pub recent_bets: Vec<RecentBet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionSummary {
    pub card_id: String,
    pub card_text: String,
    pub winning_bucket: String,
    pub total_pool: i64,
    pub total_paid: i64,
    pub winner_count: i64,
    pub loser_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LootDrop {
    pub id: String,
    pub item_key: String,
    pub name: String,
    pub icon: String,
    pub rarity: String,
    pub kind: String,
    pub value: String,
    pub coin_bonus: i64,
    pub opened_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlayerLoadout {
    pub cursor_skin: Option<String>,
    pub title: Option<String>,
    pub aura: Option<String>,
}

pub struct LootDef {
    pub key: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub rarity: &'static str,
    pub kind: &'static str,
    pub value: &'static str,
    pub coin_bonus: i64,
}

pub const LOOT_POOL: &[LootDef] = &[
    // Common
    LootDef { key: "coins_50", name: "Pocket Lint (+50 🪙)", icon: "🪙", rarity: "common", kind: "coins", value: "50", coin_bonus: 50 },
    LootDef { key: "coins_125", name: "Couch Cushion Bitcoin (+125 🪙)", icon: "🛋️", rarity: "common", kind: "coins", value: "125", coin_bonus: 125 },
    LootDef { key: "roast_ratio", name: "Skill Issue (-0 🪙, +1 Ratio)", icon: "💀", rarity: "common", kind: "roast", value: "ratio", coin_bonus: 0 },
    LootDef { key: "roast_grass", name: "Touch Grass Permit: REVOKED", icon: "🌿", rarity: "common", kind: "roast", value: "grass", coin_bonus: 0 },
    LootDef { key: "roast_gold", name: "Edit: Thanks For The Gold Kind Stranger!", icon: "🏅", rarity: "common", kind: "roast", value: "gold", coin_bonus: 0 },
    LootDef { key: "roast_nft", name: "Screenshotted JPEG ($0.00 Value)", icon: "🐒", rarity: "common", kind: "roast", value: "nft", coin_bonus: 0 },
    LootDef { key: "title_intern", name: "Title: 10x Intern", icon: "📎", rarity: "common", kind: "title", value: "10x INTERN 📎", coin_bonus: 0 },
    LootDef { key: "title_lurker", name: "Title: Certified Lurker", icon: "👀", rarity: "common", kind: "title", value: "LURKER 👀", coin_bonus: 0 },
    LootDef { key: "cursor_banana", name: "Cursor: Tactical Banana", icon: "🍌", rarity: "common", kind: "cursor", value: "banana", coin_bonus: 0 },
    // Rare
    LootDef { key: "coins_200", name: "Stimulus Check (+200 🪙)", icon: "💵", rarity: "rare", kind: "coins", value: "200", coin_bonus: 200 },
    LootDef { key: "coins_350", name: "Bug Bounty Payout (+350 🪙)", icon: "🐛", rarity: "rare", kind: "coins", value: "350", coin_bonus: 350 },
    LootDef { key: "roast_fedora", name: "\"Erm Actually\" Fedora of Cope", icon: "🤓", rarity: "rare", kind: "roast", value: "fedora", coin_bonus: 0 },
    LootDef { key: "roast_karma", name: "Downvoted to Oblivion (-420 Karma)", icon: "⬇️", rarity: "rare", kind: "roast", value: "karma", coin_bonus: 0 },
    LootDef { key: "roast_ban", name: "Permabanned From r/productivity", icon: "🔨", rarity: "rare", kind: "roast", value: "ban", coin_bonus: 0 },
    LootDef { key: "title_degen", name: "Title: Solana Degen", icon: "🎰", rarity: "rare", kind: "title", value: "SOLANA DEGEN 🎰", coin_bonus: 0 },
    LootDef { key: "title_caffeine", name: "Title: 6th Espresso", icon: "☕", rarity: "rare", kind: "title", value: "ESPRESSO ☕", coin_bonus: 0 },
    LootDef { key: "cursor_bolt", name: "Cursor: Cyber Lightning", icon: "⚡", rarity: "rare", kind: "cursor", value: "bolt", coin_bonus: 0 },
    LootDef { key: "cursor_shuriken", name: "Cursor: Ninja Shuriken", icon: "✴️", rarity: "rare", kind: "cursor", value: "shuriken", coin_bonus: 0 },
    LootDef { key: "aura_cyber", name: "Aura: Cyber Pulse Ring", icon: "💠", rarity: "rare", kind: "aura", value: "cyber", coin_bonus: 0 },
    LootDef { key: "aura_toxic", name: "Aura: Biohazard Reactor", icon: "☢️", rarity: "rare", kind: "aura", value: "toxic", coin_bonus: 0 },
    // Epic
    LootDef { key: "coins_600", name: "VC Seed Round (+600 🪙)", icon: "💰", rarity: "epic", kind: "coins", value: "600", coin_bonus: 600 },
    LootDef { key: "coins_1000", name: "Series A Pitch Deck (+1,000 🪙)", icon: "📈", rarity: "epic", kind: "coins", value: "1000", coin_bonus: 1000 },
    LootDef { key: "roast_receipt", name: "Non-Refundable Pixel Receipt", icon: "🧾", rarity: "epic", kind: "roast", value: "receipt", coin_bonus: 0 },
    LootDef { key: "title_mod", name: "Title: Certified Reddit Mod", icon: "🤓", rarity: "epic", kind: "title", value: "REDDIT MOD 🤓", coin_bonus: 0 },
    LootDef { key: "title_chad", name: "Title: Giga Chad", icon: "🗿", rarity: "epic", kind: "title", value: "GIGA CHAD 🗿", coin_bonus: 0 },
    LootDef { key: "title_bugmaker", name: "Title: Pushes To Main", icon: "💣", rarity: "epic", kind: "title", value: "PUSHES TO MAIN 💣", coin_bonus: 0 },
    LootDef { key: "cursor_flame", name: "Cursor: Hellfire Blade", icon: "🔥", rarity: "epic", kind: "cursor", value: "flame", coin_bonus: 0 },
    LootDef { key: "cursor_diamond", name: "Cursor: Liquid Prism", icon: "💎", rarity: "epic", kind: "cursor", value: "diamond", coin_bonus: 0 },
    LootDef { key: "cursor_sword", name: "Cursor: Moonlight Greatsword", icon: "⚔️", rarity: "epic", kind: "cursor", value: "sword", coin_bonus: 0 },
    LootDef { key: "aura_sakura", name: "Aura: Anime Protagonist", icon: "🌸", rarity: "epic", kind: "aura", value: "sakura", coin_bonus: 0 },
    LootDef { key: "aura_inferno", name: "Aura: Hellfire Halo", icon: "🔥", rarity: "epic", kind: "aura", value: "inferno", coin_bonus: 0 },
    // Legendary
    LootDef { key: "coins_1800", name: "Insider Alpha (+1,800 🪙)", icon: "🏦", rarity: "legendary", kind: "coins", value: "1800", coin_bonus: 1800 },
    LootDef { key: "coins_3333", name: "Golden Goose Bailout (+3,333 🪙)", icon: "🪿", rarity: "legendary", kind: "coins", value: "3333", coin_bonus: 3333 },
    LootDef { key: "roast_audit", name: "IRS Aura Audit Notice", icon: "🕵️", rarity: "legendary", kind: "roast", value: "audit", coin_bonus: 0 },
    LootDef { key: "title_whale", name: "Title: Aura Whale", icon: "🐋", rarity: "legendary", kind: "title", value: "AURA WHALE 🐋", coin_bonus: 0 },
    LootDef { key: "title_crab", name: "Title: Fearless Rustacean", icon: "🦀", rarity: "legendary", kind: "title", value: "RUSTACEAN 🦀", coin_bonus: 0 },
    LootDef { key: "cursor_crown", name: "Cursor: Sovereign Crown", icon: "👑", rarity: "legendary", kind: "cursor", value: "crown", coin_bonus: 0 },
    LootDef { key: "cursor_ufo", name: "Cursor: Cosmic Abductor", icon: "🛸", rarity: "legendary", kind: "cursor", value: "ufo", coin_bonus: 0 },
    LootDef { key: "cursor_brain", name: "Cursor: 400 IQ Galaxy Brain", icon: "🧠", rarity: "legendary", kind: "cursor", value: "brain", coin_bonus: 0 },
    LootDef { key: "aura_rust", name: "Aura: Zero-GC Blazing Rust", icon: "🦀", rarity: "legendary", kind: "aura", value: "rust", coin_bonus: 0 },
    LootDef { key: "aura_void", name: "Aura: Event Horizon Singularity", icon: "🕳️", rarity: "legendary", kind: "aura", value: "void", coin_bonus: 0 },
    // Mythic
    LootDef { key: "coins_5000", name: "RUGPULL JACKPOT (+5,000 🪙)", icon: "🤑", rarity: "mythic", kind: "coins", value: "5000", coin_bonus: 5000 },
    LootDef { key: "coins_10000", name: "ELON TWEET PUMP (+10,000 🪙)", icon: "🚀", rarity: "mythic", kind: "coins", value: "10000", coin_bonus: 10000 },
    LootDef { key: "title_overlord", name: "Title: AI Overlord", icon: "🤖", rarity: "mythic", kind: "title", value: "AI OVERLORD 🤖", coin_bonus: 0 },
    LootDef { key: "title_oligarch", name: "Title: Liquid Glass Oligarch", icon: "💸", rarity: "mythic", kind: "title", value: "OLIGARCH 💸", coin_bonus: 0 },
    LootDef { key: "cursor_scythe", name: "Cursor: Mythic Reaper", icon: "💀", rarity: "mythic", kind: "cursor", value: "scythe", coin_bonus: 0 },
    LootDef { key: "cursor_vortex", name: "Cursor: Celestial Star-Blade", icon: "🌀", rarity: "mythic", kind: "cursor", value: "vortex", coin_bonus: 0 },
    LootDef { key: "aura_godmode", name: "Aura: Ascended Prism God", icon: "🌈", rarity: "mythic", kind: "aura", value: "godmode", coin_bonus: 0 },
];

pub fn crate_meta(crate_id: &str) -> Option<(&'static str, i64)> {
    match crate_id {
        "standard" => Some(("Standard Crate", 100)),
        "glass" => Some(("Liquid Glass Case", 350)),
        "aura" => Some(("Admin Aura Relic Chest", 1000)),
        _ => None,
    }
}

fn roll_rarity(crate_id: &str) -> &'static str {
    let rnd = (Uuid::new_v4().as_u128() % 1000) as u32;
    match crate_id {
        "standard" => {
            if rnd < 540 { "common" }
            else if rnd < 820 { "rare" }
            else if rnd < 935 { "epic" }
            else if rnd < 985 { "legendary" }
            else { "mythic" }
        }
        "glass" => {
            if rnd < 140 { "common" }
            else if rnd < 520 { "rare" }
            else if rnd < 810 { "epic" }
            else if rnd < 955 { "legendary" }
            else { "mythic" }
        }
        "aura" => {
            if rnd < 160 { "rare" }
            else if rnd < 570 { "epic" }
            else if rnd < 875 { "legendary" }
            else { "mythic" }
        }
        _ => "common",
    }
}

fn roll_loot(crate_id: &str) -> &'static LootDef {
    let tier = roll_rarity(crate_id);
    let matches: Vec<&'static LootDef> = LOOT_POOL.iter().filter(|i| i.rarity == tier).collect();
    if matches.is_empty() {
        return &LOOT_POOL[0];
    }
    let idx = (Uuid::new_v4().as_u128() % (matches.len() as u128)) as usize;
    matches[idx]
}

pub struct TodoDB {
    conn: Mutex<Connection>,
    max_todos: i64,
}

fn sanitize_name(raw: Option<&str>, fallback_id: &str) -> String {
    let cleaned: String = raw
        .unwrap_or("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(24)
        .collect();
    if cleaned.is_empty() {
        let prefix: String = fallback_id.chars().take(4).collect();
        format!("Degen#{prefix}")
    } else {
        cleaned
    }
}

fn row_to_todo(row: &rusqlite::Row<'_>) -> rusqlite::Result<Todo> {
    let id: String = row.get(0)?;
    let text: String = row.get(1)?;
    let raw_status: String = row.get(2)?;
    let raw_priority: String = row.get(3)?;
    let done_int: i64 = row.get(4)?;
    let position: i64 = row.get(5)?;
    let created_at: i64 = row.get(6)?;
    let completed_at: Option<i64> = row.get(7)?;
    let raw_created_by: String = row.get(8).unwrap_or_else(|_| "web".to_string());

    let status = if is_valid_status(&raw_status) {
        raw_status
    } else if done_int != 0 {
        "done".to_string()
    } else {
        "todo".to_string()
    };
    let priority = if is_valid_priority(&raw_priority) {
        raw_priority
    } else {
        "medium".to_string()
    };
    let done = status == "done";
    let created_by = if raw_created_by.trim().is_empty() {
        "web".to_string()
    } else {
        raw_created_by
    };

    Ok(Todo {
        id,
        text,
        status,
        priority,
        done,
        position,
        created_at,
        completed_at,
        created_by,
    })
}

impl TodoDB {
    pub fn open(db_path: &str, max_todos: i64) -> Result<Self, String> {
        if db_path != ":memory:" {
            if let Some(parent) = Path::new(db_path).parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
            }
        }
        let conn = if db_path == ":memory:" {
            Connection::open_in_memory()
        } else {
            Connection::open(db_path)
        }
        .map_err(|e| e.to_string())?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS todos (
               id TEXT PRIMARY KEY,
               text TEXT NOT NULL,
               status TEXT NOT NULL DEFAULT 'todo',
               priority TEXT NOT NULL DEFAULT 'medium',
               done INTEGER NOT NULL DEFAULT 0,
               position INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               completed_at INTEGER,
               created_by TEXT NOT NULL DEFAULT 'web'
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
             CREATE TABLE IF NOT EXISTS admin_sessions (
               token_hash TEXT PRIMARY KEY,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS loot_inventory (
               id TEXT PRIMARY KEY,
               player_id TEXT NOT NULL,
               item_key TEXT NOT NULL,
               name TEXT NOT NULL,
               icon TEXT NOT NULL,
               rarity TEXT NOT NULL,
               kind TEXT NOT NULL,
               value TEXT NOT NULL,
               coin_bonus INTEGER NOT NULL DEFAULT 0,
               opened_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_loot_player ON loot_inventory(player_id, opened_at DESC);
             CREATE TABLE IF NOT EXISTS player_loadout (
               player_id TEXT PRIMARY KEY,
               cursor_skin TEXT,
               title TEXT,
               aura TEXT,
               updated_at INTEGER NOT NULL
             );",
        )
        .map_err(|e| e.to_string())?;

        // Safe schema migrations for pre-existing DBs
        let mut loadout_cols = Vec::new();
        {
            let mut stmt = conn
                .prepare("PRAGMA table_info(player_loadout)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            for r in rows.flatten() {
                loadout_cols.push(r);
            }
        }
        if !loadout_cols.iter().any(|c| c == "aura") {
            let _ = conn.execute("ALTER TABLE player_loadout ADD COLUMN aura TEXT", []);
        }
        let mut cols = Vec::new();
        {
            let mut stmt = conn
                .prepare("PRAGMA table_info(todos)")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .map_err(|e| e.to_string())?;
            for r in rows.flatten() {
                cols.push(r);
            }
        }
        if !cols.iter().any(|c| c == "status") {
            let _ = conn.execute_batch(
                "ALTER TABLE todos ADD COLUMN status TEXT NOT NULL DEFAULT 'todo';
                 UPDATE todos SET status = CASE WHEN done = 1 THEN 'done' ELSE 'todo' END;",
            );
        }
        if !cols.iter().any(|c| c == "priority") {
            let _ = conn.execute(
                "ALTER TABLE todos ADD COLUMN priority TEXT NOT NULL DEFAULT 'medium'",
                [],
            );
        }
        if !cols.iter().any(|c| c == "position") {
            let _ = conn.execute(
                "ALTER TABLE todos ADD COLUMN position INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }
        if !cols.iter().any(|c| c == "completed_at") {
            let _ = conn.execute("ALTER TABLE todos ADD COLUMN completed_at INTEGER", []);
        }
        if !cols.iter().any(|c| c == "created_by") {
            let _ = conn.execute(
                "ALTER TABLE todos ADD COLUMN created_by TEXT NOT NULL DEFAULT 'web'",
                [],
            );
        }

        Ok(Self {
            conn: Mutex::new(conn),
            max_todos,
        })
    }

    pub fn list(&self, status_filter: Option<&str>, priority_filter: Option<&str>) -> Vec<Todo> {
        let conn = self.conn.lock().unwrap();
        let mut clauses: Vec<&str> = Vec::new();
        let mut binds: Vec<rusqlite::types::Value> = Vec::new();

        if let Some(st) = status_filter {
            match st {
                "all" => {}
                "active" => clauses.push("status != 'done'"),
                "completed" => clauses.push("status = 'done'"),
                s if is_valid_status(s) => {
                    clauses.push("status = ?");
                    binds.push(rusqlite::types::Value::Text(s.to_string()));
                }
                _ => {}
            }
        }
        if let Some(pr) = priority_filter {
            if is_valid_priority(pr) {
                clauses.push("priority = ?");
                binds.push(rusqlite::types::Value::Text(pr.to_string()));
            }
        }

        let mut sql =
            "SELECT id, text, status, priority, done, position, created_at, completed_at, created_by FROM todos"
                .to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY position ASC, created_at DESC LIMIT ?");
        binds.push(rusqlite::types::Value::Integer(self.max_todos));

        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params_from_iter(binds), row_to_todo) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.flatten().collect()
    }

    pub fn get(&self, id: &str) -> Option<Todo> {
        let conn = self.conn.lock().unwrap();
        Self::get_conn(&conn, id)
    }

    fn get_conn(conn: &Connection, id: &str) -> Option<Todo> {
        conn.query_row(
            "SELECT id, text, status, priority, done, position, created_at, completed_at, created_by FROM todos WHERE id = ?",
            params![id],
            row_to_todo,
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn add(
        &self,
        raw_text: &str,
        status: Option<&str>,
        priority: Option<&str>,
    ) -> Result<Todo, String> {
        self.add_with_source(raw_text, status, priority, "web")
    }

    pub fn add_with_source(
        &self,
        raw_text: &str,
        status: Option<&str>,
        priority: Option<&str>,
        raw_source: &str,
    ) -> Result<Todo, String> {
        let text = raw_text.trim();
        if text.is_empty() {
            return Err("Task text is required".to_string());
        }
        if text.chars().count() > MAX_TEXT_LENGTH {
            return Err(format!("Task text must be <= {MAX_TEXT_LENGTH} characters"));
        }

        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM todos", [], |r| r.get(0))
            .unwrap_or(0);
        if count >= self.max_todos {
            return Err(format!("Todo board is full (max {} items)", self.max_todos));
        }

        let status_str = status.filter(|s| is_valid_status(s)).unwrap_or("todo");
        let priority_str = priority.filter(|p| is_valid_priority(p)).unwrap_or("medium");
        let created_by = if raw_source.trim().is_empty() {
            "web".to_string()
        } else {
            raw_source.trim().to_string()
        };
        let done_int: i64 = if status_str == "done" { 1 } else { 0 };
        let min_pos: Option<i64> = conn
            .query_row(
                "SELECT MIN(position) FROM todos WHERE status = ?",
                params![status_str],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        let position = min_pos.unwrap_or(1) - 1;
        let id = Uuid::new_v4().to_string();
        let created_at = now_ms();
        let completed_at = if done_int == 1 { Some(created_at) } else { None };

        conn.execute(
            "INSERT INTO todos (id, text, status, priority, done, position, created_at, completed_at, created_by) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![id, text, status_str, priority_str, done_int, position, created_at, completed_at, created_by],
        )
        .map_err(|e| e.to_string())?;

        Ok(Todo {
            id,
            text: text.to_string(),
            status: status_str.to_string(),
            priority: priority_str.to_string(),
            done: done_int == 1,
            position,
            created_at,
            completed_at,
            created_by,
        })
    }

    pub fn update(
        &self,
        id: &str,
        status: Option<&str>,
        priority: Option<&str>,
        done: Option<bool>,
        text: Option<&str>,
        position: Option<i64>,
    ) -> Result<Option<(Todo, Option<ResolutionSummary>)>, String> {
        let mut conn = self.conn.lock().unwrap();
        let Some(existing) = Self::get_conn(&conn, id) else {
            return Ok(None);
        };

        let next_status = if let Some(s) = status.filter(|s| is_valid_status(s)) {
            s.to_string()
        } else if let Some(d) = done {
            if d {
                "done".to_string()
            } else if existing.status == "done" {
                "todo".to_string()
            } else {
                existing.status.clone()
            }
        } else {
            existing.status.clone()
        };

        let next_priority = priority
            .filter(|p| is_valid_priority(p))
            .map(|p| p.to_string())
            .unwrap_or_else(|| existing.priority.clone());

        let next_text = if let Some(raw) = text {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.chars().count() > MAX_TEXT_LENGTH {
                return Err(format!("Task text must be 1..{MAX_TEXT_LENGTH} characters"));
            }
            trimmed.to_string()
        } else {
            existing.text.clone()
        };

        let next_position = if let Some(pos) = position {
            pos
        } else if next_status != existing.status {
            let min_pos: Option<i64> = conn
                .query_row(
                    "SELECT MIN(position) FROM todos WHERE status = ?",
                    params![&next_status],
                    |r| r.get(0),
                )
                .ok()
                .flatten();
            min_pos.unwrap_or(1) - 1
        } else {
            existing.position
        };

        let next_done_int: i64 = if next_status == "done" { 1 } else { 0 };
        let newly_completed = existing.status != "done" && next_status == "done";
        let next_completed_at = if next_status == "done" {
            Some(existing.completed_at.unwrap_or_else(now_ms))
        } else {
            None
        };

        conn.execute(
            "UPDATE todos SET text = ?, status = ?, priority = ?, done = ?, position = ?, completed_at = ? WHERE id = ?",
            params![
                next_text,
                next_status,
                next_priority,
                next_done_int,
                next_position,
                next_completed_at,
                id
            ],
        )
        .map_err(|e| e.to_string())?;

        let resolution = if newly_completed {
            let elapsed = (next_completed_at.unwrap_or_else(now_ms) - existing.created_at).max(0);
            let winning_bucket = bucket_for_elapsed_ms(elapsed);
            Self::resolve_market_conn(&mut conn, id, &next_text, winning_bucket)?
        } else {
            None
        };

        Ok(Some((
            Todo {
                id: existing.id,
                text: next_text,
                status: next_status,
                priority: next_priority,
                done: next_done_int == 1,
                position: next_position,
                created_at: existing.created_at,
                completed_at: next_completed_at,
                created_by: existing.created_by,
            },
            resolution,
        )))
    }

    pub fn delete(&self, id: &str) -> Result<(bool, Option<ResolutionSummary>), String> {
        let mut conn = self.conn.lock().unwrap();
        let existing = Self::get_conn(&conn, id);
        let changes = conn
            .execute("DELETE FROM todos WHERE id = ?", params![id])
            .map_err(|e| e.to_string())?;
        if changes == 0 {
            return Ok((false, None));
        }
        let resolution = if let Some(card) = existing {
            if card.status != "done" {
                Self::resolve_market_conn(&mut conn, id, &card.text, "never")?
            } else {
                None
            }
        } else {
            None
        };
        Ok((true, resolution))
    }

    pub fn clear_completed(&self) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        let changes = conn
            .execute("DELETE FROM todos WHERE status = 'done' OR done = 1", [])
            .map_err(|e| e.to_string())?;
        Ok(changes as i64)
    }

    pub fn clear_completed_by_source(&self, source: &str) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        let changes = conn
            .execute(
                "DELETE FROM todos WHERE (status = 'done' OR done = 1) AND created_by = ?",
                params![source],
            )
            .map_err(|e| e.to_string())?;
        Ok(changes as i64)
    }

    pub fn create_admin_session(&self, token_hash: &str, ttl_ms: i64) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        let expires_at = now + ttl_ms;
        let _ = conn.execute("DELETE FROM admin_sessions WHERE expires_at <= ?", params![now]);
        conn.execute(
            "INSERT OR REPLACE INTO admin_sessions (token_hash, created_at, expires_at) VALUES (?, ?, ?)",
            params![token_hash, now, expires_at],
        )
        .map_err(|e| e.to_string())?;
        Ok(expires_at)
    }

    pub fn has_admin_session(&self, token_hash: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        let exp: Option<i64> = conn
            .query_row(
                "SELECT expires_at FROM admin_sessions WHERE token_hash = ?",
                params![token_hash],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        matches!(exp, Some(e) if e > now)
    }

    pub fn delete_admin_session(&self, token_hash: &str) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute("DELETE FROM admin_sessions WHERE token_hash = ?", params![token_hash]);
    }

    pub fn nuke_bets(&self) -> Result<(i64, i64), String> {
        let mut conn = self.conn.lock().unwrap();
        let open_bets: Vec<(String, i64)> = {
            let mut stmt = conn
                .prepare("SELECT player_id, amount FROM bets WHERE status = 'open'")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?;
            rows.flatten().collect()
        };
        let now = now_ms();
        let mut refunded_coins = 0i64;
        let refunded_count = open_bets.len() as i64;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (pid, amt) in open_bets {
            refunded_coins += amt;
            tx.execute(
                "UPDATE wallets SET balance = balance + ?, updated_at = ? WHERE player_id = ?",
                params![amt, now, pid],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute("DELETE FROM bets", []).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok((refunded_count, refunded_coins))
    }

    pub fn nuke_all(&self) -> Result<(i64, i64), String> {
        let (refunded_bets, _) = self.nuke_bets()?;
        let conn = self.conn.lock().unwrap();
        let cards_deleted = conn
            .execute("DELETE FROM todos", [])
            .map_err(|e| e.to_string())? as i64;
        Ok((cards_deleted, refunded_bets))
    }

    // --- Wallets & Prediction Gambling ---

    fn get_or_create_wallet_conn(
        conn: &Connection,
        raw_player_id: &str,
        raw_name: Option<&str>,
    ) -> Result<Wallet, String> {
        let player_id: String = raw_player_id.trim().chars().take(64).collect();
        if player_id.is_empty() {
            return Err("playerId is required".to_string());
        }
        let now = now_ms();
        let existing = conn
            .query_row(
                "SELECT player_id, name, balance, wins, losses, total_won FROM wallets WHERE player_id = ?",
                params![&player_id],
                |r| {
                    Ok(Wallet {
                        player_id: r.get(0)?,
                        name: r.get(1)?,
                        balance: r.get(2)?,
                        wins: r.get(3)?,
                        losses: r.get(4)?,
                        total_won: r.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|e| e.to_string())?;

        if let Some(mut w) = existing {
            if let Some(rn) = raw_name {
                let next_name = sanitize_name(Some(rn), &player_id);
                if next_name != w.name {
                    conn.execute(
                        "UPDATE wallets SET name = ?, updated_at = ? WHERE player_id = ?",
                        params![&next_name, now, &player_id],
                    )
                    .map_err(|e| e.to_string())?;
                    w.name = next_name;
                }
            }
            return Ok(w);
        }

        let name = sanitize_name(raw_name, &player_id);
        conn.execute(
            "INSERT INTO wallets (player_id, name, balance, wins, losses, total_won, updated_at) VALUES (?, ?, ?, 0, 0, 0, ?)",
            params![&player_id, &name, STARTING_BALANCE, now],
        )
        .map_err(|e| e.to_string())?;

        Ok(Wallet {
            player_id,
            name,
            balance: STARTING_BALANCE,
            wins: 0,
            losses: 0,
            total_won: 0,
        })
    }

    pub fn get_or_create_wallet(
        &self,
        raw_player_id: &str,
        raw_name: Option<&str>,
    ) -> Result<Wallet, String> {
        let conn = self.conn.lock().unwrap();
        Self::get_or_create_wallet_conn(&conn, raw_player_id, raw_name)
    }

    pub fn bailout_wallet(
        &self,
        player_id: &str,
        raw_name: Option<&str>,
    ) -> Result<(Wallet, i64), String> {
        let conn = self.conn.lock().unwrap();
        let mut wallet = Self::get_or_create_wallet_conn(&conn, player_id, raw_name)?;
        if wallet.balance >= BAILOUT_THRESHOLD {
            return Err(format!(
                "You still got {} coins! Bailout unlocks below {BAILOUT_THRESHOLD}.",
                wallet.balance
            ));
        }
        let now = now_ms();
        wallet.balance += BAILOUT_AMOUNT;
        conn.execute(
            "UPDATE wallets SET balance = ?, updated_at = ? WHERE player_id = ?",
            params![wallet.balance, now, &wallet.player_id],
        )
        .map_err(|e| e.to_string())?;
        Ok((wallet, BAILOUT_AMOUNT))
    }

    pub fn money_spin(
        &self,
        player_id: &str,
        raw_name: Option<&str>,
        wager: i64,
    ) -> Result<(Wallet, i64, &'static str), String> {
        if wager != 50 && wager != 250 {
            return Err("Pick a 50 🪙 or 250 🪙 spin!".to_string());
        }
        let conn = self.conn.lock().unwrap();
        let mut wallet = Self::get_or_create_wallet_conn(&conn, player_id, raw_name)?;
        if wallet.balance < wager {
            return Err(format!(
                "Need {wager} 🪙 to spin (you have {} 🪙)!",
                wallet.balance
            ));
        }

        let roll = (Uuid::new_v4().as_u128() % 100) as u32;
        let (mult, label) = if roll < 48 {
            (0.0, "BUST 💀 House took your lunch money")
        } else if roll < 76 {
            (1.8, "💵 Cash Out (1.8x)")
        } else if roll < 92 {
            (3.0, "💰 Money Surge (3x)")
        } else if roll < 98 {
            (6.0, "🤑 Big Cash Bag (6x)")
        } else {
            (15.0, "👑💵 MEGA CASH JACKPOT (15x)")
        };

        let payout = ((wager as f64) * mult).round() as i64;
        let now = now_ms();
        wallet.balance = wallet.balance - wager + payout;
        if payout > 0 {
            wallet.wins += 1;
            wallet.total_won += payout;
        } else {
            wallet.losses += 1;
        }

        conn.execute(
            "UPDATE wallets SET balance = ?, wins = ?, losses = ?, total_won = ?, updated_at = ? WHERE player_id = ?",
            params![
                wallet.balance,
                wallet.wins,
                wallet.losses,
                wallet.total_won,
                now,
                &wallet.player_id
            ],
        )
        .map_err(|e| e.to_string())?;

        Ok((wallet, payout, label))
    }

    pub fn place_bet(
        &self,
        player_id: &str,
        player_name: Option<&str>,
        card_id: &str,
        bucket: &str,
        amount: i64,
    ) -> Result<(Bet, Wallet), String> {
        if amount < 10 {
            return Err("Minimum bet is 10 coins".to_string());
        }
        if amount > 100_000 {
            return Err("Max single bet is 100,000 coins".to_string());
        }
        if !is_valid_bucket(bucket) {
            return Err("Invalid prediction bucket".to_string());
        }

        let mut conn = self.conn.lock().unwrap();
        let card = Self::get_conn(&conn, card_id).ok_or_else(|| "Card not found".to_string())?;
        if card.status == "done" {
            return Err("Market closed — card is already done".to_string());
        }
        let elapsed = now_ms() - card.created_at;
        if let Some(max_ms) = bucket_max_ms(bucket) {
            if elapsed >= max_ms {
                return Err(format!(
                    "Too late! \"{}\" window already expired for this card",
                    bucket_label(bucket)
                ));
            }
        }

        let mut wallet = Self::get_or_create_wallet_conn(&conn, player_id, player_name)?;
        if wallet.balance < amount {
            return Err(format!(
                "Not enough Aura Coins (balance: {})",
                wallet.balance
            ));
        }

        let bet_id = Uuid::new_v4().to_string();
        let now = now_ms();
        wallet.balance -= amount;

        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE wallets SET balance = ?, updated_at = ? WHERE player_id = ?",
            params![wallet.balance, now, &wallet.player_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO bets (id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at) VALUES (?, ?, ?, ?, ?, ?, ?, 'open', 0, ?, NULL)",
            params![
                &bet_id,
                &card.id,
                &card.text,
                &wallet.player_id,
                &wallet.name,
                bucket,
                amount,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;

        Ok((
            Bet {
                id: bet_id,
                card_id: card.id,
                card_text: card.text,
                player_id: wallet.player_id.clone(),
                player_name: wallet.name.clone(),
                bucket: bucket.to_string(),
                amount,
                status: "open".to_string(),
                payout: 0,
                created_at: now,
                resolved_at: None,
            },
            wallet,
        ))
    }

    fn resolve_market_conn(
        conn: &mut Connection,
        card_id: &str,
        card_text: &str,
        winning_bucket: &str,
    ) -> Result<Option<ResolutionSummary>, String> {
        let open_bets: Vec<(String, String, String, i64)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id, player_id, bucket, amount FROM bets WHERE card_id = ? AND status = 'open'",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![card_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(|e| e.to_string())?;
            rows.flatten().collect()
        };

        if open_bets.is_empty() {
            return Ok(None);
        }

        let now = now_ms();
        let total_pool: i64 = open_bets.iter().map(|b| b.3).sum();
        let winning_pool: i64 = open_bets
            .iter()
            .filter(|b| b.2 == winning_bucket)
            .map(|b| b.3)
            .sum();

        let mut total_paid: i64 = 0;
        let mut winner_count: i64 = 0;
        let mut loser_count: i64 = 0;

        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for (bet_id, player_id, bucket, amount) in &open_bets {
            if bucket == winning_bucket {
                winner_count += 1;
                let raw_share = if winning_pool > 0 {
                    ((*amount as f64 / winning_pool as f64) * total_pool as f64).round() as i64
                } else {
                    *amount
                };
                let mult = match winning_bucket {
                    "speedrun" => 2.5,
                    "today" => 2.0,
                    "eventually" => 1.7,
                    _ => 1.5,
                };
                let speed_bonus = match winning_bucket {
                    "speedrun" => 150,
                    "today" => 50,
                    _ => 0,
                };
                let min_bonus = ((*amount as f64) * mult).round() as i64 + speed_bonus;
                let payout = raw_share.max(min_bonus);
                total_paid += payout;

                tx.execute(
                    "UPDATE bets SET status = 'won', payout = ?, resolved_at = ? WHERE id = ?",
                    params![payout, now, bet_id],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE wallets SET balance = balance + ?, wins = wins + 1, total_won = total_won + ?, updated_at = ? WHERE player_id = ?",
                    params![payout, payout, now, player_id],
                )
                .map_err(|e| e.to_string())?;
            } else {
                loser_count += 1;
                tx.execute(
                    "UPDATE bets SET status = 'lost', payout = 0, resolved_at = ? WHERE id = ?",
                    params![now, bet_id],
                )
                .map_err(|e| e.to_string())?;
                tx.execute(
                    "UPDATE wallets SET losses = losses + 1, updated_at = ? WHERE player_id = ?",
                    params![now, player_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;

        Ok(Some(ResolutionSummary {
            card_id: card_id.to_string(),
            card_text: card_text.to_string(),
            winning_bucket: winning_bucket.to_string(),
            total_pool,
            total_paid,
            winner_count,
            loser_count,
        }))
    }

    pub fn get_markets_map(&self) -> HashMap<String, MarketSummary> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, card_id, player_name, bucket, amount, created_at FROM bets WHERE status = 'open' ORDER BY created_at DESC",
        ) {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        };

        let mut map: HashMap<String, MarketSummary> = HashMap::new();
        let rows = match stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return map,
        };

        for (id, card_id, player_name, raw_bucket, amount, created_at) in rows.flatten() {
            let bucket = if is_valid_bucket(&raw_bucket) {
                raw_bucket
            } else {
                "never".to_string()
            };
            let entry = map.entry(card_id.clone()).or_insert_with(|| {
                let mut pools = HashMap::new();
                for b in VALID_BUCKETS {
                    pools.insert(b.to_string(), 0);
                }
                MarketSummary {
                    card_id: card_id.clone(),
                    total_pool: 0,
                    bet_count: 0,
                    pools,
                    recent_bets: Vec::new(),
                }
            });
            entry.total_pool += amount;
            entry.bet_count += 1;
            *entry.pools.entry(bucket.clone()).or_insert(0) += amount;
            if entry.recent_bets.len() < 8 {
                entry.recent_bets.push(RecentBet {
                    id,
                    player_name,
                    bucket,
                    amount,
                    created_at,
                });
            }
        }
        map
    }

    pub fn get_player_bets(&self, player_id: &str, limit: i64) -> Vec<Bet> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, card_id, card_text, player_id, player_name, bucket, amount, status, payout, created_at, resolved_at FROM bets WHERE player_id = ? ORDER BY created_at DESC LIMIT ?",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![player_id, limit], |r| {
            Ok(Bet {
                id: r.get(0)?,
                card_id: r.get(1)?,
                card_text: r.get(2)?,
                player_id: r.get(3)?,
                player_name: r.get(4)?,
                bucket: r.get(5)?,
                amount: r.get(6)?,
                status: r.get(7)?,
                payout: r.get(8)?,
                created_at: r.get(9)?,
                resolved_at: r.get(10)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.flatten().collect()
    }

    pub fn get_leaderboard(&self, limit: i64) -> Vec<Wallet> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT player_id, name, balance, wins, losses, total_won FROM wallets ORDER BY balance DESC, wins DESC LIMIT ?",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![limit], |r| {
            Ok(Wallet {
                player_id: r.get(0)?,
                name: r.get(1)?,
                balance: r.get(2)?,
                wins: r.get(3)?,
                losses: r.get(4)?,
                total_won: r.get(5)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.flatten().collect()
    }

    // --- Lootboxes & Player Cosmetics ---

    fn get_loadout_conn(conn: &Connection, player_id: &str) -> PlayerLoadout {
        conn.query_row(
            "SELECT cursor_skin, title, aura FROM player_loadout WHERE player_id = ?",
            params![player_id],
            |r| {
                Ok(PlayerLoadout {
                    cursor_skin: r.get(0)?,
                    title: r.get(1)?,
                    aura: r.get(2)?,
                })
            },
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or_default()
    }

    fn get_inventory_conn(conn: &Connection, player_id: &str, limit: i64) -> Vec<LootDrop> {
        let mut stmt = match conn.prepare(
            "SELECT id, item_key, name, icon, rarity, kind, value, coin_bonus, opened_at FROM loot_inventory WHERE player_id = ? ORDER BY opened_at DESC LIMIT ?",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![player_id, limit], |r| {
            Ok(LootDrop {
                id: r.get(0)?,
                item_key: r.get(1)?,
                name: r.get(2)?,
                icon: r.get(3)?,
                rarity: r.get(4)?,
                kind: r.get(5)?,
                value: r.get(6)?,
                coin_bonus: r.get(7)?,
                opened_at: r.get(8)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.flatten().collect()
    }

    pub fn get_loot_state(
        &self,
        raw_player_id: &str,
        raw_name: Option<&str>,
    ) -> Result<(Wallet, Vec<LootDrop>, PlayerLoadout), String> {
        let conn = self.conn.lock().unwrap();
        let wallet = Self::get_or_create_wallet_conn(&conn, raw_player_id, raw_name)?;
        let inv = Self::get_inventory_conn(&conn, &wallet.player_id, 40);
        let loadout = Self::get_loadout_conn(&conn, &wallet.player_id);
        Ok((wallet, inv, loadout))
    }

    pub fn open_lootbox(
        &self,
        raw_player_id: &str,
        raw_name: Option<&str>,
        crate_id: &str,
    ) -> Result<(Wallet, LootDrop, Vec<LootDrop>, PlayerLoadout, &'static str), String> {
        let (crate_name, cost) =
            crate_meta(crate_id).ok_or_else(|| "Unknown lootbox tier".to_string())?;
        let mut conn = self.conn.lock().unwrap();
        let mut wallet = Self::get_or_create_wallet_conn(&conn, raw_player_id, raw_name)?;
        if wallet.balance < cost {
            return Err(format!(
                "Need {cost} 🪙 to crack a {crate_name} (you have {} 🪙)!",
                wallet.balance
            ));
        }

        let rolled = roll_loot(crate_id);
        let now = now_ms();
        let drop_id = Uuid::new_v4().to_string();
        wallet.balance = wallet.balance - cost + rolled.coin_bonus;
        wallet.total_won += rolled.coin_bonus;

        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE wallets SET balance = ?, total_won = ?, updated_at = ? WHERE player_id = ?",
            params![wallet.balance, wallet.total_won, now, &wallet.player_id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO loot_inventory (id, player_id, item_key, name, icon, rarity, kind, value, coin_bonus, opened_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                &drop_id,
                &wallet.player_id,
                rolled.key,
                rolled.name,
                rolled.icon,
                rolled.rarity,
                rolled.kind,
                rolled.value,
                rolled.coin_bonus,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;

        let drop = LootDrop {
            id: drop_id,
            item_key: rolled.key.to_string(),
            name: rolled.name.to_string(),
            icon: rolled.icon.to_string(),
            rarity: rolled.rarity.to_string(),
            kind: rolled.kind.to_string(),
            value: rolled.value.to_string(),
            coin_bonus: rolled.coin_bonus,
            opened_at: now,
        };
        let inv = Self::get_inventory_conn(&conn, &wallet.player_id, 40);
        let loadout = Self::get_loadout_conn(&conn, &wallet.player_id);
        Ok((wallet, drop, inv, loadout, crate_name))
    }

    pub fn equip_loot(
        &self,
        raw_player_id: &str,
        item_id: Option<&str>,
        clear_slot: Option<&str>,
    ) -> Result<PlayerLoadout, String> {
        let player_id: String = raw_player_id.trim().chars().take(64).collect();
        if player_id.is_empty() {
            return Err("playerId is required".to_string());
        }
        let conn = self.conn.lock().unwrap();
        let mut loadout = Self::get_loadout_conn(&conn, &player_id);

        if let Some(slot) = clear_slot {
            match slot {
                "cursor" => loadout.cursor_skin = None,
                "title" => loadout.title = None,
                "aura" => loadout.aura = None,
                "all" => {
                    loadout.cursor_skin = None;
                    loadout.title = None;
                    loadout.aura = None;
                }
                _ => return Err("Invalid slot to clear".to_string()),
            }
        } else if let Some(iid) = item_id {
            let found: Option<(String, String)> = conn
                .query_row(
                    "SELECT kind, value FROM loot_inventory WHERE id = ? AND player_id = ?",
                    params![iid, &player_id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let (kind, value) = found.ok_or_else(|| "You don't own that item!".to_string())?;
            match kind.as_str() {
                "cursor" => loadout.cursor_skin = Some(value),
                "title" => loadout.title = Some(value),
                "aura" => loadout.aura = Some(value),
                _ => return Err("That item can't be equipped — spend or flex it!".to_string()),
            }
        } else {
            return Err("Provide itemId or clear slot".to_string());
        }

        conn.execute(
            "INSERT OR REPLACE INTO player_loadout (player_id, cursor_skin, title, aura, updated_at) VALUES (?, ?, ?, ?, ?)",
            params![&player_id, &loadout.cursor_skin, &loadout.title, &loadout.aura, now_ms()],
        )
        .map_err(|e| e.to_string())?;
        Ok(loadout)
    }
}
