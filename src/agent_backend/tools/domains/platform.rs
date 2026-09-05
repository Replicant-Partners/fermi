// src/agent_backend/tools/domains/platform.rs
//
// Phase 4 domain migration: Platform tools.
//
// Six tools:
//   search_knowledge    — requires_workspace: false
//   query_ontology      — requires_workspace: false
//   execute_agent       — requires_workspace: false, is_delegation: true
//   list_agents         — requires_workspace: false
//   web_search          — requires_workspace: false
//   delegate_to_agent   — requires_workspace: true, is_delegation: true
//
// Each is a zero-size struct implementing PlatformTool. execute() bodies are
// inlined verbatim from tools_legacy.rs — no dispatch through ToolRegistry.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::WorkspaceMessage;

use crate::agent_backend::executor::{AgentExecutor, ExecutionContext};
use crate::agent_backend::tool_executor::ToolAwareExecutor;
use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Platform-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(SearchKnowledge),
        Arc::new(QueryOntology),
        Arc::new(ExecuteAgent),
        Arc::new(ListAgents),
        Arc::new(WebSearch),
        Arc::new(DelegateToAgent),
    ]
}

// ─── Execute implementations ─────────────────────────────────────────────────

async fn execute_search_knowledge(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for search_knowledge")?;

    // Generate embedding for the query
    let embedding = ctx
        .embedder
        .generate(query)
        .await
        .map_err(|e| format!("Embedding generation failed: {}", e))?;

    // Search similar episodes
    let results = ctx
        .memory_store
        .search_similar_episodes(agent_id, &embedding, limit)
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    // Format results
    let formatted: Vec<serde_json::Value> = results
        .iter()
        .map(|(episode, distance)| {
            json!({
                "query": episode.query,
                "context": episode.context,
                "timestamp": episode.timestamp_ref.to_rfc3339(),
                "similarity": 1.0 - distance,
            })
        })
        .collect();

    serde_json::to_string_pretty(&formatted).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_query_ontology(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let include_rules = input
        .get("include_rules")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_entities = input
        .get("include_entities")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let include_facts = input
        .get("include_facts")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let agent_id = ctx
        .current_agent_id
        .ok_or("No agent context for query_ontology")?;

    let mut result = json!({});

    if include_rules {
        let rules = ctx
            .memory_store
            .get_agent_semantic_rules(agent_id)
            .await
            .map_err(|e| format!("Failed to get rules: {}", e))?;
        let rules_json: Vec<serde_json::Value> = rules
            .iter()
            .map(|r| {
                json!({
                    "content": r.rule_content,
                    "description": r.rule_description,
                    "confidence": r.confidence_score,
                    "status": r.verification_status,
                })
            })
            .collect();
        result["rules"] = json!(rules_json);
    }

    if include_entities {
        let entities = ctx
            .memory_store
            .get_agent_entities(agent_id)
            .await
            .map_err(|e| format!("Failed to get entities: {}", e))?;
        let entities_json: Vec<serde_json::Value> = entities
            .iter()
            .map(|e| {
                json!({
                    "name": e.entity_name,
                    "type": e.entity_type,
                    "summary": e.summary,
                })
            })
            .collect();
        result["entities"] = json!(entities_json);
    }

    if include_facts {
        let facts = ctx
            .memory_store
            .get_agent_facts(agent_id)
            .await
            .map_err(|e| format!("Failed to get facts: {}", e))?;
        let facts_json: Vec<serde_json::Value> = facts
            .iter()
            .map(|f| {
                json!({
                    "relation_type": f.relation_type,
                    "confidence": f.confidence,
                    "reasoning": f.reasoning,
                })
            })
            .collect();
        result["facts"] = json!(facts_json);
    }

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

/// Persist an episode for a delegated child execution (mig-198).
///
/// Before this existed, both delegation tools ran a child agent, read its
/// `reasoning` and `evidence`, and dropped the rest of the `AgentOutput` on the
/// floor. The child's tokens, cost, provider and model were never recorded, so
/// a compound agent under-reported its true cost by its entire fan-out and a
/// delegate-only agent had no economic record at all.
///
/// Writes the child's OWN episode rather than folding its tokens into the
/// caller's, so each agent stays separately costable and creditable — the
/// premise the marketplace rests on. Priced through the same
/// `agent_output_to_episode` / `AgentOutput::cost()` path as every other
/// episode, so a delegated run cannot drift onto a different cost basis.
///
/// Best-effort by design: a bookkeeping failure must never fail the delegation
/// the caller is waiting on. Logged at `warn` because a silent gap here
/// under-reports real spend, and returns the new episode id so nested
/// delegation can carry the chain further.
/// `episode_id` is minted by the caller BEFORE the child runs, so it can be
/// placed on the child's own `ToolContext.parent_episode_id` and a grandchild
/// can link to it. Same reason the request handler mints ahead of execution
/// (mig-197): a row that is written later cannot be pointed at by a task that
/// starts earlier.
async fn record_delegated_episode(
    ctx: &ToolContext,
    target_agent_id: Uuid,
    agent_slug: &str,
    pulse: crate::episode_boundary::Pulse,
    task: &str,
    output: &crate::agent_backend::executor::AgentOutput,
) -> Option<Uuid> {
    let mut episode = crate::episodes::agent_output_to_episode(target_agent_id, task, output);
    episode.parent_episode_id = ctx.parent_episode_id;
    // Findable as delegated work without having to join on the parent.
    episode.tags.push("delegated".to_string());
    if let Some(caller) = ctx.current_agent_id {
        episode.tags.push(format!("delegated_by:{caller}"));
    }

    // Through the boundary, and by the bare `store_episode` before it.
    //
    // This is the route the paper's sentence was written about, and it was the
    // least governed of the three: the child's field contract was never
    // enforced here, so an agent that grades its own output when a person calls
    // it graded nothing when a peer did. `store_episode` is also deprecated for
    // a reason that lands on this path specifically — it writes NULL provenance,
    // so every delegated child episode on the platform has no embedding and is
    // invisible to retrieval. That is not fixed here: `provenance: None`
    // preserves it deliberately, because embedding on the delegation hop is a
    // per-fan-out cost decision and not a bug to slip into a refactor.
    match crate::episode_boundary::persist_opened(
        pulse,
        crate::episode_boundary::Write {
            store: &ctx.memory_store,
            db: ctx.db.as_ref(),
            agent_slug,
            episode,
            // The parent named the peer, by name, in the tool call. That is the
            // same category as a person naming an agent in the path — no router
            // was consulted — and it is the reading Loop 4 needs, because an
            // outcome under a deliberate selection says something about the
            // agent rather than about a fallback.
            route: crate::route_trust::RouteSelection::CallerNamed,
            provenance: None,
            source_ref: Some(serde_json::json!({
                "kind": "delegated_execution",
                "delegated_by": ctx.current_agent_id,
                "agent_id": target_agent_id,
            })),
            // The delegation hop, and `ToolContext` has carried the workspace all
            // along. This is the call site that matters most: an agent invoking a
            // peer is what workspace work IS, and every one of those pulses has
            // been recorded as though it happened nowhere.
            workspace: ctx.workspace_id,
        },
    )
    .await
    {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(
                target_agent = %target_agent_id,
                parent_episode = ?ctx.parent_episode_id,
                error = %e,
                "[delegation] failed to record child episode — this run's cost \
                 will be missing from per-forecast and per-agent totals",
            );
            None
        }
    }
}

pub(crate) async fn execute_execute_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    // Support both "agent_id" (MCP convention) and "agent_name" (legacy)
    let agent_name = input
        .get("agent_id")
        .or_else(|| input.get("agent_name"))
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_id")?;
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;

    // mig-198: minted before the child runs, so it can be placed on the
    // child's own ToolContext and used as the id of the episode recorded for
    // this delegated execution. An id generated after the fact could not be
    // handed to a task that has already started.
    //
    // Reserved here rather than inside the cross-workspace branch below, which
    // is where the reservation used to live. That branch was not the only one
    // that hands the id out, and the branch that reserved was chosen by where
    // the target's uuid happened to already be resolved — which is why the
    // target is now resolved once, here, for the reservation's sake.
    let target_db_id: Option<Uuid> = match ctx.db.as_ref() {
        Some(db) => sqlx::query_scalar("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
            .bind(agent_name)
            .fetch_optional(db)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let child_pulse = match target_db_id {
        Some(tid) => crate::episode_boundary::Pulse::open(&ctx.memory_store, tid, query).await,
        // No `agents` row for the target, so there is nothing to reserve
        // against — and nothing is recorded for this run either, which the
        // warning below the execution says out loud. The id still exists
        // because the child's tool context needs a non-null root.
        None => crate::episode_boundary::Pulse::after_the_fact(
            Uuid::new_v4(),
            "the target has no agents row, so there is nothing to reserve \
             against and no child episode is written for this run at all",
        ),
    };
    let child_episode_id = child_pulse.episode_id;

    // Optional cross-workspace delegation: when workspace_id is provided,
    // the target agent runs inside that workspace's full context (tools,
    // workspace git, KG). This is the seam between Rabble creatures and
    // kask-app workspaces (e.g. kask-app-wild).
    let target_workspace_id: Option<uuid::Uuid> = input
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok());

    // Get the target agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent not found: {}", e))?;

    // Enrich card with KG context from past dream cycles
    let card = if let Some(ref db) = ctx.db {
        let (enriched, _) = crate::agent_backend::kg_context::enrich_with_kg_context_by_name(
            &ctx.memory_store,
            &ctx.embedder,
            db,
            agent_name,
            query,
            card,
        )
        .await;
        enriched
    } else {
        card
    };

    // Build a minimal AgentStmt for execution
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: query.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    // Captured before `card` moves into the execution context below: the
    // envelope needs only the declared contract, and holding a whole card
    // alive for it would be wrong.
    let declared_output_contract = card.capabilities.output_contract.clone();

    // Phase C: validate the caller's query against the callee's declared
    // input schema BEFORE dispatch. Non-blocking — records Gate::InputSchema
    // but never halts execution (backward compat with untyped callers).
    // Symmetric to envelope::build, which validates the output afterward.
    let _input_report = crate::agent_backend::envelope::validate_input(
        agent_name,
        card.capabilities.input_contract.as_ref(),
        query,
    );

    let context = ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        // Delegated child inherits the parent execution's funding.
        credentials: ctx.credentials.clone(),
        // ...but NOT the parent's attachments, and that is not a dropped frame.
        //
        // Attachments belong to a request, not to a session. Delegation builds a
        // new request whose content is the text the parent chose to send, so the
        // child was never promised an image. Propagating one silently would hand
        // a frame to an agent that may not declare `accepts: image`, and the
        // parent would have no way to know whether it arrived.
        //
        // If a compound agent needs to pass a photograph to a specialist, that
        // wants to be an explicit argument on `execute_agent` — visible in the
        // call, and checkable against the child's declared inputs.
        attachments: Vec::new(),
    };

    let output = if let Some(ws_id) = target_workspace_id {
        // ── Cross-workspace delegation ────────────────────────────────
        // Build a full ToolContext for the target workspace so the
        // delegated agent has access to its workspace git, tools, and KG.
        // Anti-recursion: use with_workspace_no_delegation to strip
        // further cross-workspace calls from the sub-agent's tool list.
        if let Some(ref db) = ctx.db {
            // Look up workspace slug for git context
            let slug: String = sqlx::query_scalar("SELECT slug FROM teams WHERE id = $1")
                .bind(ws_id)
                .fetch_optional(db)
                .await
                .ok()
                .flatten()
                .unwrap_or_default();

            // Look up the calling agent's DB UUID for current_agent_id
            let calling_agent_id: Option<uuid::Uuid> =
                sqlx::query_scalar("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
                    .bind(agent_name)
                    .fetch_optional(db)
                    .await
                    .ok()
                    .flatten();

            // The child's row is reserved above, before either branch of this
            // `if` — not here. Everything the child delegates during its run
            // points at `child_episode_id`, and while the row was written only
            // after the run finished, a child that failed to record orphaned
            // every grandchild permanently. 6 of the platform's 12 delegation
            // edges are in that state.

            let target_tool_context = std::sync::Arc::new(ToolContext {
                // The child's own episode, so anything IT delegates to links
                // to the child rather than skipping a level (mig-198).
                parent_episode_id: Some(child_episode_id),
                credentials: ctx.credentials.clone(),
                memory_store: ctx.memory_store.clone(),
                embedder: ctx.embedder.clone(),
                registry: ctx.registry.clone(),
                current_agent_id: calling_agent_id,
                workspace_id: Some(ws_id),
                workspace_slug: Some(slug.clone()),
                workspace_git: ctx.workspace_git.clone(),
                db: ctx.db.clone(),
                gas_fees: ctx.gas_fees.clone(),
                user_id: ctx.user_id.clone(),
                user_secrets: None,
                eval_trigger: ctx.eval_trigger.clone(),
                remote_mcp: None,
            });

            let tool_executor = crate::agent_backend::tool_executor::ToolAwareExecutor::new(
                ctx.registry.executor_arc(),
                crate::agent_backend::tools::PlatformToolRegistry::workspace_no_delegation(),
                target_tool_context,
            );

            tool_executor
                .execute(&stmt, &context)
                .await
                .map_err(|e| format!("Cross-workspace agent execution failed: {}", e))?
        } else {
            // No DB — fall back to base executor
            ctx.registry
                .execute_agent(&stmt, &context)
                .await
                .map_err(|e| format!("Agent execution failed: {}", e))?
        }
    } else {
        // ── Standard (same-workspace or global) execution ────────────
        // Execute via the base executor (no tools — prevents recursion
        // in the common case where workspace_id is not specified).
        ctx.registry
            .execute_agent(&stmt, &context)
            .await
            .map_err(|e| format!("Agent execution failed: {}", e))?
    };

    // mig-198: record the child's own cost. Needs the target's DB uuid, which
    // this tool never resolved because it only ever needed the card by name.
    // When there is no DB handle we cannot write an episode at all, so the
    // spend stays unrecorded — logged rather than passed over in silence,
    // because that is a hole in the cost ledger and should be visible as one.
    //
    // Reuses the uuid resolved for the reservation. It was looked up three
    // times in this function against the same name, and the copy here ran only
    // on the path that reaches the end — so the id the tool context advertised
    // and the id this row was written under were resolved by separate queries
    // that could disagree.
    if ctx.db.is_some() {
        match target_db_id {
            Some(target_db_id) => {
                record_delegated_episode(
                    ctx,
                    target_db_id,
                    agent_name,
                    child_pulse,
                    query,
                    &output,
                )
                .await;
            }
            None => tracing::warn!(
                agent = %agent_name,
                "[delegation] target agent not found in DB; child episode not \
                 recorded and its cost will be missing from totals",
            ),
        }
    } else {
        tracing::debug!(
            agent = %agent_name,
            "[delegation] no DB handle; child episode not recorded",
        );
    }

    // Format the output — include metadata.reasoning so callers can
    // parse domain-specific JSON (e.g. forage_scout's structured response)
    // ── the delegation envelope (additive) ────────────────────────────
    //
    // Every key below this is unchanged. The envelope is added alongside so
    // existing coordinator prompts, which read `response` and `evidence`,
    // keep working byte-for-byte.
    //
    // What it adds is the thing delegation never had: the child's OWN
    // document, enforced against its grounding contract, with per-block
    // provenance. Before this, `response` was `metadata.reasoning` — a
    // per-agent parser's reading of the output — and a fabricated field
    // stripped at the creature-module boundary passed freely between agents.
    let envelope = crate::agent_backend::envelope::build(
        agent_name,
        declared_output_contract.as_ref(),
        &output,
        child_episode_id,
    );

    // Third consumer of the same verdict: the trend.
    //
    // The coordinator reads `envelope.validation` on this hop and
    // `gate_trust` counts it in aggregate, but neither accrues per agent —
    // the counters are process-local and reset on deploy. Without this row,
    // "is this member's output getting better or worse" has no answer, which
    // is the input loop 4 needs to change a roster on measured contribution.
    //
    // Writes nothing when nothing was checked. See `schema_conformance`.
    if let Some(ref db) = ctx.db {
        if let Some(status) = envelope
            .pointer("/validation/status")
            .and_then(|s| s.as_str())
        {
            if crate::schema_conformance::score_for(status).is_some() {
                if let Ok(Some(target_db_id)) = sqlx::query_scalar::<_, Uuid>(
                    "SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1",
                )
                .bind(agent_name)
                .fetch_optional(db)
                .await
                {
                    crate::schema_conformance::record(
                        db,
                        target_db_id,
                        child_episode_id,
                        status,
                        envelope.get("type").and_then(|t| t.as_str()),
                    )
                    .await;
                }
            }
        }
    }

    let result = json!({
        "agent": output.agent_name,
        "confidence": output.confidence,
        "status": format!("{:?}", output.status),
        "response": output.metadata.reasoning,
        "envelope": envelope,
        "evidence": output.evidence.iter().map(|e| {
            json!({
                "summary": e.summary,
                "key_findings": e.key_findings,
                "strength": e.strength,
            })
        }).collect::<Vec<_>>(),
    });

    serde_json::to_string_pretty(&result).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_delegate_to_agent(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let agent_name = input
        .get("agent_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: agent_name")?;
    let task = input
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: task")?;

    let ws_id = ctx
        .workspace_id
        .ok_or("delegate_to_agent requires a workspace context")?;
    let ws_slug = ctx.workspace_slug.as_deref().unwrap_or("");

    let pool = ctx.memory_store.pool();

    // Verify agent is in workspace
    let agent_row = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
         JOIN agents a ON a.agent_id = wa.agent_id
         WHERE wa.workspace_id = $1 AND a.agent_name = $2",
    )
    .bind(ws_id)
    .bind(agent_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .ok_or_else(|| format!("Agent '{}' is not in this workspace", agent_name))?;

    let target_agent_id: Uuid = agent_row.get("agent_id");
    let display: String = agent_row
        .try_get::<Option<String>, _>("display_alias")
        .unwrap_or(None)
        .unwrap_or_else(|| agent_name.to_string());

    // Post delegation message to workspace chat
    let delegation_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: ctx
            .current_agent_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        sender_name: Some(format!(
            "{} → {}",
            ctx.current_agent_id.map(|_| "compound").unwrap_or("system"),
            display
        )),
        content: format!("Delegating to {}: {}", display, task),
        message_type: "system_event".to_string(),
        metadata: json!({"delegation": true, "target": agent_name}),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };
    let _ = ctx
        .memory_store
        .store_workspace_message(&delegation_msg)
        .await;

    // Resolve agent card
    let card = ctx
        .registry
        .get(agent_name)
        .map_err(|e| format!("Agent card not found: {}", e))?;

    // Enrich card with KG context from past dream cycles
    let (card, _) = crate::agent_backend::kg_context::enrich_with_kg_context(
        &ctx.memory_store,
        &ctx.embedder,
        target_agent_id,
        task,
        card,
    )
    .await;

    // Build execution context
    let stmt = crate::ast::AgentStmt {
        name: agent_name.to_string(),
        agent_type: Some(card.agent_type.clone()),
        query: task.to_string(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };

    let context = ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        // Delegated child inherits the parent execution's funding.
        credentials: ctx.credentials.clone(),
        // ...but NOT the parent's attachments, and that is not a dropped frame.
        //
        // Attachments belong to a request, not to a session. Delegation builds a
        // new request whose content is the text the parent chose to send, so the
        // child was never promised an image. Propagating one silently would hand
        // a frame to an agent that may not declare `accepts: image`, and the
        // parent would have no way to know whether it arrived.
        //
        // If a compound agent needs to pass a photograph to a specialist, that
        // wants to be an explicit argument on `execute_agent` — visible in the
        // call, and checkable against the child's declared inputs.
        attachments: Vec::new(),
    };

    // mig-198: minted before the child runs so it can be handed to the child's
    // own ToolContext below, letting a grandchild link to it.
    //
    // And reserved, which it was not. Minting early lets a grandchild NAME this
    // episode; only writing the row early lets it RESOLVE one. This site put
    // the id straight onto the tool context two lines down and wrote the row
    // only after the child returned, so a child that died mid-fan-out orphaned
    // everything it had already spawned. The sibling delegation tool reserved
    // and this one did not — the same asymmetry, one function apart.
    let child_pulse =
        crate::episode_boundary::Pulse::open(&ctx.memory_store, target_agent_id, task).await;
    let child_episode_id = child_pulse.episode_id;

    // Build a ToolAwareExecutor with workspace tools but NO delegation
    let tool_context = Arc::new(ToolContext {
        // The child's own episode, so nested delegation links to the child
        // rather than skipping a level (mig-198).
        parent_episode_id: Some(child_episode_id),
        credentials: ctx.credentials.clone(),
        memory_store: ctx.memory_store.clone(),
        embedder: ctx.embedder.clone(),
        registry: ctx.registry.clone(),
        current_agent_id: Some(target_agent_id),
        workspace_id: Some(ws_id),
        workspace_slug: Some(ws_slug.to_string()),
        workspace_git: ctx.workspace_git.clone(),
        db: ctx.db.clone(),
        gas_fees: ctx.gas_fees.clone(),
        user_id: ctx.user_id.clone(),
        user_secrets: ctx.user_secrets.clone(),
        // Delegated child agents inherit the parent's trigger capability.
        eval_trigger: ctx.eval_trigger.clone(),
        remote_mcp: None,
    });

    let tool_executor = ToolAwareExecutor::new(
        ctx.registry.executor_arc(),
        crate::agent_backend::tools::PlatformToolRegistry::workspace_no_delegation(),
        tool_context,
    );

    let output = tool_executor
        .execute(&stmt, &context)
        .await
        .map_err(|e| format!("Delegation failed: {}", e))?;

    // mig-198: record the child's own cost before its output is reduced to
    // prose. Everything below this line throws the token accounting away.
    record_delegated_episode(ctx, target_agent_id, agent_name, child_pulse, task, &output).await;

    let raw_response = output.metadata.reasoning.clone().unwrap_or_default();
    // Post the result as a workspace message from the delegated agent.
    //
    // Pass the raw LLM response through verbatim (see issue #2 / docs/specs/
    // 09_RESEARCH_AGENT_OUTPUT_STRIPPED.md). Falling back to evidence summaries
    // alone destroys structured JSON outputs from research-tier agents.
    let raw_response = output.metadata.reasoning.clone().unwrap_or_default();
    let evidence_text = output
        .evidence
        .iter()
        .filter_map(|e| {
            let s = e.summary.as_deref().unwrap_or("").trim();
            if s.is_empty() {
                None
            } else {
                Some(s.to_string())
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let result_text = if !raw_response.trim().is_empty() {
        raw_response.clone()
    } else {
        evidence_text.clone()
    };

    let result_msg = WorkspaceMessage {
        message_id: Uuid::new_v4(),
        workspace_id: ws_id,
        sender_type: "agent".to_string(),
        sender_id: target_agent_id.to_string(),
        sender_name: Some(display.clone()),
        content: if result_text.is_empty() {
            "(no output)".to_string()
        } else {
            result_text.clone()
        },
        message_type: "execution_result".to_string(),
        metadata: json!({
            "delegated_by": ctx.current_agent_id,
            "tokens_used": output.tokens_used,
            "tool_invocations": output.tool_invocations.len(),
            "loop_iterations": output.loop_iterations,
            "raw_response": raw_response,
        }),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };
    let _ = ctx.memory_store.store_workspace_message(&result_msg).await;

    // Return result to calling agent
    Ok(if result_text.is_empty() {
        format!("{} completed the delegation but produced no text output. Check workspace files for artifacts.", display)
    } else {
        result_text
    })
}

async fn execute_list_agents(ctx: &ToolContext) -> Result<String, String> {
    let cards = ctx
        .registry
        .list_cards()
        .map_err(|e| format!("Failed to list agents: {}", e))?;

    let agents: Vec<serde_json::Value> = cards
        .iter()
        .map(|c| {
            json!({
                "id": c.agent_id,
                "type": c.agent_type,
                "description": c.metadata.description,
                "skills": c.capabilities.skills,
            })
        })
        .collect();

    serde_json::to_string_pretty(&agents).map_err(|e| format!("Serialization error: {}", e))
}

pub(crate) async fn execute_web_search(input: &serde_json::Value) -> Result<String, String> {
    let query = input
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: query")?;
    let count = input
        .get("count")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(10) as usize;
    let freshness = input.get("freshness").and_then(|v| v.as_str());

    let api_key = std::env::var("BRAVE_SEARCH_API_KEY")
        .map_err(|_| "BRAVE_SEARCH_API_KEY environment variable not set. Get a free API key at https://brave.com/search/api/".to_string())?;

    let client = reqwest::Client::new();
    let mut req = client
        .get("https://api.search.brave.com/res/v1/web/search")
        .header("Accept", "application/json")
        .header("X-Subscription-Token", &api_key)
        .query(&[
            ("q", query),
            ("count", &count.to_string()),
            ("search_lang", "en"),
        ]);

    if let Some(f) = freshness {
        req = req.query(&[("freshness", f)]);
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Brave Search request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Brave Search API error {}: {}", status, body));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Brave Search response: {}", e))?;

    let results = data
        .get("web")
        .and_then(|w| w.get("results"))
        .and_then(|r| r.as_array());

    let Some(results) = results else {
        return Ok("No web results found for this query.".to_string());
    };

    if results.is_empty() {
        return Ok("No web results found for this query.".to_string());
    }

    let mut output = format!("## Web Search Results for: {}\n\n", query);
    for (i, result) in results.iter().enumerate() {
        let title = result
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(no title)");
        let url = result.get("url").and_then(|v| v.as_str()).unwrap_or("");
        let description = result
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("(no description)");
        let age = result.get("age").and_then(|v| v.as_str()).unwrap_or("");
        let published = result
            .get("page_age")
            .and_then(|v| v.as_str())
            .or(if age.is_empty() { None } else { Some(age) })
            .unwrap_or("(date unknown)");

        output.push_str(&format!(
            "**{}. {}**\n{}\n{}\n{}\n\n",
            i + 1,
            title,
            url,
            published,
            description
        ));
    }

    // Truncate to avoid context overflow
    if output.len() > 12_000 {
        output.truncate(12_000);
        output.push_str("\n... [truncated]");
    }

    Ok(output)
}

// ─── search_knowledge ─────────────────────────────────────────────────────────

struct SearchKnowledge;

#[async_trait]
impl PlatformTool for SearchKnowledge {
    fn name(&self) -> &'static str {
        "search_knowledge"
    }

    fn description(&self) -> &'static str {
        "Search the agent's episodic memory for relevant past experiences using semantic similarity. Returns the most relevant episodes."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query to find relevant knowledge"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_search_knowledge(input, ctx).await
    }
}

// ─── query_ontology ───────────────────────────────────────────────────────────

struct QueryOntology;

#[async_trait]
impl PlatformTool for QueryOntology {
    fn name(&self) -> &'static str {
        "query_ontology"
    }

    fn description(&self) -> &'static str {
        "Query the agent's knowledge graph to retrieve semantic rules, entities, and facts. Specify which types to include."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "include_rules": {
                    "type": "boolean",
                    "description": "Include semantic rules (default: true)",
                    "default": true
                },
                "include_entities": {
                    "type": "boolean",
                    "description": "Include entities (default: true)",
                    "default": true
                },
                "include_facts": {
                    "type": "boolean",
                    "description": "Include facts/relationships (default: true)",
                    "default": true
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_query_ontology(input, ctx).await
    }
}

