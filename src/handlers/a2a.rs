//! # ABW as A2A Provider — HTTP handlers
//!
//! Phase 1 ✓  `GET  /a2a/:slug/agent-card.json` — public discovery
//! Phase 2 ✓  `POST /a2a/:slug/message:send`     — sync execution, API key auth
//!             `GET  /a2a/:slug/tasks/:episode_id` — task poll
//! Phase 3    `POST /a2a/:slug/message:stream`    — SSE streaming (TODO)
//!
//! Pure mapping logic → `fermi::a2a_card`, `fermi::a2a_task`.
//! Design: `docs/DESIGN_a2a_provider.md`

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use fermi::agent_backend::tools::{PlatformToolRegistry, ToolContext};
use fermi::agent_backend::{
    executor::{AgentExecutor, AgentStatus},
    tool_executor::ToolAwareExecutor,
    ExecutionContext,
};
use fermi::episode_boundary;
use fermi::gas::{charge_execution_with_royalty, charge_gas};
use fermi_auth::{api_keys, get_or_create_wallet, types::ApiKey};
use futures_core::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    agent_output_to_episode, resolve_agent, resolve_agent_card, resolve_agent_owner_secrets,
    AppState,
};

// ─── A2A Wire types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub message: A2AMessage,
    #[serde(default)]
    pub configuration: MessageConfiguration,
}

#[derive(Debug, Deserialize)]
pub struct A2AMessage {
    #[serde(rename = "messageId", default)]
    pub message_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub parts: Vec<A2APart>,
}

#[derive(Debug, Deserialize, Default)]
pub struct MessageConfiguration {
    /// `true` → return immediately with SUBMITTED Task; caller polls.
    /// `false` (default) → block until complete, return COMPLETED Task.
    #[serde(rename = "returnImmediately", default)]
    pub return_immediately: bool,
    /// Optional: register a push notification webhook at request time.
    /// The platform fires this webhook when the task reaches a terminal state.
    #[serde(rename = "taskPushNotificationConfig", default)]
    pub push_config: Option<InlinePushConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InlinePushConfig {
    /// Webhook URL the platform will POST a StreamResponse to.
    pub url: String,
    /// Optional caller-provided token for HMAC/bearer verification.
    #[serde(default)]
    pub token: Option<String>,
    /// Optional authentication for the platform to include in the POST.
    #[serde(default)]
    pub authentication: Option<InlinePushAuth>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InlinePushAuth {
    /// HTTP auth scheme, e.g. "Bearer" or "Basic".
    pub scheme: String,
    /// Credentials to attach.
    #[serde(default)]
    pub credentials: Option<String>,
}

/// Body for `POST /a2a/:slug/tasks/:episode_id/pushNotificationConfigs`.
#[derive(Debug, Deserialize)]
pub struct CreatePushConfigBody {
    pub url: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(rename = "authentication", default)]
    pub authentication: Option<InlinePushAuth>,
}

#[derive(Debug, Deserialize)]
pub struct A2APart {
    /// Plain-text query.
    pub text: Option<String>,
    /// Structured data (JSON). Serialised to a string and used as the query.
    pub data: Option<Value>,
    /// Raw bytes (base64). Not supported in Phase 2.
    pub raw: Option<String>,
}

// ─── Phase 1: discovery ───────────────────────────────────────────────────

/// `GET /a2a/:slug/agent-card.json`
///
/// A2A v1.0 AgentCard for any published, public ABW agent. No auth required.
pub async fn agent_card_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &slug).await?;

    let is_public = db_agent.status == "published" && db_agent.visibility == "public";
    let is_system = db_agent.tier == "system";
    if !is_public || is_system {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Agent '{}' is not available via A2A", slug),
        ));
    }

    let card = resolve_agent_card(&state, &db_agent);
    let a2a_card =
        fermi::a2a_card::agent_card_to_a2a_from_card(&slug, db_agent.description.as_deref(), &card);

    let body = serde_json::to_string_pretty(&a2a_card)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        body,
    )
        .into_response())
}

// ─── Phase 2: execution ───────────────────────────────────────────────────

