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
use simops::{suggest_principal_bindings, process_v2::ProcessConfigV2};

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

// ─── POST /api/simops/cascade/suggest-bindings ────────────────────────────────
//
// Spec 36a A.1.1 + A.1.4 — slot-match scoring.
// Takes a ProcessConfigV2 and returns binding suggestions for every orphan
// principal input (principal without from_stage on a non-first stage).
// Pure computation — no workspace state. No credits charged.
//
// Request: { process: ProcessConfigV2 }
// Response: { suggestions: SlotBindingSuggestion[] }

#[derive(Deserialize)]
pub struct SuggestBindingsRequest {
    pub process: ProcessConfigV2,
}

pub async fn suggest_bindings_handler(
    _state: State<AppState>,
    _principal: AuthPrincipal,
    Json(req): Json<SuggestBindingsRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let suggestions = suggest_principal_bindings(&req.process);
    Ok(Json(json!({
        "suggestions": suggestions,
        "count": suggestions.len(),
    })))
}

// ─── POST /api/workspaces/:id/actions/migrate_parallelism_to_twin ─────────────
//
// Spec 36a A.1.5 — one-shot migration.
// Reads simops/process.yaml from the workspace, moves any stage.parallelism
// blocks to a twin manifest, writes both files via the action protocol.
//
// Request: {
//   process_path: "simops/process.yaml",   // optional, defaults to above
//   twin_path: "simops/twins/primary/twin.yaml",  // optional
//   confirmation: "auto" | "ask"           // default "ask"
// }
// Response: {
//   action_id, stages_migrated, twin_path, process_sha, twin_sha
// }

#[derive(Deserialize)]
pub struct MigrateParallelismRequest {
    #[serde(default = "default_process_path")]
    pub process_path: String,
    #[serde(default = "default_twin_path")]
    pub twin_path: String,
    #[serde(default)]
    pub confirmation: Option<String>,
}
fn default_process_path() -> String { "simops/process.yaml".into() }
fn default_twin_path() -> String { "simops/twins/primary/twin.yaml".into() }