// ─── execute_agent ────────────────────────────────────────────────────────────

struct ExecuteAgent;

#[async_trait]
impl PlatformTool for ExecuteAgent {
    fn name(&self) -> &'static str {
        "execute_agent"
    }

    fn description(&self) -> &'static str {
        "Invoke another agent with a query and get its response. When workspace_id is provided, the sub-agent runs inside that workspace's full context (cross-workspace delegation — used for Rabble creatures to consume kask-app workspaces). Without workspace_id, the sub-agent runs a single turn without tools.\n\nREAD `envelope` BEFORE YOU WEIGH THE ANSWER. The result carries an `envelope` describing what actually crossed the hop, and `envelope.validation.status` is the member's document checked against the type that member itself declared:\n· `valid` — checked and conforming. Use `envelope.payload`, which is the member's own typed document and is better than re-reading its prose.\n· `invalid` — the document CONTRADICTS the type its producer declared; `violations` names the paths. Do not silently average it in. Discount it, say in your output that you did, and prefer another member or another route for this kind of task.\n· `unverified_no_schema` / `unverified_no_payload` / `unverified_unsupported_schema` — NOT a pass. Nothing was checked, because the member declares no type, returned prose, or declared a schema the validator cannot evaluate. Treat it as unverified evidence and weight it below a `valid` member.\nAlso check `envelope.provenance.blocks`: a `tool_verified` value is a measurement, `model_inference` is a judgement, and combining them as if they were the same kind of number is how a coordinator launders a guess into a result. `grounding_enforced: false` means nobody has written a grounding contract for that member — an absence, not a clean bill of health."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "The agent_id of the agent to invoke (e.g. 'forage_scout', 'wild_companion')"
                },
                "agent_name": {
                    "type": "string",
                    "description": "Alias for agent_id (legacy parameter name)"
                },
                "query": {
                    "type": "string",
                    "description": "The query to send to the agent"
                },
                "workspace_id": {
                    "type": "string",
                    "description": "Optional: UUID of the target workspace. When provided, the agent runs inside that workspace's context with full tool access (cross-workspace delegation). Used for Rabble creatures to consume kask-app-wild or other app workspaces."
                }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    fn is_delegation(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_execute_agent(input, ctx).await
    }
}