/// `POST /a2a/:slug/message:send`
///
/// Execute an ABW agent synchronously (default) or non-blocking.
/// Requires `Authorization: Bearer ferm_...` with scope `a2a:invoke:*`
/// or `a2a:invoke:<slug>`.
///
/// Blocking (default): runs the agent, returns a COMPLETED Task.
/// Non-blocking (`returnImmediately: true`): reserves an episode, returns
/// a SUBMITTED Task, caller polls via `GET /a2a/:slug/tasks/:episode_id`.
/// Dispatch `POST /a2a/:slug/:method` to the right handler.
///
/// ## Why this exists rather than two routes
///
/// The A2A REST transport names its custom methods AIP-136 style, with a colon:
/// `message:send`, `message:stream`. Those were registered as two literal axum
/// routes and the process **panicked at boot**:
///
/// ```text
/// Invalid route "/a2a/:slug/message:stream": insertion failed due to
/// conflict with previously registered route: /a2a/:slug/message:send
/// ```
///
/// axum 0.7 routes through `matchit` 0.7, where `:` opens a path parameter
/// **anywhere in a segment**, not only at its start. So `message:send` was never
/// a literal: it parsed as the static text `message` followed by a parameter
/// named `send`. `message:stream` parsed as the same static text followed by a
/// parameter named `stream`, two differently-named parameters in one slot, which
/// is a conflict. Had only one of them existed there would have been no panic
/// and the route would have quietly matched `/a2a/x/messageANYTHING`.
///
/// matchit 0.7 has no escape for a literal colon — the `{brace}` syntax that
/// would allow one arrives with axum 0.8. So the segment is captured whole and
/// compared here, which keeps the URLs exactly as the spec writes them.
///
/// Unknown methods answer 404 in the A2A error envelope rather than falling
/// through to the SPA fallback, because a client that misspells a method should
/// be told so in the protocol it is speaking.
pub async fn method_dispatch_handler(
    State(state): State<AppState>,
    Path((slug, method)): Path<(String, String)>,
    headers: HeaderMap,
    body: Json<SendMessageRequest>,
) -> Response {
    match method.as_str() {
        "message:send" => send_message_handler(State(state), Path(slug), headers, body)
            .await
            .into_response(),
        "message:stream" => stream_message_handler(State(state), Path(slug), headers, body)
            .await
            .into_response(),
        other => (
            StatusCode::NOT_FOUND,
            Json(a2a_error_body(
                404,
                &format!(
                    "unknown A2A method `{other}`. This endpoint serves \
                     `message:send` and `message:stream`."
                ),
                "METHOD_NOT_FOUND",
            )),
        )
            .into_response(),
    }
}

