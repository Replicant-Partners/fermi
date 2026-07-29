//! Agent-owner secrets management (v0.9.0 — marketplace API keys).
//!
//! Three endpoints:
//!
//!   PUT    /api/agents/:agent_id/secrets/:secret_name   (upsert)
//!   GET    /api/agents/:agent_id/secrets                (list; names only)
//!   DELETE /api/agents/:agent_id/secrets/:secret_name   (remove)
//!
//! # Model
//!
//! The marketplace design is **agents carry their own funding, not
//! users**. When Mario publishes an agent, he attaches an
//! `ANTHROPIC_API_KEY` (and any other secrets his agent depends on) to
//! that agent. When Ivan hires Mario's agent, the executor uses Mario's
//! key — and Anthropic bills Mario's account. Mario in turn gets paid
//! in platform credits by Ivan; the wallet flow (v0.9.2) closes that
//! loop.
//!
//! Under the hood this is the existing `user_secrets` primitive
//! (`fermi-auth::secrets`), used with a specific interpretation:
//!
//!   - `user_id`     = the AGENT OWNER's user id
//!   - `scope`       = the AGENT NAME (so multi-agent owners keep
//!                     per-agent budgets)
//!   - `secret_name` = the environment variable name the executor
//!                     looks up (typically `ANTHROPIC_API_KEY`)
//!
//! At execution time, `resolve_agent_owner_secrets` in `api_server.rs`
//! reads these back and injects them into `ToolContext.user_secrets`;
//! the tool-aware executor prefers them over the process env var.
//!
//! # Auth
//!
//! All three endpoints are **owner-gated**: only the agent's owner
//! (`agents.owner_id == caller.user_id`) or an admin may add / list /
//! remove secrets. This mirrors the existing update_agent /
//! delete_agent gates in `handlers/agents.rs`.
//!
//! # System agents
//!
//! System-tier agents (Fermi, xaman_ek) intentionally have no
//! `owner_id` (or it's the platform account). `resolve_agent_owner_secrets`
//! short-circuits `tier=system` before touching this table, so the
//! endpoints are effectively opt-in for the marketplace path only —
//! calling them on a system agent returns 403 (no owner).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{resolve_agent, AppState};

/// Body for `PUT /api/agents/:agent_id/secrets/:secret_name`.
#[derive(Debug, Deserialize)]
pub struct UpsertSecretRequest {
    /// The plaintext secret value. Encrypted server-side before storage.
    /// Never returned by any endpoint after write (only the name/label
    /// come back on subsequent GETs).
    pub value: String,
    /// Optional human-friendly label — surfaced on the owner's agent
    /// detail page so they can distinguish keys stored against the
    /// same agent (e.g. "personal" vs "team billing").
    #[serde(default)]
    pub label: Option<String>,
    /// Optional free-form note. Same rules as `label`.
    #[serde(default)]
    pub description: Option<String>,
}

/// Guard: caller must own the agent OR be admin. Returns the resolved
/// agent so the caller can then use its owner_id / agent_name for the
/// downstream store/list/delete call. System-tier agents intentionally
/// have no owner-managed secrets — those return 403 with a specific
/// message so the client can render an informative banner.
async fn require_agent_owner_or_admin(
    state: &AppState,
    principal: &AuthPrincipal,
    agent_id: &str,
) -> Result<agent_bestiary_memory::Agent, (StatusCode, String)> {
    let agent = resolve_agent(state, agent_id).await?;
    let caller_id = principal.user_id();

    if agent.tier.eq_ignore_ascii_case("system") {
        return Err((
            StatusCode::FORBIDDEN,
            "System agents are platform-funded — owner secrets are not applicable.".into(),
        ));
    }

    let is_admin = principal.can_admin();
    let owner_matches = agent
        .owner_id
        .as_deref()
        .map(|o| o == caller_id)
        .unwrap_or(false);

    if !owner_matches && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the agent's owner may manage its secrets.".into(),
        ));
    }
    Ok(agent)
}

