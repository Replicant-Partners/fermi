//! Eval framework handlers — test cases, runs, LLM-as-judge, regression detection.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, rbac, AuthPrincipal, ObjectType, Visibility};

// ─── Eval-scope authorization helper ───────────────────────────────
//
// Eval test-cases / runs against a curated (system-tier) agent are
// open to any authenticated caller — that's the platform benchmark
// affordance. For community-tier agents the caller must have Admin
// (owner + platform admin). v0.10.5: substrate RBAC replaces the
// hand-rolled `owner_id != user_id && tier != 'curated'` check that
// lived at 6 sites in this file.
async fn require_eval_authority(
    state: &AppState,
    principal: &AuthPrincipal,
    agent: &Agent,
) -> Result<(), (StatusCode, String)> {
    if agent.tier == "curated" {
        return Ok(());
    }
    rbac::require_admin_on(
        &state.db,
        principal,
        ObjectType::Agent,
        &agent.agent_id.to_string(),
        agent.owner_id.as_deref().unwrap_or(""),
        Visibility::Private,
    )
    .await
    .map(|_| ())
}
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;

use agent_bestiary_evaluators::{
    AggregatedSignal, BrierEvaluator, ConflictFlag, Dimension, EvaluatorRegistry,
    LlmJudgeEvaluator, ProjectionScoringEvaluator, RegistryOutcome,
};
use agent_bestiary_memory::{
    Agent, EpisodeBundle, EvalRun, EvalSignal, EvalTestCase, TranscriptRole, TranscriptTurn,
};
use agent_bestiary_observability::{EpisodeScorer, ObservabilityWorker};
use async_trait::async_trait;
use fermi::agent_backend::AgentStatus;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::handlers::eval_brier::{AgentNameResolver, BrierLookupSqlx};
use crate::handlers::eval_judge::LlmJudgeAnthropic;
use crate::handlers::eval_projection::ProjectionLookupSqlx;
use crate::{
    agent_output_to_episode, create_notification, resolve_agent, resolve_agent_card, AppState,
};

// Track B — native evaluator family (registered per eval run)
use evaluator_character::CharacterEvaluator;
use evaluator_faithfulness::FaithfulnessEvaluator;
use evaluator_lifelong::LifelongBenchEvaluator;
use evaluator_sotopia::SotopiaEvaluator;
use evaluator_wildguard::WildGuardEvaluator;

// ─── Eval Framework Handlers ────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateTestCaseRequest {
    query: String,
    expected_output: Option<String>,
    rubric: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateTestCaseRequest {
    query: Option<String>,
    expected_output: Option<String>,
    rubric: Option<String>,
    tags: Option<Vec<String>>,
    is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct TriggerEvalRunRequest {
    #[serde(default)]
    judge: bool,
    #[serde(default)]
    tags: Vec<String>,
}

pub async fn list_eval_test_cases_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let cases = state
        .memory_store
        .list_eval_test_cases(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "test_cases": cases })))
}

pub async fn create_eval_test_case_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<CreateTestCaseRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;
    require_eval_authority(&state, &principal, &db_agent).await?;
    let tc = EvalTestCase {
        test_case_id: uuid::Uuid::new_v4(),
        agent_id: db_agent.agent_id,
        query: body.query,
        expected_output: body.expected_output,
        rubric: body.rubric,
        tags: body.tags,
        is_active: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let id = state
        .memory_store
        .create_eval_test_case(&tc)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "test_case_id": id })))
}

pub async fn update_eval_test_case_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, test_case_id)): Path<(String, String)>,
    Json(body): Json<UpdateTestCaseRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;
    require_eval_authority(&state, &principal, &db_agent).await?;
    let tc_id: uuid::Uuid = test_case_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid test_case_id".into()))?;

    // Fetch existing, merge updates
    let cases = state
        .memory_store
        .list_eval_test_cases(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let existing = cases
        .iter()
        .find(|c| c.test_case_id == tc_id)
        .ok_or((StatusCode::NOT_FOUND, "Test case not found".into()))?;

    let updated = EvalTestCase {
        test_case_id: tc_id,
        agent_id: db_agent.agent_id,
        query: body.query.unwrap_or_else(|| existing.query.clone()),
        expected_output: body
            .expected_output
            .or_else(|| existing.expected_output.clone()),
        rubric: body.rubric.or_else(|| existing.rubric.clone()),
        tags: body.tags.unwrap_or_else(|| existing.tags.clone()),
        is_active: body.is_active.unwrap_or(existing.is_active),
        created_at: existing.created_at,
        updated_at: chrono::Utc::now(),
    };
    state
        .memory_store
        .update_eval_test_case(&updated)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "updated": true })))
}

pub async fn delete_eval_test_case_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((agent_id, test_case_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;
    require_eval_authority(&state, &principal, &db_agent).await?;
    let tc_id: uuid::Uuid = test_case_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid test_case_id".into()))?;
    state
        .memory_store
        .deactivate_eval_test_case(tc_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "deleted": true })))
}

pub async fn trigger_eval_run_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<TriggerEvalRunRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;
    require_eval_authority(&state, &principal, &db_agent).await?;
    let agent_name = agent_id.clone();
    let (run_id, total_cases) =
        trigger_eval_run_core(&state, db_agent, agent_name, user_id, body.judge, body.tags).await?;
    Ok(Json(json!({
        "run_id": run_id,
        "status": "running",
        "total_cases": total_cases,
    })))
}

