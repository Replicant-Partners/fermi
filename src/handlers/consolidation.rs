//! Consolidation, dreaming budget, and episodes handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{credit_charge, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;

use agent_bestiary_memory::{
    ConsolidationLock, ConsolidationWorker, EmbeddingGenerator, LLMProviderConfig,
    LLMProviderFactory, ProviderType,
};
use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::{resolve_agent, resolve_agent_card, AppState};
// ─── Dreaming budget ───────────────────────────────────────────────

pub async fn get_dreaming_budget(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    Ok(Json(json!({
        "agent_id": agent_id,
        "agent_uuid": db_agent.agent_id,
        "budget_credits": db_agent.dreaming_budget_credits,
        "credits_used": db_agent.dreaming_credits_used,
        "credits_remaining": db_agent.dreaming_budget_credits - db_agent.dreaming_credits_used,
        "budget_reset_at": db_agent.dreaming_budget_reset_at,
        "last_consolidated_at": db_agent.last_consolidated_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct SetBudgetRequest {
    budget_credits: i32,
}

pub async fn set_dreaming_budget(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<SetBudgetRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    sqlx::query(
        "UPDATE agents SET dreaming_budget_credits = $1, dreaming_credits_used = 0, dreaming_budget_reset_at = NOW() WHERE agent_id = $2",
    )
    .bind(body.budget_credits)
    .bind(db_agent.agent_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "updated",
        "agent_id": agent_id,
        "budget_credits": body.budget_credits,
    })))
}

// ─── Paginated episodes ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct EpisodesParams {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn get_agent_episodes_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(params): Query<EpisodesParams>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let (episodes, total) = state
        .memory_store
        .get_episodes_paginated(db_agent.agent_id, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let episodes_json: Vec<Value> = episodes
        .iter()
        .map(|ep| {
            json!({
                "episode_id": ep.episode_id,
                "timestamp": ep.timestamp_ref,
                "query": ep.query,
                "status": ep.execution_status.to_string(),
                "error_details": ep.error_details,
                "execution_time_ms": ep.execution_time_ms,
                "tokens_used": ep.tokens_used,
                "cost_usd": ep.cost_usd,
                "consolidated": ep.consolidated,
                "tags": ep.tags,
            })
        })
        .collect();

    Ok(Json(json!({
        "episodes": episodes_json,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

// ─── Dream budget top-up ───────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TopupBudgetRequest {
    credits: i32,
}

pub async fn topup_dreaming_budget_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<TopupBudgetRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let user_id = principal.user_id();

    // Only owner can top up
    if db_agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "Only the agent owner can top up dream budget".into(),
        ));
    }

    let credits = body.credits.max(1).min(1000);

    // Charge from wallet
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if wallet.balance < credits {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "Insufficient credits: need {}, have {}",
                credits, wallet.balance
            ),
        ));
    }

    credit_charge(
        &state.db,
        wallet.wallet_id,
        credits,
        "dream_topup",
        &format!("Dream budget top-up for agent {}", agent_id),
        None,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Increase dreaming budget
    let new_budget = db_agent.dreaming_budget_credits + credits;
    sqlx::query("UPDATE agents SET dreaming_budget_credits = $1 WHERE agent_id = $2")
        .bind(new_budget)
        .bind(db_agent.agent_id)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "status": "topped_up",
        "agent_id": agent_id,
        "credits_added": credits,
        "new_budget": new_budget,
        "credits_used": db_agent.dreaming_credits_used,
        "credits_remaining": new_budget - db_agent.dreaming_credits_used,
    })))
}

// ─── Consolidation trigger ─────────────────────────────────────────

