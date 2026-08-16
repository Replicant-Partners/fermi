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
    MemoryStore, ProviderType,
};
use agent_bestiary_ontology::{GitConfig, GitManager, MermaidGenerator, SnapshotManager};
use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::ExecutionContext;
use fermi::ast;
use std::str::FromStr;

use crate::{resolve_agent, resolve_credential, AppState};

/// Snapshot an agent's ontology after a consolidation cycle.
///
/// Returns the new `snapshot_id`, or `None` if a snapshot could not be made.
/// **Never fails the cycle.** Consolidation's real output is already durable in
/// `entities` / `facts` / `semantic_rules`; a snapshot is provenance and a
/// rendered view of it. The CLI takes the same stance ("Don't fail
/// consolidation if snapshot fails").
///
/// The most common expected failure is `NoEntities`: `MermaidGenerator::generate`
/// refuses to draw a diagram for an agent with no live entities, which a
/// degraded (`?allow_degraded=true`, no extraction model) run can legitimately
/// produce. That is a skip, not an error.
async fn snapshot_ontology(
    state: &AppState,
    agent_id: uuid::Uuid,
    job_id: uuid::Uuid,
) -> Option<uuid::Uuid> {
    // Deliberately separate from `GIT_REPOS_PATH`, which `WorkspaceGitManager`
    // uses. That manager lays out `{base}/workspaces/{slug}` while this one
    // uses `{base}/{agent_name}`, so sharing a root means an agent named
    // `workspaces` collides with the workspace tree.
    let base_path = std::env::var("AGENT_ONTOLOGY_REPOS_PATH")
        .unwrap_or_else(|_| "./repos/ontologies".to_string());

    let git_config = GitConfig {
        base_path,
        author_name: "Fermi ADM".to_string(),
        author_email: "adm@fermi.ai".to_string(),
        branch: "main".to_string(),
        // Push is hardcoded off rather than read from `GIT_AUTO_PUSH`.
        // `GitManager::commit_ontology` is a synchronous fn, and its libgit2
        // push has no timeout — a push to an unreachable host would block a
        // tokio worker thread for the OS TCP timeout. Nothing on the dreaming
        // path is worth that risk, and local commits already produce the real
        // SHA that satisfies `ontology_snapshots.git_commit_sha NOT NULL`.
        github_org: None,
        github_token: None,
        auto_push: false,
        remote_name: "origin".to_string(),
    };

    let git_manager = match GitManager::new(git_config) {
        Ok(g) => g,
        Err(e) => {
            tracing::warn!(
                agent_id = %agent_id, error = %e,
                "[consolidation] git manager unavailable — no ontology snapshot"
            );
            return None;
        }
    };

    // `MemoryStore` is not `Clone`, and `MermaidGenerator` owns one too, so
    // build two from the existing pool. `from_pool` shares the pool rather
    // than opening a second one.
    let mermaid = MermaidGenerator::new(MemoryStore::from_pool(state.db.clone()));
    let manager = SnapshotManager::new(
        MemoryStore::from_pool(state.db.clone()),
        mermaid,
        git_manager,
    );

    match manager.create_snapshot(agent_id, Some(job_id)).await {
        Ok(snapshot_id) => {
            tracing::info!(
                agent_id = %agent_id, %snapshot_id, %job_id,
                "[consolidation] ontology snapshot created"
            );
            Some(snapshot_id)
        }
        Err(e) => {
            tracing::warn!(
                agent_id = %agent_id, error = %e,
                "[consolidation] ontology snapshot skipped"
            );
            None
        }
    }
}

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

#[derive(Debug, serde::Deserialize, Default)]
pub struct ConsolidateQuery {
    /// Run even when no extraction LLM is available.
    ///
    /// A degraded run cannot produce facts or semantic rules — those paths are
    /// gated on the LLM — so it is almost never what an operator wants. Opt-in
    /// only, and the response says plainly what was given up.
    #[serde(default)]
    pub allow_degraded: bool,
}

