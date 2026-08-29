//! Workspace chat, SSE stream, and agent hire/add/remove.
//! Workspace handlers — shared imports.
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, Sse},
    Json,
};
use fermi::gas::charge_gas;
use fermi_auth::{
    credit_charge, credit_charge_purchased_only, credit_deposit_typed, get_or_create_wallet, teams,
    AuthPrincipal,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::convert::Infallible;
use std::sync::Arc;

use agent_bestiary_memory::{Agent, CoherenceEvaluation, WorkspaceMessage};
use agent_bestiary_ontology::WorkspaceGitManager;
use coherence_core::types::{ConversationId, Message as CoherenceMessage, ParticipantId};
use coherence_engine::SettlingEngine;
use coherence_observer::ConversationObserver;

use fermi::agent_backend::executor::AgentExecutor;
use fermi::agent_backend::kg_context::enrich_with_kg_context;
use fermi::agent_backend::tool_executor::ToolAwareExecutor;
use fermi::agent_backend::tools::{ToolContext, ToolRegistry};
use fermi::agent_backend::ExecutionContext;
use fermi::ast;

use super::core::{charge_workspace_gas, get_workspace_slug, parse_at_mention};
use crate::handlers::agents::CreateAgentRequest;
use crate::{agent_output_to_episode, resolve_agent, resolve_agent_card, AppState};

// ─── execution_result content policy ───────────────────────────────
//
// Bug history (issue #2 / docs/specs/09_RESEARCH_AGENT_OUTPUT_STRIPPED.md):
// the previous implementation reconstructed `content` from
// `format!("**Evidence:**\n- {summary}")` using parsed evidence summaries.
// Research-tier agents whose system prompts emit JSON contracts
// (`supply_chain_oracle`, `comparator`, `sidestream_miner`, …) yielded
// empty summaries through that parse, so `content` collapsed to the
// literal 18-byte string `"\n\n**Evidence:**\n- "` regardless of the
// LLM's real output. 193k tokens / 0 usable content per the reported
// reproduction.
//
// Follow-up (issue #4): with the raw-response channel working, JSON-
// contract agents started emitting their primary JSON answer followed by
// a `**Evidence:**` addendum that duplicated the same JSON (the evidence
// parser falls back to "stuff the whole text into summary" when the
// response doesn't match the EvidenceData/EvidenceJson shape). 2× token
// cost on `content`, and downstream JSON extractors broke on the doubling.
//
// Policy:
//   1. The raw LLM response (`AgentOutput.metadata.reasoning`) is the
//      source of truth for `content`. Pass it through verbatim.
//   2. If parsed evidence summaries contain real signal AND are not
//      already substrings of the raw response (so they add information
//      rather than duplicating it), append an `**Evidence:**` block as
//      an addendum — not a replacement.
//   3. Empty bullets are suppressed entirely (no `- ` leftovers).
//   4. If the LLM genuinely returned nothing, say so honestly instead
//      of emitting the old empty-template artifact.
//
// `raw_response` is also exposed in metadata for machine consumers
// (kask's `_extractBomItems`, comparator narrative readers, etc.) that
// want the canonical text without re-parsing markdown.
fn format_execution_result_content(raw_response: &str, evidence_summaries: &[&str]) -> String {
    let trimmed_raw = raw_response.trim();

    let evidence_block = evidence_summaries
        .iter()
        .filter_map(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else if !trimmed_raw.is_empty() && trimmed_raw.contains(trimmed) {
                // Issue #4 — the addendum is a substring of the raw response
                // (typical pattern: the JSON parser failed and stuffed the
                // whole response back as `summary`). Skip — it would just
                // duplicate the primary content.
                None
            } else {
                Some(format!("- {}", trimmed))
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    if trimmed_raw.is_empty() {
        if evidence_block.is_empty() {
            "(agent returned no content)".to_string()
        } else {
            format!("**Evidence:**\n{}", evidence_block)
        }
    } else if evidence_block.is_empty() {
        raw_response.to_string()
    } else {
        format!("{}\n\n**Evidence:**\n{}", trimmed_raw, evidence_block)
    }
}

// ─── Workspace Chat ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PostMessageRequest {
    content: String,
    #[serde(default)]
    message_type: Option<String>,
    #[serde(default)]
    metadata: Option<Value>,
}

/// Broadcast a workspace message to SSE subscribers.
///
/// Two-layer delivery:
/// 1. In-process Tokio broadcast — sub-millisecond, same replica only.
/// 2. Postgres NOTIFY — cross-replica fan-out so multiple api-server
///    instances can serve SSE streams for the same workspace.
///    Channel name: "ws_<workspace_uuid_hex>" (no hyphens, max 63 chars).
pub(super) fn broadcast_message(state: &AppState, workspace_id: uuid::Uuid, msg_json: &Value) {
    // Layer 1: in-process
    let _ = state.ws_broadcast.send(crate::WorkspaceEvent {
        workspace_id,
        message: msg_json.clone(),
    });

    // Layer 2: Postgres NOTIFY (fire-and-forget, best-effort)
    let pool = state.db.clone();
    let channel = format!("ws_{}", workspace_id.as_simple());
    let payload = serde_json::to_string(msg_json).unwrap_or_default();
    tokio::spawn(async move {
        let _ = sqlx::query("SELECT pg_notify($1, $2)")
            .bind(&channel)
            .bind(&payload)
            .execute(&pool)
            .await;
    });
}

/// Build the standard message JSON shape from a WorkspaceMessage.
pub(super) fn message_to_json(m: &WorkspaceMessage) -> Value {
    json!({
        "message_id": m.message_id,
        "sender_type": m.sender_type,
        "sender_id": m.sender_id,
        "sender_name": m.sender_name,
        "content": m.content,
        "message_type": m.message_type,
        "metadata": m.metadata,
        "created_at": m.created_at.to_rfc3339(),
    })
}

/// Load workspace context files from the git repo's context/ directory.
pub async fn load_workspace_context(workspace_git: &WorkspaceGitManager, slug: &str) -> String {
    let files = match workspace_git.list_files(slug, Some("context")) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let mut context_parts = Vec::new();
    for file in &files {
        if file.is_dir {
            continue;
        }
        if let Ok(content) = workspace_git.read_file(slug, &file.path) {
            context_parts.push(format!("--- {} ---\n{}", file.path, content));
        }
    }
    if context_parts.is_empty() {
        String::new()
    } else {
        format!("[Workspace Context]\n{}", context_parts.join("\n\n"))
    }
}

pub async fn post_workspace_message_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<PostMessageRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Verify membership
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Charge message gas — UNLESS the client explicitly tagged this
    // message as a bookkeeping event (metadata.cost_class == 'event_append').
    //
    // Apps like SimOps use workspace messages as an append-only event log
    // for everything from 'process.saved' commits to 'insight.accepted'
    // status updates. Charging gas per append makes audit-trail discipline
    // economically punishing — the discovery flow needs 50-100 events
    // before the workspace even has a finalised pipeline. The cost_class
    // taxonomy (defined in docs/specs/01_APP_PRIMITIVE.md §4) classifies
    // these as 'event_append' and explicitly mandates zero gas.
    //
    // Real agent-initiated chat messages stay billable as before. We also
    // keep the charge when message_type is 'agent_invocation' because
    // that's the path that fires LLM work and needs to cover its cost
    // even if the metadata happens to mention event_append.
    // A message is a free bookkeeping event when:
    //   (a) metadata.cost_class == "event_append"  (the explicit contract), OR
    //   (b) message_type == "event_append"          (kask shorthand — avoids
    //       needing a full metadata object for every SimOps log write)
    //
    // Either path is accepted so kask doesn't have to coordinate a metadata
    // object construction for every cascade.ran / insight.accepted / process.saved
    // write — setting message_type is enough.
    let is_event_append = req.message_type.as_deref() == Some("event_append")
        || matches!(
            req.metadata
                .as_ref()
                .and_then(|m| m.get("cost_class"))
                .and_then(|v| v.as_str()),
            Some("event_append")
        );
    let is_invocation_path = req.message_type.as_deref() == Some("agent_invocation")
        || parse_at_mention(&req.content).is_some();
    if !(is_event_append && !is_invocation_path) {
        let charge_result = charge_workspace_gas(
            &state.db,
            ws_uuid,
            &workspace_id,
            state.gas_fees.message_send,
            "gas_fee",
            "Chat message",
            None,
        )
        .await;

        // Only hard-fail (402) on LLM invocation paths — those actually cost money.
        // Plain chat messages and bookkeeping writes (cascade.ran, process.saved, etc.)
        // are allowed to proceed even when the workspace wallet is empty.
        // This prevents the workspace from becoming completely unusable just because
        // the wallet balance drifted to 0 while teams.workspace_budget still shows credits
        // (a known sync issue when admin-granted credits — which land in granted_balance —
        // are used to fund a workspace that requires purchased_balance for transfers).
        if is_invocation_path {
            charge_result?;
        }
        // Non-invocation: log the failure but proceed — the message is written.
        // The workspace owner sees the low-balance warning on the next workspace load.
    }

    // Detect @agent_name invocation
    let at_mention = parse_at_mention(&req.content);
    let is_invocation =
        req.message_type.as_deref() == Some("agent_invocation") || at_mention.is_some();

    // Look up user display name
    let display_name: Option<String> =
        sqlx::query_scalar("SELECT display_name FROM users WHERE user_id = $1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id: ws_uuid,
        sender_type: "user".to_string(),
        sender_id: user_id.clone(),
        sender_name: display_name.or_else(|| Some(user_id.clone())),
        content: req.content.clone(),
        message_type: if is_invocation {
            "agent_invocation".to_string()
        } else {
            "chat".to_string()
        },
        metadata: req.metadata.clone().unwrap_or(json!({})),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };

    let msg_id = state
        .memory_store
        .store_workspace_message(&msg)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Broadcast user message to SSE subscribers
    broadcast_message(&state, ws_uuid, &message_to_json(&msg));

    // If @agent invocation, spawn background execution
    if is_invocation {
        // Extract target agent and query
        let (target_agent, query) = if let Some((name, q)) = at_mention {
            (name, q)
        } else if let Some(meta) = &req.metadata {
            let name = meta
                .get("target_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let q = meta
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or(&req.content)
                .to_string();
            (name, q)
        } else {
            ("".to_string(), req.content.clone())
        };

        if !target_agent.is_empty() {
            // Verify agent is in workspace
            let agent_in_ws = sqlx::query(
                "SELECT a.agent_id, a.agent_name, a.display_alias FROM workspace_agents wa
                 JOIN agents a ON a.agent_id = wa.agent_id
                 WHERE wa.workspace_id = $1 AND a.agent_name = $2",
            )
            .bind(ws_uuid)
            .bind(&target_agent)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

            if let Some(agent_row) = agent_in_ws {
                let agent_name: String = agent_row.try_get("agent_name").unwrap_or_default();
                let agent_display: Option<String> =
                    agent_row.try_get("display_alias").unwrap_or(None);
                let display = agent_display.unwrap_or_else(|| agent_name.clone());

                // Clone what we need for the background task
                let state2 = state.clone();
                let ws_id = workspace_id.clone();
                let ws_uuid2 = ws_uuid;
                let query2 = query.clone();
                let agent_name2 = agent_name.clone();
                let display2 = display.clone();
                let user_id2 = user_id.clone();

                tokio::spawn(async move {
                    // Load workspace context
                    let slug = get_workspace_slug(&state2.db, ws_uuid2)
                        .await
                        .unwrap_or_default();
                    let ws_context = load_workspace_context(&state2.workspace_git, &slug).await;

                    // Build augmented query: git context + kask context_bundle + user message.
                    // kask sends metadata.context_bundle as a pre-serialized JSON object
                    // containing process, variations, recent_events, annotations, budget,
                    // workspace_agents. Prepend it so the companion sees the full workspace
                    // state without a separate file-read round-trip.
                    let bundle_block = req
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("context_bundle"))
                        .map(|b| {
                            format!(
                                "[CONTEXT BUNDLE]\n{}\n[/CONTEXT BUNDLE]",
                                serde_json::to_string_pretty(b).unwrap_or_default()
                            )
                        })
                        .unwrap_or_default();

                    let augmented_query = match (ws_context.is_empty(), bundle_block.is_empty()) {
                        (true, true) => query2.clone(),
                        (false, true) => format!("{}\n\n{}", ws_context, query2),
                        (true, false) => format!("{}\n\n{}", bundle_block, query2),
                        (false, false) => {
                            format!("{}\n\n{}\n\n{}", ws_context, bundle_block, query2)
                        }
                    };

                    // Resolve and execute
                    let result = async {
                        let db_agent = resolve_agent(&state2, &agent_name2).await?;
                        let card = resolve_agent_card(&state2, &db_agent);

                        // Enrich card with KG context from past dream cycles
                        let t_kg = tokio::time::Instant::now();
                        let (card, _kg_query_embedding) = enrich_with_kg_context(
                            &state2.memory_store,
                            &state2.embedder,
                            db_agent.agent_id,
                            &augmented_query,
                            card,
                        )
                        .await;
                        tracing::info!(elapsed_ms = t_kg.elapsed().as_millis() as u64, "kg_context_enrich");

                        let agent_stmt = ast::AgentStmt {
                            name: agent_name2.clone(),
                            agent_type: Some(card.agent_type.clone()),
                            query: augmented_query.clone(),
                            executor: Some(ast::ExecutorType::LLM),
                            schedule: None,
                            driver_refs: vec![],
                            depends_on: vec![],
                            confidence_threshold: None,
                        };
                        let program = ast::Program {
                            statements: vec![ast::Statement::Agent(agent_stmt.clone())],
                        };
                        // SPEC_28 — provider credentials for this run,
                        // resolved from the AGENT's owning principal.
                        //
                        // Note this differs from `user_secrets` below,
                        // which is keyed on the *caller* (`user_id2`).
                        // Model keys must follow the agent's owner, not
                        // whoever @-mentioned it, or an invoker would
                        // unknowingly pay for someone else's agent.
                        let credentials =
                            crate::build_execution_credentials(&state2, &db_agent, &card).await;

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

                        // Resolve user secrets for this agent
                        let user_secrets = if let Some(ref encryptor) = state2.secret_encryptor {
                            fermi_auth::get_secrets_for_agent(
                                &state2.db, encryptor, &user_id2, &agent_name2,
                            )
                            .await
                            .ok()
                            .filter(|s| !s.is_empty())
                        } else {
                            None
                        };

                        // Check for missing required secrets
                        if let Some(ref req_secrets) = db_agent.requires_secrets {
                            if let Ok(requirements) = serde_json::from_value::<Vec<serde_json::Value>>(req_secrets.clone()) {
                                let resolved = user_secrets.as_ref();
                                let missing: Vec<String> = requirements.iter()
                                    .filter(|r| r.get("is_required").and_then(|v| v.as_bool()).unwrap_or(true))
                                    .filter(|r| {
                                        let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                        !resolved.map_or(false, |s| s.contains_key(name))
                                    })
                                    .filter_map(|r| r.get("label").and_then(|v| v.as_str()).map(|s| s.to_string()))
                                    .collect();

                                if !missing.is_empty() {
                                    let hint = format!(
                                        "@{} needs credentials to function: {}. Go to Profile > Connections to add them.",
                                        agent_name2,
                                        missing.join(", ")
                                    );
                                    // Store as system message in workspace
                                    let _ = sqlx::query(
                                        "INSERT INTO workspace_messages (workspace_id, sender_type, sender_id, sender_name, content, message_type, metadata)
                                         VALUES ($1, 'system', 'system', 'System', $2, 'system', '{}'::jsonb)"
                                    )
                                    .bind(ws_uuid2)
                                    .bind(&hint)
                                    .execute(&state2.db)
                                    .await;
                                }
                            }
                        }

                        // Minted ahead of the tool context: the episode is
                        // stored further down, but delegated children need the
                        // id from inside the tool loop (mig-198).
                        let episode_id = uuid::Uuid::new_v4();

                        // Use ToolAwareExecutor with workspace tools
                        let tool_context = Arc::new(ToolContext {
                            // Root of this execution's delegation tree (mig-198).
                            parent_episode_id: Some(episode_id),
                            memory_store: state2.memory_store.clone(),
                            embedder: state2.embedder.clone(),
                            registry: state2.registry.clone(),
                            current_agent_id: Some(db_agent.agent_id),
                            workspace_id: Some(ws_uuid2),
                            workspace_slug: Some(slug.clone()),
                            workspace_git: Some(state2.workspace_git.clone()),
                            db: Some(state2.db.clone()),
                            gas_fees: Some(state2.gas_fees.clone()),
                            user_id: Some(user_id2.clone()),
                            user_secrets,
                            credentials,
                            eval_trigger: Some(Arc::new(
                                crate::handlers::eval::EvalTriggerImpl {
                                    state: state2.clone(),
                                },
                            )),
                            remote_mcp: None,
                        });
                        let tool_executor = ToolAwareExecutor::new(
                            state2.registry.executor_arc(),
                            ToolRegistry::with_workspace(),
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

                        // Record stats
                        let _ = state2.registry.record_execution(&agent_name2, &output);

                        // Store episode with Spec 22 provenance
                        let mut episode =
                            agent_output_to_episode(db_agent.agent_id, &query2, &output);
                        // Use the id advertised to the tool context, so children
                        // that stamped it as their parent resolve to this row.
                        episode.episode_id = episode_id;
                        // Stamp the (agent, human) dyad from the message sender so
                        // workspace conversations feed the companion loop.
                        let dyad_id =
                            agent_bestiary_memory::dyad_id(db_agent.agent_id, &user_id2);
                        episode.dyad_id = Some(dyad_id.clone());
                        // Persona version — see execution.rs. Without it the
                        // observability worker skips the entry entirely.
                        episode.persona_version_at_write =
                            Some(db_agent.persona_version);
                        crate::spawn_dyad_observation(
                            &state2,
                            db_agent.agent_id,
                            dyad_id,
                            &query2,
                            &output,
                        );
                        let embed_text = format!(
                            "{} {}",
                            query2,
                            output.metadata.reasoning.as_deref().unwrap_or("")
                        );
                        let t_embed = tokio::time::Instant::now();
                        let provenance =
                            match state2.embedder.generate_provenanced(&embed_text).await {
                                Ok(p) => {
                                    tracing::info!(
                                        elapsed_ms = t_embed.elapsed().as_millis() as u64,
                                        model = %p.model_id,
                                        site = "workspace_message_at_mention",
                                        "embed_call"
                                    );
                                    Some(p)
                                }
                                Err(_) => None,
                            };
                        let source_ref = serde_json::json!({
                            "kind": "workspace_message_at_mention",
                            "agent_id": db_agent.agent_id,
                            "workspace_id": ws_uuid2,
                        });
                        // Counted. Note what happens next if this fails: the
                        // timeline write below references this episode's id, and
                        // `agent_timeline_entries.episode_id` is a foreign key,
                        // so one lost episode silently costs two loop sinks.
                        // `execution_stream` guards its equivalent spawn on the
                        // episode having landed; this site and
                        // `rabble_workspace` do not. Instrumented first so the
                        // guard can be shown to have changed something.
                        let stored = fermi::write_accounting::observe(
                            fermi::write_accounting::Sink::Episodes,
                            state2
                                .memory_store
                                .store_episode_with_provenance(
                                    episode.clone(),
                                    provenance.as_ref(),
                                    Some(source_ref),
                                )
                                .await,
                        );


                        // Guarded on the episode having landed.
                        //
                        // `agent_timeline_entries.episode_id` is a foreign key
                        // to the row above. When that write failed — swallowed,
                        // as this one is — the timeline write was attempted
                        // anyway, violated the key, and was swallowed in turn:
                        // one failure, two loop sinks lost, no signal anywhere.
                        // `execution_stream` has always guarded its equivalent
                        // spawn; this site and `rabble_workspace` did not.
                        if stored.is_some() {
                        // Make this turn visible to drift + anomaly detection.
                        crate::handlers::live_observability::spawn_live_observation(
                            &state2,
                            crate::handlers::live_observability::LiveObservation {
                                episode,
                                agent: db_agent.clone(),
                                response: output
                                    .metadata
                                    .reasoning
                                    .clone()
                                    .unwrap_or_default(),
                                session_id: Some("live:workspace".to_string()),
                                rupture_detected: false,
                            },
                        );
                        }

                        // Charge execution gas from workspace wallet
                        let tokens = output.tokens_used.unwrap_or(0) as i32;
                        let (exec_fee, gas_fee) = state2.gas_fees.execution_fee(tokens);
                        let total = exec_fee + gas_fee;
                        let _ = charge_workspace_gas(
                            &state2.db,
                            ws_uuid2,
                            &ws_id,
                            total,
                            "execution_fee",
                            &format!("@{} execution ({}tk)", agent_name2, tokens),
                            None,
                        )
                        .await;

                        // Auto-commit ontology snapshot to workspace repo
                        if !slug.is_empty() {
                            if let Ok(snapshot) = sqlx::query(
                                "SELECT version, mermaid_content, dream_synopsis FROM ontology_snapshots
                                 WHERE agent_id = $1 ORDER BY created_at DESC LIMIT 1"
                            )
                            .bind(db_agent.agent_id)
                            .fetch_optional(&state2.db)
                            .await
                            {
                                if let Some(snap) = snapshot {
                                    let version: i32 = snap.try_get("version").unwrap_or(0);
                                    let mermaid: Option<String> = snap.try_get("mermaid_content").unwrap_or(None);
                                    let synopsis: Option<String> = snap.try_get("dream_synopsis").unwrap_or(None);
                                    let content = format!(
                                        "# Ontology Snapshot v{}\n\n{}\n\n{}",
                                        version,
                                        synopsis.as_deref().unwrap_or(""),
                                        mermaid.as_deref().unwrap_or("(no diagram)")
                                    );
                                    let path = format!("ontology/{}/snapshot_v{}.md", agent_name2, version);
                                    let _ = state2.workspace_git.commit_file(
                                        &slug, &path, &content,
                                        &format!("Ontology snapshot v{} for {}", version, agent_name2),
                                    );
                                }
                            }
                        }

                        // Hand the agent UUID back out to the surrounding
                        // scope so the result-message construction can resolve
                        // the current version (Doc 12 § Capability 2).
                        // `episode_id` rides out too, so the result message can
                        // name the artifact this hop carried (migration 222).
                        Ok::<_, (StatusCode, String)>((output, db_agent.agent_id, episode_id))
                    }
                    .await;

                    // Doc 12 § Capability 2 — agent version stamp on the
                    // execution_result. Resolved once, after the executor
                    // returns, by looking up MAX(version_number) for this
                    // agent. Best-effort: if the lookup fails or the agent
                    // has no version history, the keys are present but null.
                    let agent_uuid_opt: Option<uuid::Uuid> =
                        result.as_ref().ok().map(|(_, id, _)| *id);

                    // The join (migration 222), extracted the same way and for
                    // the same reason. `None` on the error path is correct: the
                    // executor failed before persisting an episode, so there is
                    // no artifact for this arrow to point at.
                    let episode_id_opt: Option<uuid::Uuid> =
                        result.as_ref().ok().map(|(_, _, eid)| *eid);
                    let (av_id, av_num): (Option<uuid::Uuid>, Option<i32>) = match agent_uuid_opt {
                        Some(agent_uuid) => state2
                            .memory_store
                            .get_current_agent_version(agent_uuid)
                            .await
                            .ok()
                            .flatten()
                            .map(|v| (Some(v.version_id), Some(v.version_number)))
                            .unwrap_or((None, None)),
                        None => (None, None),
                    };

                    // Post result message. See `format_execution_result_content`
                    // below for content policy (issue #2 / Doc 09).
                    //
                    // Observability fields plumbed from AgentMetadata:
                    //   - stop_reason     — LLM-reported finish state
                    //   - resolved_model  — model actually used (not card-declared)
                    //   - provider        — provider actually called
                    //   - failure_reason  — set when the executor decided the run
                    //                       did not produce real output (issue #3)
                    //   - loop_iterations — tool-loop iteration count
                    //   - agent_version_{id,number} — Doc 12 § Capability 2
                    let (content, metadata, msg_type) = match result {
                        Ok((output, _agent_uuid, _episode_id)) => {
                            let raw_response =
                                output.metadata.reasoning.clone().unwrap_or_default();
                            let evidence_summaries: Vec<&str> = output
                                .evidence
                                .iter()
                                .map(|e| e.summary.as_deref().unwrap_or(""))
                                .collect();
                            let content =
                                format_execution_result_content(&raw_response, &evidence_summaries);
                            let meta = json!({
                                "agent_name": agent_name2,
                                "confidence": output.confidence,
                                "execution_time_ms": output.execution_time_ms,
                                "tokens_used": output.tokens_used,
                                "status": format!("{:?}", output.status),
                                "evidence_count": output.evidence.len(),
                                // Verbatim LLM output for machine consumers — kask's
                                // `_extractBomItems`, comparator narrative readers,
                                // etc. — so they don't have to re-parse the markdown.
                                "raw_response": raw_response,
                                // Observability (issue #3 / Doc 10)
                                "stop_reason": output.metadata.stop_reason,
                                "resolved_model": output.metadata.model_used,
                                "provider": output.metadata.provider,
                                "failure_reason": output.metadata.failure_reason,
                                "loop_iterations": output.loop_iterations,
                                // Agent version (issue #5 / Doc 12)
                                "agent_version_id": av_id,
                                "agent_version_number": av_num,
                            });
                            (content, meta, "execution_result".to_string())
                        }
                        Err((_status, err_msg)) => (
                            format!("Execution failed: {}", err_msg),
                            json!({
                                "agent_name": agent_name2,
                                "error": true,
                                "agent_version_id": av_id,
                                "agent_version_number": av_num,
                            }),
                            "execution_result".to_string(),
                        ),
                    };

                    let result_msg = WorkspaceMessage {
                        message_id: uuid::Uuid::new_v4(),
                        workspace_id: ws_uuid2,
                        sender_type: "agent".to_string(),
                        sender_id: agent_name2.clone(),
                        sender_name: Some(display2),
                        content,
                        message_type: msg_type,
                        metadata,
                        created_at: chrono::Utc::now(),
                        // The join, and the only site that carries a real one
                        // (migration 222). The id is minted before the tool
                        // context and is what the episode is stored under, so
                        // this arrow in the workflow diagram can be joined to the
                        // belt the gates drew for that same artifact.
                        //
                        // `None` when the executor failed before persisting an
                        // episode. That is a real distinction and not a lost
                        // reference: there is no artifact for the arrow to point
                        // at, which is different from an artifact whose join was
                        // never written.
                        episode_id: episode_id_opt,
                    };
                    let _ = state2
                        .memory_store
                        .store_workspace_message(&result_msg)
                        .await;
                    broadcast_message(&state2, ws_uuid2, &message_to_json(&result_msg));
                });
            } else {
                // Agent not in workspace — post system error
                let err_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "system".to_string(),
                    sender_name: Some("System".to_string()),
                    content: format!("Agent '{}' is not in this workspace. Use Hire or Add to bring them in first.", target_agent),
                    message_type: "system_event".to_string(),
                    metadata: json!({}),
                    created_at: chrono::Utc::now(),
                    episode_id: None,
                };
                let _ = state.memory_store.store_workspace_message(&err_msg).await;
                broadcast_message(&state, ws_uuid, &message_to_json(&err_msg));
            }
        }
    }

    // Auto-evaluate coherence every N messages (background, best-effort)
    let auto_eval_interval: i64 = std::env::var("COHERENCE_AUTO_EVAL_INTERVAL")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let store = state.memory_store.clone();
    let broadcast_tx = state.ws_broadcast.clone();
    tokio::spawn(async move {
        // Check last eval time
        let since = match store.get_latest_coherence(ws_uuid).await {
            Ok(Some(e)) => e.created_at,
            _ => chrono::DateTime::<chrono::Utc>::MIN_UTC,
        };
        let count = store
            .count_workspace_messages_since(ws_uuid, since)
            .await
            .unwrap_or(0);
        if count >= auto_eval_interval {
            // Run coherence evaluation (no gas charge for auto-eval)
            let messages = match store.get_workspace_messages(ws_uuid, 50, None).await {
                Ok(m) => m,
                Err(_) => return,
            };
            if messages.is_empty() {
                return;
            }
            let conv_id = ConversationId(ws_uuid);
            let coherence_msgs: Vec<CoherenceMessage> = messages
                .iter()
                .rev()
                .map(|m| {
                    let pid = ParticipantId(
                        uuid::Uuid::parse_str(&m.sender_id)
                            .unwrap_or_else(|_| uuid::Uuid::new_v4()),
                    );
                    CoherenceMessage::new(pid, &m.content)
                })
                .collect();

            let observer = ConversationObserver::new(conv_id);
            let mut system = observer.observe(&coherence_msgs);
            let engine = SettlingEngine::with_defaults();
            engine.settle(&mut system);
            let snapshot = system.snapshot();

            let principle_scores =
                serde_json::to_value(&snapshot.principle_scores).unwrap_or(json!({}));
            // Same classification as the on-demand path. This is the one that
            // runs automatically, so if only one of the two recorded the
            // incoherence type, the trend would be built from whichever
            // sessions someone happened to evaluate by hand.
            let assessment = coherence_core::classify_incoherence(
                &snapshot.principle_scores,
                snapshot.global_coherence.score,
            );
            let health_indicators = json!({
                "feedback_action": serde_json::to_value(&snapshot.feedback_action).unwrap_or(json!("unknown")),
                "converged": snapshot.global_coherence.converged,
                "accepted_count": snapshot.global_coherence.accepted_count,
                "rejected_count": snapshot.global_coherence.rejected_count,
                "incoherence_type": assessment.incoherence_type.as_str(),
                "tension_band": assessment.band.as_str(),
                "productive": assessment.incoherence_type.is_productive(),
                "should_remedy": assessment.incoherence_type.should_remedy(),
                "homophily_risk": assessment.homophily_risk,
                "incoherence_rationale": assessment.rationale,
            });

            let eval = CoherenceEvaluation {
                eval_id: uuid::Uuid::new_v4(),
                workspace_id: ws_uuid,
                global_score: snapshot.global_coherence.score,
                quality_label: snapshot.global_coherence.quality_label().to_string(),
                principle_scores: principle_scores.clone(),
                health_indicators: health_indicators.clone(),
                utterance_count: snapshot.utterance_stats.total as i32,
                message_window: Some(json!({
                    "message_count": messages.len(),
                    "auto": true,
                })),
                created_at: chrono::Utc::now(),
            };

            // The `Err` was not bound at all here, so a failed automatic
            // evaluation left no trace of any kind — while the on-demand twin in
            // `workspace::coherence` propagates. Loop 3's trend was therefore
            // built from whichever of the two happened to succeed.
            let stored = fermi::write_accounting::observe(
                fermi::write_accounting::Sink::CoherenceEvaluations,
                store.store_coherence_evaluation(&eval).await,
            );
            if let Some(eval_id) = stored {
                let update_msg = WorkspaceMessage {
                    message_id: uuid::Uuid::new_v4(),
                    workspace_id: ws_uuid,
                    sender_type: "system".to_string(),
                    sender_id: "coherence_evaluator".to_string(),
                    sender_name: Some("Coherence Evaluator".to_string()),
                    content: format!(
                        "Coherence: {:.0}% ({}) | {} utterances",
                        eval.global_score * 100.0,
                        eval.quality_label,
                        eval.utterance_count,
                    ),
                    message_type: "coherence_update".to_string(),
                    metadata: json!({
                        "eval_id": eval_id,
                        "global_score": eval.global_score,
                        "quality_label": eval.quality_label,
                        "auto": true,
                    }),
                    created_at: chrono::Utc::now(),
                    episode_id: None,
                };
                let _ = store.store_workspace_message(&update_msg).await;
                let _ = broadcast_tx.send(crate::WorkspaceEvent {
                    workspace_id: ws_uuid,
                    message: message_to_json(&update_msg),
                });
            }
        }
    });

    Ok(Json(json!({
        "message_id": msg_id,
        "sender_type": msg.sender_type,
        "sender_id": msg.sender_id,
        "content": msg.content,
        "message_type": msg.message_type,
        "created_at": msg.created_at,
    })))
}

#[derive(Debug, Deserialize)]
pub struct MessageQuery {
    limit: Option<i64>,
    before: Option<String>,
}

pub async fn get_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<MessageQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let limit = params.limit.unwrap_or(50).min(200);
    let before = params.before.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });

    let messages = state
        .memory_store
        .get_workspace_messages(ws_uuid, limit, before)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

