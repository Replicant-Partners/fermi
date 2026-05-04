//! Agent execution handler.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fermi::agent_backend::executor::AgentStatus;
use fermi::agent_backend::kg_context::enrich_with_kg_context;
use fermi::gas::{charge_gas, check_low_balance};
use fermi_auth::{credit_charge, get_or_create_wallet, AuthPrincipal};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use crate::{
    agent_output_to_episode, create_notification, resolve_agent, resolve_agent_card, AppState,
};

// ─── Agent execution ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    query: String,
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

    // 1a. Enrich card with relevant KG context from past dream cycles
    let card = enrich_with_kg_context(
        &state.memory_store,
        &state.embedder,
        db_agent.agent_id,
        &body.query,
        card,
    )
    .await;

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

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
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
        user_secrets: None,
    });
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

    // 4. Record stats in registry
    let _ = state.registry.record_execution(&agent_id, &output);

    // 5. Store as ADM episode (with embedding)
    let mut episode = agent_output_to_episode(db_agent.agent_id, &body.query, &output);

    // Generate embedding from query + output summary
    let embed_text = format!(
        "{} {}",
        body.query,
        output.metadata.reasoning.as_deref().unwrap_or("")
    );
    match state.embedder.generate(&embed_text).await {
        Ok(embedding) => episode.embedding = Some(embedding),
        Err(e) => eprintln!("Warning: embedding generation failed: {}", e),
    }

    let episode_id = state
        .memory_store
        .store_episode(episode)
        .await
        .map_err(|e| {
            eprintln!("Warning: failed to store episode: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // 6. Charge credits via GasFees struct
    let tokens = output.tokens_used.unwrap_or(0) as i32;
    let (execution_fee, gas_fee) = state.gas_fees.execution_fee(tokens);

    // Charge execution fee (warning on failure — work is already done)
    let ep_id_str = episode_id.to_string();
    if let Err(e) = credit_charge(
        &state.db,
        wallet.wallet_id,
        execution_fee,
        "execution_fee",
        &format!("Execute {} ({}tk)", agent_id, tokens),
        Some(ep_id_str.as_str()),
    )
    .await
    {
        eprintln!("Warning: failed to charge execution fee: {}", e);
    }

    // Charge gas fee (hard error)
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

    // 7. Fire notifications (execution failure, low balance)
    if matches!(output.status, AgentStatus::Failed | AgentStatus::Timeout) {
        let db = state.db.clone();
        let uid = caller_id.clone();
        let aid = agent_id.clone();
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
        "metadata": {
            "model_used": output.metadata.model_used,
            "reasoning": output.metadata.reasoning,
        }
    })))
}
