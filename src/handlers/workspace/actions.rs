//! Generalised workspace action protocol.
//!
//! Every structured mutation an agent (via action blocks), CLI command, or
//! MCP tool call makes to a workspace's canonical document is recorded in
//! `workspace_action_log` and optionally pended for human confirmation.
//!
//! Six action types — generalised from the SimOps action grammar but
//! applicable to any App hosted on ABW:
//!
//!   POST /api/workspaces/:id/actions/mutate_document
//!   POST /api/workspaces/:id/actions/fork_state
//!   POST /api/workspaces/:id/actions/compare
//!   POST /api/workspaces/:id/actions/invoke_member
//!   POST /api/workspaces/:id/actions/annotate_schema
//!   POST /api/workspaces/:id/actions/annotate
//!
//!   GET  /api/workspaces/:id/actions              — list recent actions
//!   GET  /api/workspaces/:id/actions/pending       — list pending confirmations
//!   POST /api/workspaces/:id/actions/:action_id/accept
//!   POST /api/workspaces/:id/actions/:action_id/reject
//!   GET  /api/workspaces/:id/annotations           — list annotations
//!   DELETE /api/workspaces/:id/annotations/:id     — resolve an annotation
//!
//! Design principles:
//! - Every action is recorded before it is applied (append-only log).
//! - `confirmation: "auto"` actions are applied immediately and marked applied=true.
//! - `confirmation: "ask"` actions are recorded as pending; a human must
//!   POST to /accept or /reject before they take effect.
//! - During alpha, callers may pass `force_ask: true` to override "auto" — the
//!   kask client uses this to gate all mutate_document actions behind a diff modal.
//! - The apply step for mutate_document / fork_state writes to the workspace git
//!   via the existing WorkspaceGitManager. Other action types record intent only
//!   (the client dispatches the actual side effect and calls back /accept).

use std::sync::Arc;

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

use fermi_auth::{teams, AuthPrincipal};

use crate::AppState;

// ─── Shared request helpers ───────────────────────────────────────────────────

