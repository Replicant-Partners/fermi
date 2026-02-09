//! Agent lifecycle handlers — publish, archive, restore, fork, fork-pricing.
//!
//! Thin wrappers over src/workflows/ business logic.
//! Added in Sprint L (L3/L4).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi_auth::AuthPrincipal;
use serde::Deserialize;
use serde_json::{json, Value};

// AppState and resolve_agent are defined at the binary crate root (api_server.rs)
use crate::{resolve_agent, AppState};
use fermi::workflows::{fork, publish_pipeline};

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
    let user_id = principal.user_id();

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

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

// ─── Publish ───────────────────────────────────────────────────────

pub async fn publish_checks_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    let checks = publish_pipeline::run_publish_checks(&db_agent);
    let can_publish = publish_pipeline::can_publish(&checks);

    Ok(Json(json!({
        "checks": checks,
        "can_publish": can_publish,
    })))
}

pub async fn publish_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    let (transition, checks) =
        publish_pipeline::publish_agent(&state.db, &db_agent, &user_id, &state.gas_fees)
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!({
        "transition": { "from": transition.from, "to": transition.to },
        "checks": checks,
    })))
}

pub async fn archive_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    let transition = publish_pipeline::archive_agent(&state.db, &db_agent)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!({
        "transition": { "from": transition.from, "to": transition.to },
    })))
}

pub async fn restore_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Not the owner of this agent".to_string(),
        ));
    }

    let transition = publish_pipeline::restore_agent(&state.db, &db_agent)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    Ok(Json(json!({
        "transition": { "from": transition.from, "to": transition.to },
    })))
}
