mod captcha;
mod db;
mod mcp;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{header, HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    env,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use subtle::ConstantTimeEq;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::captcha::TurnstileGate;
use crate::db::{bucket_label, now_ms, ResolutionSummary, TodoDB};
use crate::mcp::handle_mcp_payload;

const PEER_COLORS: [&str; 7] = [
    "#a78bfa", "#22d3ee", "#34d399", "#fbbf24", "#fb7185", "#60a5fa", "#f472b6",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivityEvent {
    id: String,
    action: String,
    source: String,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    card_id: Option<String>,
    at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PeerState {
    id: String,
    color: String,
    typing: bool,
    dragging_id: Option<String>,
    is_admin: bool,
}

struct AppState {
    db: TodoDB,
    captcha: TurnstileGate,
    tx: broadcast::Sender<String>,
    peers: Mutex<HashMap<String, PeerState>>,
    activity: Mutex<VecDeque<ActivityEvent>>,
    rate_buckets: Mutex<HashMap<String, (u32, i64)>>,
    admin_login_buckets: Mutex<HashMap<String, (u32, i64)>>,
    mcp_calls: AtomicU64,
    api_key_hash: Option<[u8; 32]>,
    admin_key_hash: Option<[u8; 32]>,
    auth_mode: String,
    write_limit_per_min: u32,
}

fn sha256_bytes(val: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(val.as_bytes());
    let out = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

fn sha256_hex(val: &str) -> String {
    sha256_bytes(val)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
}

impl AppState {
    fn client_ip(&self, headers: &HeaderMap) -> String {
        if let Some(ip) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
            return ip.trim().to_string();
        }
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = xff.split(',').next() {
                return first.trim().to_string();
            }
        }
        "local".to_string()
    }

    fn extract_api_key(&self, headers: &HeaderMap, query_key: Option<&str>) -> String {
        if let Some(auth) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
            if let Some(rest) = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")) {
                return rest.trim().to_string();
            }
        }
        if let Some(hk) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
            return hk.trim().to_string();
        }
        if let Some(qk) = query_key {
            return qk.trim().to_string();
        }
        String::new()
    }

    fn is_key_valid(&self, candidate: &str) -> bool {
        let Some(expected) = &self.api_key_hash else {
            return true;
        };
        if candidate.is_empty() {
            return false;
        }
        let hashed = sha256_bytes(candidate);
        bool::from(hashed.ct_eq(expected))
    }

    fn authorize(&self, headers: &HeaderMap, query_key: Option<&str>, is_write: bool) -> bool {
        if self.auth_mode == "none" {
            return true;
        }
        if self.auth_mode == "writes" && !is_write {
            return true;
        }
        self.is_key_valid(&self.extract_api_key(headers, query_key))
    }

    fn has_valid_api_key_header(&self, headers: &HeaderMap) -> bool {
        if self.api_key_hash.is_none() {
            return false;
        }
        let k = self.extract_api_key(headers, None);
        !k.is_empty() && self.is_key_valid(&k)
    }

    fn extract_admin_token(&self, headers: &HeaderMap, query_token: Option<&str>) -> String {
        if let Some(tok) = headers.get("x-admin-token").and_then(|v| v.to_str().ok()) {
            return tok.trim().to_string();
        }
        if let Some(qt) = query_token {
            return qt.trim().to_string();
        }
        String::new()
    }

    fn is_admin_token_valid(&self, tok: &str) -> bool {
        let Some(expected) = &self.admin_key_hash else {
            return false;
        };
        if tok.is_empty() {
            return false;
        }
        let hashed = sha256_bytes(tok);
        if bool::from(hashed.ct_eq(expected)) {
            return true;
        }
        self.db.has_admin_session(&sha256_hex(tok))
    }

    fn is_admin(&self, headers: &HeaderMap, query_token: Option<&str>) -> bool {
        let tok = self.extract_admin_token(headers, query_token);
        self.is_admin_token_valid(&tok)
    }

    fn check_admin_login_rate(&self, ip: &str) -> bool {
        let now = now_ms();
        let mut map = self.admin_login_buckets.lock().unwrap();
        let entry = map.entry(ip.to_string()).or_insert((0, now + 60_000));
        if now >= entry.1 {
            *entry = (1, now + 60_000);
            return true;
        }
        if entry.0 >= 10 {
            return false;
        }
        entry.0 += 1;
        true
    }

    fn check_rate_limit(&self, ip: &str) -> bool {
        let now = now_ms();
        let mut map = self.rate_buckets.lock().unwrap();
        let entry = map.entry(ip.to_string()).or_insert((0, now + 60_000));
        if now >= entry.1 {
            *entry = (1, now + 60_000);
            return true;
        }
        if entry.0 >= self.write_limit_per_min {
            return false;
        }
        entry.0 += 1;
        true
    }

    fn record_activity(
        &self,
        action: &str,
        source: &str,
        summary: String,
        card_id: Option<String>,
    ) -> ActivityEvent {
        let ev = ActivityEvent {
            id: Uuid::new_v4().to_string(),
            action: action.to_string(),
            source: source.to_string(),
            summary,
            card_id,
            at: now_ms(),
        };
        let mut log = self.activity.lock().unwrap();
        log.push_front(ev.clone());
        while log.len() > 25 {
            log.pop_back();
        }
        ev
    }

    fn record_resolution_activity(&self, res: &ResolutionSummary, source: &str) -> ActivityEvent {
        let label = bucket_label(&res.winning_bucket);
        let short: String = res.card_text.chars().take(28).collect();
        let summary = if res.winner_count > 0 {
            format!("🎰 \"{short}\" settled → {label} WON! Paid {} 🪙", res.total_paid)
        } else {
            format!("🎰 \"{short}\" settled → {label} (house swept {} 🪙)", res.total_pool)
        };
        self.record_activity("settled", source, summary, Some(res.card_id.clone()))
    }

    fn presence_json(&self) -> Value {
        let map = self.peers.lock().unwrap();
        let list: Vec<PeerState> = map.values().cloned().collect();
        let online = list.len();
        let typing_count = list.iter().filter(|p| p.typing).count();
        let dragging_ids: Vec<String> = list.iter().filter_map(|p| p.dragging_id.clone()).collect();
        let peers_slice: Vec<PeerState> = list.into_iter().take(12).collect();
        json!({
            "online": online,
            "peers": peers_slice,
            "typingCount": typing_count,
            "draggingIds": dragging_ids
        })
    }

    fn recent_activity(&self) -> Vec<ActivityEvent> {
        let log = self.activity.lock().unwrap();
        log.iter().take(15).cloned().collect()
    }

    fn broadcast_sync(
        &self,
        last_event: Option<&ActivityEvent>,
        resolution: Option<&ResolutionSummary>,
    ) {
        let presence = self.presence_json();
        let payload = json!({
            "type": "sync",
            "engine": "rust-axum-blazingly-fast",
            "todos": self.db.list(None, None),
            "markets": self.db.get_markets_map(),
            "leaderboard": self.db.get_leaderboard(10),
            "online": presence["online"],
            "peers": presence["peers"],
            "typingCount": presence["typingCount"],
            "draggingIds": presence["draggingIds"],
            "authMode": self.auth_mode,
            "mcpCalls": self.mcp_calls.load(Ordering::Relaxed),
            "activity": self.recent_activity(),
            "lastEvent": last_event,
            "resolution": resolution
        });
        let _ = self.tx.send(payload.to_string());
    }

    fn broadcast_presence(&self) {
        let presence = self.presence_json();
        let payload = json!({
            "type": "presence",
            "online": presence["online"],
            "peers": presence["peers"],
            "typingCount": presence["typingCount"],
            "draggingIds": presence["draggingIds"]
        });
        let _ = self.tx.send(payload.to_string());
    }
}

