//! Xaman Ek session API handlers.
//!
//! Routes:
//!   GET  /api/xaman/sessions              list active sessions for current user
//!   POST /api/xaman/sessions              create or update session
//!   GET  /api/xaman/sessions/:id          get single session
//!   POST /api/xaman/sessions/:id/message  append a message turn + call xaman_ek agent
//!   POST /api/xaman/sessions/:id/complete mark session completed
//!   DELETE /api/xaman/sessions/:id        abandon session

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use fermi_auth::AuthPrincipal;
use crate::AppState;

// ─── List sessions ────────────────────────────────────────────────────────────

pub async fn list_xaman_sessions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let rows = sqlx::query(
        "SELECT session_id, session_type, title, status, page_context,
                in_progress, created_at, last_active_at
         FROM xaman_sessions
         WHERE user_id = $1 AND status = 'active'
         ORDER BY last_active_at DESC
         LIMIT 10",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let sessions: Vec<Value> = rows.iter().map(|r| json!({
        "session_id":    r.try_get::<Uuid,_>("session_id").unwrap().to_string(),
        "session_type":  r.try_get::<String,_>("session_type").unwrap_or_default(),
        "title":         r.try_get::<Option<String>,_>("title").unwrap_or(None),
        "status":        r.try_get::<String,_>("status").unwrap_or_default(),
        "page_context":  r.try_get::<Option<String>,_>("page_context").unwrap_or(None),
        "in_progress":   r.try_get::<Value,_>("in_progress").unwrap_or(json!({})),
        "created_at":    r.try_get::<chrono::DateTime<Utc>,_>("created_at").map(|t| t.to_rfc3339()).unwrap_or_default(),
        "last_active_at":r.try_get::<chrono::DateTime<Utc>,_>("last_active_at").map(|t| t.to_rfc3339()).unwrap_or_default(),
    })).collect();

    Ok(Json(json!({ "sessions": sessions })))
}

// ─── Create session ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub session_type: Option<String>,
    pub title: Option<String>,
    pub page_context: Option<String>,
    pub in_progress: Option<Value>,
}

pub async fn create_xaman_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<CreateSessionRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let session_id = Uuid::new_v4();
    let session_type = body.session_type.as_deref().unwrap_or("free");
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO xaman_sessions
         (session_id, user_id, session_type, title, page_context, in_progress,
          messages, status, created_at, last_active_at)
         VALUES ($1,$2,$3,$4,$5,$6,'[]'::jsonb,'active',$7,$7)",
    )
    .bind(session_id)
    .bind(&user_id)
    .bind(session_type)
    .bind(&body.title)
    .bind(&body.page_context)
    .bind(body.in_progress.as_ref().unwrap_or(&json!({})))
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "session_id": session_id,
        "session_type": session_type,
        "status": "active",
        "created_at": now.to_rfc3339(),
    })))
}

// ─── Get session ──────────────────────────────────────────────────────────────

pub async fn get_xaman_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    let row = sqlx::query(
        "SELECT session_id, user_id, session_type, title, status, page_context,
                in_progress, messages, created_at, last_active_at
         FROM xaman_sessions WHERE session_id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Session not found".into()))?;

    Ok(Json(json!({
        "session_id":    row.try_get::<Uuid,_>("session_id").unwrap().to_string(),
        "session_type":  row.try_get::<String,_>("session_type").unwrap_or_default(),
        "title":         row.try_get::<Option<String>,_>("title").unwrap_or(None),
        "status":        row.try_get::<String,_>("status").unwrap_or_default(),
        "page_context":  row.try_get::<Option<String>,_>("page_context").unwrap_or(None),
        "in_progress":   row.try_get::<Value,_>("in_progress").unwrap_or(json!({})),
        "messages":      row.try_get::<Value,_>("messages").unwrap_or(json!([])),
        "created_at":    row.try_get::<chrono::DateTime<Utc>,_>("created_at").map(|t| t.to_rfc3339()).unwrap_or_default(),
        "last_active_at":row.try_get::<chrono::DateTime<Utc>,_>("last_active_at").map(|t| t.to_rfc3339()).unwrap_or_default(),
    })))
}