/// Resolve workspace UUID and verify the caller is a member.
async fn resolve_workspace(
    state: &AppState,
    workspace_id: &str,
    user_id: &str,
) -> Result<(Uuid, String), (StatusCode, String)> {
    let ws_uuid: Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Verify membership
    let role = teams::get_member_role(&state.db, ws_uuid, user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if role.is_none() {
        return Err((StatusCode::FORBIDDEN, "Not a workspace member".to_string()));
    }

    // Get slug for git operations
    let slug: String = sqlx::query("SELECT slug FROM teams WHERE id = $1")
        .bind(ws_uuid)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Workspace not found".to_string()))?
        .try_get("slug")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((ws_uuid, slug))
}

/// Insert an action into workspace_action_log and return its ID.
async fn log_action(
    state: &AppState,
    workspace_id: Uuid,
    action_type: &str,
    emitted_by_type: &str,
    emitted_by_id: &str,
    app_schema: Option<&str>,
    payload: &Value,
    confirmation: &str,
    source_message_id: Option<Uuid>,
) -> Result<Uuid, (StatusCode, String)> {
    let action_id: Uuid = sqlx::query(
        r#"INSERT INTO workspace_action_log
           (workspace_id, emitted_by_type, emitted_by_id, action_type,
            app_schema, payload, confirmation, source_message_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING action_id"#,
    )
    .bind(workspace_id)
    .bind(emitted_by_type)
    .bind(emitted_by_id)
    .bind(action_type)
    .bind(app_schema)
    .bind(payload)
    .bind(confirmation)
    .bind(source_message_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("action_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(action_id)
}

// ─── 1. mutate_document ───────────────────────────────────────────────────────
//
// Patch the App's canonical document (simops equivalent: edit_process).
// Writes to the workspace git when applied.

#[derive(Deserialize)]
pub struct MutateDocumentRequest {
    /// App schema slug (e.g. "kask_simops"). Used for validation + logging.
    pub app_schema: Option<String>,
    /// Document path relative to workspace root (e.g. "simops/process.yaml").
    pub path: String,
    /// The patch to apply. Format is app-specific; stored verbatim.
    pub patch: Value,
    /// Human-readable rationale for the change.
    pub rationale: Option<String>,
    /// "auto" = apply immediately; "ask" = pend for human confirmation.
    /// Server always treats as "ask" when force_ask is true.
    pub confirmation: Option<String>,
    /// Kask alpha flag: always pend regardless of confirmation value.
    pub force_ask: Option<bool>,
    /// The serialised new document content (after applying the patch).
    /// Required when confirmation resolves to "auto" so we can write it.
    /// Optional for "ask" — the client may supply it after acceptance.
    pub content: Option<String>,
    /// Source message that triggered this action (for calibration linkage).
    pub source_message_id: Option<String>,
}

pub async fn mutate_document_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<MutateDocumentRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    // Resolve confirmation: force_ask overrides "auto"
    let confirmation = if req.force_ask.unwrap_or(false) {
        "pending"
    } else {
        match req.confirmation.as_deref().unwrap_or("ask") {
            "auto" => "auto",
            _ => "pending",
        }
    };

    let payload = json!({
        "path": req.path,
        "patch": req.patch,
        "rationale": req.rationale,
        "content": req.content,
    });

    let action_id = log_action(
        &state, ws_uuid, "mutate_document",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, confirmation, source_msg_id,
    ).await?;

    // Apply immediately if auto
    let applied = if confirmation == "auto" {
        if let Some(ref content) = req.content {
            let commit_msg = req.rationale
                .as_deref()
                .unwrap_or("action: mutate_document");
            match state.workspace_git.commit_file(&slug, &req.path, content, commit_msg) {
                Ok(sha) => {
                    sqlx::query(
                        "UPDATE workspace_action_log
                         SET applied = true, applied_at = NOW(),
                             apply_result = $1, confirmation = 'auto'
                         WHERE action_id = $2",
                    )
                    .bind(json!({ "sha": sha, "path": req.path }))
                    .bind(action_id)
                    .execute(&state.db)
                    .await
                    .ok();
                    true
                }
                Err(e) => {
                    // Downgrade to pending if git write fails
                    sqlx::query(
                        "UPDATE workspace_action_log SET confirmation = 'pending' WHERE action_id = $1"
                    )
                    .bind(action_id)
                    .execute(&state.db)
                    .await
                    .ok();
                    return Err((StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Git write failed: {}", e)));
                }
            }
        } else {
            // No content supplied for auto — pend it
            sqlx::query(
                "UPDATE workspace_action_log SET confirmation = 'pending' WHERE action_id = $1"
            )
            .bind(action_id)
            .execute(&state.db)
            .await
            .ok();
            false
        }
    } else {
        false
    };

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "mutate_document",
        "confirmation": if applied { "auto" } else { "pending" },
        "applied": applied,
        "path": req.path,
    })))
}

// ─── 2. fork_state ────────────────────────────────────────────────────────────
//
// Create a named variant of the canonical document (simops: fork_variation).

#[derive(Deserialize)]
pub struct ForkStateRequest {
    pub app_schema: Option<String>,
    pub name: String,
    pub from: Option<String>,     // slug of source state; "base" or a variant slug
    pub patch: Value,
    pub hypothesis: Option<String>,
    pub source_message_id: Option<String>,
}

pub async fn fork_state_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<ForkStateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    // Derive a filesystem-safe slug from the name
    let variant_slug = req.name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();

    let payload = json!({
        "name": req.name,
        "slug": variant_slug,
        "from": req.from.as_deref().unwrap_or("base"),
        "patch": req.patch,
        "hypothesis": req.hypothesis,
    });

    let action_id = log_action(
        &state, ws_uuid, "fork_state",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, "auto", source_msg_id,
    ).await?;

    // Write the variation file to git
    let variant_path = format!("simops/variations/{}.yaml", variant_slug);
    let content = serde_yaml::to_string(&req.patch)
        .unwrap_or_else(|_| serde_json::to_string_pretty(&req.patch).unwrap_or_default());
    let commit_msg = format!(
        "fork: {} — {}",
        req.name,
        req.hypothesis.as_deref().unwrap_or("no hypothesis")
    );

    let apply_result = match state.workspace_git.commit_file(&slug, &variant_path, &content, &commit_msg) {
        Ok(sha) => {
            sqlx::query(
                "UPDATE workspace_action_log
                 SET applied = true, applied_at = NOW(), apply_result = $1
                 WHERE action_id = $2",
            )
            .bind(json!({ "sha": sha, "path": variant_path, "slug": variant_slug }))
            .bind(action_id)
            .execute(&state.db)
            .await
            .ok();
            json!({ "sha": sha, "path": variant_path, "slug": variant_slug })
        }
        Err(e) => {
            return Err((StatusCode::INTERNAL_SERVER_ERROR,
                format!("Git write failed: {}", e)));
        }
    };

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "fork_state",
        "applied": true,
        "variant_slug": variant_slug,
        "path": variant_path,
        "result": apply_result,
    })))
}