fn json_resp(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

fn captcha_required_resp(st: &AppState) -> Response {
    json_resp(
        StatusCode::PRECONDITION_REQUIRED,
        json!({
            "error": "Cloudflare Turnstile verification required!",
            "captchaRequired": true,
            "turnstileRequired": true,
            "siteKey": st.captcha.site_key
        }),
    )
}

#[derive(Deserialize)]
struct KeyQuery {
    api_key: Option<String>,
    admin_token: Option<String>,
}

#[derive(Deserialize)]
struct AdminLoginBody {
    password: Option<String>,
    key: Option<String>,
}

#[derive(Deserialize)]
struct AdminNukeBody {
    scope: Option<String>,
}

#[derive(Deserialize)]
struct WalletQuery {
    #[serde(rename = "playerId")]
    player_id: Option<String>,
    name: Option<String>,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct ClearQuery {
    completed: Option<String>,
    api_key: Option<String>,
}

#[derive(Deserialize)]
struct CreateTodoBody {
    text: Option<String>,
    status: Option<String>,
    priority: Option<String>,
}

#[derive(Deserialize)]
struct PatchTodoBody {
    status: Option<String>,
    priority: Option<String>,
    done: Option<bool>,
    text: Option<String>,
    position: Option<i64>,
}

#[derive(Deserialize)]
struct BailoutBody {
    #[serde(rename = "playerId")]
    player_id: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize)]
struct PlaceBetBody {
    #[serde(rename = "playerId")]
    player_id: Option<String>,
    #[serde(rename = "playerName")]
    player_name: Option<String>,
    #[serde(rename = "cardId")]
    card_id: Option<String>,
    bucket: Option<String>,
    amount: Option<i64>,
}

#[derive(Deserialize)]
struct VerifyCaptchaBody {
    token: Option<String>,
    #[serde(rename = "cf-turnstile-response")]
    cf_turnstile_response: Option<String>,
}

async fn health(State(st): State<Arc<AppState>>) -> Response {
    let online = st.peers.lock().unwrap().len();
    json_resp(
        StatusCode::OK,
        json!({
            "ok": true,
            "engine": "rust-axum-blazingly-fast",
            "poweredBy": "AI + 🦀 Rust",
            "turnstileEnabled": st.captcha.is_enabled(),
            "turnstileSiteKey": st.captcha.site_key,
            "adminConfigured": st.admin_key_hash.is_some(),
            "transport": "http+ws+mcp",
            "authMode": st.auth_mode,
            "online": online,
            "mcpCalls": st.mcp_calls.load(Ordering::Relaxed)
        }),
    )
}

async fn admin_status(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    json_resp(
        StatusCode::OK,
        json!({
            "adminConfigured": st.admin_key_hash.is_some(),
            "isAdmin": st.is_admin(&headers, None),
            "captchaBypass": st.is_admin(&headers, None)
        }),
    )
}

async fn admin_login(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AdminLoginBody>,
) -> Response {
    let Some(expected) = &st.admin_key_hash else {
        return json_resp(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({ "error": "ADMIN_KEY is not configured on the server" }),
        );
    };
    let ip = st.client_ip(&headers);
    if !st.check_admin_login_rate(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many admin login attempts — chill for 60s" }),
        );
    }
    let candidate = body
        .password
        .as_deref()
        .or(body.key.as_deref())
        .unwrap_or("")
        .trim();
    if candidate.is_empty() || !bool::from(sha256_bytes(candidate).ct_eq(expected)) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "Access denied — wrong Admin Password 💀" }),
        );
    }

    let raw_token = format!("adm_{}_{}", Uuid::new_v4(), Uuid::new_v4());
    let ttl_ms = 24 * 60 * 60 * 1000;
    match st.db.create_admin_session(&sha256_hex(&raw_token), ttl_ms) {
        Ok(expires_at) => json_resp(
            StatusCode::OK,
            json!({
                "ok": true,
                "adminToken": raw_token,
                "expiresAt": expires_at,
                "auraMode": true,
                "captchaBypass": true
            }),
        ),
        Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
    }
}

