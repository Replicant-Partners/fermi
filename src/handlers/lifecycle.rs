//! Agent lifecycle handlers — publish, archive, restore, fork, fork-pricing.
//!
//! Thin wrappers over src/workflows/ business logic.
//! Added in Sprint L (L3/L4).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::{rbac, AuthPrincipal, ObjectType, Visibility};
use serde::Deserialize;
use serde_json::{json, Value};

// AppState and resolve_agent are defined at the binary crate root (api_server.rs)
use crate::{resolve_agent, AppState};
use fermi::workflows::{fork, publish_pipeline};

/// Map an agent to substrate `Visibility`. Same rule as
/// `handlers::agents::agent_effective_visibility` — duplicated here
/// because Rust module visibility (`fn` vs `pub(crate) fn`) is worth
/// less than a two-line helper. If this rule ever gets more
/// complicated, extract to a shared module.
fn agent_visibility(agent: &agent_bestiary_memory::Agent) -> Visibility {
    if agent.status == "published" && agent.visibility == "public" {
        Visibility::Public
    } else if agent.visibility == "unlisted" {
        Visibility::Shared
    } else {
        Visibility::Private
    }
}

/// Log a platform-admin bypass event to `admin_bypass_events` (mig 164).
///
/// Best-effort: a logging failure must never prevent the underlying
/// action from completing. We log to tracing on failure so operators
/// can see if the audit trail has holes.
async fn log_admin_bypass(
    pool: &sqlx::PgPool,
    admin_user_id: &str,
    target_type: &str,
    target_id: &str,
    action: &str,
    details: Value,
) {
    let result = sqlx::query(
        "INSERT INTO admin_bypass_events \
         (admin_user_id, target_type, target_id, action, details) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(admin_user_id)
    .bind(target_type)
    .bind(target_id)
    .bind(action)
    .bind(&details)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::warn!(
            admin = admin_user_id, target_type, target_id, action,
            error = %e,
            "[admin_bypass] audit write failed — action still succeeded",
        );
    }
}

// ─── Fork ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ForkRequest {
    #[serde(default)]
    pub include_ontology: bool,
    #[serde(default)]
    pub include_embeddings: bool,
}

pub async fn fork_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(req): Json<ForkRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    let result = fork::fork_agent(
        &state.db,
        db_agent.agent_id,
        &user_id,
        req.include_ontology,
        req.include_embeddings,
        &state.gas_fees,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!({
        "agent_id": result.agent_id,
        "agent_name": result.agent_name,
        "total_cost": result.total_cost,
        "author_royalty": result.author_royalty,
    })))
}

// ─── Fork Pricing ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ForkPricingRequest {
    pub base_price: i32,
    pub ontology_price: Option<i32>,
    pub embedding_price: Option<i32>,
}

pub async fn update_fork_pricing_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(req): Json<ForkPricingRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // v0.10.5: substrate RBAC. Fork pricing is a monetary policy
    // decision — Admin (owner or platform admin) only. No shares.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_visibility(&db_agent),
    )
    .await?;

    let pricing = json!({
        "base_price": req.base_price,
        "ontology_price": req.ontology_price,
        "embedding_price": req.embedding_price,
    });

    sqlx::query("UPDATE agents SET fork_pricing = $1, updated_at = NOW() WHERE agent_id = $2")
        .bind(&pricing)
        .bind(db_agent.agent_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("DB error: {}", e),
            )
        })?;

    Ok(Json(json!({ "ok": true, "fork_pricing": pricing })))
}

// ─── Publish ─────────────────────────────────────────────────────

pub async fn publish_checks_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // v0.10.5: substrate RBAC. Publish readiness inspection needs
    // Admin (owner + platform admin). Not a fully-public read — the
    // check list can reveal what an unpublished draft looks like.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_visibility(&db_agent),
    )
    .await?;

    let checks = publish_pipeline::run_publish_checks(&db_agent);
    let can_publish = publish_pipeline::can_publish(&checks);

    Ok(Json(json!({
        "checks": checks,
        "can_publish": can_publish,
    })))
}