pub async fn consolidate_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Charge gas fee from caller's wallet
    let wallet = get_or_create_wallet(&state.db, "user", &user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    charge_gas(
        &state.db,
        wallet.wallet_id,
        state.gas_fees.consolidation_cycle,
        "gas_fee",
        &format!("Consolidation gas for agent {}", agent_id),
        Some(&agent_id),
    )
    .await?;

    // Check dreaming budget
    let remaining = db_agent.dreaming_budget_credits - db_agent.dreaming_credits_used;
    if remaining <= 0 {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            format!(
                "No dreaming credits remaining (used {}/{})",
                db_agent.dreaming_credits_used, db_agent.dreaming_budget_credits
            ),
        ));
    }

    // Check for unconsolidated episodes first (avoids spending a credit on empty runs)
    let episodes = state
        .memory_store
        .get_unconsolidated_episodes(db_agent.agent_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to fetch episodes: {}", e),
            )
        })?;

    if episodes.is_empty() {
        return Ok(Json(json!({
            "status": "completed",
            "agent_id": agent_id,
            "result": {
                "episodes_processed": 0,
                "clusters_identified": 0,
                "rules_extracted": 0,
                "message": "No unconsolidated episodes found"
            },
            "dreaming_credits_remaining": remaining,
        })));
    }

    // Create consolidation worker and run (with LLM if API key available)
    let pool = Arc::new(state.db.clone());
    let lock = Arc::new(ConsolidationLock::new(
        pool,
        format!("api-{}", uuid::Uuid::new_v4()),
    ));
    let worker = if let Ok(api_key) = std::env::var("ANTHROPIC_API_KEY") {
        match LLMProviderFactory::create(&LLMProviderConfig {
            provider_type: ProviderType::Anthropic,
            api_key,
            model: "claude-haiku-4-5-20251001".to_string(),
            base_url: None,
        }) {
            Ok(llm) => ConsolidationWorker::with_llm(
                state.memory_store.clone(),
                lock,
                state.embedder.clone(),
                llm,
                "api-trigger".to_string(),
            ),
            Err(_) => ConsolidationWorker::new(
                state.memory_store.clone(),
                lock,
                state.embedder.clone(),
                "api-trigger".to_string(),
            ),
        }
    } else {
        ConsolidationWorker::new(
            state.memory_store.clone(),
            lock,
            state.embedder.clone(),
            "api-trigger".to_string(),
        )
    };

    let result = worker
        .consolidate_agent(db_agent.agent_id, 0.5, 2)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Consolidation failed: {}", e),
            )
        })?;

    // Debit dreaming credit
    sqlx::query(
        "UPDATE agents SET dreaming_credits_used = dreaming_credits_used + 1, last_consolidated_at = NOW() WHERE agent_id = $1",
    )
    .bind(db_agent.agent_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Spawn dream narrator to turn consolidation results into a narrative
    {
        let narrator_state = state.clone();
        let agent_name = agent_id.clone();
        let agent_db_id = db_agent.agent_id;
        let ep = result.episodes_processed;
        let cl = result.clusters_identified;
        let rx = result.rules_extracted;
        let rv = result.rules_verified;
        let ec = result.entities_created;
        let fc = result.facts_created;
        tokio::spawn(async move {
            let narrator_id = "dream_narrator";
            let card = match narrator_state.registry.get(narrator_id) {
                Ok(c) => c,
                Err(_) => return, // narrator not available
            };
            let synopsis_input = format!(
                "Agent \"{}\" just completed a consolidation cycle (dreaming). \
                 Results: {} episodes processed, {} clusters identified, {} rules extracted, \
                 {} rules verified, {} entities created, {} facts created. \
                 Write a brief, engaging narrative about what this agent dreamed.",
                agent_name, ep, cl, rx, rv, ec, fc
            );
            let agent_stmt = ast::AgentStmt {
                name: narrator_id.to_string(),
                agent_type: Some(card.agent_type.clone()),
                query: synopsis_input,
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
            };
            match narrator_state
                .registry
                .execute_agent(&agent_stmt, &context)
                .await
            {
                Ok(output) => {
                    let narrative = output.metadata.reasoning.unwrap_or_default();
                    if narrative.is_empty() {
                        return;
                    }
                    // Store on the latest ontology snapshot for this agent
                    let _ = sqlx::query(
                        "UPDATE ontology_snapshots SET dream_synopsis = $1 \
                         WHERE agent_id = $2 AND snapshot_id = (\
                           SELECT snapshot_id FROM ontology_snapshots \
                           WHERE agent_id = $2 ORDER BY version DESC LIMIT 1\
                         )",
                    )
                    .bind(&narrative)
                    .bind(agent_db_id)
                    .execute(&narrator_state.db)
                    .await;
                    eprintln!("Dream narrator: wrote synopsis for {}", agent_name);
                }
                Err(e) => {
                    eprintln!("Dream narrator failed for {}: {:?}", agent_name, e);
                }
            }
        });
    }

    Ok(Json(json!({
        "status": "completed",
        "agent_id": agent_id,
        "result": {
            "episodes_processed": result.episodes_processed,
            "clusters_identified": result.clusters_identified,
            "rules_extracted": result.rules_extracted,
            "rules_verified": result.rules_verified,
            "rules_rejected": result.rules_rejected,
            "entities_created": result.entities_created,
            "facts_created": result.facts_created,
        },
        "dreaming_credits_remaining": remaining - 1,
    })))
}