/// Shared trigger logic used by both the HTTP handler above and the
/// `run_evaluator_registry` MCP tool (via `EvalTriggerImpl`). Loads test
/// cases, charges gas, creates the eval_run row, and spawns the
/// background runner. Returns (run_id, total_cases).
pub async fn trigger_eval_run_core(
    state: &AppState,
    db_agent: Agent,
    agent_name: String,
    user_id: String,
    judge: bool,
    tag_filter: Vec<String>,
) -> Result<(uuid::Uuid, usize), (StatusCode, String)> {
    // Load test cases (filter by tags if provided)
    let mut cases = state
        .memory_store
        .list_eval_test_cases(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !tag_filter.is_empty() {
        cases.retain(|c| c.tags.iter().any(|t| tag_filter.contains(t)));
    }
    if cases.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No test cases found".into()));
    }

    // Charge eval_run gas fee
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
        state.gas_fees.eval_run,
        "eval_fee",
        &format!("Eval run for {}", agent_name),
        None,
    )
    .await?;

    // Create run record
    let run_id = uuid::Uuid::new_v4();
    let run = EvalRun {
        run_id,
        agent_id: db_agent.agent_id,
        triggered_by: user_id.clone(),
        status: "running".into(),
        judge_enabled: judge,
        total_cases: cases.len() as i32,
        passed: 0,
        failed: 0,
        avg_latency_ms: None,
        avg_tokens: None,
        avg_judge_score: None,
        total_cost_credits: 0,
        case_results: json!([]),
        regression_detected: false,
        regression_details: None,
        aggregated_signal: None,
        conflict_flags: json!([]),
        prefilter_blocked: false,
        started_at: chrono::Utc::now(),
        completed_at: None,
        duration_ms: None,
    };
    state
        .memory_store
        .create_eval_run(&run)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Spawn background task
    let total_cases = cases.len();
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_eval_cases(
            state_clone,
            run_id,
            db_agent,
            agent_name,
            cases,
            user_id,
            judge,
        )
        .await;
    });
    Ok((run_id, total_cases))
}

/// Bridge so the `run_evaluator_registry` MCP tool can trigger an eval
/// from inside the library-side tool dispatcher (which can't reach
/// AppState directly). Construction sites that have AppState in scope
/// build one of these and stash it in ToolContext::eval_trigger.
pub struct EvalTriggerImpl {
    pub state: AppState,
}

#[async_trait::async_trait]
impl fermi::agent_backend::tools::EvalTrigger for EvalTriggerImpl {
    async fn trigger_eval(
        &self,
        agent_id: uuid::Uuid,
        user_id: String,
        judge: bool,
        tags: Vec<String>,
    ) -> Result<uuid::Uuid, String> {
        let db_agent = self
            .state
            .memory_store
            .get_agent(agent_id)
            .await
            .map_err(|e| format!("Failed to load agent: {}", e))?
            .ok_or_else(|| format!("Agent {} not found", agent_id))?;
        // Ownership check — tool caller must own the agent OR the agent is curated.
        if db_agent.owner_id.as_deref() != Some(&user_id) && db_agent.tier != "curated" {
            return Err("Not the agent owner".into());
        }
        let agent_name = db_agent.agent_name.clone();
        let (run_id, _) =
            trigger_eval_run_core(&self.state, db_agent, agent_name, user_id, judge, tags)
                .await
                .map_err(|(_, msg)| msg)?;
        Ok(run_id)
    }
}

pub async fn list_eval_runs_handler(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let runs = state
        .memory_store
        .list_eval_runs(db_agent.agent_id, 20)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "runs": runs })))
}

