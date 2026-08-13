//! Agent execution handler.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi::agent_backend::executor::AgentStatus;
use fermi::agent_backend::kg_context::enrich_with_kg_context;
use fermi::gas::{charge_execution_with_royalty, charge_gas, check_low_balance};
use fermi_auth::{get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::{
    agent_output_to_episode, create_notification, resolve_agent, resolve_agent_card,
    resolve_agent_owner_secrets, AppState,
};

// ─── Agent execution ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    query: String,
    /// How the caller decided to ask this question — see
    /// [`crate::stamp_invocation`]. Optional and free-form so a caller that
    /// knows nothing about negotiation (curl, an older console, another
    /// orchestra) keeps working unchanged.
    #[serde(default)]
    invocation: Option<serde_json::Value>,
}

pub async fn execute_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<ExecuteRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let caller_id = principal.user_id();

    // Rate limit LLM calls
    if let Err(retry) = state.rate_limits.llm.check(&format!("user:{}", caller_id)) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("LLM rate limit exceeded. Retry after {} seconds.", retry),
        ));
    }

    // 0. Check caller has credits
    let wallet = get_or_create_wallet(&state.db, "user", &caller_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Wallet error: {}", e),
            )
        })?;
    if wallet.balance <= 0 {
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "Insufficient credits".to_string(),
        ));
    }

    // 1. Resolve agent in database, then build card (registry or DB fallback)
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let card = resolve_agent_card(&state, &db_agent);

    // 1a. Enrich card with relevant KG context from past dream cycles.
    // Phase 1: returns the query embedding alongside the card so we can
    // reuse it for episode storage without a second embedding API call.
    let t_kg = tokio::time::Instant::now();
    let (card, _kg_query_embedding) = enrich_with_kg_context(
        &state.memory_store,
        &state.embedder,
        db_agent.agent_id,
        &body.query,
        card,
    )
    .await;
    tracing::info!(elapsed_ms = t_kg.elapsed().as_millis() as u64, agent = %agent_id, "kg_context_enrich");

    // v0.11.3: dynamic-roster injection. When `card` is an orchestra
    // strategist (currently `fermi`), append a `## CURRENT ROSTER`
    // block listing the live approved members from the corresponding
    // orchestra_*_members view. Non-strategist agents pass through
    // unchanged. See handlers/orchestras.rs for the full logic.
    let card = crate::handlers::orchestras::inject_orchestra_context(&state.db, card).await;

    // 2. Build execution context
    let agent_stmt = ast::AgentStmt {
        name: agent_id.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: body.query.clone(),
        executor: Some(ast::ExecutorType::LLM),
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let program = ast::Program {
        statements: vec![ast::Statement::Agent(agent_stmt.clone())],
    };

    // SPEC_28 — resolve this execution's provider credentials from the
    // agent's owning principal, once, before building the context. Every
    // executor branch reads them from here, so funding no longer depends
    // on whether the tool loop runs.
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
    };

    // Resolve the agent's remote MCP tools before building the context.
    //
    // This is the client direction: any card may declare `mcp_servers`,
    // whose tools are discovered via `tools/list`, namespaced
    // `server__tool`, and dispatched via `tools/call`. Scoped per agent
    // on purpose — builtins are global, remote servers (and their
    // credentials) must not be.
    //
    // Secrets come from the same owner-scoped set the executor uses for
    // model keys, so a third-party MCP credential is funded by whoever
    // owns the agent, not the caller.
    let owner_secrets = resolve_agent_owner_secrets(&state, &db_agent).await;
    let remote_mcp = if card.capabilities.mcp_servers.is_empty() {
        None
    } else {
        let cat = fermi::agent_backend::mcp_client::RemoteMcpCatalogue::discover(
            &card.capabilities.mcp_servers,
            owner_secrets.as_ref(),
        )
        .await;
        for (server, err) in &cat.failures {
            eprintln!(
                "[mcp_client] agent {} server '{}' unavailable: {}",
                db_agent.agent_id, server, err
            );
        }
        Some(Arc::new(cat))
    };

    // 3. Execute via ToolAwareExecutor
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
        user_id: Some(principal.user_id()),
        // v0.9.0 — Agent-owner API key routing.
        // Populate with the AGENT OWNER's secrets (not the caller's).
        // System agents get None here → executor falls back to env var
        // (platform funds). See resolve_agent_owner_secrets for the
        // full rationale.
        user_secrets: owner_secrets,
        // Carried for delegation propagation only; executors read
        // credentials from ExecutionContext.
        credentials,
        eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
            state: state.clone(),
        })),
        remote_mcp,
    });
    // Clone the Arc before moving into ToolAwareExecutor::new so the
    // post-hook below (which needs workspace_id from the same context)
    // can still read it. Arc<T> clone is cheap — just bumps a refcount.
    let tool_context_for_hook = tool_context.clone();
    let tool_executor = ToolAwareExecutor::new(
        state.registry.executor_arc(),
        ToolRegistry::standard(),
        tool_context,
    );
    let output = tool_executor
        .execute(&agent_stmt, &context)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Execution failed: {}", e),
            )
        })?;

    // 3.5 Post-agent hook: apply multiplier recommendations to workspace params.
    // If the ToolContext has a workspace_id AND the agent produced evidence with
    // [MULTIPLIER] blocks, write them to the workspace's params and trigger a refit.
    let ws_id_opt = tool_context_for_hook.workspace_id; // Copy (Option<Uuid>)m to the workspace's params and trigger a refit.
    if let Some(ws_id) = ws_id_opt {
        if !output.evidence.is_empty() {
            let pool = state.db.clone();
            let registry = state.extractor_registry.clone();
            let agent_name = agent_id.clone();
            let evidence = output.evidence.clone();
            tokio::spawn(async move {
                match crate::handlers::workspace::agent_params_hook::apply_agent_multipliers(
                    &pool,
                    &registry,
                    ws_id,
                    &agent_name,
                    &evidence,
                )
                .await
                {
                    Ok(true) => tracing::info!(
                        workspace = %ws_id,
                        agent = %agent_name,
                        "agent multipliers applied to workspace params"
                    ),
                    Ok(false) => {} // no multiplier found, nothing to do
                    Err(e) => tracing::warn!(
                        workspace = %ws_id,
                        agent = %agent_name,
                        error = %e,
                        "failed to apply agent multipliers"
                    ),
                }
            });
        }
    }

    // 4. Record stats in registry
    let _ = state.registry.record_execution(&agent_id, &output);

    // 5. Store as ADM episode (with embedding + Spec 22 provenance)
    let mut episode = agent_output_to_episode(db_agent.agent_id, &body.query, &output);
    // Record how the agent was asked, alongside how it did.
    if let Some(ref inv) = body.invocation {
        crate::stamp_invocation(&mut episode, inv);
    }
    // Stamp the (agent, human) dyad so the social tracker can accumulate
    // rapport/trust/reciprocity for this pair. Without this the episode is
    // invisible to the companion loop.
    let dyad_id = agent_bestiary_memory::dyad_id(db_agent.agent_id, &caller_id);
    episode.dyad_id = Some(dyad_id.clone());
    // Fold this exchange into the running relationship state.
    crate::spawn_dyad_observation(&state, db_agent.agent_id, dyad_id, &body.query, &output);

    // Build the embedded text ONCE and pass the same string to both the embedder
    // and the storage layer — `generate_provenanced` returns the source_text
    // bundled with the vector, guaranteeing they cannot drift.
    let embed_text = format!(
        "{} {}",
        body.query,
        output.metadata.reasoning.as_deref().unwrap_or("")
    );
    let t_embed = tokio::time::Instant::now();
    let provenance = match state.embedder.generate_provenanced(&embed_text).await {
        Ok(p) => {
            tracing::info!(
                elapsed_ms = t_embed.elapsed().as_millis() as u64,
                model = %p.model_id,
                site = "execution_handler",
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
        "kind": "execute_handler",
        "agent_id": db_agent.agent_id,
        "query_len": body.query.len(),
    });

    let episode_id = state
        .memory_store
        .store_episode_with_provenance(episode, provenance.as_ref(), Some(source_ref))
        .await
        .map_err(|e| {
            eprintln!("Warning: failed to store episode: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // 6. Charge credits via GasFees struct
    let tokens = output.tokens_used.unwrap_or(0) as i32;
    let (execution_fee, gas_fee) = state.gas_fees.execution_fee(tokens);

    // v0.10.1 credit-flow: caller pays `execution_fee`, agent owner
    // receives `execution_fee * execution_owner_royalty_pct` when the
    // agent is community/curated with an owner distinct from the
    // caller. System-tier agents (Fermi included) route 100% to the
    // platform since they are platform-funded. See
    // `charge_execution_with_royalty` for the gates.
    //
    // The old code path here was a warning-on-failure
    // `credit_charge(…, "execution_fee", …)` — same total, same tx_type
    // for the caller side, but no royalty leg. The wallet-error path
    // stays soft-fail: the work has already been done and we don't
    // want to double-bill on retry. When the royalty deposit fails
    // the platform absorbs (logged, not raised).
    let ep_id_str = episode_id.to_string();
    let (_charged, royalty_paid) = match charge_execution_with_royalty(
        &state.db,
        wallet.wallet_id,
        &caller_id,
        execution_fee,
        db_agent.owner_id.as_deref(),
        &db_agent.tier,
        &agent_id,
        tokens,
        Some(ep_id_str.as_str()),
        state.gas_fees.execution_owner_royalty_pct,
    )
    .await
    {
        Ok(pair) => pair,
        Err((status, msg)) => {
            eprintln!(
                "Warning: failed to charge execution fee: {} ({})",
                msg, status
            );
            (0, 0)
        }
    };

    // Charge gas fee (hard error). Gas stays with the platform — it's
    // the infrastructure surcharge, not the agent's fee.
    charge_gas(
        &state.db,
        wallet.wallet_id,
        gas_fee,
        "gas_fee",
        &format!("Gas fee for {}", agent_id),
        Some(ep_id_str.as_str()),
    )
    .await?;

    let total_charged = execution_fee + gas_fee;
    // Debug trace so operators can grep `[credit-flow]` in a run log
    // and see the full charge breakdown for any given execution.
    if royalty_paid > 0 {
        tracing::info!(
            agent = %agent_id,
            caller = %caller_id,
            execution_fee = execution_fee,
            gas_fee = gas_fee,
            royalty_paid = royalty_paid,
            "[credit-flow] hire settled"
        );
    }

    // 7. Fire notifications (execution failure, low balance)
    if matches!(output.status, AgentStatus::Failed | AgentStatus::Timeout) {
        let db = state.db.clone();
        let uid = caller_id.clone();
        let aid = agent_id.clone();
        // Put the reason in the notification itself. It used to say "check
        // the agent's execution history for details" — which was a dead
        // end, because the history stored the constant "Execution failed".
        let body = match output.metadata.failure_reason.as_deref() {
            Some(reason) => format!("{} (episode {})", reason, episode_id),
            None => format!(
                "No reason reported by the executor. stop_reason={}, episode {}",
                output.metadata.stop_reason.as_deref().unwrap_or("unknown"),
                episode_id
            ),
        };
        tokio::spawn(async move {
            create_notification(
                &db,
                &uid,
                "execution_failure",
                &format!("Execution failed: {}", aid),
                Some(&body),
            )
            .await;
        });
    }
    // Low balance notification
    if check_low_balance(&state.db, wallet.wallet_id).await {
        let db = state.db.clone();
        let uid = caller_id.clone();
        tokio::spawn(async move {
            create_notification(
                &db,
                &uid,
                "low_balance",
                "Low credit balance",
                Some("Your balance is below 10 credits. Buy more to keep your agents running."),
            )
            .await;
        });
    }

    // 8. Return result
    Ok(Json(json!({
        "agent_id": agent_id,
        "episode_id": episode_id,
        "status": format!("{:?}", output.status),
        "confidence": output.confidence,
        "execution_time_ms": output.execution_time_ms,
        "tokens_used": output.tokens_used,
        "credits_charged": total_charged,
        "loop_iterations": output.loop_iterations,
        "tool_invocations": output.tool_invocations.iter().map(|t| json!({
            "tool_name": t.tool_name,
            "duration_ms": t.duration_ms,
            "iteration": t.iteration,
        })).collect::<Vec<_>>(),
        "evidence": output.evidence.iter().map(|e| json!({
            "id": e.id,
            "source": e.source,
            "summary": e.summary,
            "key_findings": e.key_findings,
            "relevance": e.relevance,
        })).collect::<Vec<_>>(),
        // Failure provenance travels with the response. Without these two
        // fields a caller (the Fermi console's driver-research pass, for
        // one) sees `"status": "Failed"` and has nothing to log, retry on,
        // or show the person who paid for the run.
        "failure_reason": output.metadata.failure_reason,
        "stop_reason": output.metadata.stop_reason,
        "metadata": {
            "model_used": output.metadata.model_used,
            "provider": output.metadata.provider,
            "reasoning": output.metadata.reasoning,
            "failure_reason": output.metadata.failure_reason,
            "stop_reason": output.metadata.stop_reason,
        }
    })))
}