// ─── 3. compare ───────────────────────────────────────────────────────────────
//
// Record a comparison request and dispatch to a member agent (simops: compare_variations).
// The actual cascade + comparator invocation happens client-side or via invoke_member;
// this records the intent and returns the action_id for the client to link results back.

#[derive(Deserialize)]
pub struct CompareRequest {
    pub app_schema: Option<String>,
    pub variants: Vec<String>,
    pub metrics: Option<Vec<String>>,
    pub narrate_via: Option<String>,   // agent_id of the narrator (e.g. "comparator")
    pub source_message_id: Option<String>,
}

pub async fn compare_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<CompareRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let payload = json!({
        "variants": req.variants,
        "metrics": req.metrics.unwrap_or_default(),
        "narrate_via": req.narrate_via,
    });

    let action_id = log_action(
        &state, ws_uuid, "compare",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, "auto", source_msg_id,
    ).await?;

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "compare",
        "status": "recorded",
        "note": "Dispatch cascade + narration client-side; call /accept with results to mark applied.",
    })))
}

// ─── 4. invoke_member ─────────────────────────────────────────────────────────
//
// Call a fleet member with structured input (simops: invoke_agent).
// Records the invocation; actual execution is via the workspace messages path
// or MCP tools/call — this is the audit log entry.

#[derive(Deserialize)]
pub struct InvokeMemberRequest {
    pub app_schema: Option<String>,
    pub agent_id: String,
    pub query: Value,              // structured query (not necessarily a string)
    pub render_as: Option<String>, // hint for the client renderer
    pub source_message_id: Option<String>,
}

pub async fn invoke_member_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<InvokeMemberRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let payload = json!({
        "agent_id": req.agent_id,
        "query": req.query,
        "render_as": req.render_as,
    });

    let action_id = log_action(
        &state, ws_uuid, "invoke_member",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, "auto", source_msg_id,
    ).await?;

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "invoke_member",
        "agent_id": req.agent_id,
        "status": "recorded",
    })))
}

// ─── 5. annotate_schema ───────────────────────────────────────────────────────
//
// Attach a SOSA contract or schema metadata to a document field (simops: declare_sosa_contract).
// Stored in the action log; the field-level contract is also written to the workspace file
// if a path is provided.

#[derive(Deserialize)]
pub struct AnnotateSchemaRequest {
    pub app_schema: Option<String>,
    pub path: Option<String>,          // file path to update (optional)
    pub stage_id: Option<String>,
    pub field: String,
    pub observable_property: String,
    pub unit: String,
    pub sampling: Option<String>,
    pub rationale: Option<String>,
    pub source_message_id: Option<String>,
}

pub async fn annotate_schema_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<AnnotateSchemaRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let payload = json!({
        "path": req.path,
        "stage_id": req.stage_id,
        "field": req.field,
        "observable_property": req.observable_property,
        "unit": req.unit,
        "sampling": req.sampling.as_deref().unwrap_or("per_batch"),
        "rationale": req.rationale,
    });

    let action_id = log_action(
        &state, ws_uuid, "annotate_schema",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, "auto", source_msg_id,
    ).await?;

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "annotate_schema",
        "field": req.field,
        "observable_property": req.observable_property,
        "status": "recorded",
    })))
}

// ─── 6. annotate ─────────────────────────────────────────────────────────────
//
// Record a typed observation about the document or a fragment (simops: annotate).
// Stored in workspace_annotations for querying + resolving.

#[derive(Deserialize)]
pub struct AnnotateRequest {
    pub app_schema: Option<String>,
    pub kind: String,        // critique | insight | risk | decision
    pub target: String,      // e.g. "stage:fermentation", "process", "variation:co2-capture"
    pub body: String,
    pub severity: Option<String>,  // info | warn | block
    pub source_message_id: Option<String>,
}