/// GET /api/agents/:agent_id/eval/runs/:run_id/signals
///
/// Returns the per-evaluator, per-dimension `EvalSignal` rows for a
/// single run. Used by:
///   - the agent detail page's Eval tab (per-evaluator breakdown view)
///   - the eval_runner agent's `query_eval_signals` MCP tool
///   - the observability_coordinator's quick lookups
///
/// Public — matches `list_eval_runs_handler`'s read model (the same
/// data, just disaggregated).
pub async fn list_eval_signals_handler(
    State(state): State<AppState>,
    Path((agent_id, run_id_str)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;

    let run_id = uuid::Uuid::parse_str(&run_id_str)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid run_id".into()))?;

    let signals = state
        .memory_store
        .list_eval_signals_for_run(run_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Guard against cross-agent leakage: a run_id always belongs to one
    // agent. Surface the mismatch as 404, not silent empty.
    if let Some(first) = signals.first() {
        if first.agent_id != db_agent.agent_id {
            return Err((StatusCode::NOT_FOUND, "Run not found for this agent".into()));
        }
    }

    Ok(Json(json!({
        "agent_id": db_agent.agent_id,
        "run_id": run_id,
        "signals": signals,
        "count": signals.len(),
    })))
}

// ─── Background Eval Runner ─────────────────────────────────────────

/// Trivial `AgentNameResolver` keyed on a single agent — the eval
/// pipeline already has the agent in scope, so we never need a DB hit.
struct StaticAgentNameResolver {
    agent_id: uuid::Uuid,
    agent_name: String,
}

#[async_trait::async_trait]
impl AgentNameResolver for StaticAgentNameResolver {
    async fn resolve(&self, agent_id: uuid::Uuid) -> Option<String> {
        if agent_id == self.agent_id {
            Some(self.agent_name.clone())
        } else {
            None
        }
    }
}

pub async fn run_eval_cases(
    state: AppState,
    run_id: uuid::Uuid,
    db_agent: Agent,
    agent_name: String,
    cases: Vec<EvalTestCase>,
    user_id: String,
    judge_enabled: bool,
) {
    let card = resolve_agent_card(&state, &db_agent);
    let mut case_results: Vec<Value> = Vec::new();
    let mut total_latency: i64 = 0;
    let mut total_tokens: i64 = 0;
    let mut passed = 0i32;
    let mut failed = 0i32;
    let mut total_cost = 0i32;
    let mut judge_scores: Vec<f64> = Vec::new();

    // ── Phase 2 evaluator registry — build once outside the loop ──
    //
    // Pre-fetch async signals that the sync build_registry needs:
    //   1. PersonaConsistencySignal for LifelongBench
    //   2. ANTHROPIC_API_KEY for LLM-backed evaluators (CharacterEval, WildGuard, Sotopia)

    // LifelongBench: episode count + mean embedding for current persona version.
    // LifelongBench pre-fetch: all errors are non-fatal — evaluator returns
    // Inapplicable gracefully when signal is None.
    let lifelong_signal: Option<evaluator_lifelong::PersonaConsistencySignal> = (async {
        let persona_version = db_agent.persona_version;
        let count_row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM episodes WHERE agent_id = $1 AND persona_version_at_write = $2"
        )
        .bind(db_agent.agent_id)
        .bind(persona_version)
        .fetch_one(&state.db)
        .await
        .ok()?;
        let n_prior: i64 = count_row.try_get("cnt").unwrap_or(0);
        if n_prior < 5 {
            return None;
        }
        let curr_mean = state.memory_store
            .mean_embedding_for_persona_version(db_agent.agent_id, persona_version, 50)
            .await
            .ok()
            .flatten()?;
        let reference = if persona_version > 1 {
            state.memory_store
                .mean_embedding_for_persona_version(db_agent.agent_id, persona_version - 1, 50)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| curr_mean.clone())
        } else {
            curr_mean.clone()
        };
        let cos = agent_bestiary_observability::drift::cosine_similarity(&curr_mean, &reference)
            .unwrap_or(1.0);
        Some(evaluator_lifelong::PersonaConsistencySignal {
            within_version_cosine: cos,
            n_prior_episodes: n_prior as usize,
        })
    }).await;

    // LLM config for LLM-backed evaluators.
    let llm_api_key = std::env::var("ANTHROPIC_API_KEY").ok();

    let registry = build_registry(
        &state,
        &db_agent,
        &agent_name,
        judge_enabled,
        lifelong_signal,
        llm_api_key,
    );
    let mut per_case_signals: Vec<AggregatedSignal> = Vec::new();
    let mut any_prefilter_blocked = false;

    // Phase 3 — inline timeline-entry writer (the hot path); the
    // background scanner runs once at the end of the run.
    let scorer = EpisodeScorer::new(state.memory_store.clone());

    for tc in &cases {
        // Build execution context (same as execute_agent_handler)
        let agent_stmt = ast::AgentStmt {
            name: agent_name.clone(),
            agent_type: Some(card.agent_type.clone()),
            query: tc.query.clone(),
            executor: Some(ast::ExecutorType::LLM),
            schedule: None,
            driver_refs: vec![],
            depends_on: vec![],
            confidence_threshold: None,
        };
        let program = ast::Program {
            statements: vec![ast::Statement::Agent(agent_stmt.clone())],
        };
        // SPEC_28 — eval runs are funded by the agent under evaluation,
        // same as a normal invocation.
        let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

        let context = ExecutionContext {
            program,
            agent_card: card.clone(),
            creature_id: None,
            cognition_tier: None,
            credentials: credentials.clone(),
        };
        // Per-case id: each test case is its own execution, so each gets its
        // own root episode for delegated children to hang off (mig-198).
        let case_episode_id = uuid::Uuid::new_v4();
        let tool_context = Arc::new(ToolContext {
            // Root of this case's delegation tree (mig-198).
            parent_episode_id: Some(case_episode_id),
            credentials,
            memory_store: state.memory_store.clone(),
            embedder: state.embedder.clone(),
            registry: state.registry.clone(),
            current_agent_id: Some(db_agent.agent_id),
            workspace_id: None,
            workspace_slug: None,
            workspace_git: None,
            db: Some(state.db.clone()),
            gas_fees: Some(state.gas_fees.clone()),
            user_id: None,
            user_secrets: None,
            // Inside the eval loop itself we deliberately omit the
            // trigger — agents under eval should not trigger more evals.
            eval_trigger: None,
            remote_mcp: None,
        });
        let tool_executor = ToolAwareExecutor::new(
            state.registry.executor_arc(),
            ToolRegistry::standard(),
            tool_context,
        );

        let result = tool_executor.execute(&agent_stmt, &context).await;

        let (case_passed, exec_time, tokens, episode_id, reasoning, stored_episode) = match &result
        {
            Ok(output) => {
                let mut ep = agent_output_to_episode(db_agent.agent_id, &tc.query, output);
                // Use the id advertised to this case's tool context.
                ep.episode_id = case_episode_id;
                // Stamp persona_version_at_write so drift monitoring (Phase 3)
                // can compare embeddings across persona versions.
                ep.persona_version_at_write = Some(db_agent.persona_version);
                // Q1 (a) — populate dyad_id from triggered_by for eval-pipeline
                // executions so the social tracker has something to scope to.
                // Kept on the `eval:` origin so synthetic regression history
                // stays distinguishable from real companion conversations,
                // which the runtime handlers stamp with the `dyad:` origin.
                ep.dyad_id = Some(agent_bestiary_memory::eval_dyad_id(
                    db_agent.agent_id,
                    &user_id,
                ));
                // Generate embedding bundled with Spec 22 provenance.
                let embed_text = format!(
                    "{} {}",
                    tc.query,
                    output.metadata.reasoning.as_deref().unwrap_or("")
                );
                let t_embed = tokio::time::Instant::now();
                let provenance = match state.embedder.generate_provenanced(&embed_text).await {
                    Ok(p) => {
                        tracing::info!(
                            elapsed_ms = t_embed.elapsed().as_millis() as u64,
                            model = %p.model_id,
                            site = "eval_runner",
                            "embed_call"
                        );
                        ep.embedding = Some(p.vector.clone());
                        Some(p)
                    }
                    Err(_) => None,
                };
                let source_ref = serde_json::json!({
                    "kind": "eval_runner",
                    "agent_id": db_agent.agent_id,
                    "user_id": user_id,
                    "test_case_query_len": tc.query.len(),
                });
                let stored = ep.clone();
                let eid = state
                    .memory_store
                    .store_episode_with_provenance(ep, provenance.as_ref(), Some(source_ref))
                    .await
                    .ok();
                let ok = matches!(output.status, AgentStatus::Success);
                (
                    ok,
                    output.execution_time_ms as i64,
                    output.tokens_used.unwrap_or(0) as i64,
                    eid,
                    output.metadata.reasoning.clone(),
                    Some(stored),
                )
            }
            Err(e) => {
                eprintln!("Eval case execution failed: {}", e);
                (false, 0i64, 0i64, None, None, None)
            }
        };

        if case_passed {
            passed += 1;
        } else {
            failed += 1;
        }
        total_latency += exec_time;
        total_tokens += tokens;

        let (exec_fee, gas) = state.gas_fees.execution_fee(tokens as i32);
        total_cost += exec_fee + gas;

        // ── Phase 2 — run the evaluator registry ──
        //
        // Skip the registry on cases with no episode (executor failure
        // before storage). We still record the case in case_results
        // with a null signal.
        let signal_payload: Option<Value> = if let Some(episode) = stored_episode.as_ref() {
            // Build the bundle. Transcript synthesises (user, query) +
            // (agent, reasoning) from what's available.
            let mut transcript = vec![TranscriptTurn {
                role: TranscriptRole::User,
                content: tc.query.clone(),
                speaker_id: None,
            }];
            if let Some(text) = reasoning.as_deref() {
                if !text.trim().is_empty() {
                    transcript.push(TranscriptTurn {
                        role: TranscriptRole::Agent,
                        content: text.to_string(),
                        speaker_id: Some(agent_name.clone()),
                    });
                }
            }
            let goal_spec = tc.rubric.as_ref().map(|r| {
                json!({
                    "rubric": r,
                    "expected_output": tc.expected_output,
                })
            });
            let bundle = EpisodeBundle::from_parts(episode, &db_agent, transcript, goal_spec);

            let outcome = registry.run(&bundle).await;
            if outcome.prefilter_blocked {
                any_prefilter_blocked = true;
            }

            // Persist per-evaluator signals.
            let signals = registry_outcome_to_signals(
                run_id,
                episode_id,
                db_agent.agent_id,
                &outcome,
                episode.provenance.to_string(),
                episode.persona_version_at_write,
            );
            if let Err(e) = state.memory_store.create_eval_signals(&signals).await {
                eprintln!("Failed to persist eval signals for run {}: {}", run_id, e);
            }

            // Phase 3 — inline timeline-entry write. Cheap; lets the
            // observatory dashboard render without lag. Drift +
            // anomaly fields are filled by the background worker
            // post-loop.
            if let Err(e) = scorer
                .write_inline(
                    episode,
                    &outcome.signal,
                    Some(run_id),
                    Some(format!("eval-run:{}", run_id)),
                )
                .await
            {
                eprintln!("Failed to write timeline entry for run {}: {}", run_id, e);
            }

            // Pull the legacy avg_judge_score signal so the Phase 1
            // regression check still works for runs with the LLM judge
            // enabled. Mean of {relevance, accuracy, completeness}
            // re-projected onto the legacy 1–5 scale (×4 + 1).
            if judge_enabled {
                let judge_dims = ["relevance", "accuracy", "completeness"];
                let judge_means: Vec<f64> = outcome
                    .signal
                    .per_dimension
                    .iter()
                    .filter(|d| judge_dims.contains(&d.dimension.as_str()))
                    .map(|d| d.mean)
                    .collect();
                if !judge_means.is_empty() {
                    let unit_mean = judge_means.iter().sum::<f64>() / judge_means.len() as f64;
                    judge_scores.push(unit_mean * 4.0 + 1.0);
                }
            }

            let payload = serde_json::to_value(&outcome.signal).ok();
            per_case_signals.push(outcome.signal);
            payload
        } else {
            None
        };

        case_results.push(json!({
            "test_case_id": tc.test_case_id,
            "query": tc.query,
            "episode_id": episode_id,
            "passed": case_passed,
            "execution_time_ms": exec_time,
            "tokens_used": tokens,
            "cost_credits": exec_fee + gas,
            // Q2.a — additive: keep the legacy `judge_scores` for
            // backward-compat callers, add the richer `signal` from
            // the registry alongside.
            "judge_scores": judge_scores.last().map(|s| json!({"overall": s})),
            "signal": signal_payload,
        }));
    }

    let n = cases.len() as i64;
    let avg_judge = if judge_scores.is_empty() {
        None
    } else {
        Some(judge_scores.iter().sum::<f64>() / judge_scores.len() as f64)
    };

    // Per Q1.b — run-level aggregate of all per-case AggregatedSignals.
    // Mean per dimension across cases, conflicts = union of per-case
    // conflict dimensions. `any_prefilter_blocked` is tracked in the
    // loop above (true if any case had its dimensional evaluators
    // skipped by a pre-filter short-circuit).
    let run_aggregate = aggregate_run_signals(&per_case_signals);
    let aggregated_signal_json = serde_json::to_value(&run_aggregate).ok();
    let conflict_flags_json =
        serde_json::to_value(&run_aggregate.conflicts).unwrap_or_else(|_| json!([]));

    // Regression detection (legacy — Phase 1 logic, kept) plus
    // per-dimension drops surfaced from the run aggregate (Phase 2).
    let (regression, regression_details) = detect_regression(
        &state,
        db_agent.agent_id,
        passed,
        failed,
        avg_judge,
        if n > 0 { Some(total_latency / n) } else { None },
        Some(&run_aggregate),
    )
    .await;

    // Clone before moving into completed_run so the notification can use it
    let regression_details_for_notify = regression_details.clone();

    let completed_run = EvalRun {
        run_id,
        agent_id: db_agent.agent_id,
        triggered_by: user_id,
        status: "completed".into(),
        judge_enabled,
        total_cases: n as i32,
        passed,
        failed,
        avg_latency_ms: if n > 0 { Some(total_latency / n) } else { None },
        avg_tokens: if n > 0 {
            Some((total_tokens / n) as i32)
        } else {
            None
        },
        avg_judge_score: avg_judge,
        total_cost_credits: total_cost,
        case_results: json!(case_results),
        regression_detected: regression,
        regression_details,
        // Phase 2 — registry aggregate written here.
        aggregated_signal: aggregated_signal_json,
        conflict_flags: conflict_flags_json,
        prefilter_blocked: any_prefilter_blocked,
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        duration_ms: None,
    };

    if let Err(e) = state.memory_store.complete_eval_run(&completed_run).await {
        eprintln!("Failed to complete eval run {}: {}", run_id, e);
    }

    // Notify the agent owner when a regression is detected so the stored
    // flag actually becomes actionable, not just write-only dead storage.
    if regression {
        let db = state.db.clone();
        let owner_id = completed_run.triggered_by.clone();
        let agent_label = agent_name.clone();
        let details_snapshot = regression_details_for_notify;
        tokio::spawn(async move {
            let body = format_regression_body(&details_snapshot);
            create_notification(
                &db,
                &owner_id,
                "eval_regression",
                &format!("Eval regression detected: {}", agent_label),
                Some(&body),
            )
            .await;
        });
    }

    // Phase 3 — kick the observability worker to scan the entries we
    // just wrote. Best-effort, non-blocking. The worker computes
    // drift, runs anomaly detection, and updates the per-agent
    // checkpoint. Errors are logged, not surfaced to the eval-run
    // status (eval-run completion is independent of observability
    // scan completion).
    {
        let store = state.memory_store.clone();
        let agent_id = db_agent.agent_id;
        tokio::spawn(async move {
            let worker = ObservabilityWorker::new(store);
            match worker.scan_agent(agent_id).await {
                Ok(report) => {
                    tracing::info!(
                        agent_id = %agent_id,
                        scanned = report.entries_scanned,
                        anomalies = report.anomalies_detected,
                        drift_computations = report.drift_computations,
                        duration_ms = report.duration_ms,
                        "observability scan complete"
                    );
                }
                Err(e) => {
                    eprintln!("Observability scan for agent {} failed: {}", agent_id, e);
                }
            }
        });
    }

    // Phase 2 — eval_conflict notification when the run-level
    // aggregate flags any dimension as in conflict (the registry's
    // confidence-weighted disagreement signal). One notification per
    // run, regardless of how many dimensions disagreed.
    if !run_aggregate.conflicts.is_empty() {
        let db = state.db.clone();
        let owner_id = completed_run.triggered_by.clone();
        let agent_label = agent_name.clone();
        let conflicts = run_aggregate.conflicts.clone();
        tokio::spawn(async move {
            let body = format_conflict_body(&conflicts);
            create_notification(
                &db,
                &owner_id,
                "eval_conflict",
                &format!("Evaluator conflict detected: {}", agent_label),
                Some(&body),
            )
            .await;
        });
    }
}