#[derive(Debug, Deserialize)]
pub struct PollQuery {
    since: String,
}

pub async fn poll_workspace_messages_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<PollQuery>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let since = chrono::DateTime::parse_from_rfc3339(&params.since)
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "Invalid timestamp format".to_string(),
            )
        })?
        .with_timezone(&chrono::Utc);

    let messages = state
        .memory_store
        .get_workspace_messages_since(ws_uuid, since)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "message_id": m.message_id,
                "sender_type": m.sender_type,
                "sender_id": m.sender_id,
                "sender_name": m.sender_name,
                "content": m.content,
                "message_type": m.message_type,
                "metadata": m.metadata,
                "created_at": m.created_at,
            })
        })
        .collect();

    Ok(Json(json!({ "messages": msgs })))
}

// ─── SSE Stream ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    since: Option<String>,
}

pub async fn workspace_messages_stream_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Query(params): Query<StreamQuery>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)>
{
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Fetch missed messages if `since` is provided (reconnection catch-up)
    let backfill: Vec<Value> = if let Some(ref since_str) = params.since {
        if let Ok(since) = chrono::DateTime::parse_from_rfc3339(since_str) {
            let since_utc = since.with_timezone(&chrono::Utc);
            state
                .memory_store
                .get_workspace_messages_since(ws_uuid, since_utc)
                .await
                .unwrap_or_default()
                .iter()
                .map(message_to_json)
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let mut rx = state.ws_broadcast.subscribe();

    // Postgres LISTEN for cross-replica delivery.
    // Uses a dedicated connection (not the pool) because LISTEN is connection-scoped.
    let pg_channel = format!("ws_{}", ws_uuid.as_simple());
    let pg_listener = {
        let db_url = std::env::var("DATABASE_URL").unwrap_or_default();
        sqlx::postgres::PgListener::connect(&db_url).await.ok()
    };
    let mut pg_listener = if let Some(mut listener) = pg_listener {
        let _ = listener.listen(&pg_channel).await;
        Some(listener)
    } else {
        None
    };

    // Track message_ids seen to deduplicate in-process + pg_notify for same message.
    // In practice the same message will arrive on at most one channel per replica,
    // but guard against edge cases during deploy.
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    let stream = async_stream::stream! {
        // Send backfill messages first (catch-up on reconnect)
        for msg_json in backfill {
            if let Some(id) = msg_json.get("message_id").and_then(|v| v.as_str()) {
                seen_ids.insert(id.to_string());
            }
            let data = serde_json::to_string(&msg_json).unwrap_or_default();
            yield Ok(Event::default().data(data));
        }

        // Keepalive interval (30s) to prevent proxy timeouts
        let mut keepalive = tokio::time::interval(std::time::Duration::from_secs(30));
        keepalive.tick().await; // skip first immediate tick

        loop {
            // Build select branches depending on whether pg_listener is available
            if let Some(ref mut listener) = pg_listener {
                tokio::select! {
                    // In-process broadcast (same replica)
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                if event.workspace_id == ws_uuid {
                                    let id = event.message.get("message_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    if !id.is_empty() && seen_ids.contains(&id) {
                                        // Already delivered via pg_notify on this connection — skip
                                    } else {
                                        if !id.is_empty() { seen_ids.insert(id); }
                                        let data = serde_json::to_string(&event.message).unwrap_or_default();
                                        yield Ok(Event::default().data(data));
                                    }
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                yield Ok(Event::default().event("lagged").data("refetch"));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    // Postgres NOTIFY (cross-replica)
                    pg_result = listener.recv() => {
                        match pg_result {
                            Ok(notification) => {
                                let payload = notification.payload();
                                if let Ok(msg_json) = serde_json::from_str::<Value>(payload) {
                                    let id = msg_json.get("message_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    if !id.is_empty() && seen_ids.contains(&id) {
                                        // Already delivered via in-process broadcast — skip
                                    } else {
                                        if !id.is_empty() { seen_ids.insert(id); }
                                        yield Ok(Event::default().data(payload.to_string()));
                                    }
                                }
                            }
                            Err(_) => {
                                // LISTEN connection lost — fall back gracefully, keepalive continues
                            }
                        }
                    }
                    _ = keepalive.tick() => {
                        yield Ok(Event::default().comment("keepalive"));
                    }
                }
            } else {
                // No pg_listener available — in-process only (single replica mode)
                tokio::select! {
                    result = rx.recv() => {
                        match result {
                            Ok(event) => {
                                if event.workspace_id == ws_uuid {
                                    let data = serde_json::to_string(&event.message).unwrap_or_default();
                                    yield Ok(Event::default().data(data));
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                yield Ok(Event::default().event("lagged").data("refetch"));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                    _ = keepalive.tick() => {
                        yield Ok(Event::default().comment("keepalive"));
                    }
                }
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("keepalive"),
    ))
}

// ─── Workspace Hire / Add ──────────────────────────────────────────

/// Identifier accepted by /hire and /add — either a UUID (canonical) or
/// an agent_name handle (e.g. "supply_chain_oracle"). The string form
/// matches the rest of the public API (execute, message @mentions),
/// which means clients no longer need to do a catalog round-trip just
/// to invite a curated agent into a workspace.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AgentRef {
    Uuid(uuid::Uuid),
    Handle(String),
}

#[derive(Debug, Deserialize)]
pub struct HireAddRequest {
    agent_id: AgentRef,
    #[serde(default)]
    include_optional: bool,
}

impl HireAddRequest {
    /// Resolve the request's agent_id into a (uuid, Agent) pair. Handles
    /// both UUID and handle inputs.
    async fn resolve(&self, state: &AppState) -> Result<(uuid::Uuid, Agent), (StatusCode, String)> {
        match &self.agent_id {
            AgentRef::Uuid(u) => {
                let agent = state
                    .memory_store
                    .get_agent(*u)
                    .await
                    .map_err(|e| (StatusCode::NOT_FOUND, format!("Agent not found: {}", e)))?
                    .ok_or((StatusCode::NOT_FOUND, "Agent not found".to_string()))?;
                Ok((*u, agent))
            }
            AgentRef::Handle(h) => {
                let agent = state.memory_store.get_agent_by_name(h).await.map_err(|e| {
                    (
                        StatusCode::NOT_FOUND,
                        format!("Agent '{}' not found: {}", h, e),
                    )
                })?;
                Ok((agent.agent_id, agent))
            }
        }
    }
}

/// Post a system message to workspace chat (helper) + broadcast to SSE.
pub async fn post_system_message(state: &AppState, workspace_id: uuid::Uuid, content: &str) {
    let msg = WorkspaceMessage {
        message_id: uuid::Uuid::new_v4(),
        workspace_id,
        sender_type: "system".to_string(),
        sender_id: "system".to_string(),
        sender_name: None,
        content: content.to_string(),
        message_type: "system_event".to_string(),
        metadata: json!({}),
        created_at: chrono::Utc::now(),
        episode_id: None,
    };
    let _ = state.memory_store.store_workspace_message(&msg).await;
    broadcast_message(state, workspace_id, &message_to_json(&msg));
}

pub async fn hire_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be admin+ to hire
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;
    if !role.can_invite() {
        return Err((
            StatusCode::FORBIDDEN,
            "Admin role required to hire agents".to_string(),
        ));
    }

    // Resolve agent — accepts either UUID or handle.
    let (agent_uuid, agent) = req.resolve(&state).await?;

    // Must not own the agent (use /add for your own)
    if agent.owner_id.as_deref() == Some(&user_id) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Use /add for your own agents".to_string(),
        ));
    }

    // Agent must be public OR be a curated/system-tier agent. Curated
    // agents are platform-blessed and hireable by anyone regardless of
    // the visibility column (which historically defaults to 'private').
    let curated_or_system = agent.tier == "curated" || agent.tier == "system";
    if agent.visibility != "public" && !curated_or_system {
        return Err((StatusCode::FORBIDDEN, "Agent is not public".to_string()));
    }

    // Check if agent is already in workspace
    let already =
        sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
            .bind(ws_uuid)
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if already.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Agent is already in this workspace".to_string(),
        ));
    }

    // Charge hire gas from workspace wallet
    let agent_id_str = agent_uuid.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_hire,
        "gas_fee",
        &format!("Hire agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'hired') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(agent_uuid)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} hired {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "hired",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Hired agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    // ─── Auto-hire dependencies ───
    let card = resolve_agent_card(&state, &agent);
    let deps = &card.dependencies;
    let mut deps_hired: Vec<String> = Vec::new();
    let mut deps_gas: i32 = 0;

    // Collect dep names to hire: required always, optional if requested
    let mut dep_names: Vec<String> = deps.required.clone();
    if req.include_optional {
        dep_names.extend(deps.optional.clone());
    }

    for dep_name in &dep_names {
        // Resolve dep agent by name
        let dep_agent = match state.memory_store.get_agent_by_name(dep_name).await {
            Ok(a) => a,
            Err(_) => continue, // Skip if not found in DB
        };

        // Check if already in workspace
        let already =
            sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
                .bind(ws_uuid)
                .bind(dep_agent.agent_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None);

        if already.is_some() {
            continue; // Already hired
        }

        // Charge gas for dep hire
        let _ = charge_workspace_gas(
            &state.db,
            ws_uuid,
            &workspace_id,
            state.gas_fees.agent_hire,
            "gas_fee",
            &format!("Auto-hire dep {}", dep_name),
            None,
        )
        .await;

        // Insert workspace_agents row
        let _ = sqlx::query(
            "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'hired') ON CONFLICT DO NOTHING",
        )
        .bind(ws_uuid)
        .bind(dep_agent.agent_id)
        .bind(&user_id)
        .execute(&state.db)
        .await;

        deps_hired.push(dep_name.clone());
        deps_gas += state.gas_fees.agent_hire;
    }

    if !deps_hired.is_empty() {
        post_system_message(
            &state,
            ws_uuid,
            &format!("Auto-hired dependencies: {}", deps_hired.join(", ")),
        )
        .await;
    }

    // ─── Inject workflow scaffold from compound agent ───
    let mut scaffold_injected = false;
    if let Some(ref tmpl) = card.workflow_template {
        let existing: Option<String> =
            sqlx::query_scalar("SELECT workflow_mermaid FROM teams WHERE id = $1")
                .bind(ws_uuid)
                .fetch_optional(&state.db)
                .await
                .ok()
                .flatten();

        if existing.as_ref().map_or(true, |s| s.is_empty()) {
            let scaffold_meta = serde_json::json!({
                "source": "workflow_template",
                "compound_agent": agent.agent_name,
                "stages": tmpl.stages,
                "is_scaffold": true,
                "generated_at": chrono::Utc::now().to_rfc3339(),
            });
            let _ = sqlx::query(
                "UPDATE teams SET workflow_mermaid = $1, workflow_meta = $2 WHERE id = $3",
            )
            .bind(&tmpl.mermaid)
            .bind(&scaffold_meta)
            .bind(ws_uuid)
            .execute(&state.db)
            .await;
            scaffold_injected = true;
        }
    }

    Ok(Json(json!({
        "message": "Agent hired successfully",
        "agent_name": agent.agent_name,
        "relationship": "hired",
        "gas_charged": state.gas_fees.agent_hire + deps_gas,
        "dependencies_hired": deps_hired,
        "scaffold_injected": scaffold_injected,
    })))
}

pub async fn add_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<HireAddRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;

    // Must be workspace member
    let _role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    // Resolve agent — accepts either UUID or handle, must own it.
    let (agent_uuid, agent) = req.resolve(&state).await?;

    if agent.owner_id.as_deref() != Some(&user_id) {
        return Err((
            StatusCode::FORBIDDEN,
            "You don't own this agent. Use /hire instead.".to_string(),
        ));
    }

    // Check if agent is already in workspace
    let already =
        sqlx::query("SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
            .bind(ws_uuid)
            .bind(agent_uuid)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if already.is_some() {
        return Err((
            StatusCode::CONFLICT,
            "Agent is already in this workspace".to_string(),
        ));
    }

    // Charge add gas from workspace wallet
    let agent_id_str = agent_uuid.to_string();
    charge_workspace_gas(
        &state.db,
        ws_uuid,
        &workspace_id,
        state.gas_fees.agent_add,
        "gas_fee",
        &format!("Add agent {}", agent.agent_name),
        Some(&agent_id_str),
    )
    .await?;

    // Insert workspace_agents row
    sqlx::query(
        "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship) VALUES ($1, $2, $3, 'owned') ON CONFLICT DO NOTHING",
    )
    .bind(ws_uuid)
    .bind(agent_uuid)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} added {} to the workspace", user_id, agent.agent_name),
    )
    .await;

    // Auto-commit agent card to workspace git repo
    let wg = state.workspace_git.clone();
    let agent_name = agent.agent_name.clone();
    let db_clone = state.db.clone();
    tokio::spawn(async move {
        if let Ok(slug) = get_workspace_slug(&db_clone, ws_uuid).await {
            let card = serde_json::json!({
                "agent_name": agent_name,
                "relationship": "owned",
            });
            let _ = wg.commit_file(
                &slug,
                &format!("agents/{}.json", agent_name),
                &serde_json::to_string_pretty(&card).unwrap_or_default(),
                &format!("Added agent: {}", agent_name),
            );
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = COALESCE(git_latest_commit, ''), git_commit_count = git_commit_count + 1 WHERE id = $1",
            )
            .bind(ws_uuid)
            .execute(&db_clone)
            .await;
        }
    });

    Ok(Json(json!({
        "message": "Agent added successfully",
        "agent_name": agent.agent_name,
        "relationship": "owned",
        "gas_charged": state.gas_fees.agent_add,
    })))
}

