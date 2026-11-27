use serde_json::{json, Value};

use crate::db::{bucket_label, is_valid_bucket, ResolutionSummary, TodoDB, MAX_TEXT_LENGTH};

const PROTOCOL_VERSION: &str = "2025-03-26";

pub struct McpMutationInfo {
    pub resolution: Option<ResolutionSummary>,
    pub custom_summary: Option<String>,
    pub card_id: Option<String>,
}

fn tools_schema() -> Value {
    json!([
        {
            "name": "list_todos",
            "description": "List tasks on the shared Kanban & todo board along with active prediction pools.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": {
                        "type": "string",
                        "enum": ["all", "todo", "in_progress", "done", "active", "completed"],
                        "description": "Filter by Kanban column or completion state (default: all)."
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["low", "medium", "high"],
                        "description": "Optional priority filter."
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "add_todo",
            "description": "Add a new task card to the shared Kanban board.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_LENGTH },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "done"] },
                    "priority": { "type": "string", "enum": ["low", "medium", "high"] }
                },
                "required": ["text"],
                "additionalProperties": false
            }
        },
        {
            "name": "move_todo",
            "description": "Move a task card to another Kanban column (moving to done settles prediction bets!), update priority, or edit text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "done"] },
                    "priority": { "type": "string", "enum": ["low", "medium", "high"] },
                    "text": { "type": "string", "minLength": 1, "maxLength": MAX_TEXT_LENGTH },
                    "done": { "type": "boolean" }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "toggle_todo",
            "description": "Toggle a task between done and todo (or set done explicitly).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "done": { "type": "boolean" }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "delete_todo",
            "description": "Delete a single task card from the shared board by ID (restricted to tasks originally created by MCP).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        },
        {
            "name": "clear_completed",
            "description": "Delete completed tasks in the Done column that were originally created by MCP.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "place_bet",
            "description": "Place an Aura Coin prediction bet on how fast a card will be completed (speedrun=<5m, today=5m-1h, eventually=1h-24h, never=24h+/deleted).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "cardId": { "type": "string" },
                    "bucket": { "type": "string", "enum": ["speedrun", "today", "eventually", "never"] },
                    "amount": { "type": "integer", "minimum": 10 },
                    "playerName": { "type": "string" }
                },
                "required": ["cardId", "bucket", "amount"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_leaderboard",
            "description": "Get the top prediction gamblers leaderboard and open card betting markets.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }
    ])
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_success(data: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&data).unwrap_or_default() }]
    })
}

fn tool_failure(message: &str) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": message }]
    })
}

