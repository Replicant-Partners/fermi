//! Consolidation, dreaming budget, and episodes handlers.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge, get_or_create_wallet, rbac, AuthPrincipal, ObjectType, Visibility,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;

use agent_bestiary_memory::{
    ConsolidationLock, ConsolidationWorker, LLMProvider, LLMProviderConfig, LLMProviderFactory,
    ProviderType,
};
use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::ExecutionContext;
use fermi::ast;
use std::str::FromStr;

use crate::{resolve_agent, resolve_credential, AppState};

/// Resolve a member of the `dream_coordinator` compound by what it produces.
/// The coordinator card names its members declaratively (its `dependencies`);
/// we pick the member whose card declares it produces `produces_label`. Swap
/// the members in dream_coordinator's card and this pipeline follows. Falls
/// back to `default_name` when the coordinator or member is unavailable.
fn dream_member(state: &AppState, produces_label: &str, default_name: &str) -> String {
    let Ok(coord) = state.registry.get("dream_coordinator") else {
        return default_name.to_string();
    };
    for name in &coord.dependencies.required {
        if let Ok(member) = state.registry.get(name) {
            if member.produces.iter().any(|p| p == produces_label) {
                return name.clone();
            }
        }
    }
    default_name.to_string()
}

/// Build the extraction "brain" for consolidation from the dream_coordinator's
/// declared EXTRACT member (whichever produces `semantic-rules` — the
/// `ontologist` by default). Provider + model come from that member's card
/// (not hardcoded); the API key resolves from the credential store — tier=system
/// routes to the `abw-system` principal, so the platform funds learning via its
/// system key. Returns `None` (unresolved / unfunded / unknown provider) so the
/// worker falls back to pattern-based extraction instead of crashing.
async fn build_extraction_llm(state: &AppState) -> Option<Arc<dyn LLMProvider>> {
    let extractor = dream_member(state, "semantic-rules", "ontologist");
    let card = state.registry.get(&extractor).ok()?;
    let provider = card.capabilities.provider.clone();
    let model = card.capabilities.model.clone();
    let db_agent = resolve_agent(state, &extractor).await.ok()?;
    let api_key = resolve_credential(state, &db_agent, &provider).await?;
    let provider_type = ProviderType::from_str(&provider).ok()?;
    LLMProviderFactory::create(&LLMProviderConfig {
        provider_type,
        api_key,
        model,
        base_url: None,
    })
    .ok()
}
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

    // v0.10.5: substrate RBAC. Dreaming top-up debits the caller's
    // wallet and credits the agent's budget — Admin (owner or
    // platform admin) only.
    rbac::require_admin_on(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        Visibility::Private,
    )
    .await?;

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
) -> Result<(StatusCode, Json<Value>), (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;

    // Check dreaming budget BEFORE charging
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

    // Check for unconsolidated episodes BEFORE charging
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
        return Ok((
            StatusCode::OK,
            Json(json!({
                "status": "completed",
                "agent_id": agent_id,
                "result": {
                    "episodes_processed": 0,
                    "clusters_identified": 0,
                    "rules_extracted": 0,
                    "message": "No unconsolidated episodes found"
                },
                "dreaming_credits_remaining": remaining,
            })),
        ));
    }

    // Only charge gas after confirming there's work to do
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

    // Phase 3.1 (Spec 21): spawn consolidation as background job, return 202 immediately.
    // Gas is charged before spawn so a job that never starts still has a visible charge.
    let job_id = uuid::Uuid::new_v4();
    let spawn_state = state.clone();
    let spawn_agent_id = db_agent.agent_id;
    let spawn_agent_name = agent_id.clone();
    let spawn_remaining = remaining;

    tokio::spawn(async move {
        let pool = Arc::new(spawn_state.db.clone());
        let lock = Arc::new(ConsolidationLock::new(pool, format!("api-{}", job_id)));
        // Extraction brain = the `ontologist` system agent (card-configured
        // provider/model, funded by abw-system's stored key). No env-var key
        // path — that was the old system-tier shortcut this model replaces.
        let worker = match build_extraction_llm(&spawn_state).await {
            Some(llm) => ConsolidationWorker::with_llm(
                spawn_state.memory_store.clone(),
                lock,
                spawn_state.embedder.clone(),
                llm,
                format!("api-{}", job_id),
            ),
            None => ConsolidationWorker::new(
                spawn_state.memory_store.clone(),
                lock,
                spawn_state.embedder.clone(),
                format!("api-{}", job_id),
            ),
        };

        match worker.consolidate_agent(spawn_agent_id, 0.5, 2).await {
            Ok(result) => {
                // Debit dreaming credit
                let _ = sqlx::query(
                    "UPDATE agents SET dreaming_credits_used = dreaming_credits_used + 1, \
                     last_consolidated_at = NOW() WHERE agent_id = $1",
                )
                .bind(spawn_agent_id)
                .execute(&spawn_state.db)
                .await;

                // Update job record if it exists
                let _ = spawn_state
                    .memory_store
                    .update_consolidation_job(
                        job_id,
                        result.episodes_processed as i32,
                        result.clusters_identified as i32,
                        result.rules_extracted as i32,
                        result.rules_verified as i32,
                        result.rules_rejected as i32,
                        result.entities_created as i32,
                        result.facts_created as i32,
                    )
                    .await;
                let _ = spawn_state
                    .memory_store
                    .complete_consolidation_job(job_id, "completed", None)
                    .await;

                // Spawn dream narrator
                let ep = result.episodes_processed;
                let cl = result.clusters_identified;
                let rx = result.rules_extracted;
                let rv = result.rules_verified;
                let ec = result.entities_created;
                let fc = result.facts_created;
                let narrator_state = spawn_state.clone();
                let aname = spawn_agent_name.clone();
                tokio::spawn(async move {
                    // Declarative: the narrator is the dream_coordinator member
                    // that produces `dream-synopsis` (dream_narrator by default).
                    let narrator_id =
                        dream_member(&narrator_state, "dream-synopsis", "dream_narrator");
                    let card = match narrator_state.registry.get(&narrator_id) {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    let synopsis_input = format!(
                        "Agent \"{}\" just completed a consolidation cycle (dreaming). \
                         Results: {} episodes processed, {} clusters identified, {} rules extracted, \
                         {} rules verified, {} entities created, {} facts created. \
                         Write a brief, engaging narrative about what this agent dreamed.",
                        aname, ep, cl, rx, rv, ec, fc
                    );
                    let agent_stmt = ast::AgentStmt {
                        name: narrator_id.clone(),
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
                        creature_id: None,
                        cognition_tier: None,
                    };
                    if let Ok(output) = narrator_state
                        .registry
                        .execute_agent(&agent_stmt, &context)
                        .await
                    {
                        let narrative = output.metadata.reasoning.unwrap_or_default();
                        if !narrative.is_empty() {
                            let _ = sqlx::query(
                                "UPDATE ontology_snapshots SET dream_synopsis = $1 \
                                 WHERE agent_id = $2 AND snapshot_id = (\
                                   SELECT snapshot_id FROM ontology_snapshots \
                                   WHERE agent_id = $2 ORDER BY version DESC LIMIT 1)",
                            )
                            .bind(&narrative)
                            .bind(spawn_agent_id)
                            .execute(&narrator_state.db)
                            .await;
                        }
                    }
                });
            }
            Err(e) => {
                tracing::error!(agent_id = %spawn_agent_id, error = %e, "consolidation failed");
                let _ = spawn_state
                    .memory_store
                    .complete_consolidation_job(job_id, "failed", Some(e.to_string()))
                    .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "accepted",
            "job_id": job_id,
            "agent_id": agent_id,
            "message": "Consolidation started.",
            "poll": format!("/api/agents/{}/consolidation/jobs/{}", agent_id, job_id),
            "dreaming_credits_remaining": spawn_remaining - 1,
        })),
    ))
}