pub async fn remove_workspace_agent_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((workspace_id, agent_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let ws_uuid: uuid::Uuid = workspace_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid workspace ID".to_string()))?;
    // Path param accepts either UUID or handle — mirror the /hire and
    // /add JSON-body behaviour so clients can speak a single identifier
    // form across the whole workspace agents API.
    let agent_uuid: uuid::Uuid = match agent_id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => state
            .memory_store
            .get_agent_by_name(&agent_id)
            .await
            .map(|a| a.agent_id)
            .map_err(|e| {
                (
                    StatusCode::NOT_FOUND,
                    format!("Agent '{}' not found: {}", agent_id, e),
                )
            })?,
    };

    // Must be admin+ or the person who added
    let role = teams::get_member_role(&state.db, ws_uuid, &user_id)
        .await
        .map_err(|_| (StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?
        .ok_or((StatusCode::FORBIDDEN, "Not a workspace member".to_string()))?;

    let row = sqlx::query(
        "SELECT added_by FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(ws_uuid)
    .bind(agent_uuid)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let row = row.ok_or((StatusCode::NOT_FOUND, "Agent not in workspace".to_string()))?;
    let added_by: String = row.try_get("added_by").unwrap_or_default();

    if added_by != user_id && !role.can_admin() {
        return Err((
            StatusCode::FORBIDDEN,
            "Must be admin or the person who added".to_string(),
        ));
    }

    sqlx::query("DELETE FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2")
        .bind(ws_uuid)
        .bind(agent_uuid)
        .execute(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    post_system_message(
        &state,
        ws_uuid,
        &format!("{} removed an agent from the workspace", user_id),
    )
    .await;

    Ok(Json(json!({ "message": "Agent removed from workspace" })))
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::format_execution_result_content;

    /// Regression for issue #2: when the LLM returns a real response,
    /// `content` must contain it verbatim — not a stripped Evidence template.
    #[test]
    fn raw_response_passes_through_when_evidence_summaries_empty() {
        let raw = r#"{"items":[{"sku":"X","qty":1}],"confidence":0.7}"#;
        // Empty / blank summaries — the JSON-contract case from issue #2.
        let summaries: Vec<&str> = vec!["", "   "];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(content, raw, "raw LLM response must pass through verbatim");
    }

    /// The exact byte pattern this code used to emit pre-fix.
    /// If we ever regress, this test catches it immediately.
    #[test]
    fn never_emits_legacy_empty_evidence_template() {
        let raw = "any non-empty LLM output";
        let summaries: Vec<&str> = vec!["", "   "];
        let content = format_execution_result_content(raw, &summaries);
        assert_ne!(content, "\n\n**Evidence:**\n- ");
        assert!(!content.is_empty());
        assert!(
            content.len() > 18,
            "post-fix content must carry actual signal, not the 18-byte artifact"
        );
    }

    /// Truly empty LLM output gets an honest placeholder, not the artifact.
    #[test]
    fn empty_llm_output_with_no_evidence_emits_placeholder() {
        let content = format_execution_result_content("", &[]);
        assert_eq!(content, "(agent returned no content)");
    }

    #[test]
    fn empty_llm_output_with_evidence_falls_back_to_evidence_block() {
        let summaries = vec!["finding A", "finding B"];
        let content = format_execution_result_content("", &summaries);
        assert_eq!(content, "**Evidence:**\n- finding A\n- finding B");
    }

    #[test]
    fn raw_response_with_non_empty_evidence_gets_evidence_addendum() {
        let raw = "Analysis paragraph.";
        let summaries = vec!["finding A", "finding B"];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(
            content,
            "Analysis paragraph.\n\n**Evidence:**\n- finding A\n- finding B"
        );
    }

    #[test]
    fn evidence_summaries_are_trimmed_and_blank_lines_dropped() {
        let raw = "Body.";
        let summaries = vec!["  finding A  ", "", "  ", "finding B"];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(content, "Body.\n\n**Evidence:**\n- finding A\n- finding B");
    }

    /// Specifically asserts that whatever this function returns is something
    /// kask's `_extractBomItems` (or any consumer) can plausibly parse —
    /// i.e. non-empty and not the literal pre-fix artifact.
    #[test]
    fn json_contract_response_remains_parseable() {
        let raw = r#"{"items":[{"sku":"ALU-001","qty":12.5}],"notes":"primary"}"#;
        let summaries: Vec<&str> = vec![]; // EvidenceJson default → empty vec
        let content = format_execution_result_content(raw, &summaries);
        assert!(content.contains("\"items\""));
        assert!(content.contains("ALU-001"));
    }

    // ─── Issue #4 — addendum must not duplicate the raw response ───────

    /// Regression for issue #4: the evidence parser used to stuff the entire
    /// LLM response into `summary` when it couldn't deserialise it as
    /// EvidenceData. The formatter would then emit the same JSON twice —
    /// once as the raw response, once as a `**Evidence:**` addendum bullet.
    /// Doubled token cost, broke greedy JSON extractors downstream.
    #[test]
    fn addendum_does_not_duplicate_raw_response_when_summary_equals_raw() {
        let raw = r#"{"items":[{"name":"Tea","unit_cost":42}],"total_bom_cost":42}"#;
        // The bug shape: evidence summary IS the full raw response.
        let summaries: Vec<&str> = vec![raw];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(
            content, raw,
            "addendum equal to raw response must be suppressed entirely"
        );
        assert_eq!(
            content.matches("\"items\"").count(),
            1,
            "primary JSON must appear exactly once"
        );
        assert!(
            !content.contains("**Evidence:**"),
            "no Evidence heading when the addendum would duplicate"
        );
    }

    /// Subset case: the summary is a non-trivial substring of the raw
    /// response (e.g. an extract or quote). Still skip — the substring is
    /// already in the primary content.
    #[test]
    fn addendum_suppressed_when_summary_is_substring_of_raw() {
        let raw = "Analysis paragraph mentioning the key driver of seasonality.";
        let summaries: Vec<&str> = vec!["key driver of seasonality"];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(content, raw);
        assert!(!content.contains("**Evidence:**"));
    }

    /// Mixed case: some summaries duplicate the raw, others add real signal.
    /// The duplicates are filtered, the unique ones survive in the addendum.
    #[test]
    fn mixed_summaries_filter_only_the_duplicates() {
        let raw = "Body mentions item A in detail.";
        let summaries: Vec<&str> = vec!["item A", "extra finding from external source"];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(
            content,
            "Body mentions item A in detail.\n\n**Evidence:**\n- extra finding from external source"
        );
    }

    /// The supply_chain_oracle reproduction from issue #4 — the same JSON
    /// must not appear twice in `content`.
    #[test]
    fn supply_chain_oracle_repro_does_not_double_json() {
        let raw = r#"```json
{
  "items": [
    {"name": "Tea", "unit_cost": 42, "unit": "kg"},
    {"name": "Raw Cane Sugar", "unit_cost": 1.2, "unit": "kg"}
  ],
  "risks": [],
  "total_bom_cost": 43.2,
  "oracle_note": "Mid-market pricing applied"
}
```"#;
        // The bug shape from production: summary == raw.
        let summaries: Vec<&str> = vec![raw];
        let content = format_execution_result_content(raw, &summaries);
        assert_eq!(
            content.matches("oracle_note").count(),
            1,
            "JSON must not be duplicated in content"
        );
        assert_eq!(
            content.matches("Raw Cane Sugar").count(),
            1,
            "items must not appear twice"
        );
    }
}
