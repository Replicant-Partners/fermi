//! Agent execution SSE stream — real-time push of execution progress.
//!
//! `POST /api/agents/:agent_id/execute/stream` opens a Server-Sent Events
//! connection that delivers progress events as the agent executes:
//!
//!   - `started`          — execution has begun, includes agent metadata
//!   - `progress`         — phase update (executing, tool_call, etc.)
//!   - `evidence`         — a key finding has been extracted
//!   - `complete`         — full execution result (same shape as non-streaming endpoint)
//!   - `error`            — execution failed
//!
//! Auth: same as `/api/agents/:id/execute` — requires authenticated user with credits.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use futures_core::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use fermi::agent_backend::executor::{AgentExecutor, AgentStatus};
use fermi::agent_backend::kg_context::enrich_with_kg_context;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;
use fermi::gas::{charge_execution_with_royalty, charge_gas, check_low_balance};
use fermi_auth::{get_or_create_wallet, AuthPrincipal};

use crate::{
    agent_output_to_episode, create_notification, resolve_agent, resolve_agent_card,
    resolve_agent_owner_secrets, AppState,
};

#[derive(Debug, Deserialize)]
pub struct StreamExecuteRequest {
    pub query: String,
}

/// `POST /api/agents/:agent_id/execute/stream`
///
/// Opens an SSE connection that streams execution progress events.
/// The final `complete` event contains the same payload as the
/// non-streaming `/execute` endpoint.
pub async fn execute_agent_stream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<StreamExecuteRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    let caller_id = principal.user_id();

    // ── Rate limit ─────────────────────────────────────────────────
    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{}", caller_id)) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("Rate limited. Retry after {}s.", retry),
        ));
    }

    // ── Credit check ───────────────────────────────────────────────
    let wallet = get_or_create_wallet(&state.db, "user", &caller_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Wallet: {}", e)))?;
    if wallet.balance <= 0 {
        return Err((StatusCode::PAYMENT_REQUIRED, "Insufficient credits".into()));
    }

    // ── Resolve agent ──────────────────────────────────────────────
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let card = resolve_agent_card(&state, &db_agent);
    let agent_db_id = db_agent.agent_id;
    let agent_name = db_agent.agent_name.clone();
    let query = body.query.clone();

    // ── Enrich card with KG context from past dream cycles ─────────
    let t_kg = tokio::time::Instant::now();
    let (card, _kg_query_embedding) = enrich_with_kg_context(
        &state.memory_store,
        &state.embedder,
        agent_db_id,
        &query,
        card,
    )
    .await;
    tracing::info!(
        elapsed_ms = t_kg.elapsed().as_millis() as u64,
        "kg_context_enrich"
    );

    let model = card.capabilities.model.clone();

    // ── Build execution context ────────────────────────────────────
    // v0.11.3: dynamic-roster injection into strategist system prompts.
    // Non-strategist agents pass through unchanged. See
    // handlers/orchestras.rs::inject_orchestra_context.
    let card = crate::handlers::orchestras::inject_orchestra_context(&state.db, card).await;

    let agent_stmt = ast::AgentStmt {
        name: agent_id.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: query.clone(),
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
        creature_id: None,
        cognition_tier: None,
    };

    // ── Build executor ─────────────────────────────────────────────
    // JSON-format agents (fermi) bypass tool loop via inner executor directly.
    // Meta-agents (tagged "meta-agent") return structured JSON decompositions —
    // the tool loop would inject search tools that make the LLM return narrative
    // instead of the JSON schema. Bypass on tag OR legacy string heuristic.
    let is_meta_agent = card.metadata.tags.iter().any(|t| t == "meta-agent");
    let prompt_demands_format = is_meta_agent
        || card
            .system_prompt
            .as_ref()
            .map(|p| p.contains("ONLY") || p.contains("raw JSON"))
            .unwrap_or(false);

    let executor: Arc<dyn AgentExecutor> = if prompt_demands_format {
        state.registry.executor_arc()
    } else {
        let tool_context = Arc::new(ToolContext {
            memory_store: state.memory_store.clone(),
            embedder: state.embedder.clone(),
            registry: state.registry.clone(),
            current_agent_id: Some(agent_db_id),
            workspace_id: None,
            workspace_slug: None,
            workspace_git: None,
            db: Some(state.db.clone()),
            gas_fees: Some(state.gas_fees.clone()),
            user_id: Some(caller_id.clone()),
            // v0.9.0 — Agent-owner API key routing.
            // Same resolution as the non-streaming path in execution.rs:
            // agent-owned key for third-party agents, env fallback
            // (platform key) for tier=System.
            user_secrets: resolve_agent_owner_secrets(&state, &db_agent).await,
            eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
                state: state.clone(),
            })),
        });
        Arc::new(ToolAwareExecutor::new(
            state.registry.executor_arc(),
            ToolRegistry::standard(),
            tool_context,
        ))
    };

    // ── Build SSE stream ──────────────────────────────────
    let state_clone = state.clone();
    let caller_clone = caller_id.clone();
    let wallet_id = wallet.wallet_id;
    let gas_fees = state.gas_fees.clone();
    let agent_id_clone = agent_id.clone();
    // v0.10.1 credit-flow: capture the owner + tier at handler entry
    // so the async stream closure can route the royalty on completion.
    let agent_owner_id = db_agent.owner_id.clone();
    let agent_tier = db_agent.tier.clone();

    let stream = async_stream::stream! {
        let start = Instant::now();

        // ── Event: started ─────────────────────────────────────────
        yield Ok(Event::default().event("started").data(
            json!({
                "agent_id": agent_id_clone,
                "agent_name": agent_name,
                "model": model,
                "query": query,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
            .to_string(),
        ));

        // ── Event: progress ────────────────────────────────────────
        yield Ok(Event::default().event("progress").data(
            json!({
                "phase": "executing",
                "message": format!("Running {}…", agent_name),
                "elapsed_ms": start.elapsed().as_millis() as u64,
            })
            .to_string(),
        ));

        // ── Execute the agent ──────────────────────────────────────
        // Phase 1: single blocking call. Phase 2 will replace with
        // Anthropic streaming for token-by-token reasoning_delta events.
        let result = executor.execute(&agent_stmt, &context).await;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                // Record stats
                let _ = state_clone.registry.record_execution(&agent_id_clone, &output);

                // Store as ADM episode (with embedding + Spec 22 provenance)
                let episode = agent_output_to_episode(
                    agent_db_id,
                    &query,
                    &output,
                );

                // Build the embedded text ONCE; provenance binds it to the vector.
                let embed_text = format!(
                    "{} {}",
                    query,
                    output.metadata.reasoning.as_deref().unwrap_or("")
                );
                let t_embed = tokio::time::Instant::now();
                let provenance = match state_clone.embedder.generate_provenanced(&embed_text).await
                {
                    Ok(p) => {
                        tracing::info!(
                            elapsed_ms = t_embed.elapsed().as_millis() as u64,
                            model = %p.model_id,
                            site = "execution_stream_handler",
                            "embed_call"
                        );
                        Some(p)
                    }
                    Err(e) => {
                        eprintln!("Warning: embedding generation failed: {}", e);
                        None
                    }
                };

                let source_ref = serde_json::json!({
                    "kind": "execute_stream_handler",
                    "agent_id": agent_db_id,
                    "query_len": query.len(),
                });

                // Store episode
                let episode_id = match state_clone
                    .memory_store
                    .store_episode_with_provenance(episode, provenance.as_ref(), Some(source_ref))
                    .await
                {
                    Ok(id) => Some(id),
                    Err(e) => {
                        eprintln!("Warning: episode storage failed: {}", e);
                        None
                    }
                };

                // Charge credits
                let tokens = output.tokens_used.unwrap_or(0) as i32;
                let (execution_fee, gas_fee_amount) = gas_fees.execution_fee(tokens);

                let ep_id_str = episode_id.map(|id| id.to_string()).unwrap_or_default();
                let ep_ref = if ep_id_str.is_empty() { None } else { Some(ep_id_str.as_str()) };

                // v0.10.1 credit-flow: caller pays `execution_fee`,
                // owner receives `execution_fee * royalty_pct` when the
                // agent is community/curated with an owner distinct
                // from the caller. Same gates as the non-streaming
                // execute_agent_handler in handlers/execution.rs.
                let royalty_paid = match charge_execution_with_royalty(
                    &state_clone.db,
                    wallet_id,
                    &caller_clone,
                    execution_fee,
                    agent_owner_id.as_deref(),
                    &agent_tier,
                    &agent_id_clone,
                    tokens,
                    ep_ref,
                    gas_fees.execution_owner_royalty_pct,
                )
                .await
                {
                    Ok((_charged, royalty)) => royalty,
                    Err((status, msg)) => {
                        eprintln!(
                            "Warning: failed to charge execution fee: {} ({})",
                            msg, status
                        );
                        0
                    }
                };

                let _ = charge_gas(
                    &state_clone.db,
                    wallet_id,
                    gas_fee_amount,
                    "gas_fee",
                    &format!("Gas fee for {}", agent_id_clone),
                    ep_ref,
                )
                .await;

                let total_charged = execution_fee + gas_fee_amount;
                if royalty_paid > 0 {
                    tracing::info!(
                        agent = %agent_id_clone,
                        caller = %caller_clone,
                        execution_fee = execution_fee,
                        gas_fee = gas_fee_amount,
                        royalty_paid = royalty_paid,
                        "[credit-flow] hire settled (stream)"
                    );
                }

                // ── Event: evidence (for each finding) ─────────────
                for ev in &output.evidence {
                    for finding in &ev.key_findings {
                        yield Ok(Event::default().event("evidence").data(
                            json!({
                                "finding": finding,
                                "source": ev.source,
                                "relevance": ev.relevance.unwrap_or(0.5),
                                "elapsed_ms": start.elapsed().as_millis() as u64,
                            })
                            .to_string(),
                        ));
                    }
                }

                // ── Event: complete ────────────────────────────────
                let response = json!({
                    "agent_id": output.agent_name,
                    "confidence": output.confidence,
                    "credits_charged": total_charged,
                    "episode_id": episode_id,
                    "evidence": output.evidence.iter().map(|e| json!({
                        "id": e.id,
                        "source": e.source,
                        "summary": e.summary.clone().unwrap_or_default(),
                        "key_findings": e.key_findings,
                        "relevance": e.relevance.unwrap_or(0.0),
                    })).collect::<Vec<_>>(),
                    "execution_time_ms": elapsed_ms,
                    "loop_iterations": output.loop_iterations,
                    "metadata": {
                        "model_used": output.metadata.model_used,
                        "reasoning": output.metadata.reasoning,
                    },
                    "status": format!("{:?}", output.status),
                    "tokens_used": output.tokens_used,
                    "tool_invocations": output.tool_invocations.iter().map(|t| json!({
                        "tool_name": t.tool_name,
                        "duration_ms": t.duration_ms,
                    })).collect::<Vec<_>>(),
                });

                yield Ok(Event::default().event("complete").data(response.to_string()));

                // Fire notifications for failures
                if matches!(output.status, AgentStatus::Failed | AgentStatus::Timeout) {
                    let db = state_clone.db.clone();
                    let uid = caller_clone.clone();
                    let aid = agent_id_clone.clone();
                    tokio::spawn(async move {
                        create_notification(
                            &db,
                            &uid,
                            "execution_failure",
                            &format!("Execution failed: {}", aid),
                            Some("Check the agent's execution history for details."),
                        )
                        .await;
                    });
                }

                // Low balance notification
                if check_low_balance(&state_clone.db, wallet_id).await {
                    let db = state_clone.db.clone();
                    let uid = caller_clone.clone();
                    tokio::spawn(async move {
                        create_notification(
                            &db,
                            &uid,
                            "low_balance",
                            "Your credit balance is low",
                            Some("Purchase more credits to continue using agents."),
                        )
                        .await;
                    });
                }
            }
            Err(e) => {
                // ── Event: error ───────────────────────────────────
                yield Ok(Event::default().event("error").data(
                    json!({
                        "agent_id": agent_id_clone,
                        "error": format!("{}", e),
                        "elapsed_ms": elapsed_ms,
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
