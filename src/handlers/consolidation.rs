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

/// Write one episode recording that a dream-pipeline member did work.
///
/// WHY THIS EXISTS
///
/// The dreaming pipeline ran entirely off-ledger. `build_extraction_llm`
/// reads the ontologist's card only to pick a provider, a model and whose
/// credential pays, then hands the worker a bare `Arc<dyn LLMProvider>`; the
/// worker calls `generate_raw` directly. No agent execution ever occurs. The
/// narrator is invoked through `registry.execute_agent`, but episodes are
/// written by *handlers*, and this one never wrote any.
///
/// Everything the ontologist produces is stamped with the SUBJECT agent's id
/// (`SemanticRule { agent_id, .. }` in the memory crate), and even the gas is
/// charged against the subject. So the platform's most frequently-invoked
/// system agent showed zero episodes, zero executions and zero cost on every
/// surface, while visibly producing entities for everyone else. The
/// observatory was not lying; there was genuinely nothing to show.
///
/// Three consequences this fixes:
///   * the ontologist could never be evaluated, and — having no episodes —
///     could never itself be consolidated. Loop 1 was structurally closed to
///     the one agent that powers Loop 1 for everybody else.
///   * its token spend was unattributable.
///   * extraction quality had no per-cycle record; `rules_rejected` was the
///     only proxy.
///
/// GRANULARITY: one episode per member per cycle, not per LLM call. A cycle
/// makes one extraction call per cluster plus batches for entities and facts,
/// so per-call would write ~25 rows for a 20-cluster cycle and hand the
/// ontologist a backlog that grows faster than it could ever dream it down.
/// Per-cycle keeps the volume bounded, matches the grain `consolidation_jobs`
/// already records at, and is the right unit to evaluate: "did this cycle
/// extract well?" is a question worth scoring; "did call 14 of 25 parse?" is
/// not.
///
/// Never fails the cycle. A missing ledger row is bad; losing a completed
/// consolidation because bookkeeping failed is worse.
#[allow(clippy::too_many_arguments)]
async fn record_dream_member_episode(
    state: &AppState,
    member_name: &str,
    role: &str,
    subject_id: uuid::Uuid,
    subject_name: &str,
    job_id: uuid::Uuid,
    query: String,
    outcome: Value,
    usage: agent_bestiary_memory::ExtractorUsage,
    model: Option<String>,
    provider: Option<String>,
    status: agent_bestiary_memory::ExecutionStatus,
    error: Option<String>,
    elapsed_ms: i64,
) {
    let Ok(member) = resolve_agent(state, member_name).await else {
        tracing::warn!(
            member = member_name,
            "[dream-ledger] member not resolvable; its work stays unrecorded"
        );
        return;
    };

    let mut tags = vec![
        "dream_pipeline".to_string(),
        format!("role:{}", role),
        // Queryable both ways: what this member worked on, and which cycle.
        format!("subject:{}", subject_name),
        format!("job:{}", job_id),
        match status {
            agent_bestiary_memory::ExecutionStatus::Success => "status:success",
            agent_bestiary_memory::ExecutionStatus::Failure => "status:error",
            _ => "status:partial",
        }
        .to_string(),
    ];
    if let Some(m) = &model {
        tags.push(format!("model:{}", m));
    }

    let episode = agent_bestiary_memory::Episode {
        episode_id: uuid::Uuid::new_v4(),
        agent_id: member.agent_id,
        parent_episode_id: None,
        timestamp_ref: chrono::Utc::now(),
        query,
        context: json!({
            "kind": "dream_pipeline",
            "role": role,
            "job_id": job_id,
            // The work belongs to the member; the OUTPUT belongs to the
            // subject. Recording both ends means the split is legible instead
            // of being something you have to know.
            "subject_agent_id": subject_id,
            "subject_agent_name": subject_name,
            "outcome": outcome,
            "llm_calls": usage.calls,
            "model_used": model,
            "provider": provider,
            "funding_principal": "abw-system",
        }),
        execution_status: status,
        error_details: error,
        execution_time_ms: elapsed_ms,
        // Cost lives on the PER-CALL rows, not here.
        //
        // Both are episodes for the same agent, so `agent_execution_rollup` sums
        // them: carrying the cycle total here as well as on each call would
        // double every figure the extractor reports. The per-call rows are the
        // more accurate ledger anyway — that is where the spend is actually
        // incurred, one round-trip at a time — so this row keeps the aggregate in
        // `context` for reading and contributes nothing to the sums.
        tokens_used: None,
        input_tokens: None,
        output_tokens: None,
        cost_usd: None,
        cost_basis: None,
        cost_rate_key: None,
        // No embedding, and pre-consolidated. Both deliberate, and they go
        // together. (The per-call rows written by `record_extraction_call_episodes`
        // are the learning material; this row is the cycle marker.)
        //
        // This row exists to make the work VISIBLE and COSTED, not to be
        // learned from. Its `query` is a template that differs only in the
        // subject's name, so embedding it would cluster every cycle together
        // on the boilerplate and extract a rule about the sentence rather than
        // about the work. Left unembedded it would be DBSCAN noise anyway
        // (`find_neighbors` returns nothing without a vector), so it would be
        // consumed by a cycle that then reported zero yield — which Loop 1a
        // correctly reads as a fault, and it would be a fault I manufactured.
        //
        // So it never enters the consolidation queue: no phantom backlog on
        // the Dreaming tab, no zero-yield cycles fabricated out of
        // bookkeeping.
        //
        // The honest limitation: this does NOT let the ontologist learn from
        // its own extraction work. Doing that needs the real prompts and
        // responses, which is a separate decision about storing extraction
        // transcripts — not something to inherit silently from a ledger row.
        embedding: None,
        consolidated: true,
        tags,
        // The platform observed its own pipeline. Not a human assertion.
        provenance: agent_bestiary_memory::Provenance::AutoPass,
        authority_weight: 0.5,
        dyad_id: None,
        persona_version_at_write: Some(member.persona_version),
        provider_used: provider,
        model_used: model,
        response_text: None,
        assertions: None,
    };

    let source_ref = json!({
        "kind": "dream_pipeline",
        "role": role,
        "job_id": job_id,
        "subject_agent_id": subject_id,
    });

    if let Err(e) = state
        .memory_store
        .store_episode_with_provenance(episode, None, Some(source_ref))
        .await
    {
        tracing::warn!(
            member = member_name, error = %e,
            "[dream-ledger] failed to record member episode"
        );
    }
}