async fn admin_logout(State(st): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let tok = st.extract_admin_token(&headers, None);
    if !tok.is_empty() {
        st.db.delete_admin_session(&sha256_hex(&tok));
    }
    json_resp(StatusCode::OK, json!({ "ok": true }))
}

async fn admin_nuke(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<AdminNukeBody>,
) -> Response {
    if !st.is_admin(&headers, None) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "Admin Aura Mode required to drop nukes 👑" }),
        );
    }
    let scope = body.scope.as_deref().unwrap_or("done");
    match scope {
        "done" => match st.db.clear_completed() {
            Ok(deleted) => {
                let ev = st.record_activity(
                    "cleared",
                    "web",
                    format!("👑☢️ ADMIN nuked {deleted} finished card(s)"),
                    None,
                );
                st.broadcast_sync(Some(&ev), None);
                json_resp(StatusCode::OK, json!({ "ok": true, "scope": "done", "deleted": deleted }))
            }
            Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
        },
        "bets" => match st.db.nuke_bets() {
            Ok((refunded_bets, refunded_coins)) => {
                let ev = st.record_activity(
                    "cleared",
                    "web",
                    format!("👑🎰 ADMIN wiped markets & refunded {refunded_coins} 🪙 across {refunded_bets} open bet(s)"),
                    None,
                );
                st.broadcast_sync(Some(&ev), None);
                json_resp(
                    StatusCode::OK,
                    json!({
                        "ok": true,
                        "scope": "bets",
                        "refundedBets": refunded_bets,
                        "refundedCoins": refunded_coins
                    }),
                )
            }
            Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
        },
        "all" => match st.db.nuke_all() {
            Ok((cards_deleted, refunded_bets)) => {
                let ev = st.record_activity(
                    "cleared",
                    "web",
                    format!("👑☢️ ADMIN NUKED THE ENTIRE BOARD ({cards_deleted} cards, {refunded_bets} open bets refunded)"),
                    None,
                );
                st.broadcast_sync(Some(&ev), None);
                json_resp(
                    StatusCode::OK,
                    json!({
                        "ok": true,
                        "scope": "all",
                        "cardsDeleted": cards_deleted,
                        "refundedBets": refunded_bets
                    }),
                )
            }
            Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
        },
        _ => json_resp(
            StatusCode::BAD_REQUEST,
            json!({ "error": "Invalid nuke scope (use 'done', 'bets', or 'all')" }),
        ),
    }
}

