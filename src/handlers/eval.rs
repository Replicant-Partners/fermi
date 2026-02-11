//! Eval framework handlers — test cases, runs, LLM-as-judge, regression detection.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;

use agent_bestiary_memory::{Agent, EvalRun, EvalTestCase, MemoryStore};
use fermi::agent_backend::AgentStatus;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

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
    if db_agent.owner_id.as_deref() != Some(&user_id) && db_agent.tier != "curated" {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".into()));
    }
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
    if db_agent.owner_id.as_deref() != Some(&user_id) && db_agent.tier != "curated" {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".into()));
    }
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
    if db_agent.owner_id.as_deref() != Some(&user_id) && db_agent.tier != "curated" {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".into()));
    }
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
    if db_agent.owner_id.as_deref() != Some(&user_id) && db_agent.tier != "curated" {
        return Err((StatusCode::FORBIDDEN, "Not the agent owner".into()));
    }

    // Load test cases (filter by tags if provided)
    let mut cases = state
        .memory_store
        .list_eval_test_cases(db_agent.agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if !body.tags.is_empty() {
        cases.retain(|c| c.tags.iter().any(|t| body.tags.contains(t)));
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
        &format!("Eval run for {}", agent_id),
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
        judge_enabled: body.judge,
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
    let agent_name = agent_id.clone();
    let judge = body.judge;
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

    Ok(Json(json!({
        "run_id": run_id,
        "status": "running",
        "total_cases": total_cases,
    })))
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

// ─── Background Eval Runner ─────────────────────────────────────────

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
        let context = ExecutionContext {
            program,
            agent_card: card.clone(),
        };
        let tool_context = Arc::new(ToolContext {
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
        });
        let tool_executor = ToolAwareExecutor::new(
            state.registry.executor_arc(),
            ToolRegistry::standard(),
            tool_context,
        );

        let result = tool_executor.execute(&agent_stmt, &context).await;

        let (case_passed, exec_time, tokens, episode_id, reasoning) = match &result {
            Ok(output) => {
                let mut ep = agent_output_to_episode(db_agent.agent_id, &tc.query, output);
                // Generate embedding
                let embed_text = format!(
                    "{} {}",
                    tc.query,
                    output.metadata.reasoning.as_deref().unwrap_or("")
                );
                if let Ok(embedding) = state.embedder.generate(&embed_text).await {
                    ep.embedding = Some(embedding);
                }
                let eid = state.memory_store.store_episode(ep).await.ok();
                let ok = matches!(output.status, AgentStatus::Success);
                (
                    ok,
                    output.execution_time_ms as i64,
                    output.tokens_used.unwrap_or(0) as i64,
                    eid,
                    output.metadata.reasoning.clone(),
                )
            }
            Err(e) => {
                eprintln!("Eval case execution failed: {}", e);
                (false, 0i64, 0i64, None, None)
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

        // LLM-as-judge scoring (if enabled and execution succeeded)
        let judge_result = if judge_enabled && case_passed {
            score_with_judge(&tc, reasoning.as_deref()).await
        } else {
            None
        };

        if let Some(ref js) = judge_result {
            if let Some(overall) = js.get("overall").and_then(|v| v.as_f64()) {
                judge_scores.push(overall);
            }
        }

        case_results.push(json!({
            "test_case_id": tc.test_case_id,
            "query": tc.query,
            "episode_id": episode_id,
            "passed": case_passed,
            "execution_time_ms": exec_time,
            "tokens_used": tokens,
            "cost_credits": exec_fee + gas,
            "judge_scores": judge_result,
        }));
    }

    let n = cases.len() as i64;
    let avg_judge = if judge_scores.is_empty() {
        None
    } else {
        Some(judge_scores.iter().sum::<f64>() / judge_scores.len() as f64)
    };

    // Regression detection
    let (regression, regression_details) = detect_regression(
        &state,
        db_agent.agent_id,
        passed,
        failed,
        avg_judge,
        if n > 0 { Some(total_latency / n) } else { None },
    )
    .await;

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
        started_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
        duration_ms: None,
    };

    if let Err(e) = state.memory_store.complete_eval_run(&completed_run).await {
        eprintln!("Failed to complete eval run {}: {}", run_id, e);
    }
}

// ─── LLM-as-Judge ───────────────────────────────────────────────────

pub async fn score_with_judge(
    test_case: &EvalTestCase,
    reasoning: Option<&str>,
) -> Option<serde_json::Value> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok()?;
    let judge_prompt = format!(
        "You are an evaluation judge. Score the following agent output on three dimensions.\n\
         Each score is 1-5 (1=terrible, 5=excellent).\n\n\
         QUERY: {}\n\
         {}\
         {}\
         AGENT OUTPUT:\n{}\n\n\
         Respond with ONLY valid JSON:\n\
         {{\"relevance\": N, \"accuracy\": N, \"completeness\": N, \"overall\": N.N, \"reasoning\": \"...\"}}\n\
         where overall = average of the three scores.",
        test_case.query,
        test_case
            .expected_output
            .as_ref()
            .map(|e| format!("EXPECTED OUTPUT: {}\n", e))
            .unwrap_or_default(),
        test_case
            .rubric
            .as_ref()
            .map(|r| format!("SCORING RUBRIC: {}\n", r))
            .unwrap_or_default(),
        reasoning.unwrap_or("(no output)")
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&json!({
            "model": "claude-3-haiku-20240307",
            "max_tokens": 300,
            "messages": [{"role": "user", "content": judge_prompt}]
        }))
        .send()
        .await
        .ok()?;

    let body: Value = resp.json().await.ok()?;
    let text = body["content"][0]["text"].as_str()?;
    serde_json::from_str(text).ok()
}

// ─── Regression Detection ───────────────────────────────────────────

pub async fn detect_regression(
    state: &AppState,
    agent_id: uuid::Uuid,
    current_passed: i32,
    current_failed: i32,
    current_judge: Option<f64>,
    current_avg_latency: Option<i64>,
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

    let detected = !regressions.is_empty();
    let details = if detected {
        Some(json!(regressions))
    } else {
        None
    };
    (detected, details)
}
