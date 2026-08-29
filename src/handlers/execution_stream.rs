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
    /// How the caller decided to ask this question — see
    /// [`crate::stamp_invocation`]. Optional so any existing caller keeps
    /// working; this is the path the Fermi console actually uses, so it is
    /// where the negotiation signal enters the record.
    #[serde(default)]
    pub invocation: Option<serde_json::Value>,
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
    let invocation = body.invocation.clone();

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

    // SPEC_28 — credentials resolved once, read by BOTH branches below.
    // The `prompt_demands_format` branch delegates straight to the shared
    // startup executor; before this, that branch could only ever use the
    // platform's env key, silently mis-billing every structured-output
    // agent (16 of the 17 that take it declare `anthropic`).
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
        // Text-only path: this caller carries no image. Stated rather than
        // defaulted, so a path that should carry one cannot acquire the field
        // silently.
        attachments: Vec::new(),
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

    // Minted before the executor because the episode is only written later,
    // inside the SSE stream, while delegated children need the id from inside
    // the tool loop (mig-198). The bypass branch builds no tool context and so
    // can delegate to nothing; the id is then simply this episode's own id.
    let minted_episode_id = uuid::Uuid::new_v4();

    // Reserved, not merely minted — the row must exist before the id reaches a
    // child, or a run that fails part-way orphans everything it spawned. Same
    // reasoning as the non-streaming path.
    if let Err(e) = state
        .memory_store
        .reserve_episode(minted_episode_id, agent_db_id, &body.query)
        .await
    {
        tracing::warn!(
            agent = %agent_db_id, error = %e,
            "could not reserve the episode; delegated children will point at a \
             row that does not exist",
        );
    }

    let executor: Arc<dyn AgentExecutor> = if prompt_demands_format {
        state.registry.executor_arc()
    } else {
        let tool_context = Arc::new(ToolContext {
            // Root of this execution's delegation tree (mig-198).
            parent_episode_id: Some(minted_episode_id),
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
            credentials: credentials.clone(),
            eval_trigger: Some(Arc::new(crate::handlers::eval::EvalTriggerImpl {
                state: state.clone(),
            })),
            remote_mcp: None,
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
    // The declared input ports, captured before the spawn so the streaming
    // path can verify the interface match the same way execution.rs does.
    let declared_accepts = card.accepts.clone();
    // The declared output type, captured for the same reason `declared_accepts`
    // is: both execute endpoints must check, or the unchecked one becomes the
    // one callers use. That sentence is already in this file about grounding
    // and input binding; the schema was the third check it did not cover.
    let declared_output_contract = card.capabilities.output_contract.clone();
    let agent_tier = db_agent.tier.clone();
    // Captured for the live observability pass, which needs the agent's
    // persona version and card snapshot after the handler frame is gone.
    let db_agent_obs = db_agent.clone();

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
                let mut episode = agent_output_to_episode(
                    agent_db_id,
                    &query,
                    &output,
                );
                // Use the id advertised to the tool context, so children that
                // already stamped it as their parent resolve to this row.
                episode.episode_id = minted_episode_id;
                // Record how the agent was asked, alongside how it did.
                if let Some(ref inv) = invocation {
                    crate::stamp_invocation(&mut episode, inv);
                }
                // And why it was chosen. Unconditional, for the same reason as
                // on the non-streaming path: the block above only runs when the
                // caller sends an invocation block, and both endpoints must
                // stamp or the unstamped one becomes the one callers use.
                if invocation
                    .as_ref()
                    .and_then(|i| i.get("route_reason"))
                    .and_then(|v| v.as_str())
                    .is_none()
                {
                    fermi::route_trust::stamp(
                        &mut episode,
                        fermi::route_trust::RouteSelection::CallerNamed,
                    );
                }
                // Grounding, for the same reason the bind check is here: both
                // execute endpoints must check, or the unchecked one becomes
                // the one callers use. `enforce` is a no-op for agents with no
                // field contract.
                // The document as the agent produced it, and the same
                // document after enforcement. Both are needed and they are
                // different: `enforce` NULLS ungrounded fields, so the claimed
                // values -- the evidence for every later verification -- exist
                // only in the first.
                //
                // Hoisted out of the block below, which used to scope `report`
                // and mutate a local. The schema check further down then
                // re-extracted and re-enforced, with a comment admitting the
                // compromise, so `enforce` ran twice per streamed response and
                // nothing could reach the claimed values. One pass now serves
                // all three readers.
                let claimed_doc = output
                    .raw_response
                    .as_deref()
                    .and_then(fermi::agent_backend::envelope::extract_json);
                let mut enforced_doc = claimed_doc.clone();
                let grounding_report = match enforced_doc.as_mut() {
                    Some(doc) => fermi::grounding_trust::enforce(&agent_id_clone, doc),
                    None => fermi::grounding_trust::Report::default(),
                };
                // Every contracted field, with its grade and the claim behind
                // it, from the report just computed rather than a second pass.
                let graded = match claimed_doc.as_ref() {
                    Some(doc) => fermi::grounding_trust::graded_fields(
                        &agent_id_clone,
                        doc,
                        &grounding_report,
                    ),
                    None => Vec::new(),
                };
                {
                    let report = grounding_report;
                    if !report.is_clean() {
                        tracing::warn!(
                            agent = %agent_id_clone,
                            episode = %minted_episode_id,
                            violations = report.violations.len(),
                            "grounding contract violated on the streaming execute path"
                        );
                    }
                    crate::stamp_grounding(&mut episode, &report);
                    // And a ledger row, so the belt on a streamed artifact
                    // carries what the gate decided rather than only a
                    // recomputation. Absent here while both other execute paths
                    // wrote one, which is the asymmetry that keeps recurring.
                    {
                        let has_contract =
                            fermi::grounding_trust::contracts_for(&agent_id_clone)
                                .next()
                                .is_some();
                        fermi::gate_trust::decided_for_episode(
                            fermi::gate_trust::Gate::Grounding,
                            if !has_contract {
                                fermi::gate_trust::Decision::Undetermined
                            } else if report.is_clean() {
                                fermi::gate_trust::Decision::Approved
                            } else {
                                fermi::gate_trust::Decision::Refused
                            },
                            (!report.is_clean())
                                .then(|| format!("{} violation(s)", report.violations.len()))
                                .as_deref(),
                            minted_episode_id,
                        );
                    }

                    // Tell Loop 2. This path stamped and did not raise, so a
                    // violation on the streaming endpoint was recorded on the
                    // episode and never reached a reviewer.
                    //
                    // `None` for the episode id: on this path the episode row
                    // is written later, and `anomaly_events.episode_id` is a
                    // foreign key. Passing an unwritten id is the race that
                    // made the original raise fail silently on the other
                    // endpoint for the life of the feature.
                    fermi::grounding_anomaly::spawn_raise(
                        std::sync::Arc::clone(&state_clone.memory_store),
                        agent_id_clone.clone(),
                        None,
                        report.clone(),
                    );
                }
                // Does the document match the type the agent declared?
                //
                // Enforce first, then verify what remains — the block above
                // has already run, so the document re-extracted here is
                // checked in the right order. Re-extracted rather than
                // threaded out of that block because it is scoped and the
                // grounding pass there mutates a local; the alternative is
                // widening a scope for a second consumer, which is a larger
                // change to a closure than this earns.
                {
                    // The enforced document from the single pass above.
                    // "Enforce first, then verify what remains" still holds --
                    // it is the same ordering, with one enforcement instead of
                    // two.
                    let doc = enforced_doc.as_ref();
                    let schema = declared_output_contract
                        .as_ref()
                        .and_then(|oc| oc.get("schema"))
                        .filter(|v| v.is_object());
                    let status = match (schema, doc) {
                        (Some(sch), Some(d)) => {
                            let r = fermi::schema_validate::validate(sch, d);
                            if r.is_valid() {
                                "valid"
                            } else if r.is_contradiction() {
                                "invalid"
                            } else {
                                "unverified_unsupported_schema"
                            }
                        }
                        (None, _) => "unverified_no_schema",
                        (Some(_), None) => "unverified_no_payload",
                    };
                    fermi::gate_trust::decided_about(
                        fermi::gate_trust::Gate::OutputSchema,
                        fermi::agent_backend::envelope::decision_for(status),
                        Some(&format!("{agent_id_clone}: {status}")),
                        Some(&agent_id_clone),
                    );
                    if status == "invalid" {
                        tracing::warn!(
                            agent = %agent_id_clone,
                            episode = %minted_episode_id,
                            "output contradicts the type the agent itself \
                             declared, on the streaming execute path"
                        );
                    }
                    if fermi::schema_conformance::score_for(status).is_some() {
                        fermi::schema_conformance::record(
                            &state_clone.db,
                            agent_db_id,
                            minted_episode_id,
                            status,
                            declared_output_contract
                                .as_ref()
                                .and_then(|oc| oc.get("produces_schema"))
                                .and_then(|v| v.as_str()),
                        )
                        .await;
                    }
                }
                // Verify the asking against the card — see execution.rs. Both
                // execute endpoints must check, or the unchecked one becomes
                // the one callers use.
                {
                    let verified = fermi::port_trust::bind_input(&declared_accepts);
                    let claimed = invocation
                        .as_ref()
                        .and_then(|i| i.get("input_binding"))
                        .and_then(|v| v.as_str());
                    if verified.is_mismatch() {
                        tracing::warn!(
                            agent = %agent_name,
                            declared = ?declared_accepts,
                            "free-text query sent to an agent that declares no text input"
                        );
                    }
                    crate::stamp_input_binding(&mut episode, &verified, claimed);
                }
                // Stamp the (agent, human) dyad — see execution.rs.
                let dyad_id = agent_bestiary_memory::dyad_id(agent_db_id, &caller_clone);
                episode.dyad_id = Some(dyad_id.clone());
                // Persona version — see execution.rs. Without it the
                // observability worker skips the entry and drift never fires.
                episode.persona_version_at_write = Some(db_agent_obs.persona_version);
                crate::spawn_dyad_observation(
                    &state_clone,
                    agent_db_id,
                    dyad_id,
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

                let episode_for_observation = episode.clone();

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

                // ── Loop 2's routine half, mirroring `execution.rs` ──────
                //
                // `spawn_raise` above passes `None` for the episode id because
                // on this path the row is written HERE, below it. The enqueue
                // cannot do that: `assertion_verifications.episode_id` is a
                // real foreign key, so it has to wait for the write and take
                // the id the write returned. That is why this block is here and
                // not beside the grounding pass.
                //
                // `episode_id` is an `Option` because storage on this path
                // logs and continues. No episode means no enqueue — and that
                // is a loss worth seeing rather than papering over, so it is
                // counted as a failed write rather than skipped silently.
                if !graded.is_empty() {
                    let db = state_clone.db.clone();
                    let agent = agent_id_clone.clone();
                    let graded_for_queue = graded.clone();
                    match episode_id {
                        Some(eid) => {
                            tokio::spawn(async move {
                                let e = fermi::verification_queue::enqueue(
                                    &db,
                                    eid,
                                    &agent,
                                    &graded_for_queue,
                                )
                                .await;
                                if e.queued > 0 {
                                    tracing::info!(
                                        agent = %agent,
                                        episode = %eid,
                                        queued = e.queued,
                                        to_tool = e.to_tool,
                                        to_human = e.to_human,
                                        already_settled = e.already_settled,
                                        "contracted fields queued for verification (stream)"
                                    );
                                }
                                if e.is_problem() {
                                    tracing::warn!(
                                        agent = %agent,
                                        episode = %eid,
                                        failed = e.failed,
                                        not_representable = ?e.not_representable,
                                        "some contracted claims could not be queued \
                                         for verification; each is a claim nobody \
                                         will ever check"
                                    );
                                }
                            });
                        }
                        None => {
                            tracing::warn!(
                                agent = %agent_id_clone,
                                fields = graded.len(),
                                "the episode write failed, so {} contracted \
                                 claim(s) could not be queued for verification",
                                graded.len()
                            );
                        }
                    }
                }

                // Retain the agent's quantified judgement as a claim.
                //
                // The non-streaming `execute_agent_handler` has done this
                // since mig-187; this handler never has. That gap was the
                // whole of the remaining loss after migration 213, because
                // the Fermi Console runs almost everything through THIS
                // route: it prefers the stream for progress events, and
                // only falls back to `/execute` when a stream drops. So
                // "claims are enabled for forecast-bound runs" would have
                // been true and still produced nothing.
                //
                // Deliberately mirrors `execution.rs` rather than sharing a
                // helper: the two handlers already duplicate their episode,
                // credit and royalty logic, and the thing worth preventing
                // is not the duplication but the two paths silently
                // DIVERGING. Keep them edited in pairs.
                // The envelope read is shared with `execution.rs` through
                // `fermi::claim_outcome`, and it is the one piece of this
                // mirroring that should NOT be duplicated: two independent
                // reads of two JSON keys can diverge, the divergence is
                // invisible, and there is nothing for a shared function to
                // paper over. The surrounding episode/credit/royalty logic
                // stays mirrored, for the reason given above.
                let claim_binding =
                    fermi::claim_outcome::binding_from_invocation(invocation.as_ref());
                if claim_binding.forecast_id.is_some() && !output.evidence.is_empty() {
                    let pool = state_clone.db.clone();
                    let registry = state_clone.extractor_registry.clone();
                    let claim_agent = agent_id_clone.clone();
                    let evidence = output.evidence.clone();
                    // This route has no workspace — `workspace_id` is hardcoded
                    // `None` in its tool context above, which is also what
                    // `binding_from_invocation` leaves it as.
                    let binding = claim_binding;
                    let log_forecast = binding.forecast_id.clone().unwrap_or_default();
                    tokio::spawn(async move {
                        match crate::handlers::workspace::agent_params_hook::apply_agent_multipliers(
                            &pool,
                            &registry,
                            &binding,
                            &claim_agent,
                            &evidence,
                            Some(minted_episode_id),
                        )
                        .await
                        {
                            // The outcome was previously dropped entirely on
                            // this path — `if let Err` reads only the failure,
                            // so a run that declined to write a claim looked
                            // exactly like one that wrote three. This route is
                            // forecast-bound by construction (`workspace_id`
                            // is hardcoded `None` above), so `no_driver_for_agent`
                            // is unreachable here and `unbound` means the
                            // console sent no driver.
                            Ok(o) => tracing::info!(
                                forecast = %log_forecast,
                                agent = %claim_agent,
                                outcome = o.label(),
                                "claim retention outcome"
                            ),
                            // Best-effort: a lost claim must never fail a run
                            // the caller has already paid for. Warned, not
                            // swallowed, because a claim cannot be
                            // reconstructed after the fact.
                            Err(e) => tracing::warn!(
                                forecast = %log_forecast,
                                error = %e,
                                "claim retention failed on the streaming path"
                            ),
                        }
                    });
                }

                // Make this turn visible to drift + anomaly detection.
                if episode_id.is_some() {
                    crate::handlers::live_observability::spawn_live_observation(
                        &state_clone,
                        crate::handlers::live_observability::LiveObservation {
                            episode: episode_for_observation,
                            agent: db_agent_obs.clone(),
                            response: output
                                .metadata
                                .reasoning
                                .clone()
                                .unwrap_or_default(),
                            session_id: Some("live:execute_stream".to_string()),
                            rupture_detected: false,
                        },
                    );
                }

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
                        "provider": output.metadata.provider,
                        "reasoning": output.metadata.reasoning,
                        "failure_reason": output.metadata.failure_reason,
                        "stop_reason": output.metadata.stop_reason,
                    },
                    // Mirrored at the top level so a streaming client can
                    // branch on the failure without reaching into metadata.
                    "failure_reason": output.metadata.failure_reason,
                    "stop_reason": output.metadata.stop_reason,
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
                    // Carry the reason — see execution.rs for why the old
                    // "check the execution history" text was a dead end.
                    let body = match output.metadata.failure_reason.as_deref() {
                        Some(reason) => reason.to_string(),
                        None => format!(
                            "No reason reported by the executor. stop_reason={}",
                            output.metadata.stop_reason.as_deref().unwrap_or("unknown")
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