pub async fn send_message_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // ── 1. Auth ──────────────────────────────────────────────────────────
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;
    let caller_id = api_key.user_id.clone();

    // ── 2. Resolve + validate agent ─────────────────────────────────────
    let db_agent = resolve_agent(&state, &slug)
        .await
        .map_err(|(s, m)| (s, Json(a2a_error_body(s.as_u16(), &m, "AGENT_NOT_FOUND"))))?;

    let is_public = db_agent.status == "published" && db_agent.visibility == "public";
    let is_system = db_agent.tier == "system";
    if !is_public || is_system {
        return Err((
            StatusCode::NOT_FOUND,
            Json(a2a_error_body(
                404,
                &format!("Agent '{}' is not available via A2A", slug),
                "AGENT_NOT_FOUND",
            )),
        ));
    }

    // ── 3. Credit check ─────────────────────────────────────────────────
    let wallet = get_or_create_wallet(&state.db, "user", &caller_id)
        .await
        .map_err(|e| credit_error(&format!("Wallet error: {}", e), None))?;

    if wallet.balance <= 0 {
        let base = fermi::agent_backend::credentials::abw_base_url();
        return Err(credit_error(
            "Insufficient credits. Top up your balance to continue.",
            Some(&format!("{}/credits/topup?ref=a2a", base)),
        ));
    }

    // ── 4. Extract query from A2A message parts ──────────────────────────
    let query = extract_query(&req)?;

    // ── 5. Rate limit ────────────────────────────────────────────────────
    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{}", caller_id)) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(a2a_error_body(
                429,
                &format!("Rate limit exceeded. Retry after {} seconds.", retry),
                "RATE_LIMIT",
            )),
        ));
    }

    // ── 6. Resolve card + KG enrichment ─────────────────────────────────
    let card = resolve_agent_card(&state, &db_agent);
    let (card, _) = fermi::agent_backend::kg_context::enrich_with_kg_context(
        &state.memory_store,
        &state.embedder,
        db_agent.agent_id,
        &query,
        card,
    )
    .await;

    // ── 7. Open episode pulse ─────────────────────────────────────────────
    let pulse = episode_boundary::Pulse::open(&state.memory_store, db_agent.agent_id, &query).await;
    let episode_id = pulse.episode_id;

    // Non-blocking mode: return immediately with SUBMITTED Task.
    // The caller must poll GET /a2a/:slug/tasks/:episode_id.
    // (The agent will NOT run — this is a future Phase 2B feature.)
    if req.configuration.return_immediately {
        return Ok(Json(fermi::a2a_task::submitted_task(
            episode_id, &caller_id,
        )));
    }

    // ── 8. Build execution context ──────────────────────────────────────
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;
    let owner_secrets = resolve_agent_owner_secrets(&state, &db_agent).await;

    let agent_stmt = fermi::ast::AgentStmt {
        name: slug.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: query.clone(),
        executor: Some(fermi::ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };
    let context = ExecutionContext {
        program: fermi::ast::Program {
            statements: vec![fermi::ast::Statement::Agent(agent_stmt.clone())],
        },
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
        attachments: vec![],
    };

    let tool_context = Arc::new(ToolContext {
        parent_episode_id: Some(episode_id),
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(db_agent.agent_id),
        workspace_id: None,
        workspace_slug: None,
        workspace_git: None,
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: Some(caller_id.clone()),
        user_secrets: owner_secrets,
        credentials,
        eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
            state: state.clone(),
        })),
        remote_mcp: None,
    });

    // ── 9. Execute ────────────────────────────────────────────────────────
    let executor = ToolAwareExecutor::new(
        state.registry.executor_arc(),
        PlatformToolRegistry::standard(),
        tool_context,
    );
    let output = executor.execute(&agent_stmt, &context).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(a2a_error_body(
                500,
                &format!("Execution failed: {}", e),
                "INTERNAL_ERROR",
            )),
        )
    })?;

    // ── 10. Grade + validate ──────────────────────────────────────────────
    let graded = pulse.grade(
        &db_agent.agent_name,
        card.capabilities.output_contract.as_ref(),
        output.raw_response.as_deref(),
    );

    // ── 11. Record + store episode ────────────────────────────────────────
    let _ = state.registry.record_execution(&slug, &output);

    let mut episode = agent_output_to_episode(db_agent.agent_id, &query, &output);
    episode.episode_id = episode_id;
    episode.persona_version_at_write = Some(db_agent.persona_version);

    // Mark as A2A-sourced in episode tags.
    episode.tags.push("route:a2a".to_string());
    episode.tags.push(format!("a2a:slug:{}", slug));

    let input_binding = fermi::port_trust::bind_input(&card.accepts);
    crate::stamp_input_binding(&mut episode, &input_binding, None);

    let embed_text = format!(
        "{} {}",
        query,
        output.metadata.reasoning.as_deref().unwrap_or("")
    );
    let provenance = state.embedder.generate_provenanced(&embed_text).await.ok();
    let source_ref = json!({
        "kind": "a2a_handler",
        "agent_slug": slug,
        "caller_user_id": caller_id,
    });

    let stored_id = episode_boundary::close(
        pulse,
        &graded,
        episode_boundary::Write {
            store: &state.memory_store,
            db: Some(&state.db),
            agent_slug: &db_agent.agent_name,
            episode,
            route: fermi::route_trust::RouteSelection::CallerNamed,
            provenance: provenance.as_ref(),
            source_ref: Some(source_ref),
            // Inbound A2A: an external client called this agent over the protocol.
            // There is no workspace binding on that path yet — when A2A tasks gain
            // one, this is where it goes.
            workspace: None,
        },
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(a2a_error_body(500, &e.to_string(), "INTERNAL_ERROR")),
        )
    })?;

    // ── 12. Charge credits ────────────────────────────────────────────────
    let tokens = output.tokens_used.unwrap_or(0) as i32;
    let (execution_fee, gas_fee) = state.gas_fees.execution_fee(tokens);
    let ep_id_str = stored_id.to_string();

    let _ = charge_execution_with_royalty(
        &state.db,
        wallet.wallet_id,
        &caller_id,
        execution_fee,
        db_agent.owner_id.as_deref(),
        &db_agent.tier,
        &slug,
        tokens,
        Some(ep_id_str.as_str()),
        state.gas_fees.execution_owner_royalty_pct,
    )
    .await;

    let _ = charge_gas(
        &state.db,
        wallet.wallet_id,
        gas_fee,
        "gas_fee",
        &format!("A2A gas fee for {}", slug),
        Some(ep_id_str.as_str()),
    )
    .await;

    // ── 13. Build Task response ─────────────────────────────────────────────
    let task = if matches!(
        output.status,
        fermi::agent_backend::executor::AgentStatus::Failed
            | fermi::agent_backend::executor::AgentStatus::Timeout
    ) {
        let reason = output
            .metadata
            .failure_reason
            .as_deref()
            .unwrap_or("Execution failed");
        fermi::a2a_task::failed_task(stored_id, &caller_id, reason)
    } else {
        fermi::a2a_task::completed_task(stored_id, &caller_id, output.raw_response.as_deref())
    };

    // ── 14. Register inline push config (if provided) + fire webhook ──────
    // If the caller supplied a push config in the request, register it and
    // deliver the completed Task payload immediately (task already done).
    if let Some(ref pc) = req.configuration.push_config {
        let pool = state.db.clone();
        let auth_scheme = pc.authentication.as_ref().map(|a| a.scheme.as_str());
        let auth_creds = pc
            .authentication
            .as_ref()
            .and_then(|a| a.credentials.as_deref());
        if let Ok(cfg_id) = crate::a2a_webhook::register(
            &pool,
            stored_id,
            &slug,
            &caller_id,
            &pc.url,
            auth_scheme,
            auth_creds,
            pc.token.as_deref(),
        )
        .await
        {
            // Deliver the completed Task payload right away.
            let cfg = crate::a2a_webhook::PushConfig {
                config_id: cfg_id,
                webhook_url: pc.url.clone(),
                auth_scheme: pc.authentication.as_ref().map(|a| a.scheme.clone()),
                auth_credentials: pc
                    .authentication
                    .as_ref()
                    .and_then(|a| a.credentials.clone()),
            };
            let payload = task.clone();
            tokio::spawn(async move {
                match crate::a2a_webhook::deliver(&cfg, &payload).await {
                    Ok(()) => crate::a2a_webhook::record_delivery(&pool, cfg_id, true, None).await,
                    Err(e) => {
                        crate::a2a_webhook::record_delivery(&pool, cfg_id, false, Some(&e)).await
                    }
                }
            });
        }
    } else {
        // Fire any previously-registered push configs for this task_id.
        crate::a2a_webhook::fire_for_task(state.db.clone(), stored_id, task.clone());
    }

    Ok(Json(task))
}

