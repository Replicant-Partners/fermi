//! The grounding service: ABW grounds an agent it does not host.
//!
//! # The product in three calls
//!
//! ```text
//! POST /v1/ground/runs                       API key (scope ground:*)
//!      { agent, query }                   -> { run_id, run_token, tools, expires_at }
//! POST /v1/ground/runs/:run_id/tools/:tool   run token
//!      { ...tool input }                  -> the tool's output   (metered)
//! POST /v1/ground/runs/:run_id/output        run token
//!      { response }                       -> the enforced document and its verdicts
//! ```
//!
//! The agent runs wherever its owner runs it. What makes its output groundable
//! is that its evidence comes through ABW: every tool call is made by ABW,
//! recorded on the run, and read back when the output is graded. That is the
//! difference between `tool_verified` meaning "a tool ABW ran returned this"
//! and meaning "the agent says so".
//!
//! # Same boundary as a hosted agent
//!
//! The output crosses `episode_boundary` exactly as a hosted agent's does:
//! reserved when the run opens, graded against the agent's card contract,
//! assessed for completeness against the recorded tool calls, stamped, stored,
//! queued for verification. A grounded external output and a hosted output are
//! the same kind of row, and the trace and `/grounding/:id` render both.
//!
//! # Who pays for what
//!
//! Checking is free. Tool calls are charged to the wallet of the user who
//! opened the run, at `GAS_GROUND_TOOL_CALL` credits each, because tools are
//! what cost money and they are also the evidence.

use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use fermi::agent_backend::executor::{AgentMetadata, AgentOutput, AgentStatus, ToolInvocation};
use fermi::agent_backend::tools::{PlatformToolRegistry, ToolContext};
use fermi::episode_boundary;
use fermi::gate_trust::{self, Decision, Gate};
use fermi_auth::{api_keys, get_or_create_wallet, types::ApiKey, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    agent_output_to_episode, resolve_agent, resolve_agent_card, resolve_agent_owner_secrets,
    AppState,
};

type ApiError = (StatusCode, Json<Value>);

/// How long a run token lives if the output is never submitted.
const RUN_TTL_MINUTES: i64 = 60;
const TOKEN_PREFIX: &str = "grun_";

fn err(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.into() } })),
    )
}

fn bearer(headers: &HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| {
            err(
                StatusCode::UNAUTHORIZED,
                "AUTH_REQUIRED",
                "Provide: Authorization: Bearer <token>",
            )
        })
}

fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn mint_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("{TOKEN_PREFIX}{hex}")
}

/// The API key opening a run must carry `ground:*` or `ground:<agent_name>`.
fn has_ground_scope(key: &ApiKey, agent_name: &str) -> bool {
    key.scopes
        .iter()
        .any(|s| s == "ground:*" || *s == format!("ground:{agent_name}"))
}