async fn verify_auth(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
) -> Response {
    let provided = st.extract_api_key(&headers, q.api_key.as_deref());
    let valid = st.is_key_valid(&provided);
    json_resp(
        if valid { StatusCode::OK } else { StatusCode::UNAUTHORIZED },
        json!({
            "authMode": st.auth_mode,
            "hasKeyConfigured": st.auth_mode != "none",
            "keyProvided": !provided.is_empty(),
            "valid": valid
        }),
    )
}

async fn verify_captcha(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<VerifyCaptchaBody>,
) -> Response {
    let ip = st.client_ip(&headers);
    let token = body
        .token
        .as_deref()
        .or(body.cf_turnstile_response.as_deref())
        .unwrap_or("");
    match st.captcha.verify_turnstile(&ip, token).await {
        Ok((one_time, credits)) => json_resp(
            StatusCode::OK,
            json!({ "ok": true, "captchaToken": one_time, "credits": credits }),
        ),
        Err(msg) => json_resp(StatusCode::FORBIDDEN, json!({ "error": msg })),
    }
}

async fn list_todos(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), false) {
        return json_resp(StatusCode::UNAUTHORIZED, json!({ "error": "API key required" }));
    }
    let presence = st.presence_json();
    json_resp(
        StatusCode::OK,
        json!({
            "engine": "rust-axum-blazingly-fast",
            "todos": st.db.list(None, None),
            "markets": st.db.get_markets_map(),
            "leaderboard": st.db.get_leaderboard(10),
            "online": presence["online"],
            "peers": presence["peers"],
            "typingCount": presence["typingCount"],
            "draggingIds": presence["draggingIds"],
            "authMode": st.auth_mode,
            "mcpCalls": st.mcp_calls.load(Ordering::Relaxed),
            "activity": st.recent_activity()
        }),
    )
}

async fn create_todo(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
    Json(body): Json<CreateTodoBody>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "API key required to add tasks" }),
        );
    }
    let ip = st.client_ip(&headers);
    if !st.check_rate_limit(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many requests, slow down" }),
        );
    }

    // Adaptive Cloudflare Turnstile on adding tasks (bypassed when Admin or authenticated via API key)
    if !st.is_admin(&headers, None) && !st.has_valid_api_key_header(&headers) {
        let cap_tok = headers.get("x-captcha-token").and_then(|v| v.to_str().ok());
        if st.captcha.check_or_require(&ip, cap_tok).is_err() {
            if let Some(cf_tok) = headers.get("x-turnstile-token").and_then(|v| v.to_str().ok()) {
                if let Ok((one_time, _)) = st.captcha.verify_turnstile(&ip, cf_tok).await {
                    let _ = st.captcha.check_or_require(&ip, Some(&one_time));
                } else {
                    return captcha_required_resp(&st);
                }
            } else {
                return captcha_required_resp(&st);
            }
        }
    }

    let Some(text) = body.text.as_deref() else {
        return json_resp(StatusCode::BAD_REQUEST, json!({ "error": "Task text is required" }));
    };

    match st.db.add(text, body.status.as_deref(), body.priority.as_deref()) {
        Ok(todo) => {
            let short: String = todo.text.chars().take(48).collect();
            let ev = st.record_activity(
                "added",
                "web",
                format!("Spawned \"{short}\""),
                Some(todo.id.clone()),
            );
            st.broadcast_sync(Some(&ev), None);
            json_resp(StatusCode::CREATED, json!({ "todo": todo }))
        }
        Err(msg) => json_resp(StatusCode::BAD_REQUEST, json!({ "error": msg })),
    }
}