/// Resolve a member of the `dream_coordinator` compound by what it produces.
/// The coordinator card names its members declaratively (its `dependencies`);
/// we pick the member whose card declares it produces `produces_label`. Swap
/// the members in dream_coordinator's card and this pipeline follows. Falls
/// back to `default_name` when the coordinator or member is unavailable.
/// Is this agent a declared member of the dreaming pipeline?
///
/// Such an agent is invoked as a *service* — the ontologist as a bare
/// `LLMProvider`, the narrator through the registry — so before the dream
/// ledger existed neither produced episodes, and both read as "never executed"
/// on every surface. The loop-health panel needs to tell that apart from an
/// agent that genuinely has never run, because the remedies are opposite: one
/// needs invoking, the other has been running all along.
///
/// Resolved from `dream_coordinator`'s declared dependencies rather than a
/// hardcoded name list, for the same reason `dream_member` is: swap the members
/// on the card and this follows.
pub(crate) fn is_dream_pipeline_member(state: &AppState, agent_name: &str) -> bool {
    let Ok(coord) = state.registry.get("dream_coordinator") else {
        // Fall back to the defaults `dream_member` would have used, so the
        // distinction does not silently vanish when the coordinator card is
        // unavailable.
        return matches!(agent_name, "ontologist" | "dream_narrator");
    };
    coord.dependencies.required.iter().any(|n| n == agent_name)
        || coord.dependencies.optional.iter().any(|n| n == agent_name)
}

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
/// How many of the extractor's own rules ride along on each extraction call.
///
/// Shared by the API path and the batch CLI so the extractor cannot behave
/// differently depending on which entry point invoked it.
pub(crate) const EXTRACTOR_GUIDANCE_LIMIT: i64 = 20;

/// How long a rule gets to be retrieved before its silence counts against the
/// extractor.
///
/// This is the resolution delay, and it is the only judgement call in the
/// signal. Too short and every rule looks useless because nothing has had cause
/// to recall it yet; too long and the extractor gets no feedback for a month.
/// Seven days is chosen to be an obvious round number rather than a tuned one —
/// same reasoning as `rate_card::ASSUMED_OUTPUT_SHARE`. It belongs here, once,
/// not re-derived at the query.
const RULE_RESOLUTION_DAYS: i64 = 7;

