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
    /// Images to send with the question.
    ///
    /// `#[serde(default)]`, so every existing caller — curl, the console, another
    /// orchestra — is unaffected and sends nothing.
    ///
    /// An attachment that cannot be delivered is a **400**, never a quieter
    /// answer. See `src/attachments.rs`: a request whose frame goes missing still
    /// returns a confident answer generated from the text alone, and nothing
    /// downstream can tell that apart from an answer that had the picture.
    #[serde(default)]
    attachments: Vec<AttachmentRequest>,
}

/// One image, as a caller sends it.
///
/// Kept separate from [`fermi::attachments::ImageAttachment`] so the wire shape
/// can accept what callers actually send — a `data:` URL prefix, odd casing — and
/// normalise on the way in, rather than making every client get it exactly right.
#[derive(Debug, Deserialize)]
pub struct AttachmentRequest {
    /// e.g. `image/jpeg`. Checked against a closed allowlist.
    media_type: String,
    /// Base64 payload. A `data:image/...;base64,` prefix is stripped.
    data: String,
    /// Optional provenance note, e.g. `glasses temple button`. Not interpreted.
    #[serde(default)]
    source: Option<String>,
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
        // The limiter is an in-memory map with no export, so its refusals have
        // never been visible — including the case that matters most, which is a
        // per-process limiter quietly doing a fraction of its job behind more
        // than one replica.
        fermi::gate_trust::decided(
            fermi::gate_trust::Gate::RateLimit,
            fermi::gate_trust::Decision::Refused,
            Some("llm limiter"),
        );
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("LLM rate limit exceeded. Retry after {} seconds.", retry),
        ));
    }
    fermi::gate_trust::decided(
        fermi::gate_trust::Gate::RateLimit,
        fermi::gate_trust::Decision::Approved,
        None,
    );

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
        // Counted. Every credit refusal in the system returned before the
        // `credit_ledger` INSERT, so the platform recorded what it spent and
        // nothing about what it declined — spend was observable and demand was
        // not.
        fermi::gate_trust::decided(
            fermi::gate_trust::Gate::Credit,
            fermi::gate_trust::Decision::Refused,
            Some("execute: wallet balance <= 0"),
        );
        return Err((
            StatusCode::PAYMENT_REQUIRED,
            "Insufficient credits".to_string(),
        ));
    }
    fermi::gate_trust::decided(
        fermi::gate_trust::Gate::Credit,
        fermi::gate_trust::Decision::Approved,
        None,
    );

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

    // Normalise the attachments, then refuse the request if any of them cannot
    // be delivered to the model this execution resolved to.
    //
    // A 400 here is the point of the whole mechanism. The alternative is
    // answering "what is this?" from the words alone, which the model will do
    // fluently, and which arrives labelled `model_inference` by a boundary that
    // cannot distinguish an inference from a photograph from an inference from
    // nothing.
    let attachments: Vec<fermi::attachments::ImageAttachment> = body
        .attachments
        .iter()
        .map(|a| {
            fermi::attachments::ImageAttachment::new(
                a.media_type.clone(),
                &a.data,
                a.source.clone(),
            )
        })
        .collect();

    {
        let deliverable = fermi::attachments::ensure_deliverable(
            &attachments,
            &card.capabilities.provider,
            &card.capabilities.model,
        );
        // Only counted when there was something to check: a request with no
        // attachment is not a decision this gate made, and counting it as an
        // approval would bury the refusal rate under the traffic of every
        // text-only call.
        if !attachments.is_empty() {
            fermi::gate_trust::decided_ok(fermi::gate_trust::Gate::Attachment, &deliverable);
        }
        if let Err(e) = deliverable {
            return Err((StatusCode::BAD_REQUEST, e.to_string()));
        }
    }

    let context = ExecutionContext {
        program,
        agent_card: card.clone(),
        creature_id: None,
        cognition_tier: None,
        credentials: credentials.clone(),
        attachments,
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
    // Minted here, ahead of BOTH the tool context and the execution, because
    // two later writers need to point at this episode before it exists:
    //   * the claim hook, spawned after execution (mig-197)
    //   * any delegated child episode, written from inside the tool loop
    //     (mig-198) — which is why it goes on the ToolContext below
    // An id generated at store time could serve neither.
    let episode_id = uuid::Uuid::new_v4();

    let tool_context = Arc::new(ToolContext {
        // Root of this execution's delegation tree (mig-198). Children stamp
        // it as their parent_episode_id so a compound run's true cost is
        // recoverable as the sum over the tree.
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

    // 3.5 Post-agent hook: retain the agent's quantified judgement, and — when
    // there is a workspace — apply it to that workspace's params and refit.
    let ws_id_opt = tool_context_for_hook.workspace_id; // Copy (Option<Uuid>)

    // The other binding a caller may have: the exact (forecast, driver) this
    // run was commissioned for.
    //
    // Read out of `invocation`, which the caller already sends and which
    // `stamp_invocation` already stores on the episode below — the same
    // statement of intent, read for a second purpose rather than a new field
    // on the wire. Tolerant throughout: a missing, non-object or wrongly-typed
    // field means "no forecast binding", never a failure, because a caller
    // that says nothing about a forecast is the normal case.
    // Read through `fermi::claim_outcome`, which declares the two keys. They
    // were string literals here and in `execution_stream.rs`, against serde
    // field names in `fermi-console` — four spellings, two crates, nothing
    // comparing them, and a rename on either side yields zero claims silently.
    let wire = fermi::claim_outcome::binding_from_invocation(body.invocation.as_ref());
    let (forecast_id_opt, driver_opt) = (wire.forecast_id, wire.driver);

    // The gate was `if let Some(ws_id) = ws_id_opt`, and that discarded every
    // judgement the Fermi Console ever produced: it executes agents with no
    // workspace, because what it has is better — the exact (forecast, driver)
    // the run is bound to. `forecast_agent_claims.workspace_id` was NOT NULL,
    // so there was no row the hook could have written and the gate was
    // correct given the schema. 61 quantified judgements, 61 discarded, zero
    // rows in the table since mig-187 created it
    // (docs/HANDOFF_loops_and_gates.md §4.2). Migration 213 makes
    // `workspace_id` nullable and requires workspace OR forecast instead, so
    // the gate is now "is there any binding at all".
    //
    // A bare `driver` with neither binding is deliberately not enough: it
    // would violate `forecast_agent_claims_has_binding` and there is nothing
    // to attach the claim to.
    //
    // The claim carries `episode_id` (minted above) so attribution is exact
    // rather than a (agent_id, driver, time-window) guess — mig-197. This hook
    // and the episode write race, and the claim usually lands first, which is
    // why the id could not simply be read back from the stored episode.
    if (ws_id_opt.is_some() || forecast_id_opt.is_some()) && !output.evidence.is_empty() {
        let pool = state.db.clone();
        let registry = state.extractor_registry.clone();
        let agent_name = agent_id.clone();
        let evidence = output.evidence.clone();
        let binding = crate::handlers::workspace::agent_params_hook::ClaimBinding {
            workspace_id: ws_id_opt,
            forecast_id: forecast_id_opt,
            driver: driver_opt,
        };
        let log_workspace = ws_id_opt.map(|w| w.to_string()).unwrap_or_default();
        let log_forecast = binding.forecast_id.clone().unwrap_or_default();
        tokio::spawn(async move {
            match crate::handlers::workspace::agent_params_hook::apply_agent_multipliers(
                &pool,
                &registry,
                &binding,
                &agent_name,
                &evidence,
                Some(episode_id),
            )
            .await
            {
                // Was `Ok(true) => info!`, `Ok(false) => {}`. The empty arm
                // covered three different states, and `forecast_agent_claims`
                // has held zero rows since mig-187 — so the first bound run
                // that still produces no claim is the observation Loop 4 has
                // been waiting for, and it would have arrived silent and
                // indistinguishable from the 65 unbound runs before it.
                Ok(o) if o.recorded() => tracing::info!(
                    workspace = %log_workspace,
                    forecast = %log_forecast,
                    agent = %agent_name,
                    outcome = o.label(),
                    "agent multiplier recorded as a claim"
                ),
                Ok(o) => tracing::info!(
                    workspace = %log_workspace,
                    forecast = %log_forecast,
                    agent = %agent_name,
                    outcome = o.label(),
                    "bound run wrote no claim"
                ),
                Err(e) => tracing::warn!(
                    workspace = %log_workspace,
                    forecast = %log_forecast,
                    agent = %agent_name,
                    error = %e,
                    "failed to apply agent multipliers"
                ),
            }
        });
    }

    // 3.6 Grounding contract — could any tool this agent has have supplied
    // what it just claimed?
    //
    // This path never ran it. `grounding_trust::enforce` was wired into six
    // creature handlers and the delegation hop, which between them cover four
    // of the nine agents holding a field contract; the remaining five were
    // enforced only when another agent called them and never when a person
    // did. A contract that applies on one route and not another is not a
    // contract, it is a convention.
    //
    // `enforce` is a pure function over the document and returns an empty
    // report for any agent without a contract, so this is a no-op for most of
    // the catalogue and cannot fail a run.
    let grounding_report = match output
        .raw_response
        .as_deref()
        .and_then(fermi::agent_backend::envelope::extract_json)
    {
        Some(mut doc) => fermi::grounding_trust::enforce(&agent_id, &mut doc),
        None => fermi::grounding_trust::Report::default(),
    };
    // The invocation gate's own verdict, counted — in three states, not two.
    //
    // `enforce` returns an empty report for an agent with no declared contract,
    // and from here that is indistinguishable from a clean pass. The first
    // version of this block said exactly that in a comment and then recorded it
    // as `Approved` anyway.
    //
    // It matters at the scale this actually runs. Measured: **5 of 3,558
    // episodes** carry a grounding tag at all. Counting the other 3,553 as
    // approvals would have the gate reporting `3558 asked, 0 refused` — which
    // reads as "a control that has never needed to fire" when the truth is "a
    // control that almost never engages". Different findings, different
    // remedies, and the row count cannot tell them apart.
    //
    // `Undetermined` is what that state is: the gate was reached and formed no
    // opinion, because there was no contract to form one against.
    let has_contract = fermi::grounding_trust::contracts_for(&agent_id)
        .next()
        .is_some();
    fermi::gate_trust::decided(
        fermi::gate_trust::Gate::Grounding,
        if !has_contract {
            fermi::gate_trust::Decision::Undetermined
        } else if grounding_report.is_clean() {
            fermi::gate_trust::Decision::Approved
        } else {
            fermi::gate_trust::Decision::Refused
        },
        (!grounding_report.is_clean())
            .then(|| format!("{} violation(s)", grounding_report.violations.len()))
            .as_deref(),
    );
    if !grounding_report.is_clean() {
        tracing::warn!(
            agent = %agent_id,
            episode = %episode_id,
            violations = grounding_report.violations.len(),
            "grounding contract violated on the execute path — fields with no possible source"
        );
    }

    // 4. Record stats in registry
    let _ = state.registry.record_execution(&agent_id, &output);

    // 5. Store as ADM episode (with embedding + Spec 22 provenance)
    let mut episode = agent_output_to_episode(db_agent.agent_id, &body.query, &output);
    // Use the id minted before the claim hook was spawned, so the claim and
    // the episode agree regardless of which write lands first (mig-197).
    episode.episode_id = episode_id;
    // Record how the agent was asked, alongside how it did.
    if let Some(ref inv) = body.invocation {
        crate::stamp_invocation(&mut episode, inv);
    }
    // And what the grounding contract made of the answer, so a consumer of
    // this episode can tell a checked document from an unchecked one.
    crate::stamp_grounding(&mut episode, &grounding_report);
    // And check the asking against what the agent advertises — server-side,
    // from the resolved card, rather than believing the caller's account of
    // it. `bind_input` shipped in v0.16.0 and was wired only into the
    // desktop console, so this path never ran it; the episode carried the
    // client's assertion instead.
    {
        let verified = fermi::port_trust::bind_input(&card.accepts);
        let claimed = body
            .invocation
            .as_ref()
            .and_then(|i| i.get("input_binding"))
            .and_then(|v| v.as_str());
        // Advisory by design — it records and continues. Counted anyway,
        // because the mismatch RATE is the number that would justify making it
        // fatal, and until now it existed only as episode tags nothing reads.
        fermi::gate_trust::decided(
            fermi::gate_trust::Gate::InputBinding,
            if verified.is_mismatch() {
                fermi::gate_trust::Decision::Refused
            } else {
                fermi::gate_trust::Decision::Approved
            },
            verified
                .is_mismatch()
                .then_some("free text to a structured port"),
        );
        if verified.is_mismatch() {
            tracing::warn!(
                agent = %card.agent_id,
                declared = ?card.accepts,
                "free-text query sent to an agent that declares no text input"
            );
        }
        crate::stamp_input_binding(&mut episode, &verified, claimed);
    }
    // Stamp the (agent, human) dyad so the social tracker can accumulate
    // rapport/trust/reciprocity for this pair. Without this the episode is
    // invisible to the companion loop.
    let dyad_id = agent_bestiary_memory::dyad_id(db_agent.agent_id, &caller_id);
    episode.dyad_id = Some(dyad_id.clone());
    // Stamp the persona version this run was produced under.
    //
    // Live episodes were built with `persona_version_at_write: None`, which
    // `EpisodeScorer` turned into `unwrap_or(1)`, and `ObservabilityWorker`
    // skips every entry with `persona_version <= 1`. Drift was therefore
    // unreachable on live traffic no matter how much of it flowed. It also
    // kept live embeddings out of `mean_embedding_for_persona_version`, whose
    // baseline query filters on this exact column — so the drift baselines
    // were built from eval fixtures alone.
    episode.persona_version_at_write = Some(db_agent.persona_version);
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

    // Kept for the observability pass below, which needs the episode's own
    // fields (dyad, persona version, provenance) after the value is moved into
    // storage.
    let episode_for_observation = episode.clone();

    let stored_episode_id = state
        .memory_store
        .store_episode_with_provenance(episode, provenance.as_ref(), Some(source_ref))
        .await
        .map_err(|e| {
            eprintln!("Warning: failed to store episode: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })?;

    // ── Loop 2's seed ─────────────────────────────────────────────────────
    //
    // Below the episode write, and that placement is load-bearing.
    // `anomaly_events.episode_id` is a real foreign key; the original raise sat
    // ~200 lines above referencing an id whose row did not exist yet, and lost
    // the race whenever anything between them took time. The ordering is
    // enforced by the binding rather than by this comment: `stored_episode_id`
    // is produced by `store_episode_with_provenance` above and does not exist
    // before it, so moving this block up is a compile error.
    //
    // The event itself is built by `grounding_anomaly`, which is the only
    // place in the system that turns a violation into a Loop 2 input. This was
    // an inline copy — the ninth call site of `enforce` and the only one that
    // raised — and eight other paths had no equivalent at all.
    fermi::grounding_anomaly::spawn_raise(
        std::sync::Arc::clone(&state.memory_store),
        agent_id.clone(),
        Some(stored_episode_id),
        grounding_report.clone(),
    );

    // Make this turn visible to drift + anomaly detection. Without a timeline
    // entry the observability worker never sees live traffic, so the HITL
    // queue is fed only by eval runs. Deterministic evaluators only — no LLM
    // tokens, no added latency; see `handlers::live_observability`.
    crate::handlers::live_observability::spawn_live_observation(
        &state,
        crate::handlers::live_observability::LiveObservation {
            episode: episode_for_observation,
            agent: db_agent.clone(),
            // Same field the eval pipeline puts in the agent transcript turn.
            response: output.metadata.reasoning.clone().unwrap_or_default(),
            session_id: Some("live:execute".to_string()),
            rupture_detected: false,
        },
    );

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
    let ep_id_str = stored_episode_id.to_string();
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
            Some(reason) => format!("{} (episode {})", reason, stored_episode_id),
            None => format!(
                "No reason reported by the executor. stop_reason={}, episode {}",
                output.metadata.stop_reason.as_deref().unwrap_or("unknown"),
                stored_episode_id
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
        "stored_episode_id": stored_episode_id,
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