async fn clear_completed(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<ClearQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "API key required to clear tasks" }),
        );
    }
    let ip = st.client_ip(&headers);
    if !st.check_rate_limit(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many requests, slow down" }),
        );
    }
    if q.completed.as_deref() != Some("true") {
        return json_resp(
            StatusCode::BAD_REQUEST,
            json!({ "error": "Specify ?completed=true to clear completed tasks" }),
        );
    }

    if st.admin_key_hash.is_some() && !st.is_admin(&headers, None) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "Admin Aura Mode required to bulk nuke done cards 👑" }),
        );
    }

    // Adaptive Cloudflare Turnstile on bulk delete (bypassed when Admin or authenticated via API key)
    if !st.is_admin(&headers, None) && !st.has_valid_api_key_header(&headers) {
        let cap_tok = headers.get("x-captcha-token").and_then(|v| v.to_str().ok());
        if st.captcha.check_or_require(&ip, cap_tok).is_err() {
            if let Some(cf_tok) = headers.get("x-turnstile-token").and_then(|v| v.to_str().ok()) {
                if let Ok((one_time, _)) = st.captcha.verify_turnstile(&ip, cf_tok).await {
                    let _ = st.captcha.check_or_require(&ip, Some(&one_time));
                } else {
                    return captcha_required_resp(&st);
                }
            } else {
                return captcha_required_resp(&st);
            }
        }
    }

    match st.db.clear_completed() {
        Ok(deleted) => {
            if deleted > 0 {
                let s = if deleted == 1 { "" } else { "s" };
                let ev = st.record_activity(
                    "cleared",
                    "web",
                    format!("Nuked {deleted} finished card{s}"),
                    None,
                );
                st.broadcast_sync(Some(&ev), None);
            }
            json_resp(StatusCode::OK, json!({ "deleted": deleted }))
        }
        Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
    }
}

async fn patch_todo(
    State(st): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
    Json(body): Json<PatchTodoBody>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "API key required to edit tasks" }),
        );
    }
    let ip = st.client_ip(&headers);
    if !st.check_rate_limit(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many requests, slow down" }),
        );
    }
    let before = st.db.get(&id);
    match st.db.update(
        &id,
        body.status.as_deref(),
        body.priority.as_deref(),
        body.done,
        body.text.as_deref(),
        body.position,
    ) {
        Ok(Some((updated, resolution))) => {
            let moved = before.as_ref().map(|b| b.status != updated.status).unwrap_or(false);
            let short: String = updated.text.chars().take(42).collect();
            let mut ev = st.record_activity(
                if moved { "moved" } else { "edited" },
                "web",
                if moved {
                    format!("Slid \"{short}\" → {}", updated.status.replace('_', " "))
                } else {
                    format!("Rewrote \"{short}\"")
                },
                Some(updated.id.clone()),
            );
            if let Some(ref res) = resolution {
                ev = st.record_resolution_activity(res, "web");
            }
            st.broadcast_sync(Some(&ev), resolution.as_ref());
            json_resp(
                StatusCode::OK,
                json!({ "todo": updated, "resolution": resolution }),
            )
        }
        Ok(None) => json_resp(StatusCode::NOT_FOUND, json!({ "error": "Todo not found" })),
        Err(msg) => json_resp(StatusCode::BAD_REQUEST, json!({ "error": msg })),
    }
}

async fn delete_todo(
    State(st): State<Arc<AppState>>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "API key required to delete tasks" }),
        );
    }
    let ip = st.client_ip(&headers);
    if !st.check_rate_limit(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many requests, slow down" }),
        );
    }

    // Adaptive Cloudflare Turnstile on removing tasks (bypassed when Admin or authenticated via API key)
    if !st.is_admin(&headers, None) && !st.has_valid_api_key_header(&headers) {
        let cap_tok = headers.get("x-captcha-token").and_then(|v| v.to_str().ok());
        if st.captcha.check_or_require(&ip, cap_tok).is_err() {
            if let Some(cf_tok) = headers.get("x-turnstile-token").and_then(|v| v.to_str().ok()) {
                if let Ok((one_time, _)) = st.captcha.verify_turnstile(&ip, cf_tok).await {
                    let _ = st.captcha.check_or_require(&ip, Some(&one_time));
                } else {
                    return captcha_required_resp(&st);
                }
            } else {
                return captcha_required_resp(&st);
            }
        }
    }

    let before = st.db.get(&id);
    match st.db.delete(&id) {
        Ok((true, resolution)) => {
            let summary = match before {
                Some(b) => {
                    let short: String = b.text.chars().take(44).collect();
                    format!("Yeeted \"{short}\"")
                }
                None => "Yeeted a card into the void".to_string(),
            };
            let mut ev = st.record_activity("deleted", "web", summary, Some(id.clone()));
            if let Some(ref res) = resolution {
                ev = st.record_resolution_activity(res, "web");
            }
            st.broadcast_sync(Some(&ev), resolution.as_ref());
            json_resp(
                StatusCode::OK,
                json!({ "deleted": id, "resolution": resolution }),
            )
        }
        Ok((false, _)) => json_resp(StatusCode::NOT_FOUND, json!({ "error": "Todo not found" })),
        Err(e) => json_resp(StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": e })),
    }
}