pub async fn consolidate_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Query(q): Query<ConsolidateQuery>,
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

    // ── Preflight the extractor BEFORE charging or consuming anything ───────
    //
    // `build_extraction_llm` is a chain of `?` on an Option: a missing
    // ontologist card, an unresolvable agent, or — most likely — no funded
    // provider credential for the platform principal all return None.
    //
    // Until now that None was handled by running consolidation ANYWAY with no
    // extractor. The run consumed its episodes, completed the job, debited a
    // dreaming credit and produced nothing. It reported success and learned
    // nothing, which is indistinguishable from a healthy cycle on every
    // surface the platform had.
    //
    // Measured cost of that on this deployment: two batch runs (2026-05-16 and
    // 2026-06-22) burned 91 cycles over ~1,500 episodes for exactly zero
    // entities, facts and rules, and left 62 agents with an empty ontology and
    // no episodes left to retry with.
    //
    // So resolve it here, up front. No extractor means no learning is possible,
    // and the correct response is to refuse before spending anything rather
    // than to succeed at nothing.
    let extraction_llm = build_extraction_llm(&state).await;
    if extraction_llm.is_none() && !q.allow_degraded {
        return Err((
            // 424: the request is fine, a dependency it needs is not.
            StatusCode::FAILED_DEPENDENCY,
            "No extraction model available for consolidation. The `ontologist` agent's \
             provider credential could not be resolved, so this cycle could consolidate \
             episodes but could not extract any entities, facts or rules from them — it \
             would consume the episodes and learn nothing. Fund the ontologist's provider \
             credential for the platform principal, or pass ?allow_degraded=true to run \
             anyway (heuristic entities only; no facts, no rules)."
                .to_string(),
        ));
    }
    let degraded = extraction_llm.is_none();

    // Only charge gas after confirming there's work to do AND that the work can
    // actually produce something.
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
    let spawn_llm = extraction_llm;

    // Create the job row NOW, under the id we are about to hand the client.
    //
    // This used to be missing entirely, and the consequence was the most
    // confusing possible failure: consolidation worked perfectly — episodes
    // consolidated, rules extracted, a job row created, updated and completed
    // — but the worker did all of that under an id it generated internally,
    // while the client was handed this fabricated one. Every status poll and
    // every refresh looked up a row that had never existed, so the UI reported
    // success and then showed nothing. Both writes below were `let _ =`, so
    // "0 rows affected" was invisible.
    //
    // Creating it here also means the job is visible as `running` the instant
    // the client gets its 202, instead of only appearing once the work ends.
    state
        .memory_store
        .create_consolidation_job_with_id(
            job_id,
            db_agent.agent_id,
            episodes[0].episode_id,
            episodes[episodes.len() - 1].episode_id,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create consolidation job: {}", e),
            )
        })?;
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
        // Already resolved (and required) before gas was charged; reuse it so
        // the run cannot silently differ from what preflight approved.
        let worker = match spawn_llm {
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

        // Pass the client's job_id through so the worker's own statistics and
        // completion land on the row the client is polling.
        match worker
            .consolidate_agent_with_job(spawn_agent_id, 0.5, 2, Some(job_id))
            .await
        {
            Ok(result) => {
                // Debit dreaming credit. Logged rather than swallowed: silently
                // failing to debit means the budget shown to the operator drifts
                // from what was actually spent, and `last_consolidated_at` is
                // what the maturity view reads to say when an agent last dreamt.
                if let Err(e) = sqlx::query(
                    "UPDATE agents SET dreaming_credits_used = dreaming_credits_used + 1, \
                     last_consolidated_at = NOW() WHERE agent_id = $1",
                )
                .bind(spawn_agent_id)
                .execute(&spawn_state.db)
                .await
                {
                    tracing::error!(
                        agent_id = %spawn_agent_id, error = %e,
                        "[consolidation] failed to debit dreaming credit / stamp \
                         last_consolidated_at — budget and maturity will read stale"
                    );
                }

                // The worker already recorded statistics and marked the job
                // completed against this same job_id, so there is nothing to
                // update here. Re-completing it would only risk clobbering the
                // worker's numbers with a second write.

                // Snapshot the ontology so it visibly develops over time.
                //
                // Until now `create_snapshot` had exactly one call site, in the
                // standalone `consolidate` CLI, so no agent dreamt through the
                // API ever produced a snapshot row. That left the Mermaid
                // diagram, the git provenance and `evolution_commits` frozen at
                // nothing regardless of how much the agent learned — and it
                // made the narrator's `UPDATE ontology_snapshots` below a
                // permanent no-op, because it targeted a row nothing inserted.
                let snapshot_id = snapshot_ontology(&spawn_state, spawn_agent_id, job_id).await;

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
                    // SPEC_28 — dream_narrator is a platform-service agent;
                    // funded from the `abw-system` principal's store.
                    let credentials = match crate::resolve_agent(&narrator_state, &narrator_id)
                        .await
                    {
                        Ok(db_agent) => {
                            crate::build_execution_credentials(&narrator_state, &db_agent, &card)
                                .await
                        }
                        Err(_) => {
                            fermi::agent_backend::credentials::ResolvedCredentials::unfunded_arc()
                        }
                    };

                    let context = ExecutionContext {
                        program,
                        agent_card: card,
                        creature_id: None,
                        cognition_tier: None,
                        credentials,
                    };
                    if let Ok(output) = narrator_state
                        .registry
                        .execute_agent(&agent_stmt, &context)
                        .await
                    {
                        let narrative = output.metadata.reasoning.unwrap_or_default();
                        // Target the snapshot this cycle produced, by id.
                        // `ORDER BY version DESC LIMIT 1` was both a no-op
                        // (nothing inserted rows) and a race: `version` is a
                        // read-modify-write with no unique constraint, so two
                        // concurrent cycles can share a version and the
                        // synopsis could land on the wrong row.
                        if let (false, Some(sid)) = (narrative.is_empty(), snapshot_id) {
                            if let Err(e) = sqlx::query(
                                "UPDATE ontology_snapshots SET dream_synopsis = $1 \
                                 WHERE snapshot_id = $2",
                            )
                            .bind(&narrative)
                            .bind(sid)
                            .execute(&narrator_state.db)
                            .await
                            {
                                tracing::warn!(
                                    snapshot_id = %sid, error = %e,
                                    "[consolidation] failed to store dream synopsis"
                                );
                            }
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
            "message": if degraded {
                "Consolidation started in DEGRADED mode: no extraction model, so no facts \
                 or semantic rules can be produced. Episodes will be left unconsolidated \
                 so they can be re-dreamt once an extractor is available."
            } else {
                "Consolidation started."
            },
            // Surfaced so a caller can tell "ran" from "ran and could learn".
            // Conflating those is what let 91 cycles report success while
            // extracting nothing.
            "degraded": degraded,
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