// ─── Phase 2 — registry plumbing ────────────────────────────────────

/// Build the evaluator registry once per run. The judge is registered
/// only when `judge_enabled`; Brier is always registered (it returns
/// `Inapplicable` when the agent has no resolved forecasts, which the
/// aggregator skips silently).
fn build_registry(
    state: &AppState,
    db_agent: &Agent,
    agent_name: &str,
    judge_enabled: bool,
    lifelong_signal: Option<evaluator_lifelong::PersonaConsistencySignal>,
    llm_api_key: Option<String>,
) -> EvaluatorRegistry {
    let mut registry = EvaluatorRegistry::new();

    // ── Pre-filters (run serially, can short-circuit) ────────────────────

    // WildGuard: pattern-only by default; LLM fallback when API key present.
    let wildguard = if let Some(ref key) = llm_api_key {
        WildGuardEvaluator::with_llm(Some(evaluator_wildguard::LlmConfig {
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            api_key: key.clone(),
            model: "claude-haiku-4-5-20251001".into(),
        }))
    } else {
        WildGuardEvaluator::new()
    };
    registry.register(Arc::new(wildguard));

    // Faithfulness: grounding check — fires when tool_invocations/evidence
    // are present in episode context (wired in agent_output_to_episode).
    registry.register(Arc::new(FaithfulnessEvaluator::new()));

    // ── Dimensional evaluators (run in parallel) ─────────────────────────

    if judge_enabled {
        let judge: Arc<dyn agent_bestiary_evaluators::LlmJudge> =
            Arc::new(LlmJudgeAnthropic::new());
        registry.register(Arc::new(LlmJudgeEvaluator::new(judge)));
    }

    // Sotopia: social goals — fires when test case has a rubric (goal_spec).
    // With LLM key: structured 1-10 scoring. Without: keyword heuristic.
    let sotopia = if let Some(ref key) = llm_api_key {
        SotopiaEvaluator::with_llm(evaluator_sotopia::LlmConfig {
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            api_key: key.clone(),
            model: "claude-haiku-4-5-20251001".into(),
        })
    } else {
        SotopiaEvaluator::new()
    };
    registry.register(Arc::new(sotopia));

    // LifelongBench: persona consistency — fires when ≥5 prior episodes exist.
    // Signal pre-fetched async above and injected here.
    let lifelong = match lifelong_signal {
        Some(sig) => LifelongBenchEvaluator::with_signal(sig),
        None => LifelongBenchEvaluator::new(), // returns Inapplicable gracefully
    };
    registry.register(Arc::new(lifelong));

    // CharacterEval: persona fidelity + value alignment — fires for any agent
    // with a system prompt ≥ 20 chars.
    // With LLM key: structured scoring. Without: heuristic commitment matching.
    let character = if let Some(ref key) = llm_api_key {
        CharacterEvaluator::with_llm(evaluator_character::LlmConfig {
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            api_key: key.clone(),
            model: "claude-haiku-4-5-20251001".into(),
        })
    } else {
        CharacterEvaluator::new()
    };
    registry.register(Arc::new(character));

    // Brier calibration (hard-verified; existing).
    let resolver: Arc<dyn AgentNameResolver> = Arc::new(StaticAgentNameResolver {
        agent_id: db_agent.agent_id,
        agent_name: agent_name.to_string(),
    });
    let brier_lookup =
        Arc::new(BrierLookupSqlx::new(state.db.clone()).with_agent_name_resolver(resolver));
    registry.register(Arc::new(BrierEvaluator::new(brier_lookup)));

    // Projection accuracy (hard-verified; SimOps only — self-filters).
    let projection_lookup = Arc::new(ProjectionLookupSqlx::new(state.db.clone()));
    registry.register(Arc::new(ProjectionScoringEvaluator::new(projection_lookup)));

    registry
}