async fn get_wallet(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<WalletQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), false) {
        return json_resp(StatusCode::UNAUTHORIZED, json!({ "error": "API key required" }));
    }
    let Some(pid) = q.player_id.as_deref() else {
        return json_resp(StatusCode::BAD_REQUEST, json!({ "error": "playerId required" }));
    };
    match st.db.get_or_create_wallet(pid, q.name.as_deref()) {
        Ok(wallet) => {
            let bets = st.db.get_player_bets(&wallet.player_id, 20);
            let leaderboard = st.db.get_leaderboard(10);
            json_resp(
                StatusCode::OK,
                json!({ "wallet": wallet, "bets": bets, "leaderboard": leaderboard }),
            )
        }
        Err(msg) => json_resp(StatusCode::BAD_REQUEST, json!({ "error": msg })),
    }
}

async fn bailout_wallet(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
    Json(body): Json<BailoutBody>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(StatusCode::UNAUTHORIZED, json!({ "error": "API key required" }));
    }
    let Some(pid) = body.player_id.as_deref() else {
        return json_resp(StatusCode::BAD_REQUEST, json!({ "error": "playerId required" }));
    };
    match st.db.bailout_wallet(pid, body.name.as_deref()) {
        Ok((wallet, granted)) => {
            let ev = st.record_activity(
                "bet",
                "web",
                format!("💸 {} begged for a +{granted} 🪙 bailout", wallet.name),
                None,
            );
            st.broadcast_sync(Some(&ev), None);
            json_resp(StatusCode::OK, json!({ "wallet": wallet, "granted": granted }))
        }
        Err(msg) => json_resp(StatusCode::BAD_REQUEST, json!({ "error": msg })),
    }
}

async fn get_leaderboard(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), false) {
        return json_resp(StatusCode::UNAUTHORIZED, json!({ "error": "API key required" }));
    }
    json_resp(
        StatusCode::OK,
        json!({
            "leaderboard": st.db.get_leaderboard(10),
            "markets": st.db.get_markets_map()
        }),
    )
}

async fn place_bet(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
    Json(body): Json<PlaceBetBody>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), true) {
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "API key required to gamble" }),
        );
    }
    let ip = st.client_ip(&headers);
    if !st.check_rate_limit(&ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Chill out degenerate, rate limit hit" }),
        );
    }
    let (Some(pid), Some(cid), Some(bucket), Some(amount)) = (
        body.player_id.as_deref(),
        body.card_id.as_deref(),
        body.bucket.as_deref(),
        body.amount,
    ) else {
        return json_resp(
            StatusCode::BAD_REQUEST,
            json!({ "error": "Required: playerId, cardId, bucket, amount" }),
        );
    };

    match st
        .db
        .place_bet(pid, body.player_name.as_deref(), cid, bucket, amount)
    {
        Ok((bet, wallet)) => {
            let short: String = bet.card_text.chars().take(26).collect();
            let ev = st.record_activity(
                "bet",
                "web",
                format!(
                    "🎰 {} bet {} 🪙 on {} for \"{short}\"",
                    wallet.name,
                    bet.amount,
                    bucket_label(&bet.bucket)
                ),
                Some(bet.card_id.clone()),
            );
            st.broadcast_sync(Some(&ev), None);
            let bets = st.db.get_player_bets(&wallet.player_id, 20);
            json_resp(
                StatusCode::CREATED,
                json!({ "bet": bet, "wallet": wallet, "bets": bets }),
            )
        }
        Err(msg) => json_resp(StatusCode::BAD_REQUEST, json!({ "error": msg })),
    }
}

async fn mcp_get(State(st): State<Arc<AppState>>) -> Response {
    json_resp(
        StatusCode::OK,
        json!({
            "name": "todo-kanban-rust-mcp",
            "engine": "rust-axum-blazingly-fast",
            "transport": "streamable-http",
            "endpoint": "/mcp",
            "authMode": st.auth_mode,
            "hint": "Send JSON-RPC 2.0 POST requests to /mcp"
        }),
    )
}