// ─── list_agents ──────────────────────────────────────────────────────────────

struct ListAgents;

#[async_trait]
impl PlatformTool for ListAgents {
    fn name(&self) -> &'static str {
        "list_agents"
    }

    fn description(&self) -> &'static str {
        "List all available agents in the registry with their names, types, and descriptions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        let _ = input;
        execute_list_agents(ctx).await
    }
}

// ─── web_search ───────────────────────────────────────────────────────────────

struct WebSearch;

#[async_trait]
impl PlatformTool for WebSearch {
    fn name(&self) -> &'static str {
        "web_search"
    }

    fn description(&self) -> &'static str {
        "Search the web for current information using Brave Search. Returns recent news, articles, and web pages with titles, URLs, descriptions, and publication dates. Use this to get up-to-date evidence that goes beyond your training data cutoff."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query. Be specific: include names, dates, ticker symbols, or event terms. E.g. 'RKLB Q4 2025 earnings revenue' or 'Fed interest rate decision March 2026'."
                },
                "count": {
                    "type": "integer",
                    "description": "Number of results to return (default: 5, max: 10)",
                    "default": 5
                },
                "freshness": {
                    "type": "string",
                    "description": "Filter by recency: 'pd' = past day, 'pw' = past week, 'pm' = past month, 'py' = past year. Omit for all-time results.",
                    "enum": ["pd", "pw", "pm", "py"]
                }
            },
            "required": ["query"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        let _ = ctx;
        execute_web_search(input).await
    }
}