/// The tools a run may call: the ones the agent's card declares that the
/// platform can dispatch without a workspace or delegation.
///
/// Card-declared only, because the admission gate already holds every
/// `sourced` field to a tool the card declares. A run that could call any
/// tool would let evidence come from somewhere the contract never named.
fn allowed_tools(card: &fermi::agent_backend::agent_card::AgentCard) -> Vec<String> {
    let dispatchable = PlatformToolRegistry::standard().tool_names();
    let mut out: Vec<String> = card
        .capabilities
        .mcp_tools
        .iter()
        .map(|t| t.name.clone())
        .filter(|n| dispatchable.contains(&n.as_str()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// An open run, loaded by its token.
struct Run {
    run_id: Uuid,
    agent_id: Uuid,
    user_id: String,
    allowed_tools: Vec<String>,
    query: String,
}

async fn load_run(state: &AppState, headers: &HeaderMap, run_id: Uuid) -> Result<Run, ApiError> {
    let token = bearer(headers)?;
    if !token.starts_with(TOKEN_PREFIX) {
        return Err(err(
            StatusCode::UNAUTHORIZED,
            "RUN_TOKEN_REQUIRED",
            "This endpoint takes the run token returned when the run was opened, not an API key.",
        ));
    }
    let row = sqlx::query(
        "SELECT run_id, agent_id, user_id, allowed_tools, query,
                (closed_at IS NOT NULL) AS closed, (expires_at < now()) AS expired
           FROM ground_runs WHERE token_hash = $1",
    )
    .bind(hash_token(token))
    .fetch_optional(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?
    .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "INVALID_RUN_TOKEN", "Unknown run token."))?;

    let found: Uuid = row.get("run_id");
    if found != run_id {
        return Err(err(
            StatusCode::FORBIDDEN,
            "WRONG_RUN",
            "This run token belongs to a different run.",
        ));
    }
    if row.get::<bool, _>("closed") {
        return Err(err(
            StatusCode::CONFLICT,
            "RUN_CLOSED",
            "The output for this run was already submitted. Open a new run.",
        ));
    }
    if row.get::<bool, _>("expired") {
        return Err(err(
            StatusCode::GONE,
            "RUN_EXPIRED",
            "This run expired before its output was submitted. Open a new run.",
        ));
    }
    Ok(Run {
        run_id: found,
        agent_id: row.get("agent_id"),
        user_id: row.get("user_id"),
        allowed_tools: row.get("allowed_tools"),
        query: row.get("query"),
    })
}

// ─── Open ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OpenRunRequest {
    /// The agent's name or id. It must be registered on ABW and owned by the
    /// API key's user.
    pub agent: String,
    /// What the agent was asked. Stored on the episode, as for a hosted run.
    pub query: String,
}

/// `POST /v1/ground/runs`
pub async fn open_run_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<OpenRunRequest>,
) -> Result<Json<Value>, ApiError> {
    let key = match api_keys::validate_api_key(&state.db, bearer(&headers)?).await {
        Ok(AuthPrincipal::ApiKey(k)) => k,
        _ => {
            return Err(err(
                StatusCode::UNAUTHORIZED,
                "AUTH_REQUIRED",
                "Opening a run requires an ABW API key.",
            ))
        }
    };

    let db_agent = resolve_agent(&state, &req.agent)
        .await
        .map_err(|(s, m)| err(s, "AGENT_NOT_FOUND", m))?;
    if db_agent.owner_id.as_deref() != Some(key.user_id.as_str()) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "NOT_OWNER",
            "You can only ground an agent you own. Register it on ABW first.",
        ));
    }
    if !has_ground_scope(&key, &db_agent.agent_name) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "PERMISSION_DENIED",
            format!(
                "API key lacks the ground:* or ground:{} scope.",
                db_agent.agent_name
            ),
        ));
    }

    // The credit gate. The run itself is free, but a wallet that cannot pay
    // for one tool call cannot produce evidence, and opening a run it cannot
    // use would only defer the refusal.
    let wallet = get_or_create_wallet(&state.db, "user", &key.user_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;
    if wallet.balance <= 0 {
        gate_trust::decided(Gate::Credit, Decision::Refused, Some("ground.open: balance <= 0"));
        return Err(err(
            StatusCode::PAYMENT_REQUIRED,
            "INSUFFICIENT_CREDITS",
            "Top up your balance to open a grounding run.",
        ));
    }
    gate_trust::decided(Gate::Credit, Decision::Approved, None);

    let card = resolve_agent_card(&state, &db_agent);
    let tools = allowed_tools(&card);
    let has_contract = card
        .capabilities
        .output_contract
        .as_ref()
        .and_then(|oc| oc.get("grounding"))
        .and_then(|g| g.as_object())
        .is_some_and(|g| g.keys().any(|k| !k.ends_with("_provenance")))
        || fermi::grounding_trust::contracts_for(&db_agent.agent_name)
            .next()
            .is_some();

    // Reserve the episode now, so the run id is the episode id everywhere.
    let pulse =
        episode_boundary::Pulse::open(&state.memory_store, db_agent.agent_id, &req.query).await;
    let run_id = pulse.episode_id;
    let token = mint_token();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(RUN_TTL_MINUTES);

    sqlx::query(
        "INSERT INTO ground_runs (run_id, agent_id, user_id, token_hash, allowed_tools, query, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run_id)
    .bind(db_agent.agent_id)
    .bind(&key.user_id)
    .bind(hash_token(&token))
    .bind(&tools)
    .bind(&req.query)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;

    Ok(Json(json!({
        "run_id": run_id,
        "run_token": token,
        "expires_at": expires_at.to_rfc3339(),
        "tools": tools,
        "tool_price_credits": state.gas_fees.ground_tool_call,
        "contract": if has_contract { "declared" } else { "none" },
        "note": if has_contract {
            "Call tools through this run, then submit the output. Only values a tool \
             called through this run could have supplied will be graded as sourced."
        } else {
            "This agent declares no grounding contract, so its output will be recorded \
             but graded `undetermined`: nothing about its content can be checked. \
             Add an output_contract to its card to ground it."
        },
    })))
}

// ─── Tool call ────────────────────────────────────────────────────────────

/// `POST /v1/ground/runs/:run_id/tools/:tool`
pub async fn run_tool_handler(
    State(state): State<AppState>,
    Path((run_id, tool)): Path<(Uuid, String)>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let run = load_run(&state, &headers, run_id).await?;

    if !run.allowed_tools.iter().any(|t| t == &tool) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "TOOL_NOT_DECLARED",
            format!(
                "`{tool}` is not a tool this agent's card declares. Allowed: {}.",
                run.allowed_tools.join(", ")
            ),
        ));
    }

    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{}", run.user_id)) {
        gate_trust::decided(Gate::RateLimit, Decision::Refused, Some("ground.tool"));
        return Err(err(
            StatusCode::TOO_MANY_REQUESTS,
            "RATE_LIMIT",
            format!("Retry after {retry} seconds."),
        ));
    }
    gate_trust::decided(Gate::RateLimit, Decision::Approved, None);

    let price = state.gas_fees.ground_tool_call;
    let wallet = get_or_create_wallet(&state.db, "user", &run.user_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;
    if wallet.balance < price {
        gate_trust::decided(Gate::Credit, Decision::Refused, Some("ground.tool: balance < price"));
        return Err(err(
            StatusCode::PAYMENT_REQUIRED,
            "INSUFFICIENT_CREDITS",
            format!("A tool call costs {price} credit(s). Top up your balance."),
        ));
    }
    gate_trust::decided(Gate::Credit, Decision::Approved, None);

    let db_agent = resolve_agent(&state, &run.agent_id.to_string())
        .await
        .map_err(|(s, m)| err(s, "AGENT_NOT_FOUND", m))?;
    let card = resolve_agent_card(&state, &db_agent);
    let ctx = ToolContext {
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(run.agent_id),
        workspace_id: None,
        workspace_slug: None,
        workspace_git: None,
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: Some(run.user_id.clone()),
        user_secrets: resolve_agent_owner_secrets(&state, &db_agent).await,
        credentials: crate::build_execution_credentials(&state, &db_agent, &card).await,
        parent_episode_id: Some(run.run_id),
        eval_trigger: None,
        remote_mcp: None,
    };

    let started = Instant::now();
    let result = PlatformToolRegistry::standard()
        .execute(&tool, &input, &ctx)
        .await;
    let duration_ms = started.elapsed().as_millis() as i64;
    let (ok, output) = match &result {
        Ok(s) => (true, s.clone()),
        Err(e) => (false, e.clone()),
    };

    // Charged only when the tool answered. A failed call produced no
    // evidence, and charging for it would bill the owner for our outage.
    let charged = if ok {
        crate::handlers::ground::charge(&state, wallet.wallet_id, price, &tool, run.run_id).await
    } else {
        0
    };

    sqlx::query(
        "INSERT INTO ground_run_tool_calls (run_id, tool_name, input, output, ok, duration_ms, credits)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(run.run_id)
    .bind(&tool)
    .bind(&input)
    .bind(&output)
    .bind(ok)
    .bind(duration_ms)
    .bind(charged)
    .execute(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;

    match result {
        Ok(out) => Ok(Json(json!({
            "tool": tool,
            "output": out,
            "credits_charged": charged,
            "duration_ms": duration_ms,
        }))),
        Err(e) => Err(err(StatusCode::BAD_GATEWAY, "TOOL_FAILED", e)),
    }
}

async fn charge(state: &AppState, wallet_id: Uuid, price: i32, tool: &str, run_id: Uuid) -> i32 {
    let run = run_id.to_string();
    match fermi::gas::charge_gas(
        &state.db,
        wallet_id,
        price,
        "gas_fee",
        &format!("grounding run tool call: {tool}"),
        Some(run.as_str()),
    )
    .await
    {
        Ok(n) => n,
        Err((_, e)) => {
            tracing::error!(run = %run_id, tool, error = %e, "ground tool call ran and was not charged");
            0
        }
    }
}

// ─── Submit ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SubmitOutputRequest {
    /// The agent's final answer, as text. JSON inside it is found the same way
    /// it is for a hosted agent.
    #[serde(default)]
    pub response: Option<String>,
    /// Or the document directly.
    #[serde(default)]
    pub document: Option<Value>,
}

/// `POST /v1/ground/runs/:run_id/output`
pub async fn submit_output_handler(
    State(state): State<AppState>,
    Path(run_id): Path<Uuid>,
    headers: HeaderMap,
    Json(req): Json<SubmitOutputRequest>,
) -> Result<Json<Value>, ApiError> {
    let run = load_run(&state, &headers, run_id).await?;

    let raw = match (req.response, req.document) {
        (Some(r), _) => r,
        (None, Some(d)) => d.to_string(),
        (None, None) => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                "INVALID_ARGUMENT",
                "Provide `response` (text) or `document` (JSON).",
            ))
        }
    };

    // Close first, atomically, so one run is graded once.
    let closed = sqlx::query(
        "UPDATE ground_runs SET closed_at = now() WHERE run_id = $1 AND closed_at IS NULL",
    )
    .bind(run.run_id)
    .execute(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;
    if closed.rows_affected() == 0 {
        return Err(err(StatusCode::CONFLICT, "RUN_CLOSED", "Output already submitted."));
    }

    // The run record: every tool call ABW made for this run, in order.
    let calls = sqlx::query(
        "SELECT tool_name, input, output, duration_ms FROM ground_run_tool_calls
          WHERE run_id = $1 AND ok ORDER BY call_id",
    )
    .bind(run.run_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;
    let tool_invocations: Vec<ToolInvocation> = calls
        .iter()
        .enumerate()
        .map(|(i, r)| ToolInvocation {
            tool_name: r.get("tool_name"),
            input: r.get("input"),
            output: r.get("output"),
            duration_ms: r.get::<i64, _>("duration_ms").max(0) as u64,
            iteration: i as u32,
        })
        .collect();

    let db_agent = resolve_agent(&state, &run.agent_id.to_string())
        .await
        .map_err(|(s, m)| err(s, "AGENT_NOT_FOUND", m))?;
    let card = resolve_agent_card(&state, &db_agent);
    let oc = card.capabilities.output_contract.as_ref();

    let output = AgentOutput {
        agent_name: db_agent.agent_name.clone(),
        agent_type: card.agent_type.clone(),
        timestamp: chrono::Utc::now(),
        status: AgentStatus::Success,
        evidence: vec![],
        confidence: 0.0,
        sources_consulted: tool_invocations.iter().map(|t| t.tool_name.clone()).collect(),
        execution_time_ms: 0,
        tokens_used: None,
        input_tokens: None,
        output_tokens: None,
        metadata: AgentMetadata {
            reasoning: Some(raw.clone()),
            ..Default::default()
        },
        loop_iterations: tool_invocations.len() as u32,
        tool_invocations,
        raw_response: Some(raw.clone()),
    };

    // The boundary, as for a hosted agent. The episode was reserved at open.
    let pulse = episode_boundary::Pulse::reserved_upstream(run.run_id);
    let graded = pulse.grade(&db_agent.agent_name, oc, Some(&raw));
    let tools_called: Vec<&str> = output
        .tool_invocations
        .iter()
        .map(|t| t.tool_name.as_str())
        .collect();
    let completeness = pulse.assess_completeness(&graded, &tools_called);
    let validation =
        fermi::agent_backend::envelope::validation_status(oc, graded.enforced.as_ref());
    gate_trust::decided_about(
        Gate::OutputSchema,
        fermi::agent_backend::envelope::decision_for(validation),
        Some(&format!("{}: {validation}", db_agent.agent_name)),
        Some(&db_agent.agent_name),
    );

    let mut episode = agent_output_to_episode(db_agent.agent_id, &run.query, &output);
    episode.episode_id = run.run_id;
    episode.persona_version_at_write = Some(db_agent.persona_version);
    episode.tags.push("route:ground".to_string());

    let stored = episode_boundary::close(
        pulse,
        &graded,
        episode_boundary::Write {
            store: &state.memory_store,
            db: Some(&state.db),
            agent_slug: &db_agent.agent_name,
            episode,
            route: fermi::route_trust::RouteSelection::CallerNamed,
            provenance: None,
            source_ref: Some(json!({
                "kind": "ground_service",
                "agent_id": db_agent.agent_id,
                "run_id": run.run_id,
            })),
            workspace: None,
        },
    )
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()))?;

    let contract_applied = fermi::grounding_trust::contracts_for(&db_agent.agent_name)
        .next()
        .is_some()
        || oc
            .and_then(|o| o.get("grounding"))
            .and_then(|g| g.as_object())
            .is_some_and(|g| g.keys().any(|k| !k.ends_with("_provenance")));
    let reliance = fermi::reliance::reliance(fermi::reliance::Answer {
        document: graded.enforced.is_some(),
        contract_applied,
        report: &graded.report,
        completeness: Some(&completeness),
        validation,
    });
    let stripped: Vec<&str> = graded
        .report
        .violations
        .iter()
        .map(|v| v.path.as_str())
        .collect();
    let base = fermi::agent_backend::credentials::abw_base_url();

    Ok(Json(json!({
        "run_id": stored,
        "reliance": { "status": reliance, "why": fermi::reliance::why(reliance) },
        // The output to use. Values no tool called through this run could
        // have supplied are nulled.
        "document": graded.enforced,
        "grounding": {
            "stripped": stripped,
            "provenance": graded.report.provenance,
            "tools_called": tools_called,
        },
        "validation": { "status": validation },
        "completeness": {
            "asked_for": completeness.asked_for,
            "filled": completeness.filled,
            "owed": completeness.owed,
            "no_data": completeness.no_data,
        },
        "trace_url": format!("{base}/grounding/{stored}"),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(scopes: &[&str]) -> ApiKey {
        ApiKey {
            key_id: Uuid::nil(),
            user_id: "u".into(),
            name: "k".into(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn grounding_needs_its_own_scope() {
        assert!(has_ground_scope(&key(&["ground:*"]), "any"));
        assert!(has_ground_scope(&key(&["ground:weather"]), "weather"));
        assert!(!has_ground_scope(&key(&["ground:weather"]), "genome"));
        // An invoke key is a different permission: calling ABW's agents is not
        // grounding your own.
        assert!(!has_ground_scope(&key(&["a2a:invoke:*"]), "weather"));
    }

    #[test]
    fn run_tokens_are_distinct_prefixed_and_stored_hashed() {
        let a = mint_token();
        let b = mint_token();
        assert_ne!(a, b);
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(a.len(), TOKEN_PREFIX.len() + 64);
        let h = hash_token(&a);
        assert_eq!(h.len(), 64);
        assert!(!h.contains(&a[TOKEN_PREFIX.len()..]));
        assert_eq!(h, hash_token(&a));
    }
}