async fn mcp_delete() -> Response {
    StatusCode::NO_CONTENT.into_response()
}

async fn mcp_post(
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
    body_bytes: axum::body::Bytes,
) -> Response {
    let payload: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => {
            return json_resp(
                StatusCode::BAD_REQUEST,
                json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": "Parse error" } }),
            );
        }
    };

    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let tool_name = payload
        .get("params")
        .and_then(|p| p.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tool_args = payload
        .get("params")
        .and_then(|p| p.get("arguments"))
        .and_then(|v| v.as_object());

    let is_write = method == "tools/call" && tool_name != "list_todos" && tool_name != "get_leaderboard";
    if !st.authorize(&headers, q.api_key.as_deref(), is_write) {
        let id = payload.get("id").cloned().unwrap_or(Value::Null);
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32001, "message": "Unauthorized: valid API key required" } }),
        );
    }

    let ip = st.client_ip(&headers);
    if is_write && !st.check_rate_limit(&ip) {
        let id = payload.get("id").cloned().unwrap_or(Value::Null);
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": "Rate limit exceeded" } }),
        );
    }

    if method == "tools/call" {
        st.mcp_calls.fetch_add(1, Ordering::Relaxed);
    }

    let (status_code, resp_body) = handle_mcp_payload(&payload, &st.db, |info| {
        let mut action = "edited";
        let mut summary = info
            .custom_summary
            .unwrap_or_else(|| format!("Robot ran {tool_name}"));
        let card_id = info.card_id.or_else(|| {
            tool_args
                .and_then(|a| a.get("id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

        if summary.starts_with("🎰") {
            action = "bet";
        } else if tool_name == "add_todo" {
            action = "added";
            let txt: String = tool_args
                .and_then(|a| a.get("text"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(48)
                .collect();
            summary = format!("Robot spawned \"{txt}\"");
        } else if tool_name == "move_todo" || tool_name == "toggle_todo" {
            action = "moved";
            let dest = tool_args
                .and_then(|a| a.get("status"))
                .and_then(|v| v.as_str())
                .map(|s| s.replace('_', " "))
                .unwrap_or_else(|| "updated".to_string());
            summary = format!("Robot shoved a card → {dest}");
        } else if tool_name == "delete_todo" {
            action = "deleted";
            summary = "Robot yeeted a card into the void".to_string();
        } else if tool_name == "clear_completed" {
            action = "cleared";
            summary = "Robot nuked the Done column".to_string();
        }

        let mut ev = st.record_activity(action, "mcp", summary, card_id);
        if let Some(ref res) = info.resolution {
            ev = st.record_resolution_activity(res, "mcp");
        }
        st.broadcast_sync(Some(&ev), info.resolution.as_ref());
    });

    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::OK);
    match resp_body {
        Some(b) => json_resp(status, b),
        None => status.into_response(),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(st): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(q): Query<KeyQuery>,
) -> Response {
    if !st.authorize(&headers, q.api_key.as_deref(), false) {
        return json_resp(StatusCode::UNAUTHORIZED, json!({ "error": "API key required" }));
    }
    let is_admin = st.is_admin(&headers, q.admin_token.as_deref());
    ws.on_upgrade(move |socket| handle_socket(socket, st, is_admin))
}

async fn handle_socket(socket: WebSocket, st: Arc<AppState>, is_admin: bool) {
    let peer_id = Uuid::new_v4().to_string()[..8].to_string();
    let color = {
        let mut peers = st.peers.lock().unwrap();
        let color = if is_admin {
            "#fbbf24".to_string()
        } else {
            PEER_COLORS[peers.len() % PEER_COLORS.len()].to_string()
        };
        peers.insert(
            peer_id.clone(),
            PeerState {
                id: peer_id.clone(),
                color: color.clone(),
                typing: false,
                dragging_id: None,
                is_admin,
            },
        );
        color
    };

    let mut rx = st.tx.subscribe();
    let (mut sender, mut receiver) = socket.split();

    let welcome = json!({
        "type": "welcome",
        "peerId": peer_id,
        "color": color,
        "engine": "rust-axum-blazingly-fast"
    });
    if sender.send(Message::Text(welcome.to_string().into())).await.is_err() {
        st.peers.lock().unwrap().remove(&peer_id);
        st.broadcast_presence();
        return;
    }

    st.broadcast_sync(None, None);

    let (direct_tx, mut direct_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(msg) = rx.recv() => {
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Some(direct) = direct_rx.recv() => {
                    if sender.send(Message::Text(direct.into())).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    let st_recv = Arc::clone(&st);
    let pid_recv = peer_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                let s = text.as_str();
                if s == "ping" {
                    let _ = direct_tx.send("pong".to_string());
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<Value>(s) {
                    match val.get("type").and_then(|v| v.as_str()) {
                        Some("ping") => {
                            let t = val.get("t").and_then(|v| v.as_i64()).unwrap_or_else(now_ms);
                            let _ = direct_tx.send(json!({ "type": "pong", "t": t }).to_string());
                        }
                        Some("typing") => {
                            let active = val.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
                            let mut changed = false;
                            if let Some(p) = st_recv.peers.lock().unwrap().get_mut(&pid_recv) {
                                if p.typing != active {
                                    p.typing = active;
                                    changed = true;
                                }
                            }
                            if changed {
                                st_recv.broadcast_presence();
                            }
                        }
                        Some("dragging") => {
                            let card_id = val.get("cardId").and_then(|v| v.as_str()).map(|x| x.to_string());
                            let mut changed = false;
                            if let Some(p) = st_recv.peers.lock().unwrap().get_mut(&pid_recv) {
                                if p.dragging_id != card_id {
                                    p.dragging_id = card_id;
                                    changed = true;
                                }
                            }
                            if changed {
                                st_recv.broadcast_presence();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    st.peers.lock().unwrap().remove(&peer_id);
    st.broadcast_presence();
}

#[tokio::main]
async fn main() {
    let port: u16 = env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3000);
    let db_path = env::var("DB_PATH").unwrap_or_else(|_| "/data/todos.db".to_string());
    let max_todos: i64 = env::var("MAX_TODOS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);
    let write_limit_per_min: u32 = env::var("WRITE_LIMIT_PER_MIN")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(90);
    let api_key = env::var("API_KEY").unwrap_or_default().trim().to_string();
    let auth_mode = if api_key.is_empty() {
        "none".to_string()
    } else if env::var("API_KEY_MODE").ok().as_deref() == Some("all") {
        "all".to_string()
    } else {
        "writes".to_string()
    };
    let api_key_hash = if api_key.is_empty() {
        None
    } else {
        Some(sha256_bytes(&api_key))
    };

    let admin_key = env::var("ADMIN_KEY").unwrap_or_default().trim().to_string();
    let admin_key_hash = if admin_key.is_empty() {
        None
    } else {
        Some(sha256_bytes(&admin_key))
    };

    let turnstile_site_key = env::var("TURNSTILE_SITE_KEY").unwrap_or_default();
    let turnstile_secret_key = env::var("TURNSTILE_SECRET_KEY").unwrap_or_default();
    let turnstile_enabled = !turnstile_secret_key.trim().is_empty();

    let db = TodoDB::open(&db_path, max_todos).expect("Failed to open SQLite DB");
    let (tx, _) = broadcast::channel::<String>(256);

    let state = Arc::new(AppState {
        db,
        captcha: TurnstileGate::new(turnstile_site_key, turnstile_secret_key),
        tx,
        peers: Mutex::new(HashMap::new()),
        activity: Mutex::new(VecDeque::new()),
        rate_buckets: Mutex::new(HashMap::new()),
        admin_login_buckets: Mutex::new(HashMap::new()),
        mcp_calls: AtomicU64::new(0),
        api_key_hash,
        admin_key_hash,
        auth_mode: auth_mode.clone(),
        write_limit_per_min,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
        .expose_headers(Any);

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/verify", get(verify_auth))
        .route("/api/admin/status", get(admin_status))
        .route("/api/admin/login", post(admin_login))
        .route("/api/admin/logout", post(admin_logout))
        .route("/api/admin/nuke", post(admin_nuke))
        .route("/api/captcha/verify", post(verify_captcha))
        .route(
            "/api/todos",
            get(list_todos).post(create_todo).delete(clear_completed),
        )
        .route("/api/todos/{id}", patch(patch_todo).delete(delete_todo))
        .route("/api/wallet", get(get_wallet))
        .route("/api/wallet/bailout", post(bailout_wallet))
        .route("/api/leaderboard", get(get_leaderboard))
        .route("/api/bets", post(place_bet))
        .route("/mcp", get(mcp_get).post(mcp_post).delete(mcp_delete))
        .route("/ws", get(ws_handler))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!(
        "🦀 Blazingly Fast Rust Kanban + Gambling + Turnstile({turnstile_enabled}) + WS + MCP listening on http://{addr} (db: {db_path}, auth: {auth_mode})"
    );
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind port");
    axum::serve(listener, app).await.expect("Server error");
}