pub async fn annotate_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<AnnotateRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    // Validate kind and severity
    let kind = req.kind.as_str();
    if !["critique", "insight", "risk", "decision"].contains(&kind) {
        return Err((StatusCode::BAD_REQUEST,
            format!("Invalid kind '{}' — must be critique|insight|risk|decision", kind)));
    }
    let severity = req.severity.as_deref().unwrap_or("info");
    if !["info", "warn", "block"].contains(&severity) {
        return Err((StatusCode::BAD_REQUEST,
            format!("Invalid severity '{}' — must be info|warn|block", severity)));
    }

    let payload = json!({
        "kind": kind,
        "target": req.target,
        "body": req.body,
        "severity": severity,
    });

    let action_id = log_action(
        &state, ws_uuid, "annotate",
        "user", &user_id,
        req.app_schema.as_deref(),
        &payload, "auto", source_msg_id,
    ).await?;

    // Also insert into workspace_annotations for direct querying
    let annotation_id: Uuid = sqlx::query(
        r#"INSERT INTO workspace_annotations
           (workspace_id, kind, target, body, severity, app_schema,
            author_type, author_id, action_id, source_message_id)
           VALUES ($1, $2, $3, $4, $5, $6, 'user', $7, $8, $9)
           RETURNING annotation_id"#,
    )
    .bind(ws_uuid)
    .bind(kind)
    .bind(&req.target)
    .bind(&req.body)
    .bind(severity)
    .bind(req.app_schema.as_deref())
    .bind(&user_id)
    .bind(action_id)
    .bind(source_msg_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("annotation_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_id,
        "annotation_id": annotation_id,
        "action_type": "annotate",
        "kind": kind,
        "target": req.target,
        "severity": severity,
        "applied": true,
    })))
}

// ─── List actions ─────────────────────────────────────────────────────────────

pub async fn list_actions_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let rows = sqlx::query(
        "SELECT action_id, action_type, emitted_by_type, emitted_by_id,
                app_schema, payload, confirmation, applied, applied_at,
                apply_result, created_at
         FROM workspace_action_log
         WHERE workspace_id = $1
         ORDER BY created_at DESC
         LIMIT 100",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let actions: Vec<Value> = rows.iter().map(|r| json!({
        "action_id":       r.try_get::<Uuid, _>("action_id").ok(),
        "action_type":     r.try_get::<String, _>("action_type").unwrap_or_default(),
        "emitted_by_type": r.try_get::<String, _>("emitted_by_type").unwrap_or_default(),
        "emitted_by_id":   r.try_get::<String, _>("emitted_by_id").unwrap_or_default(),
        "app_schema":      r.try_get::<Option<String>, _>("app_schema").ok().flatten(),
        "payload":         r.try_get::<Value, _>("payload").unwrap_or_default(),
        "confirmation":    r.try_get::<String, _>("confirmation").unwrap_or_default(),
        "applied":         r.try_get::<bool, _>("applied").unwrap_or(false),
        "applied_at":      r.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("applied_at").ok().flatten().map(|t| t.to_rfc3339()),
        "apply_result":    r.try_get::<Option<Value>, _>("apply_result").ok().flatten(),
        "created_at":      r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
    })).collect();

    Ok(Json(json!({ "actions": actions, "total": actions.len() })))
}

// ─── List pending ─────────────────────────────────────────────────────────────

pub async fn list_pending_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let rows = sqlx::query(
        "SELECT action_id, action_type, emitted_by_type, emitted_by_id,
                app_schema, payload, created_at
         FROM workspace_action_log
         WHERE workspace_id = $1 AND confirmation = 'pending'
         ORDER BY created_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let pending: Vec<Value> = rows.iter().map(|r| json!({
        "action_id":       r.try_get::<Uuid, _>("action_id").ok(),
        "action_type":     r.try_get::<String, _>("action_type").unwrap_or_default(),
        "emitted_by_type": r.try_get::<String, _>("emitted_by_type").unwrap_or_default(),
        "emitted_by_id":   r.try_get::<String, _>("emitted_by_id").unwrap_or_default(),
        "app_schema":      r.try_get::<Option<String>, _>("app_schema").ok().flatten(),
        "payload":         r.try_get::<Value, _>("payload").unwrap_or_default(),
        "created_at":      r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
    })).collect();

    Ok(Json(json!({ "pending": pending, "count": pending.len() })))
}

// ─── Accept / Reject ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AcceptRequest {
    /// The final content to write (for mutate_document actions where content
    /// was not supplied at action creation time).
    pub content: Option<String>,
    /// Apply result to record (for actions applied client-side, e.g. compare).
    pub apply_result: Option<Value>,
}