/// `GET /a2a/:slug/tasks/:episode_id`
///
/// Poll a previously created Task by its episode ID.
/// Returns the current state (WORKING while in-progress, COMPLETED when done).
pub async fn get_task_handler(
    State(state): State<AppState>,
    Path((slug, episode_id_str)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Auth + scope.
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;
    let caller_id = api_key.user_id.clone();

    let episode_id = Uuid::parse_str(&episode_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid episode_id — must be a UUID.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;

    match state.memory_store.get_episode(episode_id).await {
        Ok(episode) => {
            use agent_bestiary_memory::ExecutionStatus;
            let raw_response = episode.response_text.as_deref();
            let task = match episode.execution_status {
                ExecutionStatus::Success | ExecutionStatus::Partial => {
                    fermi::a2a_task::completed_task(episode_id, &caller_id, raw_response)
                }
                ExecutionStatus::Failure => {
                    fermi::a2a_task::failed_task(episode_id, &caller_id, "Execution failed")
                }
            };
            Ok(Json(task))
        }
        Err(_) => Err((
            StatusCode::NOT_FOUND,
            Json(a2a_error_body(
                404,
                &format!("Task '{}' not found.", episode_id_str),
                "TASK_NOT_FOUND",
            )),
        )),
    }
}

// ─── Phase 3: SSE streaming ─────────────────────────────────────────────────

/// `POST /a2a/:slug/message:stream`
///
/// Execute an ABW agent and stream the result as A2A v1.0 `StreamResponse`
/// events over Server-Sent Events. Same auth and billing as `message:send`.
///
/// Event sequence:
/// 1. `Task { status: WORKING }` — execution started
/// 2. `artifactUpdate` — the agent's output (JSON or text)
/// 3. `statusUpdate { COMPLETED | FAILED }` — terminal state
///
/// The stream is kept alive with 15-second keepalive pings.
pub async fn stream_message_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SendMessageRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    // ── 1. Auth ──────────────────────────────────────────────────────────
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;
    let caller_id = api_key.user_id.clone();

    // ── 2. Agent check ──────────────────────────────────────────────────
    let db_agent = resolve_agent(&state, &slug)
        .await
        .map_err(|(s, m)| (s, Json(a2a_error_body(s.as_u16(), &m, "AGENT_NOT_FOUND"))))?;
    if db_agent.status != "published"
        || db_agent.visibility != "public"
        || db_agent.tier == "system"
    {
        return Err((
            StatusCode::NOT_FOUND,
            Json(a2a_error_body(
                404,
                &format!("Agent '{}' is not available via A2A", slug),
                "AGENT_NOT_FOUND",
            )),
        ));
    }

    // ── 3. Credit check ─────────────────────────────────────────────────
    let wallet = get_or_create_wallet(&state.db, "user", &caller_id)
        .await
        .map_err(|e| credit_error(&format!("Wallet error: {}", e), None))?;
    if wallet.balance <= 0 {
        let base = fermi::agent_backend::credentials::abw_base_url();
        return Err(credit_error(
            "Insufficient credits. Top up your balance to continue.",
            Some(&format!("{}/credits/topup?ref=a2a", base)),
        ));
    }

    // ── 4. Extract query ─────────────────────────────────────────────────
    let query = extract_query(&req)?;

    // ── 5. Rate limit ────────────────────────────────────────────────────
    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{}", caller_id)) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(a2a_error_body(
                429,
                &format!("Rate limit exceeded. Retry after {} seconds.", retry),
                "RATE_LIMIT",
            )),
        ));
    }

    // ── 6. Resolve card + KG enrichment ─────────────────────────────────
    let card = resolve_agent_card(&state, &db_agent);
    let (card, _) = fermi::agent_backend::kg_context::enrich_with_kg_context(
        &state.memory_store,
        &state.embedder,
        db_agent.agent_id,
        &query,
        card,
    )
    .await;

    // ── 7. Open episode pulse ─────────────────────────────────────────────
    let pulse = episode_boundary::Pulse::open(&state.memory_store, db_agent.agent_id, &query).await;
    let episode_id = pulse.episode_id;

    // ── 8. Build execution context ──────────────────────────────────────
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;
    let owner_secrets = resolve_agent_owner_secrets(&state, &db_agent).await;

    let agent_stmt = fermi::ast::AgentStmt {
        name: slug.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: query.clone(),
        executor: Some(fermi::ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };
    let context = ExecutionContext {
        program: fermi::ast::Program {
            statements: vec![fermi::ast::Statement::Agent(agent_stmt.clone())],
        },
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
        attachments: vec![],
    };

    let tool_context = Arc::new(ToolContext {
        parent_episode_id: Some(episode_id),
        memory_store: state.memory_store.clone(),
        embedder: state.embedder.clone(),
        registry: state.registry.clone(),
        current_agent_id: Some(db_agent.agent_id),
        workspace_id: None,
        workspace_slug: None,
        workspace_git: None,
        db: Some(state.db.clone()),
        gas_fees: Some(state.gas_fees.clone()),
        user_id: Some(caller_id.clone()),
        user_secrets: owner_secrets,
        credentials,
        eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
            state: state.clone(),
        })),
        remote_mcp: None,
    });

    let executor = Arc::new(ToolAwareExecutor::new(
        state.registry.executor_arc(),
        PlatformToolRegistry::standard(),
        tool_context,
    ));

    // ── 9. Capture variables for the stream closure ──────────────────────
    let state_clone = state.clone();
    let caller_clone = caller_id.clone();
    let wallet_id = wallet.wallet_id;
    let gas_fees = state.gas_fees.clone();
    let agent_db_id = db_agent.agent_id;
    let agent_name = db_agent.agent_name.clone();
    let agent_owner_id = db_agent.owner_id.clone();
    let agent_tier = db_agent.tier.clone();
    let declared_accepts = card.accepts.clone();
    let declared_oc = card.capabilities.output_contract.clone();
    let db_agent_obs = db_agent.clone();
    let slug_clone = slug.clone();

    // ── 10. Build SSE stream ──────────────────────────────────────────────
    let stream = async_stream::stream! {
        let start = Instant::now();

        // ── A2A event 1: Task WORKING ─────────────────────────────────
        // Signals that execution has started. The caller can use the task id
        // to subscribe or poll even if the connection drops.
        yield Ok(Event::default().data(
            json!({
                "task": {
                    "id": episode_id.to_string(),
                    "contextId": caller_clone,
                    "status": {
                        "state": fermi::a2a_task::state::WORKING,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "message": {
                            "role": "ROLE_AGENT",
                            "messageId": Uuid::new_v4().to_string(),
                            "parts": [{ "text": format!("Running {}…", agent_name) }]
                        }
                    }
                }
            })
            .to_string(),
        ));

        // ── Execute ───────────────────────────────────────────────────
        let result = executor.execute(&agent_stmt, &context).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                // Grade, record, store episode (same as send_message_handler).
                let graded = pulse.grade(
                    &agent_name,
                    declared_oc.as_ref(),
                    output.raw_response.as_deref(),
                );

                // OutputSchema gate.
                {
                    let doc    = graded.enforced.as_ref();
                    let schema = declared_oc.as_ref().and_then(|oc| oc.get("schema")).filter(|v| v.is_object());
                    let status = match (schema, doc) {
                        (Some(sch), Some(d)) => {
                            let r = fermi::schema_validate::validate(sch, d);
                            if r.is_valid() { "valid" } else if r.is_contradiction() { "invalid" } else { "unverified_unsupported_schema" }
                        }
                        (None, _)    => "unverified_no_schema",
                        (Some(_), None) => "unverified_no_payload",
                    };
                    fermi::gate_trust::decided_about(
                        fermi::gate_trust::Gate::OutputSchema,
                        fermi::agent_backend::envelope::decision_for(status),
                        Some(&format!("{slug_clone}: {status}")),
                        Some(&slug_clone),
                    );
                    if fermi::schema_conformance::score_for(status).is_some() {
                        fermi::schema_conformance::record(
                            &state_clone.db, agent_db_id, episode_id, status,
                            declared_oc.as_ref().and_then(|oc| oc.get("produces_schema")).and_then(|v| v.as_str()),
                        ).await;
                    }
                }

                let _ = state_clone.registry.record_execution(&slug_clone, &output);

                let mut episode = agent_output_to_episode(agent_db_id, &query, &output);
                episode.episode_id = episode_id;
                episode.persona_version_at_write = Some(db_agent_obs.persona_version);
                episode.tags.push("route:a2a_stream".to_string());
                episode.tags.push(format!("a2a:slug:{}", slug_clone));

                let verified = fermi::port_trust::bind_input(&declared_accepts);
                crate::stamp_input_binding(&mut episode, &verified, None);

                let dyad_id = agent_bestiary_memory::dyad_id(agent_db_id, &caller_clone);
                episode.dyad_id = Some(dyad_id.clone());
                crate::spawn_dyad_observation(&state_clone, agent_db_id, dyad_id, &query, &output);

                let embed_text = format!("{} {}", query, output.metadata.reasoning.as_deref().unwrap_or(""));
                let provenance = state_clone.embedder.generate_provenanced(&embed_text).await.ok();
                let source_ref = json!({ "kind": "a2a_stream_handler", "agent_slug": slug_clone });

                let episode_for_obs = episode.clone();

                let stored_id = match episode_boundary::close(
                    pulse,
                    &graded,
                    episode_boundary::Write {
                        store: &state_clone.memory_store,
                        db: Some(&state_clone.db),
                        agent_slug: &agent_name,
                        episode,
                        route: fermi::route_trust::RouteSelection::CallerNamed,
                        provenance: provenance.as_ref(),
                        source_ref: Some(source_ref),
                        // As above: inbound A2A carries no workspace.
                        workspace: None,
                    },
                )
                .await
                {
                    Ok(id) => Some(id),
                    Err(e) => { eprintln!("Warning: a2a_stream episode storage failed: {}", e); None }
                };

                if stored_id.is_some() {
                    crate::handlers::live_observability::spawn_live_observation(
                        &state_clone,
                        crate::handlers::live_observability::LiveObservation {
                            episode: episode_for_obs,
                            agent: db_agent_obs.clone(),
                            response: output.metadata.reasoning.clone().unwrap_or_default(),
                            session_id: Some("live:a2a_stream".to_string()),
                            rupture_detected: false,
                        },
                    );
                }

                // Charge credits.
                let tokens = output.tokens_used.unwrap_or(0) as i32;
                let (execution_fee, gas_fee_amt) = gas_fees.execution_fee(tokens);
                let ep_str = stored_id.map(|id| id.to_string()).unwrap_or_default();
                let ep_ref = if ep_str.is_empty() { None } else { Some(ep_str.as_str()) };

                let _ = charge_execution_with_royalty(
                    &state_clone.db, wallet_id, &caller_clone,
                    execution_fee, agent_owner_id.as_deref(), &agent_tier,
                    &slug_clone, tokens, ep_ref,
                    gas_fees.execution_owner_royalty_pct,
                ).await;
                let _ = charge_gas(&state_clone.db, wallet_id, gas_fee_amt, "gas_fee",
                    &format!("A2A stream gas fee for {}", slug_clone), ep_ref).await;

                let task_id = stored_id.unwrap_or(episode_id).to_string();

                // ── A2A event 2: artifactUpdate ───────────────────────
                // The agent's result as a typed Artifact.
                let artifact = fermi::a2a_task::build_stream_artifact(episode_id, output.raw_response.as_deref());
                yield Ok(Event::default().data(
                    json!({
                        "artifactUpdate": {
                            "taskId": task_id,
                            "contextId": caller_clone,
                            "artifact": artifact,
                            "lastChunk": true
                        }
                    })
                    .to_string(),
                ));

                // ── A2A event 3: statusUpdate COMPLETED or FAILED ─────
                let final_state = if matches!(output.status, AgentStatus::Failed | AgentStatus::Timeout) {
                    (fermi::a2a_task::state::FAILED, output.metadata.failure_reason.as_deref().unwrap_or("Execution failed").to_string())
                } else {
                    (fermi::a2a_task::state::COMPLETED, String::new())
                };

                let mut status_obj = json!({
                    "state": final_state.0,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "metadata": {
                        "elapsed_ms": elapsed_ms,
                        "tokens_used": output.tokens_used,
                        "credits_charged": execution_fee + gas_fee_amt,
                    }
                });
                if !final_state.1.is_empty() {
                    status_obj["message"] = json!({
                        "role": "ROLE_AGENT",
                        "messageId": Uuid::new_v4().to_string(),
                        "parts": [{ "text": final_state.1 }]
                    });
                }

                yield Ok(Event::default().data(
                    json!({
                        "statusUpdate": {
                            "taskId": task_id,
                            "contextId": caller_clone,
                            "status": status_obj
                        }
                    })
                    .to_string(),
                ));
            }
            Err(e) => {
                // ── A2A event: FAILED ─────────────────────────────────
                yield Ok(Event::default().data(
                    json!({
                        "statusUpdate": {
                            "taskId": episode_id.to_string(),
                            "contextId": caller_clone,
                            "status": {
                                "state": fermi::a2a_task::state::FAILED,
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                                "message": {
                                    "role": "ROLE_AGENT",
                                    "messageId": Uuid::new_v4().to_string(),
                                    "parts": [{ "text": format!("Execution failed: {}", e) }]
                                }
                            }
                        }
                    })
                    .to_string(),
                ));
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    ))
}