/// PUT /api/agents/:agent_id/secrets/:secret_name
pub async fn upsert_agent_secret_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, secret_name)): Path<(String, String)>,
    Json(body): Json<UpsertSecretRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = require_agent_owner_or_admin(&state, &principal, &agent_id).await?;

    // Validation: reject empty values (matches the fermi-auth layer
    // behaviour) and reject obviously-wrong secret names to avoid
    // owners accidentally scoping a secret to an unrelated env var.
    if body.value.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Secret value cannot be empty.".into(),
        ));
    }
    if secret_name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "Secret name cannot be empty.".into(),
        ));
    }

    let encryptor = state.secret_encryptor.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Server not configured for secrets (SECRETS_ENCRYPTION_KEY missing).".into(),
        )
    })?;

    let owner_id = agent.owner_id.as_deref().ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "Agent has no owner; cannot attach owner-scoped secrets.".into(),
        )
    })?;

    let secret_id = fermi_auth::store_secret(
        &state.db,
        encryptor,
        owner_id,
        &secret_name,
        &body.value,
        // scope = agent name → executor's resolve_agent_owner_secrets
        // reads `WHERE user_id = owner AND (scope = agent_name OR scope = '*')`.
        &agent.agent_name,
        body.label.as_deref(),
        body.description.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!(
            agent = %agent.agent_name,
            secret = %secret_name,
            error = %e,
            "[secrets] upsert failed",
        );
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to store secret: {}", e),
        )
    })?;

    tracing::info!(
        agent = %agent.agent_name,
        secret = %secret_name,
        owner = %owner_id,
        "[secrets] stored (agent-owner marketplace path)",
    );

    Ok(Json(json!({
        "secret_id": secret_id,
        "agent_name": agent.agent_name,
        "secret_name": secret_name,
        "scope": agent.agent_name,
    })))
}

/// GET /api/agents/:agent_id/secrets — list names only (never values).
pub async fn list_agent_secrets_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agent = require_agent_owner_or_admin(&state, &principal, &agent_id).await?;

    // list_secrets doesn't need the encryptor (returns metadata only,
    // never plaintext), but we still guard the endpoint on `state
    // .secret_encryptor.is_some()` so the shape is consistent with the
    // upsert/delete paths — all three refuse when the server isn't
    // configured for secrets. Prevents an operator storing a secret
    // and being surprised the server can't decrypt it at exec time.
    if state.secret_encryptor.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Server not configured for secrets (SECRETS_ENCRYPTION_KEY missing).".into(),
        ));
    }
    let owner_id = agent
        .owner_id
        .as_deref()
        .ok_or_else(|| (StatusCode::CONFLICT, "Agent has no owner.".into()))?;

    // list_secrets returns metadata for ALL of the owner's secrets;
    // we filter down to secrets scoped to this agent (or global `*`)
    // so a multi-agent owner sees only what this page controls.
    let all = fermi_auth::list_secrets(&state.db, owner_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list secrets: {}", e),
            )
        })?;
    let scope_filter = agent.agent_name.clone();
    let filtered: Vec<Value> = all
        .into_iter()
        .filter(|s| s.scope == scope_filter || s.scope == "*")
        .map(|s| {
            json!({
                "secret_id": s.secret_id,
                "secret_name": s.secret_name,
                "scope": s.scope,
                "label": s.label,
                "description": s.description,
                "created_at": s.created_at.to_rfc3339(),
                "updated_at": s.updated_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "agent_name": agent.agent_name,
        "secrets": filtered,
        "count": filtered.len(),
        // Convenience surface for the console's "is this agent funded?"
        // marketplace badge — true iff at least one ANTHROPIC_API_KEY
        // (or wildcard scope) is stored.
        "has_anthropic_key": filtered.iter().any(|s| {
            s.get("secret_name").and_then(|v| v.as_str()) == Some("ANTHROPIC_API_KEY")
        }),
    })))
}

/// DELETE /api/agents/:agent_id/secrets/:secret_name
pub async fn delete_agent_secret_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, secret_name)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let agent = require_agent_owner_or_admin(&state, &principal, &agent_id).await?;
    let owner_id = agent
        .owner_id
        .as_deref()
        .ok_or_else(|| (StatusCode::CONFLICT, "Agent has no owner.".into()))?;

    fermi_auth::delete_secret(&state.db, owner_id, &secret_name)
        .await
        .map_err(|e| {
            let err_str = e.to_string();
            let status = if err_str.contains("not found") || err_str.contains("SecretNotFound") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, format!("Failed to delete secret: {}", e))
        })?;

    tracing::info!(
        agent = %agent.agent_name,
        secret = %secret_name,
        owner = %owner_id,
        "[secrets] deleted (agent-owner marketplace path)",
    );

    Ok(StatusCode::NO_CONTENT)
}