// ─── Send message ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MessageRequest {
    pub message: String,
    pub page_context: Option<String>,
    pub in_progress: Option<Value>,
}

pub async fn xaman_session_message_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
    Json(body): Json<MessageRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Load full session state
    let row = sqlx::query(
        "SELECT messages, title, session_type, in_progress, page_context FROM xaman_sessions
         WHERE session_id = $1 AND user_id = $2 AND status = 'active'",
    )
    .bind(session_id)
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Active session not found".into()))?;

    let mut messages: Vec<Value> = row
        .try_get::<Value,_>("messages")
        .unwrap_or(json!([]))
        .as_array()
        .cloned()
        .unwrap_or_default();
    let existing_title: Option<String> = row.try_get("title").unwrap_or(None);
    let session_type: String = row.try_get("session_type").unwrap_or_else(|_| "free".into());
    let mut in_progress: Value = row.try_get::<Value,_>("in_progress").unwrap_or(json!({}));
    let stored_page_ctx: Option<String> = row.try_get("page_context").unwrap_or(None);

    // Merge any client-supplied in_progress patch first
    if let Some(ref patch) = body.in_progress {
        merge_json(&mut in_progress, patch);
    }

    // Build the session-context prefix injected into the query so xamanEK
    // knows it is in a session and what has been built so far.
    let page_ctx = body.page_context.as_deref()
        .or(stored_page_ctx.as_deref())
        .unwrap_or("");

    let in_progress_str = serde_json::to_string_pretty(&in_progress)
        .unwrap_or_else(|_| "{}".to_string());

    let session_prefix = format!(
        "[SESSION type={} id={}]\n[IN_PROGRESS]\n{}\n[/IN_PROGRESS]\n[PAGE] {} [/PAGE]\n\n",
        session_type,
        session_id,
        in_progress_str,
        page_ctx,
    );

    let full_query = format!("{}{}", session_prefix, body.message);

    // Call xaman_ek agent
    let raw_response = call_xaman_ek(&state, &user_id, &full_query)
        .await
        .unwrap_or_else(|e| format!("I encountered an error: {}. Please try again.", e));

    // Parse and strip any __UPDATE__ block from the response
    let (display_response, update_patch) = extract_update_block(&raw_response);

    // Apply update patch to in_progress if present
    if let Some(ref patch) = update_patch {
        merge_json(&mut in_progress, patch);
    }

    // Append turn to messages (keep last 40 = 20 turns)
    let now = Utc::now();
    messages.push(json!({
        "role": "user",
        "content": body.message,
        "timestamp": now.to_rfc3339(),
    }));
    messages.push(json!({
        "role": "assistant",
        "content": display_response.clone(),
        "timestamp": now.to_rfc3339(),
    }));
    if messages.len() > 40 {
        messages = messages.into_iter().rev().take(40).rev().collect();
    }

    // Auto-title from first user message if not yet set
    let new_title = if existing_title.is_none() {
        Some(body.message.chars().take(60).collect::<String>())
    } else {
        None
    };

    // Check if session is ready to create (status field in in_progress)
    let ready_to_create = in_progress.get("status")
        .and_then(|v| v.as_str())
        .map(|s| s == "ready_to_create")
        .unwrap_or(false);

    // Persist
    let messages_json = Value::Array(messages);
    sqlx::query(
        "UPDATE xaman_sessions SET
            messages = $1,
            last_active_at = $2,
            title = COALESCE($3, title),
            page_context = COALESCE($4, page_context),
            in_progress = $5
         WHERE session_id = $6",
    )
    .bind(&messages_json)
    .bind(now)
    .bind(&new_title)
    .bind(&body.page_context)
    .bind(&in_progress)
    .bind(session_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "session_id": session_id,
        "response": display_response,
        "title": new_title.or(existing_title),
        "in_progress": in_progress,
        "ready_to_create": ready_to_create,
        "session_type": session_type,
    })))
}