pub async fn accept_action_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, action_id)): Path<(String, String)>,
    Json(req): Json<AcceptRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let action_uuid: Uuid = action_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid action ID".to_string()))?;

    // Fetch the pending action
    let row = sqlx::query(
        "SELECT action_type, payload, confirmation FROM workspace_action_log
         WHERE action_id = $1 AND workspace_id = $2",
    )
    .bind(action_uuid)
    .bind(ws_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Action not found".to_string()))?;

    let action_type: String = row.try_get("action_type")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let payload: Value = row.try_get("payload")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let confirmation: String = row.try_get("confirmation")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if confirmation == "accepted" || confirmation == "rejected" {
        return Err((StatusCode::CONFLICT, "Action already resolved".to_string()));
    }

    // Apply the action
    let apply_result = match action_type.as_str() {
        "mutate_document" => {
            let path = payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = req.content
                .as_deref()
                .or_else(|| payload.get("content").and_then(|v| v.as_str()))
                .unwrap_or("");
            let rationale = payload.get("rationale").and_then(|v| v.as_str())
                .unwrap_or("action: mutate_document (accepted)");
            if !path.is_empty() && !content.is_empty() {
                match state.workspace_git.commit_file(&slug, path, content, rationale) {
                    Ok(sha) => json!({ "sha": sha, "path": path }),
                    Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Git write failed: {}", e))),
                }
            } else {
                req.apply_result.clone().unwrap_or(json!({}))
            }
        }
        _ => req.apply_result.clone().unwrap_or(json!({})),
    };

    sqlx::query(
        "UPDATE workspace_action_log
         SET confirmation = 'accepted', confirmed_by = $1, confirmed_at = NOW(),
             applied = true, applied_at = NOW(), apply_result = $2
         WHERE action_id = $3",
    )
    .bind(&user_id)
    .bind(&apply_result)
    .bind(action_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_uuid,
        "status": "accepted",
        "applied": true,
        "apply_result": apply_result,
    })))
}

#[derive(Deserialize)]
pub struct RejectRequest {
    pub note: Option<String>,
}

pub async fn reject_action_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, action_id)): Path<(String, String)>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let action_uuid: Uuid = action_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid action ID".to_string()))?;

    sqlx::query(
        "UPDATE workspace_action_log
         SET confirmation = 'rejected', confirmed_by = $1, confirmed_at = NOW(),
             rejection_note = $2
         WHERE action_id = $3 AND workspace_id = $4",
    )
    .bind(&user_id)
    .bind(req.note.as_deref())
    .bind(action_uuid)
    .bind(ws_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "action_id": action_uuid, "status": "rejected" })))
}

// ─── Annotations ─────────────────────────────────────────────────────────────

pub async fn list_annotations_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let rows = sqlx::query(
        "SELECT annotation_id, kind, target, body, severity, app_schema,
                author_type, author_id, resolved, created_at
         FROM workspace_annotations
         WHERE workspace_id = $1 AND NOT resolved
         ORDER BY created_at DESC",
    )
    .bind(ws_uuid)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let annotations: Vec<Value> = rows.iter().map(|r| json!({
        "annotation_id": r.try_get::<Uuid, _>("annotation_id").ok(),
        "kind":          r.try_get::<String, _>("kind").unwrap_or_default(),
        "target":        r.try_get::<String, _>("target").unwrap_or_default(),
        "body":          r.try_get::<String, _>("body").unwrap_or_default(),
        "severity":      r.try_get::<String, _>("severity").unwrap_or_default(),
        "app_schema":    r.try_get::<Option<String>, _>("app_schema").ok().flatten(),
        "author_type":   r.try_get::<String, _>("author_type").unwrap_or_default(),
        "author_id":     r.try_get::<String, _>("author_id").unwrap_or_default(),
        "created_at":    r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok().map(|t| t.to_rfc3339()),
    })).collect();

    Ok(Json(json!({ "annotations": annotations, "count": annotations.len() })))
}

pub async fn resolve_annotation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, annotation_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let annotation_uuid: Uuid = annotation_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid annotation ID".to_string()))?;

    sqlx::query(
        "UPDATE workspace_annotations
         SET resolved = true, resolved_by = $1, resolved_at = NOW()
         WHERE annotation_id = $2 AND workspace_id = $3",
    )
    .bind(&user_id)
    .bind(annotation_uuid)
    .bind(ws_uuid)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({ "annotation_id": annotation_uuid, "resolved": true })))
}