/// Project a `RegistryOutcome` into per-evaluator, per-dimension
/// `EvalSignal` rows for the `eval_signals` table. Non-success
/// evaluator results (`Inapplicable` + `Provider` failures) produce
/// no rows — they're tracked on the aggregated signal instead.
fn registry_outcome_to_signals(
    run_id: uuid::Uuid,
    episode_id: Option<uuid::Uuid>,
    agent_id: uuid::Uuid,
    outcome: &RegistryOutcome,
    bundle_provenance: String,
    persona_version: Option<i32>,
) -> Vec<EvalSignal> {
    let mut out: Vec<EvalSignal> = Vec::new();
    for r in &outcome.results {
        let Ok(ref eval) = r.outcome else {
            continue;
        };
        for (dim, score) in &eval.dimension_scores {
            out.push(EvalSignal {
                signal_id: uuid::Uuid::new_v4(),
                run_id: Some(run_id),
                episode_id,
                agent_id,
                evaluator_name: eval.evaluator_name.clone(),
                evaluator_version: eval.evaluator_version.clone(),
                evaluator_tier: tier_label(r.tier).to_string(),
                dimension: dim.as_str().to_string(),
                score: *score,
                confidence: eval.confidence,
                flags: serde_json::to_value(&eval.flags).unwrap_or_else(|_| json!([])),
                bundle_provenance: bundle_provenance.clone(),
                persona_version,
                model_used: eval.model_used.clone(),
                cost_credits: eval.cost_credits,
                latency_ms: r.latency_ms as i64,
                rationale: eval.rationale.clone(),
                created_at: chrono::Utc::now(),
            });
        }
    }
    out
}