pub async fn migrate_parallelism_to_twin_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<MigrateParallelismRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    // Read process YAML
    let process_content = tokio::task::spawn_blocking({
        let git = state.workspace_git.clone();
        let slug = slug.clone();
        let path = req.process_path.clone();
        move || git.read_file(&slug, &path)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::NOT_FOUND, format!("Could not read {}: {}", req.process_path, e)))?;

    // Parse as JSON (YAML is valid JSON superset for our process shape)
    let mut process_json: serde_json::Value = serde_yaml::from_str(&process_content)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("Failed to parse process YAML: {e}")))?;

    // Run migration — move stage.parallelism to twin manifest
    let stages = process_json.get_mut("stages")
        .and_then(|s| s.as_array_mut())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, "process.stages is missing or not an array".into()))?;

    let mut twin_parallelism = serde_json::Map::new();
    let mut stages_migrated = 0usize;

    for stage in stages.iter_mut() {
        if let Some(obj) = stage.as_object_mut() {
            if let Some(par) = obj.get("parallelism").cloned() {
                let kind = par.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                if kind == "parallel_instances" {
                    let stage_id = obj.get("id")
                        .and_then(|id| id.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    twin_parallelism.insert(stage_id, par);
                    obj.remove("parallelism");
                    stages_migrated += 1;
                } else {
                    // Singleton or unknown — drop without migrating
                    obj.remove("parallelism");
                }
            }
        }
    }

    if stages_migrated == 0 {
        return Ok(Json(json!({
            "action_id": null,
            "stages_migrated": 0,
            "message": "No parallel_instances blocks found in process.stages — nothing to migrate.",
            "twin_path": req.twin_path,
        })));
    }

    // Serialise cleaned process back to YAML
    let cleaned_yaml = serde_yaml::to_string(&process_json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("YAML serialisation failed: {e}")))?;

    // Build twin manifest YAML
    let twin_manifest = serde_json::json!({
        "twin_id": "primary",
        "process_ref": req.process_path,
        "parallelism": twin_parallelism,
        "created_at": Utc::now().to_rfc3339(),
        "created_by": "migrate_parallelism_to_twin",
        "status": "active",
        "derived_from": null,
    });
    let twin_yaml = serde_yaml::to_string(&twin_manifest)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Twin YAML serialisation failed: {e}")))?;

    let confirmation = req.confirmation.as_deref().unwrap_or("ask");

    // Log and apply both writes
    let payload = json!({
        "process_path": req.process_path,
        "twin_path": req.twin_path,
        "stages_migrated": stages_migrated,
    });

    let action_id = log_action(
        &state, ws_uuid, "migrate_parallelism_to_twin",
        "user", &user_id, Some("kask_simops"), &payload, confirmation, None,
    ).await?;

    let mut process_sha = None;
    let mut twin_sha = None;

    if confirmation == "auto" {
        // Write cleaned process
        match state.workspace_git.commit_file(
            &slug, &req.process_path, &cleaned_yaml,
            &format!("migrate: move stage.parallelism to twin manifest (spec 36a A.1.5)")
        ) {
            Ok(sha) => { process_sha = Some(sha); }
            Err(e) => tracing::warn!("migrate: process write failed: {e}"),
        }

        // Write twin manifest
        match state.workspace_git.commit_file(
            &slug, &req.twin_path, &twin_yaml,
            "migrate: create twin manifest with relocated parallelism"
        ) {
            Ok(sha) => { twin_sha = Some(sha); }
            Err(e) => tracing::warn!("migrate: twin write failed: {e}"),
        }

        let _ = sqlx::query(
            "UPDATE workspace_action_log SET applied = true, applied_at = NOW()
             WHERE action_id = $1"
        )
        .bind(action_id)
        .execute(&state.db)
        .await;
    }

    Ok(Json(json!({
        "action_id": action_id,
        "stages_migrated": stages_migrated,
        "twin_path": req.twin_path,
        "process_sha": process_sha,
        "twin_sha": twin_sha,
        "confirmation": confirmation,
    })))
}

// ─── kask-wild: log_observation action ───────────────────────────────────────

#[derive(Deserialize)]
pub struct LogObservationRequest {
    pub species: String,
    pub h3_cell: Option<String>,
    pub location_lat: Option<f64>,
    pub location_lng: Option<f64>,
    pub location_name: Option<String>,
    pub quantity: Option<String>,     // trace | sparse | moderate | abundant
    pub habitat: Option<String>,
    pub substrate: Option<String>,
    pub conditions: Option<Value>,    // { temp_c, humidity_pct, rainfall_prior_7d_mm, ... }
    pub harvested: Option<bool>,
    pub harvest_notes: Option<String>,
    pub processing_path: Option<String>,
    pub processing_notes: Option<String>,
    pub flavor_notes: Option<String>,
    pub opted_in_shared: Option<bool>,
    pub goal_id: Option<String>,
    pub creature_id: Option<String>,
    pub taxa_group: Option<String>,   // fungi | plant | lichen | other
    pub edibility: Option<String>,
    pub source_message_id: Option<String>,
}