// ─── Auth helpers ─────────────────────────────────────────────────────────

/// Extract and validate an API key from `Authorization: Bearer ferm_...`.
async fn extract_api_key(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ApiKey, (StatusCode, Json<Value>)> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(a2a_error_body(
                    401,
                    "Missing Authorization header. Provide: Authorization: Bearer <api-key>",
                    "AUTH_REQUIRED",
                )),
            )
        })?;

    let raw_key = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(a2a_error_body(
                401,
                "Authorization must use Bearer scheme.",
                "AUTH_REQUIRED",
            )),
        )
    })?;

    let principal = api_keys::validate_api_key(&state.db, raw_key)
        .await
        .map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                Json(a2a_error_body(
                    401,
                    "Invalid or expired API key.",
                    "AUTH_REQUIRED",
                )),
            )
        })?;

    match principal {
        fermi_auth::AuthPrincipal::ApiKey(key) => Ok(key),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            Json(a2a_error_body(
                401,
                "A2A endpoints require an API key (not a session token).",
                "AUTH_REQUIRED",
            )),
        )),
    }
}

/// Check that the API key has the a2a:invoke scope for this agent.
///
/// Accepted scopes:
/// - `a2a:invoke:*`          — invoke any A2A-enabled agent
/// - `a2a:invoke:<slug>`     — invoke one specific agent
fn check_scope(key: &ApiKey, slug: &str) -> Result<(), (StatusCode, Json<Value>)> {
    let has_scope = key
        .scopes
        .iter()
        .any(|s| s == "a2a:invoke:*" || s == &format!("a2a:invoke:{}", slug));
    if !has_scope {
        let base = fermi::agent_backend::credentials::abw_base_url();
        return Err((
            StatusCode::FORBIDDEN,
            Json(a2a_error_body(
                403,
                &format!(
                    "API key lacks the required scope. \
                     Add a2a:invoke:* or a2a:invoke:{slug} at {base}/settings/api-keys",
                ),
                "PERMISSION_DENIED",
            )),
        ));
    }
    Ok(())
}