fn tier_label(tier: agent_bestiary_evaluators::EvalTier) -> &'static str {
    match tier {
        agent_bestiary_evaluators::EvalTier::PreFilter => "pre_filter",
        agent_bestiary_evaluators::EvalTier::Dimensional => "dimensional",
    }
}

/// Per Q1.b — run-level aggregation: mean of per-case dimension means,
/// union of conflict-flagged dimensions, union of failed/inapplicable
/// evaluator names. Empty `signals` slice → empty aggregate.
fn aggregate_run_signals(signals: &[AggregatedSignal]) -> AggregatedSignal {
    if signals.is_empty() {
        return AggregatedSignal {
            per_dimension: vec![],
            conflicts: vec![],
            flags: vec![],
            active_evaluators: vec![],
            inapplicable_evaluators: vec![],
            failed_evaluators: vec![],
            failure_reasons: vec![],
        };
    }

    // Mean per dimension across cases.
    let mut sums: HashMap<Dimension, (f64, usize)> = HashMap::new();
    let mut conflict_dims: HashMap<Dimension, ConflictFlag> = HashMap::new();
    let mut active = std::collections::BTreeSet::new();
    let mut inapplicable = std::collections::BTreeSet::new();
    let mut failed = std::collections::BTreeSet::new();
    // Deduplicated across cases: the same evaluator failing on all nine test
    // cases for the same reason should read as one problem, not nine.
    let mut failure_reasons = std::collections::BTreeSet::new();
    let mut all_flags = Vec::new();

    for sig in signals {
        for d in &sig.per_dimension {
            let entry = sums.entry(d.dimension.clone()).or_insert((0.0, 0));
            entry.0 += d.mean;
            entry.1 += 1;
        }
        for c in &sig.conflicts {
            conflict_dims
                .entry(c.dimension.clone())
                .and_modify(|existing| {
                    // Track the maximum spread observed across cases for
                    // this dimension.
                    if c.spread > existing.spread {
                        existing.spread = c.spread;
                    }
                    for e in &c.evaluators {
                        if !existing.evaluators.contains(e) {
                            existing.evaluators.push(e.clone());
                        }
                    }
                })
                .or_insert_with(|| c.clone());
        }
        for e in &sig.active_evaluators {
            active.insert(e.clone());
        }
        for e in &sig.inapplicable_evaluators {
            inapplicable.insert(e.clone());
        }
        for e in &sig.failed_evaluators {
            failed.insert(e.clone());
        }
        for r in &sig.failure_reasons {
            failure_reasons.insert(r.clone());
        }
        for f in &sig.flags {
            all_flags.push(f.clone());
        }
    }

    let mut per_dimension: Vec<agent_bestiary_evaluators::DimensionAggregate> = sums
        .into_iter()
        .map(|(dim, (sum, n))| {
            let mean = sum / n as f64;
            let conflict = conflict_dims.contains_key(&dim);
            let spread = conflict_dims.get(&dim).map(|c| c.spread).unwrap_or(0.0);
            agent_bestiary_evaluators::DimensionAggregate {
                dimension: dim,
                mean,
                contributions: vec![], // run-level: per-case detail lives in eval_signals
                conflict,
                spread,
            }
        })
        .collect();
    per_dimension.sort_by(|a, b| a.dimension.as_str().cmp(b.dimension.as_str()));

    let mut conflicts: Vec<ConflictFlag> = conflict_dims.into_values().collect();
    conflicts.sort_by(|a, b| a.dimension.as_str().cmp(b.dimension.as_str()));

    AggregatedSignal {
        per_dimension,
        conflicts,
        flags: all_flags,
        active_evaluators: active.into_iter().collect(),
        inapplicable_evaluators: inapplicable.into_iter().collect(),
        failure_reasons: failure_reasons.into_iter().collect(),
        failed_evaluators: failed.into_iter().collect(),
    }
}

fn format_conflict_body(conflicts: &[ConflictFlag]) -> String {
    let mut lines = vec![format!(
        "{} dimension(s) flagged for evaluator conflict in the latest run:",
        conflicts.len()
    )];
    for c in conflicts {
        lines.push(format!(
            "• {} — spread {:.2} across {}",
            c.dimension.as_str(),
            c.spread,
            c.evaluators.join(", ")
        ));
    }
    lines.push(
        "Open the eval dashboard to review per-evaluator scores. Phase 4 will route this to the HITL queue."
            .into(),
    );
    lines.join("\n")
}

// ─── Regression Detection ───────────────────────────────────────────