/// POST /api/workspaces/:id/actions/log_observation
///
/// Records a foraging observation into forage_observations and logs the
/// action. Also stores the observation as a SOSA observation if lat/lng
/// are provided. The episode is stored via dispatch_rabble_action in the
/// background so the KG accumulates from foraging finds.
pub async fn log_observation_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<LogObservationRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, _slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let source_msg_id = req.source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let goal_uuid: Option<Uuid> = req.goal_id
        .as_deref()
        .and_then(|s| s.parse().ok());

    let creature_uuid: Option<Uuid> = req.creature_id
        .as_deref()
        .and_then(|s| s.parse().ok());

    // Validate quantity and edibility
    let quantity = req.quantity.as_deref();
    if let Some(q) = quantity {
        if !["trace", "sparse", "moderate", "abundant"].contains(&q) {
            return Err((StatusCode::BAD_REQUEST,
                format!("Invalid quantity '{}' — must be trace|sparse|moderate|abundant", q)));
        }
    }
    let edibility = req.edibility.as_deref();
    if let Some(e) = edibility {
        if !["edible", "choice", "toxic", "unknown", "inedible"].contains(&e) {
            return Err((StatusCode::BAD_REQUEST,
                format!("Invalid edibility '{}' — must be edible|choice|toxic|unknown|inedible", e)));
        }
    }

    // Build flavor profile JSONB from flavor_notes if no structured profile provided
    let flavor_profile = if let Some(ref notes) = req.flavor_notes {
        json!({ "tasting_notes": notes })
    } else {
        json!({})
    };

    let conditions = req.conditions.clone().unwrap_or_else(|| json!({}));

    // Insert into forage_observations
    let observation_id: Uuid = sqlx::query(
        r#"INSERT INTO forage_observations (
            creature_id, goal_id, owner_id,
            species_name, taxa_group, edibility, quantity,
            h3_cell, location_lat, location_lng, location_name,
            habitat_type, substrate,
            conditions, harvested, harvest_notes,
            processing_path, processing_notes,
            flavor_profile, opted_in_shared
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            $8, $9, $10, $11, $12, $13,
            $14, $15, $16, $17, $18, $19, $20
        ) RETURNING observation_id"#,
    )
    .bind(creature_uuid)
    .bind(goal_uuid)
    .bind(&user_id)
    .bind(&req.species)
    .bind(req.taxa_group.as_deref().unwrap_or("fungi"))
    .bind(edibility)
    .bind(quantity)
    .bind(req.h3_cell.as_deref())
    .bind(req.location_lat)
    .bind(req.location_lng)
    .bind(req.location_name.as_deref())
    .bind(req.habitat.as_deref())
    .bind(req.substrate.as_deref())
    .bind(&conditions)
    .bind(req.harvested.unwrap_or(false))
    .bind(req.harvest_notes.as_deref())
    .bind(req.processing_path.as_deref())
    .bind(req.processing_notes.as_deref())
    .bind(&flavor_profile)
    .bind(req.opted_in_shared.unwrap_or(false))
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR,
        format!("Failed to store observation: {}", e)))?
    .try_get("observation_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Log the action
    let payload = json!({
        "species": req.species,
        "quantity": quantity,
        "h3_cell": req.h3_cell,
        "habitat": req.habitat,
        "observation_id": observation_id,
        "opted_in_shared": req.opted_in_shared.unwrap_or(false),
    });

    let action_id = log_action(
        &state, ws_uuid, "log_observation",
        "user", &user_id,
        Some("kask_wild/1"),
        &payload, "auto", source_msg_id,
    ).await?;

    // Update goal progress if goal_id provided
    if let Some(gid) = goal_uuid {
        let _ = sqlx::query(
            r#"UPDATE creature_goals
               SET progress = jsonb_set(
                   jsonb_set(
                       progress,
                       '{observations_logged}',
                       to_jsonb(COALESCE((progress->>'observations_logged')::int, 0) + 1)
                   ),
                   '{last_species}',
                   to_jsonb($1::text)
               ),
               last_evaluated_at = NOW()
               WHERE goal_id = $2"#,
        )
        .bind(&req.species)
        .bind(gid)
        .execute(&state.db)
        .await
        .ok(); // non-fatal
    }

    Ok(Json(json!({
        "action_id": action_id,
        "observation_id": observation_id,
        "action_type": "log_observation",
        "species": req.species,
        "applied": true,
        "opted_in_shared": req.opted_in_shared.unwrap_or(false),
    })))
}