/// Emit the extractor's utility signal: of the rules it wrote that have had a
/// fair chance to be recalled, how many actually were.
///
/// ## Why this is the right signal
///
/// The question "was this rule true?" has no cheap answer, and the obvious
/// substitute — asking the extractor to grade its own output — is worthless:
/// the model that hallucinated a rule is the model judging it, so the errors are
/// correlated and the resulting number would look confident while measuring
/// nothing.
///
/// "Did this rule turn out to be worth recalling?" does have an answer, and the
/// platform now records it (`application_count`, written by
/// `kg_context::record_rule_retrievals`). It resolves late, like a forecast,
/// which is the point: it cannot be gamed at write time because it is not known
/// at write time.
///
/// ## What is deliberately excluded
///
/// * `extracted_by IS NULL` — every rule written before migration 201, plus
///   pattern-fallback rules the extractor had no hand in. NULL means "author
///   unrecorded", never "author is this agent", so attributing them would
///   credit or blame the ontologist for work it did not do.
/// * rules younger than [`RULE_RESOLUTION_DAYS`] that have never been retrieved
///   — unresolved, not unsuccessful. Counting them would make every extractor
///   look bad immediately after every cycle, which is precisely backwards.
///   A young rule that HAS been retrieved is resolved: the outcome arrived early.
///
/// Emits nothing below `MIN_RESOLVED`: a score over two rules is noise wearing a
/// number's clothes, and Loop 1a would read it as a turning loop.
/// Below this many resolved rules, say nothing rather than something meaningless.
const MIN_RESOLVED_RULES: i64 = 5;

/// Score and confidence from resolved-rule counts, or `None` when the evidence
/// is too thin to report.
///
/// Pure so the two judgement calls in this signal — when to stay silent, and how
/// much to trust a small sample — are testable without a database. Emitting a
/// score over two rules would be noise wearing a number's clothes, and Loop 1a
/// would read it as a turning loop.
fn extraction_utility(resolved: i64, retrieved: i64) -> Option<(f64, f64)> {
    if resolved < MIN_RESOLVED_RULES {
        return None;
    }
    let score = (retrieved as f64 / resolved as f64).clamp(0.0, 1.0);
    // Confidence grows with evidence and saturates. A 6-rule score and a
    // 600-rule score are both real and are not equally trustworthy, and
    // `eval_signals.confidence` is the field that already carries that.
    let confidence = (resolved as f64 / 50.0).clamp(0.1, 1.0);
    Some((score, confidence))
}