pub async fn detect_regression(
    state: &AppState,
    agent_id: uuid::Uuid,
    current_passed: i32,
    current_failed: i32,
    current_judge: Option<f64>,
    current_avg_latency: Option<i64>,
    // Phase 2 — optional run-level aggregate. When `Some`, drops on
    // any dimension > 0.10 vs. the previous run's same dimension are
    // also flagged as regressions.
    current_run_aggregate: Option<&AggregatedSignal>,
) -> (bool, Option<Value>) {
    let prev_runs = state.memory_store.list_eval_runs(agent_id, 2).await.ok();
    let prev = prev_runs.and_then(|runs| runs.into_iter().find(|r| r.status == "completed"));

    let Some(prev) = prev else {
        return (false, None);
    };

    let mut regressions = Vec::new();
    let total = current_passed + current_failed;
    let prev_total = prev.passed + prev.failed;

    // Pass rate regression (>10% drop)
    if prev_total > 0 && total > 0 {
        let prev_rate = prev.passed as f64 / prev_total as f64;
        let curr_rate = current_passed as f64 / total as f64;
        if prev_rate - curr_rate > 0.10 {
            regressions.push(json!({
                "dimension": "pass_rate",
                "previous": prev_rate,
                "current": curr_rate,
                "delta": curr_rate - prev_rate,
            }));
        }
    }

    // Judge score regression (>0.5 drop on 5-point scale)
    if let (Some(prev_j), Some(curr_j)) = (prev.avg_judge_score, current_judge) {
        if prev_j - curr_j > 0.5 {
            regressions.push(json!({
                "dimension": "judge_score",
                "previous": prev_j,
                "current": curr_j,
                "delta": curr_j - prev_j,
            }));
        }
    }

    // Latency regression (>50% slower)
    if let (Some(prev_l), Some(curr_l)) = (prev.avg_latency_ms, current_avg_latency) {
        if prev_l > 0 && (curr_l as f64 / prev_l as f64) > 1.5 {
            regressions.push(json!({
                "dimension": "latency",
                "previous_ms": prev_l,
                "current_ms": curr_l,
                "ratio": curr_l as f64 / prev_l as f64,
            }));
        }
    }

    // Phase 2 — per-dimension regressions from the registry aggregate.
    // Compares the current run's per-dimension means against the
    // previous run's `aggregated_signal.per_dimension`. A drop > 0.10
    // on any dimension flags a regression for that dimension.
    if let (Some(curr_agg), Some(prev_agg_value)) =
        (current_run_aggregate, prev.aggregated_signal.as_ref())
    {
        if let Some(prev_per_dim) = prev_agg_value
            .get("per_dimension")
            .and_then(|v| v.as_array())
        {
            // Index previous run's dimensions by name.
            let prev_means: HashMap<String, f64> = prev_per_dim
                .iter()
                .filter_map(|d| {
                    let name = d.get("dimension")?.as_str()?.to_string();
                    let mean = d.get("mean")?.as_f64()?;
                    Some((name, mean))
                })
                .collect();
            for d in &curr_agg.per_dimension {
                if let Some(prev_mean) = prev_means.get(d.dimension.as_str()) {
                    let drop = prev_mean - d.mean;
                    if drop > 0.10 {
                        regressions.push(json!({
                            "dimension": format!("dim:{}", d.dimension.as_str()),
                            "previous": prev_mean,
                            "current": d.mean,
                            "delta": -drop,
                        }));
                    }
                }
            }
        }
    }

    let detected = !regressions.is_empty();
    let details = if detected {
        Some(json!(regressions))
    } else {
        None
    };
    (detected, details)
}

// ─── Regression notification body ───────────────────────────────────

fn format_regression_body(details: &Option<Value>) -> String {
    let Some(details) = details else {
        return "Regressions detected in the latest eval run. Open the eval dashboard to review."
            .into();
    };
    let empty = vec![];
    let regressions = details.as_array().unwrap_or(&empty);

    let mut lines = vec![format!(
        "{} regression(s) detected in the latest eval run:",
        regressions.len()
    )];

    for r in regressions {
        let dim = r
            .get("dimension")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let line = match dim {
            "pass_rate" => {
                let prev = r.get("previous").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let curr = r.get("current").and_then(|v| v.as_f64()).unwrap_or(0.0);
                format!(
                    "• Pass rate: {:.0}% → {:.0}% ({:+.0} points)",
                    prev * 100.0,
                    curr * 100.0,
                    (curr - prev) * 100.0
                )
            }
            "judge_score" => {
                let prev = r.get("previous").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let curr = r.get("current").and_then(|v| v.as_f64()).unwrap_or(0.0);
                format!(
                    "• Judge score: {:.1} → {:.1} ({:+.1})",
                    prev,
                    curr,
                    curr - prev
                )
            }
            "latency" => {
                let prev = r.get("previous_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                let curr = r.get("current_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                let ratio = r.get("ratio").and_then(|v| v.as_f64()).unwrap_or(0.0);
                format!("• Latency: {}ms → {}ms ({:.1}x slower)", prev, curr, ratio)
            }
            other if other.starts_with("dim:") => {
                let dim_name = other.strip_prefix("dim:").unwrap_or(other);
                let prev = r.get("previous").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let curr = r.get("current").and_then(|v| v.as_f64()).unwrap_or(0.0);
                format!(
                    "• {}: {:.2} → {:.2} ({:+.2})",
                    dim_name,
                    prev,
                    curr,
                    curr - prev
                )
            }
            other => format!("• {}", other),
        };
        lines.push(line);
    }

    lines.push("Open the eval dashboard to review the run and update the agent.".into());
    lines.join("\n")
}

// ─── Auto-generate rubrics ────────────────────────────────────────────────────

/// POST /api/agents/:agent_id/eval/test-cases/generate-rubrics
///
/// For every active test case that has no rubric, calls the LLM to produce a
/// Sotopia-appropriate goal_spec rubric string. The rubric describes what a
/// good agent response to that query should accomplish — the social/epistemic
/// goal — so Sotopia can evaluate `goal_completion`, `social_capital`, and
/// `rapport` rather than returning Inapplicable.
///
/// The prompt uses the agent's system_prompt as context so the rubric is
/// calibrated to the agent's persona and domain.
///
/// Only rewrites test cases that currently have `rubric IS NULL`.
/// Returns: `{ updated: N, skipped: N }` where `updated` is the count of
/// cases that received a new rubric, `skipped` is cases that already had one.
pub async fn generate_rubrics_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let db_agent = resolve_agent(&state, &agent_id).await?;
    require_eval_authority(&state, &principal, &db_agent).await?;

    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "ANTHROPIC_API_KEY not set".into(),
        )
    })?;

    let cases = state
        .memory_store
        .list_eval_test_cases(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let needs_rubric: Vec<_> = cases
        .iter()
        .filter(|c| c.is_active && c.rubric.is_none())
        .collect();
    let skipped = cases.len() - needs_rubric.len();

    if needs_rubric.is_empty() {
        return Ok(Json(
            json!({ "updated": 0, "skipped": skipped, "message": "All active test cases already have rubrics." }),
        ));
    }

    // Truncate system prompt to keep the LLM request compact
    let system_excerpt = db_agent
        .system_prompt
        .as_deref()
        .unwrap_or("A helpful AI assistant.")
        .chars()
        .take(800)
        .collect::<String>();

    let client = reqwest::Client::new();
    let mut updated = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for tc in &needs_rubric {
        let prompt = format!(
            r#"You are writing a Sotopia evaluation rubric for a test case.

Agent persona (excerpt):
{system_excerpt}

Test case query:
{query}

Write a single concise rubric sentence (max 30 words) describing the social/epistemic goal
this agent should accomplish when answering this query. Frame it as a goal the agent is trying
to achieve — not a description of the ideal answer.

Examples of good rubrics:
- "Help the operator understand which cultivation stage is contributing most to carbon output."
- "Establish trust by explaining the yield prediction methodology clearly and honestly."
- "Assist the user in identifying process inefficiencies while maintaining a collaborative tone."

Return ONLY the rubric sentence. No preamble, no quotes, no extra text."#,
            system_excerpt = system_excerpt,
            query = tc.query,
        );

        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "model": "claude-haiku-4-5-20251001",
                "max_tokens": 80,
                "messages": [{"role": "user", "content": prompt}],
                "temperature": 0.3,
            }))
            .send()
            .await;

        let rubric_text = match resp {
            Err(e) => {
                errors.push(format!("{}: network error: {}", tc.test_case_id, e));
                continue;
            }
            Ok(r) => {
                let body: serde_json::Value = match r.json().await {
                    Ok(b) => b,
                    Err(e) => {
                        errors.push(format!("{}: parse error: {}", tc.test_case_id, e));
                        continue;
                    }
                };
                match body["content"][0]["text"].as_str() {
                    Some(t) => t.trim().to_string(),
                    None => {
                        errors.push(format!("{}: no text in response", tc.test_case_id));
                        continue;
                    }
                }
            }
        };

        if rubric_text.is_empty() {
            errors.push(format!("{}: empty rubric", tc.test_case_id));
            continue;
        }

        let updated_tc = EvalTestCase {
            rubric: Some(rubric_text),
            updated_at: chrono::Utc::now(),
            ..((*tc).clone())
        };

        match state.memory_store.update_eval_test_case(&updated_tc).await {
            Ok(_) => updated += 1,
            Err(e) => errors.push(format!("{}: db error: {}", tc.test_case_id, e)),
        }
    }

    Ok(Json(json!({
        "updated": updated,
        "skipped": skipped,
        "errors": errors,
        "message": format!("Generated rubrics for {updated}/{} test cases.", needs_rubric.len()),
    })))
}