// ─── delegate_to_agent ────────────────────────────────────────────────────────

struct DelegateToAgent;

#[async_trait]
impl PlatformTool for DelegateToAgent {
    fn name(&self) -> &'static str {
        "delegate_to_agent"
    }

    fn description(&self) -> &'static str {
        "Delegate a task to another workspace agent who will execute with full tool access (image generation, file writing, etc). The delegation appears as a visible message in workspace chat. Use this instead of execute_agent when the target agent needs tools to do its work."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_name": {
                    "type": "string",
                    "description": "The name of the workspace agent to delegate to"
                },
                "task": {
                    "type": "string",
                    "description": "The task description for the target agent"
                }
            },
            "required": ["agent_name", "task"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Platform
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    fn is_delegation(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_delegate_to_agent(input, ctx).await
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_platform() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Platform,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_six() {
        assert_eq!(tools().len(), 6);
    }

    #[test]
    fn delegation_flags_are_correct() {
        let tools = tools();
        let delegation: Vec<(&str, bool)> = tools
            .iter()
            .map(|t| (t.name(), t.is_delegation()))
            .collect();

        for (name, flag) in &delegation {
            match *name {
                "execute_agent" | "delegate_to_agent" => {
                    assert!(flag, "tool `{}` should be delegation", name);
                }
                _ => {
                    assert!(!flag, "tool `{}` should NOT be delegation", name);
                }
            }
        }
    }

    #[test]
    fn workspace_flags_are_correct() {
        let tools = tools();
        let requires: Vec<(&str, bool)> = tools
            .iter()
            .map(|t| (t.name(), t.requires_workspace()))
            .collect();

        for (name, flag) in &requires {
            match *name {
                "delegate_to_agent" => {
                    assert!(flag, "tool `{}` should require workspace", name);
                }
                _ => {
                    assert!(!flag, "tool `{}` should NOT require workspace", name);
                }
            }
        }
    }
}