// ─── Request helpers ──────────────────────────────────────────────────────

/// Extract a query string from the first usable A2A message Part.
///
/// Priority: text → data (serialised to JSON string) → error.
fn extract_query(req: &SendMessageRequest) -> Result<String, (StatusCode, Json<Value>)> {
    for part in &req.message.parts {
        if let Some(text) = &part.text {
            if !text.is_empty() {
                return Ok(text.clone());
            }
        }
        if let Some(data) = &part.data {
            // Structured data → compact JSON string used as the query.
            return Ok(serde_json::to_string(data).unwrap_or_default());
        }
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(a2a_error_body(
            400,
            "Message must contain at least one Part with non-empty text or data.",
            "INVALID_ARGUMENT",
        )),
    ))
}

// ─── Phase 4: Discovery directory + push notification configs ──────────────

/// `GET /.well-known/agent-directory.json`
///
/// Platform-level listing of all A2A-enabled ABW agents. Publicly accessible
/// (no auth). External agent indexes and orchestrators fetch this to discover
/// available agents without knowing their slugs in advance.
///
/// Format: `{ version, updated_at, agents: [{ name, card_url, description, tags }] }`
pub async fn agent_directory_handler(
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, String)> {
    use sqlx::Row;
    let base = fermi::agent_backend::credentials::abw_base_url();

    // Query all published + public + non-system agents.
    let rows = sqlx::query(
        "SELECT a.agent_name, a.description, a.tags
         FROM agents a
         WHERE a.status = 'published'
           AND a.visibility = 'public'
           AND a.tier != 'system'
         ORDER BY a.agent_name ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agents: Vec<Value> = rows
        .iter()
        .map(|row| {
            let name: String = row.try_get("agent_name").unwrap_or_default();
            let description: Option<String> = row.try_get("description").ok().flatten();
            let tags: Vec<String> = row.try_get("tags").unwrap_or_default();
            json!({
                "name": name,
                "card_url": format!("{}/a2a/{}/agent-card.json", base, name),
                "description": description.unwrap_or_default(),
                "tags": tags,
            })
        })
        .collect();

    let directory = json!({
        "version": "1.0",
        "platform": "Agent Bestiary",
        "platform_url": "https://agent-bestiary.world",
        "updated_at": chrono::Utc::now().to_rfc3339(),
        "agents": agents,
        "count": agents.len(),
    });

    let body = serde_json::to_string_pretty(&directory)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        body,
    )
        .into_response())
}