pub fn handle_mcp_payload<F>(payload: &Value, db: &TodoDB, mut on_mutate: F) -> (u16, Option<Value>)
where
    F: FnMut(McpMutationInfo),
{
    let Some(obj) = payload.as_object() else {
        return (400, Some(rpc_error(Value::Null, -32600, "Invalid JSON-RPC request")));
    };
    let id = obj.get("id").cloned().unwrap_or(Value::Null);
    if obj.get("jsonrpc").and_then(|v| v.as_str()) != Some("2.0") {
        return (400, Some(rpc_error(id, -32600, "Invalid JSON-RPC 2.0 payload")));
    }
    let Some(method) = obj.get("method").and_then(|v| v.as_str()) else {
        return (400, Some(rpc_error(id, -32600, "Missing method")));
    };

    if method == "notifications/initialized" || method.starts_with("notifications/") {
        return (202, None);
    }

    match method {
        "initialize" => (
            200,
            Some(rpc_result(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "todo-kanban-rust-mcp",
                        "version": "2.0.0"
                    }
                }),
            )),
        ),
        "ping" => (200, Some(rpc_result(id, json!({})))),
        "tools/list" => (200, Some(rpc_result(id, json!({ "tools": tools_schema() })))),
        "tools/call" => {
            let params = obj.get("params").and_then(|v| v.as_object());
            let name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let args = params
                .and_then(|p| p.get("arguments"))
                .and_then(|v| v.as_object());

            match name {
                "list_todos" => {
                    let status = args
                        .and_then(|a| a.get("status").or_else(|| a.get("filter")))
                        .and_then(|v| v.as_str());
                    let priority = args.and_then(|a| a.get("priority")).and_then(|v| v.as_str());
                    let todos = db.list(status, priority);
                    let markets = db.get_markets_map();
                    (
                        200,
                        Some(rpc_result(
                            id,
                            tool_success(json!({
                                "count": todos.len(),
                                "todos": todos,
                                "markets": markets
                            })),
                        )),
                    )
                }
                "add_todo" => {
                    let Some(text) = args.and_then(|a| a.get("text")).and_then(|v| v.as_str()) else {
                        return (200, Some(rpc_result(id, tool_failure("Missing string argument: text"))));
                    };
                    let status = args.and_then(|a| a.get("status")).and_then(|v| v.as_str());
                    let priority = args.and_then(|a| a.get("priority")).and_then(|v| v.as_str());
                    match db.add_with_source(text, status, priority, "mcp") {
                        Ok(todo) => {
                            on_mutate(McpMutationInfo {
                                resolution: None,
                                custom_summary: None,
                                card_id: Some(todo.id.clone()),
                            });
                            (200, Some(rpc_result(id, tool_success(json!({ "todo": todo })))))
                        }
                        Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                    }
                }
                "move_todo" => {
                    let Some(card_id) = args.and_then(|a| a.get("id")).and_then(|v| v.as_str()) else {
                        return (200, Some(rpc_result(id, tool_failure("Missing string argument: id"))));
                    };
                    let status = args.and_then(|a| a.get("status")).and_then(|v| v.as_str());
                    let priority = args.and_then(|a| a.get("priority")).and_then(|v| v.as_str());
                    let text = args.and_then(|a| a.get("text")).and_then(|v| v.as_str());
                    let done = args.and_then(|a| a.get("done")).and_then(|v| v.as_bool());
                    match db.update(card_id, status, priority, done, text, None) {
                        Ok(Some((todo, resolution))) => {
                            on_mutate(McpMutationInfo {
                                resolution: resolution.clone(),
                                custom_summary: None,
                                card_id: Some(todo.id.clone()),
                            });
                            (
                                200,
                                Some(rpc_result(
                                    id,
                                    tool_success(json!({ "todo": todo, "resolution": resolution })),
                                )),
                            )
                        }
                        Ok(None) => (
                            200,
                            Some(rpc_result(id, tool_failure(&format!("Todo not found: {card_id}")))),
                        ),
                        Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                    }
                }
                "toggle_todo" => {
                    let Some(card_id) = args.and_then(|a| a.get("id")).and_then(|v| v.as_str()) else {
                        return (200, Some(rpc_result(id, tool_failure("Missing string argument: id"))));
                    };
                    let Some(existing) = db.get(card_id) else {
                        return (
                            200,
                            Some(rpc_result(id, tool_failure(&format!("Todo not found: {card_id}")))),
                        );
                    };
                    let next_done = args
                        .and_then(|a| a.get("done"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(!existing.done);
                    match db.update(card_id, None, None, Some(next_done), None, None) {
                        Ok(Some((todo, resolution))) => {
                            on_mutate(McpMutationInfo {
                                resolution: resolution.clone(),
                                custom_summary: None,
                                card_id: Some(todo.id.clone()),
                            });
                            (
                                200,
                                Some(rpc_result(
                                    id,
                                    tool_success(json!({ "todo": todo, "resolution": resolution })),
                                )),
                            )
                        }
                        Ok(None) => (200, Some(rpc_result(id, tool_failure("Todo not found")))),
                        Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                    }
                }
                "delete_todo" => {
                    let Some(card_id) = args.and_then(|a| a.get("id")).and_then(|v| v.as_str()) else {
                        return (200, Some(rpc_result(id, tool_failure("Missing string argument: id"))));
                    };
                    let Some(existing) = db.get(card_id) else {
                        return (
                            200,
                            Some(rpc_result(id, tool_failure(&format!("Todo not found: {card_id}")))),
                        );
                    };
                    if existing.created_by != "mcp" {
                        return (
                            200,
                            Some(rpc_result(
                                id,
                                tool_failure("Permission denied: MCP can only delete tasks that MCP originally created."),
                            )),
                        );
                    }
                    match db.delete(card_id) {
                        Ok((true, resolution)) => {
                            on_mutate(McpMutationInfo {
                                resolution: resolution.clone(),
                                custom_summary: None,
                                card_id: Some(card_id.to_string()),
                            });
                            (
                                200,
                                Some(rpc_result(
                                    id,
                                    tool_success(json!({ "deleted": card_id, "resolution": resolution })),
                                )),
                            )
                        }
                        Ok((false, _)) => (
                            200,
                            Some(rpc_result(id, tool_failure(&format!("Todo not found: {card_id}")))),
                        ),
                        Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                    }
                }
                "clear_completed" => match db.clear_completed_by_source("mcp") {
                    Ok(count) => {
                        if count > 0 {
                            on_mutate(McpMutationInfo {
                                resolution: None,
                                custom_summary: Some(format!("Robot cleared {count} MCP done card(s)")),
                                card_id: None,
                            });
                        }
                        (200, Some(rpc_result(id, tool_success(json!({ "deleted": count })))))
                    }
                    Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                },
                "place_bet" => {
                    let card_id = args.and_then(|a| a.get("cardId")).and_then(|v| v.as_str());
                    let bucket = args.and_then(|a| a.get("bucket")).and_then(|v| v.as_str());
                    let amount = args.and_then(|a| a.get("amount")).and_then(|v| v.as_i64());
                    let (Some(card_id), Some(bucket), Some(amount)) = (card_id, bucket, amount) else {
                        return (
                            200,
                            Some(rpc_result(
                                id,
                                tool_failure("Required: cardId (string), bucket, amount (integer)"),
                            )),
                        );
                    };
                    if !is_valid_bucket(bucket) {
                        return (200, Some(rpc_result(id, tool_failure("Invalid bucket"))));
                    }
                    let player_name = args
                        .and_then(|a| a.get("playerName"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .unwrap_or("MCP Degen 🤖");
                    let slug: String = player_name
                        .to_lowercase()
                        .chars()
                        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                        .take(24)
                        .collect();
                    let player_id = format!("mcp-{}", slug.trim_matches('-'));
                    match db.place_bet(&player_id, Some(player_name), card_id, bucket, amount) {
                        Ok((bet, wallet)) => {
                            let summary = format!(
                                "🎰 {} bet {} 🪙 on {}",
                                wallet.name,
                                bet.amount,
                                bucket_label(&bet.bucket)
                            );
                            on_mutate(McpMutationInfo {
                                resolution: None,
                                custom_summary: Some(summary),
                                card_id: Some(card_id.to_string()),
                            });
                            (
                                200,
                                Some(rpc_result(
                                    id,
                                    tool_success(json!({ "bet": bet, "wallet": wallet })),
                                )),
                            )
                        }
                        Err(e) => (200, Some(rpc_result(id, tool_failure(&e)))),
                    }
                }
                "get_leaderboard" => (
                    200,
                    Some(rpc_result(
                        id,
                        tool_success(json!({
                            "leaderboard": db.get_leaderboard(10),
                            "markets": db.get_markets_map()
                        })),
                    )),
                ),
                _ => (200, Some(rpc_error(id, -32601, &format!("Unknown tool: {name}")))),
            }
        }
        _ => (200, Some(rpc_error(id, -32601, &format!("Method not found: {method}")))),
    }
}