/// Query params for the publish endpoint. `force=true` is admin-only
/// and skips the `can_publish` gate; every use is logged to
/// `admin_bypass_events`. See RELEASE_NOTES_v0.10.5.md.
#[derive(Debug, Deserialize)]
pub struct PublishQuery {
    #[serde(default)]
    pub force: bool,
    /// Free-form justification, stored in `admin_bypass_events.details.reason`.
    /// Optional but strongly encouraged for force-publishes so the
    /// audit trail is legible six months from now.
    #[serde(default)]
    pub reason: Option<String>,
}

pub async fn publish_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Query(q): Query<PublishQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();
    let is_owner = db_agent.owner_id.as_deref() == Some(&user_id);
    let is_admin = principal.can_admin();

    // v0.10.5: substrate RBAC. Publish requires Admin on the agent —
    // owner OR platform admin. No share can publish.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_visibility(&db_agent),
    )
    .await?;

    // Force-publish gate: only platform admins may set it, and the
    // usage is logged. Owners cannot force-publish their own agent
    // — the checks are there to protect the platform from junk
    // publishes, and owners bypassing their own checks defeats that.
    if q.force && !is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            "force=true is platform-admin only. Fix the failing checks or ask an admin to force-publish.".into(),
        ));
    }

    // Preflight: compute checks before we call publish_agent so the
    // response body carries the same shape whether the publish
    // succeeded, was force-published, or was blocked by checks.
    let preflight_checks = publish_pipeline::run_publish_checks(&db_agent);
    let will_bypass_checks = q.force && !publish_pipeline::can_publish(&preflight_checks);

    // When admin publishes on behalf of a third-party owner, charge the
    // *owner's* wallet (their agent, their fee). Preserves the economic
    // model — admin isn't subsidising nor gate-keeping.
    let fee_payer_id = db_agent.owner_id.as_deref().unwrap_or(&user_id).to_string();

    let (transition, checks) = publish_pipeline::publish_agent(
        &state.db,
        &db_agent,
        &fee_payer_id,
        &state.gas_fees,
        q.force,
    )
    .await
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if will_bypass_checks {
        let failing: Vec<_> = preflight_checks
            .iter()
            .filter(|c| !c.passed && c.severity == fermi::workflows::types::CheckSeverity::Error)
            .map(|c| json!({ "name": c.name, "message": c.message }))
            .collect();
        log_admin_bypass(
            &state.db,
            &user_id,
            "agent",
            &db_agent.agent_id.to_string(),
            "force_publish",
            json!({
                "reason":       q.reason,
                "owner_id":     fee_payer_id,
                "agent_name":   db_agent.agent_name,
                "failing":      failing,
            }),
        )
        .await;
    }

    if !is_owner {
        tracing::info!(
            agent_id = %db_agent.agent_id,
            agent_name = %db_agent.agent_name,
            owner = %fee_payer_id,
            admin = %user_id,
            forced = will_bypass_checks,
            "Agent published by admin on behalf of owner"
        );
    }

    Ok(Json(json!({
        "transition":         { "from": transition.from, "to": transition.to },
        "checks":             checks,
        "published_by_admin": !is_owner,
        "force_used":         will_bypass_checks,
    })))
}

pub async fn archive_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();
    let is_owner = db_agent.owner_id.as_deref() == Some(&user_id);

    // v0.10.5: substrate RBAC. Archive is a lifecycle transition —
    // Admin (owner + platform admin).
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_visibility(&db_agent),
    )
    .await?;

    let transition = publish_pipeline::archive_agent(&state.db, &db_agent)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if !is_owner {
        tracing::info!(
            agent_id = %db_agent.agent_id,
            agent_name = %db_agent.agent_name,
            admin = %user_id,
            "Agent archived by admin"
        );
    }

    Ok(Json(json!({
        "transition": { "from": transition.from, "to": transition.to },
        "archived_by_admin": !is_owner,
    })))
}

pub async fn restore_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();
    let is_owner = db_agent.owner_id.as_deref() == Some(&user_id);

    // v0.10.5: substrate RBAC.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        agent_visibility(&db_agent),
    )
    .await?;
    let _ = user_id;

    let transition = publish_pipeline::restore_agent(&state.db, &db_agent)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!({
        "transition": { "from": transition.from, "to": transition.to },
        "restored_by_admin": !is_owner,
    })))
}