/// GET /api/agents/:id/consolidation/jobs/:job_id
pub async fn get_consolidation_job_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, job_id_str)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let _user_id = principal.user_id();
    // v0.10.5: substrate RBAC. Consolidation job details are
    // owner-scoped read — View permission via owner + platform admin.
    // No public/shared branch because job telemetry can leak agent
    // internals.
    rbac::require_view(
        &state.db,
        &principal,
        ObjectType::Agent,
        &db_agent.agent_id.to_string(),
        db_agent.owner_id.as_deref().unwrap_or(""),
        Visibility::Private,
    )
    .await?;
    let job_id: uuid::Uuid = job_id_str
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid job_id".into()))?;

    let row = sqlx::query(
        "SELECT job_id, status, episodes_processed, clusters_identified, rules_extracted,
                rules_verified, rules_rejected, entities_created, facts_created,
                error_message, started_at, completed_at
         FROM consolidation_jobs WHERE job_id = $1 AND agent_id = $2",
    )
    .bind(job_id)
    .bind(db_agent.agent_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Job not found".into()))?;

    Ok(Json(json!({
        "job_id": job_id,
        "agent_id": agent_id,
        "status": row.try_get::<String,_>("status").unwrap_or_default(),
        "episodes_processed": row.try_get::<i32,_>("episodes_processed").unwrap_or(0),
        "clusters_identified": row.try_get::<i32,_>("clusters_identified").unwrap_or(0),
        "rules_extracted": row.try_get::<i32,_>("rules_extracted").unwrap_or(0),
        "rules_verified": row.try_get::<i32,_>("rules_verified").unwrap_or(0),
        "entities_created": row.try_get::<i32,_>("entities_created").unwrap_or(0),
        "facts_created": row.try_get::<i32,_>("facts_created").unwrap_or(0),
        "error_message": row.try_get::<Option<String>,_>("error_message").ok().flatten(),
        "started_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("started_at").ok().flatten(),
        "completed_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("completed_at").ok().flatten(),
    })))
}