// ─── Complete / abandon session ───────────────────────────────────────────────

pub async fn complete_xaman_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    sqlx::query(
        "UPDATE xaman_sessions SET status = 'completed', last_active_at = NOW()
         WHERE session_id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "completed" })))
}

pub async fn abandon_xaman_session_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(session_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    sqlx::query(
        "UPDATE xaman_sessions SET status = 'abandoned', last_active_at = NOW()
         WHERE session_id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "status": "abandoned" })))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract and parse a `__UPDATE__ { ... } __END_UPDATE__` block from xamanEK's
/// response. Returns (display_text, Option<patch_json>).
/// The update block is stripped from the display text so it never reaches the UI.
fn extract_update_block(raw: &str) -> (String, Option<Value>) {
    const START: &str = "__UPDATE__";
    const END: &str = "__END_UPDATE__";

    if let (Some(start_idx), Some(end_idx)) = (raw.find(START), raw.find(END)) {
        if start_idx < end_idx {
            let json_str = raw[start_idx + START.len()..end_idx].trim();
            let patch: Option<Value> = serde_json::from_str(json_str).ok();

            // Build display text with the block removed and whitespace cleaned up
            let before = raw[..start_idx].trim_end();
            let after = raw[end_idx + END.len()..].trim_start();
            let display = if after.is_empty() {
                before.to_string()
            } else {
                format!("{}\n\n{}", before, after)
            };
            return (display, patch);
        }
    }
    (raw.to_string(), None)
}

/// Shallow-merge `patch` into `target`. Patch keys overwrite target keys.
/// Handles dot-notation keys like "nested.field" by recursing one level.
fn merge_json(target: &mut Value, patch: &Value) {
    if let (Some(target_obj), Some(patch_obj)) = (target.as_object_mut(), patch.as_object()) {
        for (key, val) in patch_obj {
            if key.contains('.') {
                // dot-notation: split on first dot
                let mut parts = key.splitn(2, '.');
                let outer = parts.next().unwrap();
                let inner = parts.next().unwrap();
                let nested = target_obj.entry(outer).or_insert(json!({}));
                merge_json(nested, &json!({ inner: val }));
            } else {
                target_obj.insert(key.clone(), val.clone());
            }
        }
    }
}

// ─── Internal: call xaman_ek agent ───────────────────────────────────────────

async fn call_xaman_ek(
    state: &AppState,
    _user_id: &str,
    query: &str,
) -> Result<String, String> {
    use crate::{resolve_agent, resolve_agent_card};
    use fermi::agent_backend::executor::{AgentExecutor, ExecutionContext};
    use fermi::ast;

    let db_agent = resolve_agent(state, "xaman_ek")
        .await
        .map_err(|(_, msg)| msg)?;

    let card = resolve_agent_card(state, &db_agent);

    let agent_stmt = ast::AgentStmt {
        name: "xaman_ek".to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };

    let context = ExecutionContext {
        program,
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
    };

    let output = state
        .registry
        .execute_agent(&agent_stmt, &context)
        .await
        .map_err(|e| e.to_string())?;

    // Extract response text from the output — prefer reasoning (the LLM's
    // narrative output), then key_findings from evidence, then summary.
    let response = if let Some(reasoning) = &output.metadata.reasoning {
        reasoning.clone()
    } else if !output.evidence.is_empty() {
        output
            .evidence
            .iter()
            .flat_map(|e| {
                let mut parts = Vec::new();
                if let Some(ref s) = e.summary { parts.push(s.clone()); }
                parts.extend(e.key_findings.iter().cloned());
                parts
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    } else {
        "I processed your request but have no specific response to offer.".to_string()
    };

    Ok(response)
}