// ─── Batch eval run ───────────────────────────────────────────────────────────

/// POST /api/me/eval/runs/batch
///
/// Trigger eval runs across multiple agents in one call.
///
/// Request body:
/// ```json
/// {
///   "agent_ids": ["uuid1", "uuid2"] | "all",
///   "judge": true
/// }
/// ```
///
/// - `"all"` — every agent owned by the caller that has ≥1 active test case
/// - `["id1","id2"]` — specific agent UUIDs or names (subset)
///
/// Each agent's eval run is spawned as a background task independently,
/// identical to the single-agent trigger path. Agents with no test cases
/// are skipped (not an error). Credit is charged per agent that has cases.
///
/// Response:
/// ```json
/// {
///   "triggered": 5,
///   "skipped": 2,
///   "results": [
///     { "agent_id": "...", "agent_name": "...", "run_id": "...", "total_cases": 6 },
///     { "agent_id": "...", "agent_name": "...", "skipped_reason": "no test cases" }
///   ]
/// }
/// ```
pub async fn batch_eval_run_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<BatchEvalRunRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Resolve the agent list.
    let owned_agents = state
        .memory_store
        .list_agents_for_owner(&user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let target_agents: Vec<Agent> = match &body.agent_ids {
        AgentSelection::All => owned_agents,
        AgentSelection::Subset(ids) => {
            // Match by UUID or agent_name — whichever the caller provides.
            owned_agents
                .into_iter()
                .filter(|a| {
                    ids.iter()
                        .any(|id| id == &a.agent_id.to_string() || id == &a.agent_name)
                })
                .collect()
        }
    };

    if target_agents.is_empty() {
        return Ok(Json(json!({
            "triggered": 0,
            "skipped": 0,
            "results": [],
            "message": "No matching owned agents found.",
        })));
    }

    let judge = body.judge.unwrap_or(false);
    let mut results: Vec<Value> = Vec::new();
    let mut triggered = 0usize;
    let mut skipped = 0usize;

    for agent in target_agents {
        let agent_name = agent.agent_name.clone();
        let agent_id_str = agent.agent_id.to_string();

        match trigger_eval_run_core(
            &state,
            agent,
            agent_name.clone(),
            user_id.clone(),
            judge,
            vec![],
        )
        .await
        {
            Ok((run_id, total_cases)) => {
                triggered += 1;
                results.push(json!({
                    "agent_id": agent_id_str,
                    "agent_name": agent_name,
                    "run_id": run_id,
                    "total_cases": total_cases,
                    "status": "triggered",
                }));
            }
            Err((StatusCode::BAD_REQUEST, ref msg)) if msg.contains("No test cases") => {
                skipped += 1;
                results.push(json!({
                    "agent_id": agent_id_str,
                    "agent_name": agent_name,
                    "status": "skipped",
                    "skipped_reason": "no test cases",
                }));
            }
            Err((code, msg)) => {
                skipped += 1;
                results.push(json!({
                    "agent_id": agent_id_str,
                    "agent_name": agent_name,
                    "status": "error",
                    "error": format!("{}: {}", code, msg),
                }));
            }
        }
    }

    Ok(Json(json!({
        "triggered": triggered,
        "skipped": skipped,
        "results": results,
        "message": format!("Triggered {triggered} eval runs, skipped {skipped} agents."),
    })))
}

#[derive(serde::Deserialize)]
pub struct BatchEvalRunRequest {
    pub agent_ids: AgentSelection,
    pub judge: Option<bool>,
}

pub enum AgentSelection {
    All,
    Subset(Vec<String>),
}

impl<'de> serde::Deserialize<'de> for AgentSelection {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let v = serde_json::Value::deserialize(de)?;
        match v {
            serde_json::Value::String(s) if s.to_lowercase() == "all" => Ok(AgentSelection::All),
            serde_json::Value::Array(arr) => {
                let ids = arr
                    .into_iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect();
                Ok(AgentSelection::Subset(ids))
            }
            _ => Err(D::Error::custom(
                "agent_ids must be \"all\" or an array of agent id/name strings",
            )),
        }
    }
}