async fn emit_extraction_utility_signal(state: &AppState, extractor_id: uuid::Uuid) {
    let row = sqlx::query(
        "SELECT COUNT(*)                                            AS resolved,
                COUNT(*) FILTER (WHERE application_count > 0)        AS retrieved,
                COUNT(*) FILTER (WHERE application_count = 0)        AS ignored
           FROM semantic_rules
          WHERE extracted_by = $1
            AND is_active
            AND invalidated_at IS NULL
            AND (application_count > 0
                 OR created_at < NOW() - ($2 || ' days')::interval)",
    )
    .bind(extractor_id)
    .bind(RULE_RESOLUTION_DAYS.to_string())
    .fetch_optional(&state.db)
    .await;

    let Ok(Some(row)) = row else {
        if let Err(e) = row {
            tracing::warn!(error = %e, "[extraction-utility] query failed");
        }
        return;
    };

    let resolved: i64 = row.try_get("resolved").unwrap_or(0);
    let retrieved: i64 = row.try_get("retrieved").unwrap_or(0);
    let Some((score, confidence)) = extraction_utility(resolved, retrieved) else {
        tracing::info!(
            resolved,
            needed = MIN_RESOLVED_RULES,
            "[extraction-utility] too few resolved rules to score — emitting nothing"
        );
        return;
    };

    // The rationale carries the identifying key, exactly as the Brier resolver
    // does, so the `NOT EXISTS` guard below makes re-emission idempotent for a
    // given (extractor, n_resolved) pair. A later cycle with more resolved rules
    // writes a new row; the same cycle re-run writes nothing.
    let rationale = format!(
        "extraction utility: {retrieved}/{resolved} rules retrieved at least once \
         (resolution window {RULE_RESOLUTION_DAYS}d)"
    );

    let res = sqlx::query(
        "INSERT INTO eval_signals
              (agent_id, evaluator_name, evaluator_version, evaluator_tier,
               dimension, score, confidence, rationale, created_at)
         SELECT $1, 'extraction_utility_resolver', 'v1', 'dimensional',
                'extraction_utility', $2, $3, $4, NOW()
          WHERE NOT EXISTS (
              SELECT 1 FROM eval_signals
               WHERE agent_id = $1
                 AND dimension = 'extraction_utility'
                 AND rationale = $4
          )",
    )
    .bind(extractor_id)
    .bind(score)
    .bind(confidence)
    .bind(&rationale)
    .execute(&state.db)
    .await;

    match res {
        Ok(r) if r.rows_affected() > 0 => tracing::info!(
            extractor = %extractor_id, score, resolved, retrieved,
            "[extraction-utility] signal emitted — Loop 1 now has a signal half for the extractor"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!(error = %e, "[extraction-utility] emit failed"),
    }
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

    // Which provider is about to be billed for extraction. Read from the same
    // card `build_extraction_llm` used, so the ledger entry prices against the
    // provider that actually served the call rather than a guess.
    let extractor_name = dream_member(&state, "semantic-rules", "ontologist");
    let extractor_provider = state
        .registry
        .get(&extractor_name)
        .ok()
        .map(|c| c.capabilities.provider.clone());

    // Read the extractor's own learned rules back to it. Resolved here rather
    // than in the spawned task so a failure to look them up is visible on the
    // request path instead of vanishing into a background job.
    // Resolve the extractor once, for both halves of its Loop 1:
    //   identity — so rules it writes can be credited to it (migration 201)
    //   guidance — so it can read back what it has already learned
    let extractor_db = resolve_agent(&state, &extractor_name).await.ok();
    let extractor_identity = extractor_db.as_ref().map(|e| e.agent_id);
    let extractor_guidance = match &extractor_db {
        Some(e) => {
            agent_bestiary_memory::extractor_self_knowledge(
                &state.db,
                e.agent_id,
                EXTRACTOR_GUIDANCE_LIMIT,
            )
            .await
        }
        None => None,
    };
    if extractor_identity.is_none() {
        tracing::warn!(
            extractor = %extractor_name,
            "[consolidation] extractor not resolvable — rules from this cycle will be \
             unattributed and cannot contribute to its utility score"
        );
    }
    if extractor_guidance.is_some() {
        tracing::info!(
            extractor = %extractor_name,
            "[consolidation] extractor is consulting its own learned rules"
        );
    }

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
        }
        // Loop 1 for the extractor itself: read-back plus authorship. Applied
        // to both arms — a degraded run has no LLM to give guidance to, and
        // stamps nothing because the pattern fallback passes `None` explicitly,
        // but branching here would be one more place for either to get dropped.
        .with_extractor_guidance(extractor_guidance)
        .with_extractor_identity(extractor_identity)
        // Migration 203. Every rule this cycle writes records how
        // well-grounded the episodes behind it were, because the rules do not
        // stay in the table: `kg_context` injects them into other agents'
        // prompts as "Learned Knowledge". Applied to both arms for the same
        // reason as the two above — the pattern fallback reads the same
        // episodes, so a regex generalising over ungrounded text produces an
        // ungrounded rule, and exempting it would leave the one path that
        // cannot explain itself also unlabelled.
        .with_provenance_oracle(Some(Arc::new(
            fermi::provenance_oracle::DbProvenanceOracle::new(spawn_state.db.clone()),
        )));

        // Pass the client's job_id through so the worker's own statistics and
        // completion land on the row the client is polling.
        let cycle_started = std::time::Instant::now();
        let outcome = worker
            .consolidate_agent_with_job(spawn_agent_id, 0.5, 2, Some(job_id))
            .await;

        // Put the extractor's work on the ledger, success or failure. Done
        // before the match arms so a failed cycle is recorded too: "the
        // ontologist ran and errored" and "the ontologist never ran" are
        // different facts and used to be the same absence.
        record_dream_member_episode(
            &spawn_state,
            &dream_member(&spawn_state, "semantic-rules", "ontologist"),
            "extract",
            spawn_agent_id,
            &spawn_agent_name,
            job_id,
            format!(
                "Extract entities, facts and semantic rules from the unconsolidated \
                 episodes of agent \"{}\".",
                spawn_agent_name
            ),
            match &outcome {
                Ok(r) => json!({
                    "episodes_processed": r.episodes_processed,
                    "clusters_identified": r.clusters_identified,
                    "entities_created": r.entities_created,
                    "facts_created": r.facts_created,
                    "rules_extracted": r.rules_extracted,
                    "rules_verified": r.rules_verified,
                    "rules_rejected": r.rules_rejected,
                    "degraded": degraded,
                }),
                Err(e) => json!({ "failed": e.to_string(), "degraded": degraded }),
            },
            worker.extractor_usage(),
            worker.extractor_model(),
            extractor_provider.clone(),
            match &outcome {
                // A cycle that completed but extracted nothing is a Partial,
                // not a Success. Recording it as Success is precisely how 91
                // zero-yield cycles looked healthy.
                Ok(r) if r.entities_created + r.facts_created + r.rules_extracted > 0 => {
                    agent_bestiary_memory::ExecutionStatus::Success
                }
                Ok(_) => agent_bestiary_memory::ExecutionStatus::Partial,
                Err(_) => agent_bestiary_memory::ExecutionStatus::Failure,
            },
            match &outcome {
                Ok(r) if r.entities_created + r.facts_created + r.rules_extracted == 0 => Some(
                    "Cycle completed and extracted nothing — no entities, facts or rules."
                        .to_string(),
                ),
                Err(e) => Some(e.to_string()),
                _ => None,
            },
            cycle_started.elapsed().as_millis() as i64,
        )
        .await;

        // (The per-call extraction episodes — the extractor's learning material —
        // are written by the worker itself, inside the cycle. Deliberately not
        // here: this handler and the batch `consolidate` CLI both drive the same
        // worker, and a step only one of them performed would leave the extractor
        // learning from part of its work with nothing saying which part.)

        // Resolve the extractor's utility now that another cycle's rules have
        // aged. This measures PREVIOUS cycles, not the one that just ran — rules
        // written seconds ago have had no chance to be retrieved, and counting
        // them would drag the score toward zero every time the extractor did
        // work. Consolidation is simply a convenient clock: it is the only event
        // that reliably recurs for an extractor.
        if let Some(eid) = extractor_identity {
            emit_extraction_utility_signal(&spawn_state, eid).await;
        }

        match outcome {
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
                    let narrator_query = synopsis_input.clone();
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
                        // Text-only path: this caller carries no image. Stated rather than
                        // defaulted, so a path that should carry one cannot acquire the field
                        // silently.
                        attachments: Vec::new(),
                    };
                    let narrator_started = std::time::Instant::now();
                    if let Ok(output) = narrator_state
                        .registry
                        .execute_agent(&agent_stmt, &context)
                        .await
                    {
                        // Put the narrator on the ledger too.
                        //
                        // `execute_agent` returns a full `AgentOutput` with real
                        // token counts, so this uses the canonical episode
                        // constructor rather than the hand-rolled one above:
                        // same tagging, same `AgentOutput::cost()` pricing path,
                        // same shape as every other execution on the platform.
                        // The narrator was invisible for a different reason than
                        // the ontologist — episodes are written by handlers, and
                        // this handler never wrote one — but the effect was
                        // identical.
                        if let Ok(narrator_db) =
                            crate::resolve_agent(&narrator_state, &narrator_id).await
                        {
                            let mut ep = crate::agent_output_to_episode(
                                narrator_db.agent_id,
                                &narrator_query,
                                &output,
                            );
                            ep.persona_version_at_write = Some(narrator_db.persona_version);
                            ep.execution_time_ms = narrator_started.elapsed().as_millis() as i64;
                            ep.tags.push("dream_pipeline".to_string());
                            ep.tags.push("role:narrate".to_string());
                            ep.tags.push(format!("subject:{}", aname));
                            ep.tags.push(format!("job:{}", job_id));

                            // Embedded, unlike the extractor's ledger row. The
                            // narrator's output is real prose that differs
                            // every cycle, so it is genuine learning material
                            // and belongs in its consolidation queue.
                            //
                            // Embed query + narrative, as `execution.rs` does.
                            // The prompt alone is a template that varies only
                            // in the subject's name and six integers, so
                            // embedding it by itself would cluster cycles on
                            // the boilerplate — the same trap the extractor row
                            // avoids by not being embedded at all.
                            let embed_text = format!(
                                "{} {}",
                                narrator_query,
                                output.metadata.reasoning.as_deref().unwrap_or("")
                            );
                            let prov = narrator_state
                                .embedder
                                .generate_provenanced(&embed_text)
                                .await
                                .ok();
                            ep.embedding = prov.as_ref().map(|p| p.vector.clone());

                            if let Err(e) = narrator_state
                                .memory_store
                                .store_episode_with_provenance(
                                    ep,
                                    prov.as_ref(),
                                    Some(json!({
                                        "kind": "dream_pipeline",
                                        "role": "narrate",
                                        "job_id": job_id,
                                        "subject_agent_id": spawn_agent_id,
                                    })),
                                )
                                .await
                            {
                                tracing::warn!(
                                    error = %e,
                                    "[dream-ledger] failed to record narrator episode"
                                );
                            }
                        }

                        let narrative = output.metadata.reasoning.clone().unwrap_or_default();
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

/// The two judgement calls in the extraction-utility signal.
#[cfg(test)]
mod extraction_utility_tests {
    use super::*;

    /// A score over a handful of rules is noise, and Loop 1a would read the
    /// resulting signal as a turning loop. Silence is the honest output.
    #[test]
    fn too_thin_to_score_emits_nothing() {
        for resolved in 0..MIN_RESOLVED_RULES {
            assert!(
                extraction_utility(resolved, resolved).is_none(),
                "{resolved} resolved rule(s) must not produce a score"
            );
        }
        assert!(extraction_utility(MIN_RESOLVED_RULES, 0).is_some());
    }

    #[test]
    fn score_is_the_retrieved_fraction() {
        let (score, _) = extraction_utility(10, 7).unwrap();
        assert!((score - 0.7).abs() < 1e-9, "got {score}");

        // Both ends are reachable, and neither is special-cased away: an
        // extractor whose rules are never recalled has earned a 0, and that must
        // be reportable rather than rounded into "no data".
        assert_eq!(extraction_utility(20, 0).unwrap().0, 0.0);
        assert_eq!(extraction_utility(20, 20).unwrap().0, 1.0);
    }

    /// `eval_signals.score` has a CHECK constraint of [0,1]. A retrieved count
    /// exceeding resolved should be impossible from the query, but clamping
    /// means a counting bug degrades the number instead of failing the insert
    /// and losing the signal entirely.
    #[test]
    fn score_stays_inside_the_column_constraint() {
        let (score, conf) = extraction_utility(10, 99).unwrap();
        assert!((0.0..=1.0).contains(&score), "got {score}");
        assert!((0.0..=1.0).contains(&conf), "got {conf}");
    }

    /// A 6-rule score and a 600-rule score are both real and are not equally
    /// trustworthy. `confidence` is the field that already carries that, so it
    /// must actually vary.
    #[test]
    fn confidence_grows_with_evidence_and_saturates() {
        let thin = extraction_utility(5, 3).unwrap().1;
        let mid = extraction_utility(25, 15).unwrap().1;
        let thick = extraction_utility(500, 300).unwrap().1;

        assert!(thin < mid, "{thin} !< {mid}");
        assert!(mid < thick, "{mid} !< {thick}");
        assert_eq!(thick, 1.0, "confidence must saturate, not exceed 1");
        assert!(thin >= 0.1, "never zero — the score IS evidence, just weak");
    }

    /// The resolution delay is the only tuned number here. It must be a real
    /// window: zero would score rules the instant they are written (before
    /// anything could have recalled them, so every extractor looks useless), and
    /// a very long one starves the extractor of feedback.
    #[test]
    fn the_resolution_window_is_a_sane_delay() {
        assert!(
            (1..=30).contains(&RULE_RESOLUTION_DAYS),
            "RULE_RESOLUTION_DAYS = {RULE_RESOLUTION_DAYS}"
        );
    }
}