/// `POST /a2a/:slug/tasks/:episode_id/pushNotificationConfigs`
///
/// Register a webhook to receive task completion notifications.
/// Useful when the caller used `returnImmediately: true` and wants to be
/// notified asynchronously instead of polling.
pub async fn create_push_config_handler(
    State(state): State<AppState>,
    Path((slug, episode_id_str)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<CreatePushConfigBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;
    let caller_id = api_key.user_id.clone();

    let task_id = Uuid::parse_str(&episode_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid episode_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;

    let auth_scheme = body.authentication.as_ref().map(|a| a.scheme.as_str());
    let auth_creds = body
        .authentication
        .as_ref()
        .and_then(|a| a.credentials.as_deref());

    let config_id = crate::a2a_webhook::register(
        &state.db,
        task_id,
        &slug,
        &caller_id,
        &body.url,
        auth_scheme,
        auth_creds,
        body.token.as_deref(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(a2a_error_body(500, &e, "INTERNAL_ERROR")),
        )
    })?;

    Ok(Json(json!({
        "id": config_id.to_string(),
        "taskId": task_id.to_string(),
        "url": body.url,
        "token": body.token,
        "authentication": body.authentication.as_ref().map(|a| json!({ "scheme": a.scheme })),
    })))
}

/// `GET /a2a/:slug/tasks/:episode_id/pushNotificationConfigs`
///
/// List all push configs registered for a task.
pub async fn list_push_configs_handler(
    State(state): State<AppState>,
    Path((slug, episode_id_str)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;

    let task_id = Uuid::parse_str(&episode_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid episode_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT config_id, webhook_url, auth_scheme, token, created_at, delivered_at, delivery_attempts
         FROM a2a_push_configs
         WHERE task_id = $1 AND caller_user_id = $2
         ORDER BY created_at DESC",
    )
    .bind(task_id)
    .bind(&api_key.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(a2a_error_body(500, &e.to_string(), "INTERNAL_ERROR")),
    ))?;

    let configs: Vec<Value> = rows.iter().map(|row| {
        json!({
            "id": row.try_get::<Uuid, _>("config_id").ok().map(|u| u.to_string()),
            "taskId": task_id.to_string(),
            "url": row.try_get::<String, _>("webhook_url").ok(),
            "token": row.try_get::<Option<String>, _>("token").ok().flatten(),
            "deliveredAt": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at")
                .ok().flatten().map(|t| t.to_rfc3339()),
            "deliveryAttempts": row.try_get::<i32, _>("delivery_attempts").unwrap_or(0),
        })
    }).collect();

    Ok(Json(json!({ "configs": configs, "nextPageToken": "" })))
}

/// `GET /a2a/:slug/tasks/:episode_id/pushNotificationConfigs/:config_id`
pub async fn get_push_config_handler(
    State(state): State<AppState>,
    Path((slug, episode_id_str, config_id_str)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;

    let task_id = Uuid::parse_str(&episode_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid episode_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;
    let config_id = Uuid::parse_str(&config_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid config_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;

    use sqlx::Row;
    let row = sqlx::query(
        "SELECT config_id, webhook_url, auth_scheme, token, created_at, delivered_at, delivery_attempts
         FROM a2a_push_configs
         WHERE config_id = $1 AND task_id = $2 AND caller_user_id = $3",
    )
    .bind(config_id)
    .bind(task_id)
    .bind(&api_key.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(a2a_error_body(500, &e.to_string(), "INTERNAL_ERROR")),
    ))?
    .ok_or_else(|| (
        StatusCode::NOT_FOUND,
        Json(a2a_error_body(404, "Push config not found.", "PUSH_CONFIG_NOT_FOUND")),
    ))?;

    Ok(Json(json!({
        "id": config_id.to_string(),
        "taskId": task_id.to_string(),
        "url": row.try_get::<String, _>("webhook_url").ok(),
        "token": row.try_get::<Option<String>, _>("token").ok().flatten(),
        "deliveredAt": row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("delivered_at")
            .ok().flatten().map(|t| t.to_rfc3339()),
        "deliveryAttempts": row.try_get::<i32, _>("delivery_attempts").unwrap_or(0),
    })))
}

/// `DELETE /a2a/:slug/tasks/:episode_id/pushNotificationConfigs/:config_id`
pub async fn delete_push_config_handler(
    State(state): State<AppState>,
    Path((slug, episode_id_str, config_id_str)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let api_key = extract_api_key(&state, &headers).await?;
    check_scope(&api_key, &slug)?;

    let task_id = Uuid::parse_str(&episode_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid episode_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;
    let config_id = Uuid::parse_str(&config_id_str).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(a2a_error_body(
                400,
                "Invalid config_id.",
                "INVALID_ARGUMENT",
            )),
        )
    })?;

    let deleted = sqlx::query(
        "DELETE FROM a2a_push_configs
         WHERE config_id = $1 AND task_id = $2 AND caller_user_id = $3
         RETURNING config_id",
    )
    .bind(config_id)
    .bind(task_id)
    .bind(&api_key.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(a2a_error_body(500, &e.to_string(), "INTERNAL_ERROR")),
        )
    })?;

    if deleted.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(a2a_error_body(
                404,
                "Push config not found.",
                "PUSH_CONFIG_NOT_FOUND",
            )),
        ));
    }

    Ok(Json(
        json!({ "status": "deleted", "configId": config_id.to_string() }),
    ))
}

// ─── Error helpers ─────────────────────────────────────────────────────────

/// Build an A2A v1.0 error response body (HTTP+JSON binding format).
fn a2a_error_body(status: u16, message: &str, reason: &str) -> Value {
    json!({
        "error": {
            "code": status,
            "message": message,
            "details": [{
                "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                "reason": reason,
                "domain": "agent-bestiary.world"
            }]
        }
    })
}

/// 402 Payment Required with a Stripe top-up URL.
fn credit_error(message: &str, topup_url: Option<&str>) -> (StatusCode, Json<Value>) {
    let mut details = vec![json!({
        "@type": "type.googleapis.com/google.rpc.ErrorInfo",
        "reason": "INSUFFICIENT_CREDITS",
        "domain": "agent-bestiary.world"
    })];
    if let Some(url) = topup_url {
        details[0]["metadata"] = json!({ "topup_url": url });
    }
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "error": {
                "code": 402,
                "message": message,
                "details": details
            }
        })),
    )
}
