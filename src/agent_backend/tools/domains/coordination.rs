// src/agent_backend/tools/domains/coordination.rs
//
// Phase 4 domain migration: Coordination tools.
//
// Nine tools (all requires_workspace: true):
//   declare_intention            — is_delegation: false (default)
//   solicit_agent_plan           — is_delegation: true
//   check_conflicts
//   get_intention_map
//   clear_intention
//   suggest_differentiation
//   emit_coherence_signal
//   record_coordination_observation
//   propose_composition_change
//
// Each is a zero-size struct implementing PlatformTool. execute() bodies are
// inlined verbatim from tools_legacy.rs — no dispatch through ToolRegistry.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use agent_bestiary_memory::embeddings::EmbeddingGenerator;

use crate::agent_backend::tools::helpers::resolve_agent_id;
use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Coordination-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(DeclareIntention),
        Arc::new(SolicitAgentPlan),
        Arc::new(CheckConflicts),
        Arc::new(GetIntentionMap),
        Arc::new(ClearIntention),
        Arc::new(SuggestDifferentiation),
        Arc::new(EmitCoherenceSignal),
        Arc::new(RecordCoordinationObservation),
        Arc::new(ProposeCompositionChange),
    ]
}

// ─── Private helpers ─────────────────────────────────────────────────────────

/// Load the workspace's active intention map, joined to agent names.
async fn load_intentions(
    db: &sqlx::PgPool,
    workspace_id: Uuid,
) -> Result<Vec<crate::intentions::Intention>, String> {
    let rows = sqlx::query(
        "SELECT i.intention_id, i.agent_id, a.agent_name, i.action_type, i.tool,
                i.description, i.targets, i.depends_on, i.embedding,
                i.source, i.declared_by
           FROM workspace_intentions i
           JOIN agents a ON a.agent_id = i.agent_id
          WHERE i.workspace_id = $1 AND i.status = 'active'
          ORDER BY i.declared_at",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map_err(|e| format!("Failed to load intentions: {e}"))?;

    Ok(rows
        .iter()
        .map(|r| crate::intentions::Intention {
            intention_id: r
                .try_get::<Uuid, _>("intention_id")
                .map(|u| u.to_string())
                .unwrap_or_default(),
            agent_id: r
                .try_get::<Uuid, _>("agent_id")
                .map(|u| u.to_string())
                .unwrap_or_default(),
            agent_name: r.try_get("agent_name").unwrap_or_default(),
            action_type: r.try_get("action_type").unwrap_or_default(),
            tool: r.try_get("tool").ok(),
            description: r.try_get("description").unwrap_or_default(),
            targets: r.try_get("targets").unwrap_or_default(),
            depends_on: r.try_get("depends_on").unwrap_or_default(),
            embedding: r
                .try_get::<Option<pgvector::Vector>, _>("embedding")
                .ok()
                .flatten()
                .map(|v| v.to_vec()),
            // A read failure lands on `Unattributed` rather than on a stronger
            // claim (mig-218). If the column is missing because the migration
            // has not run, every row reads as second-hand — which suppresses
            // duplication detection until it has, and that is the right way
            // round: no overlap warnings beats warnings we cannot vouch for.
            source: crate::intentions::IntentionSource::from_db(
                r.try_get::<String, _>("source")
                    .unwrap_or_default()
                    .as_str(),
            ),
            declared_by: r
                .try_get::<Option<Uuid>, _>("declared_by")
                .ok()
                .flatten()
                .map(|u| u.to_string()),
        })
        .collect())
}

/// Output names already completed in this workspace, so a `depends_on` entry
/// can be judged satisfied.
async fn produced_outputs(db: &sqlx::PgPool, workspace_id: Uuid) -> Vec<String> {
    sqlx::query_scalar::<_, Vec<String>>(
        "SELECT targets FROM workspace_intentions
          WHERE workspace_id = $1 AND status = 'completed'",
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await
    .map(|rows| rows.into_iter().flatten().collect())
    .unwrap_or_default()
}

fn intention_ctx(ctx: &ToolContext) -> Result<(Uuid, &sqlx::PgPool), String> {
    let ws = ctx
        .workspace_id
        .ok_or_else(|| "intention tools must be called inside a workspace".to_string())?;
    let db = ctx
        .db
        .as_ref()
        .ok_or_else(|| "intention tools require a database context".to_string())?;
    Ok((ws, db))
}

/// The one place an intention row is written.
///
/// Shared by `declare_intention` (a model choosing to register a plan) and
/// `solicit_agent_plan` (the platform recording an answer it asked for), so the
/// supersede-then-insert-then-check sequence has a single implementation and
/// the two paths cannot drift on provenance.
///
/// `source` is decided by the caller and never by the input, which is the whole
/// point of mig-218: a tool argument saying "this is the agent's own plan"
/// would be a claim the platform cannot check, made by the party with the most
/// reason to overstate it.
///
/// Takes explicit dependencies rather than a `&ToolContext` because
/// [`crate::plan_solicitation`] calls it too, and that module must not depend
/// on the tool layer's shape — the floor runs from an HTTP handler that builds
/// no `ToolContext` at all.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn write_intention(
    db: &sqlx::PgPool,
    embedder: &dyn EmbeddingGenerator,
    workspace_id: Uuid,
    agent_id: Uuid,
    declared_by: Option<Uuid>,
    action_type: &str,
    tool: Option<&str>,
    description: &str,
    targets: &[String],
    depends_on: &[String],
    source: crate::intentions::IntentionSource,
) -> Result<serde_json::Value, String> {
    if !matches!(
        action_type,
        "tool_call" | "research" | "synthesis" | "writing" | "review" | "idle"
    ) {
        return Err(format!("unknown action_type: {action_type}"));
    }

    // Embed the description so duplication detection is semantic. Populated
    // here, on the write path — not deferred to a worker that will not do it.
    let embedding = embedder
        .generate(description)
        .await
        .ok()
        .map(pgvector::Vector::from);
    if embedding.is_none() {
        tracing::warn!(
            %agent_id,
            "could not embed intention; duplication detection degrades to \
             resource and dependency signals for this declaration"
        );
    }

    // One live intention per agent: supersede the previous rather than
    // accumulating stale rows that generate phantom conflicts forever.
    sqlx::query(
        "UPDATE workspace_intentions
            SET status = 'superseded', resolved_at = NOW()
          WHERE workspace_id = $1 AND agent_id = $2 AND status = 'active'",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to supersede prior intention: {e}"))?;

    let intention_id: Uuid = sqlx::query_scalar(
        "INSERT INTO workspace_intentions
           (workspace_id, agent_id, action_type, tool, description,
            targets, depends_on, embedding, source, declared_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         RETURNING intention_id",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .bind(action_type)
    .bind(tool)
    .bind(description)
    .bind(targets)
    .bind(depends_on)
    .bind(embedding)
    .bind(source.as_str())
    .bind(declared_by)
    .fetch_one(db)
    .await
    .map_err(|e| format!("Failed to declare intention: {e}"))?;

    // Check immediately: an intention declared and not checked is the same as
    // no intention at all.
    let intentions = load_intentions(db, workspace_id).await?;
    let produced = produced_outputs(db, workspace_id).await;
    let conflicts = crate::intentions::detect_conflicts(
        &intentions,
        &produced,
        Some(
            &intentions
                .iter()
                .find(|i| i.intention_id == intention_id.to_string())
                .map(|i| i.agent_name.clone())
                .unwrap_or_default(),
        ),
    );
    let grounding = crate::intentions::Grounding::of(&intentions);

    Ok(json!({
        "intention_id": intention_id,
        "source": source.as_str(),
        "signal": crate::intentions::overall_signal(&conflicts),
        "conflicts": conflicts,
        "active_intentions": intentions.len(),
        // Reported on every write, not only on request. A CLEAR signal over a
        // map the team never confirmed is the reading most likely to be
        // mistaken for coordination, so the caveat travels with the signal.
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
    }))
}

// ─── Execute implementations ─────────────────────────────────────────────────

async fn execute_declare_intention(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;

    let action_type = input
        .get("action_type")
        .and_then(|v| v.as_str())
        .unwrap_or("research");
    let description = input
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "description is required".to_string())?;
    let tool = input.get("tool").and_then(|v| v.as_str());
    let str_list = |key: &str| -> Vec<String> {
        input
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let targets = str_list("targets");
    let depends_on = str_list("depends_on");

    // Provenance, derived and not asked for (mig-218).
    //
    // An agent registering its own next action is stating an intention. An
    // agent registering somebody else's is stating a belief about one, and
    // until now the two produced identical rows — which mattered because the
    // second case is the only one that has ever happened in production: the
    // strategist's Stage 0 declares on every member's behalf from a transcript.
    let source = match ctx.current_agent_id {
        Some(caller) if caller == agent_id => crate::intentions::IntentionSource::SelfDeclared,
        Some(_) => crate::intentions::IntentionSource::Inferred,
        // No caller identity: we cannot claim first-hand, so we do not.
        None => crate::intentions::IntentionSource::Unattributed,
    };

    let mut out = write_intention(
        db,
        ctx.embedder.as_ref(),
        workspace_id,
        agent_id,
        ctx.current_agent_id,
        action_type,
        tool,
        description,
        &targets,
        &depends_on,
        source,
    )
    .await?;

    if !source.is_first_hand() {
        out["note"] = json!(
            "Recorded as second-hand: you declared this for another agent, so it \
             is your reading of that agent's plan rather than its own statement. \
             Overlap detection between two second-hand rows is suppressed. Use \
             solicit_agent_plan to ask the agent directly and record what it \
             actually says."
        );
    }

    serde_json::to_string_pretty(&out).map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_solicit_agent_plan(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let target = resolve_agent_id(input, "agent_id", ctx).await?;

    let asker = crate::plan_solicitation::Asker {
        db: db.clone(),
        memory_store: ctx.memory_store.clone(),
        embedder: ctx.embedder.clone(),
        registry: ctx.registry.clone(),
        credentials: ctx.credentials.clone(),
    };

    // `freshness: None` — no staleness window on this path.
    //
    // The floor yields to a plan the member stated recently, because it is
    // spending an LLM call speculatively. A strategist that has read the map
    // and chosen to ask anyway has a reason the platform cannot see, and
    // overruling it here would make the tool weaker than the automatic
    // behaviour it is supposed to improve on.
    let outcome = crate::plan_solicitation::solicit(
        &asker,
        workspace_id,
        ctx.current_agent_id,
        target,
        input.get("context").and_then(|v| v.as_str()),
        None,
        ctx.parent_episode_id,
    )
    .await;

    use crate::plan_solicitation::Solicited;
    match outcome {
        Solicited::Recorded {
            intention_id,
            description,
            signal,
        } => {
            // Re-read for the caller-facing extras. `solicit` returns the
            // decision; the map view is this layer's job.
            let intentions = load_intentions(db, workspace_id).await?;
            let grounding = crate::intentions::Grounding::of(&intentions);
            serde_json::to_string_pretty(&json!({
                "intention_id": intention_id,
                "agent_id": target,
                "description": description,
                "source": "solicited",
                "signal": signal,
                "active_intentions": intentions.len(),
                "grounding": grounding,
                "grounding_reading": grounding.reading(),
                "note": "Recorded as this agent's own plan. Its view of who should \
                         own what is not written as anyone's intention — solicit those \
                         agents directly if you want their answer.",
            }))
            .map_err(|e| format!("Serialization error: {e}"))
        }
        Solicited::AlreadyFresh { source } => serde_json::to_string_pretty(&json!({
            "agent_id": target,
            "status": "already_fresh",
            "source": source.as_str(),
            "note": "This member already has a current first-hand plan; nothing was asked.",
        }))
        .map_err(|e| format!("Serialization error: {e}")),
        Solicited::NotAMember => Err(format!(
            "Agent {target} is not a member of this workspace; refusing to record \
             its plan in this workspace's intention map."
        )),
        Solicited::Unreachable { error } => Err(error),
        Solicited::Unparseable { reply_excerpt } => Err(format!(
            "That agent did not return a parseable plan, so nothing was recorded. \
             Its reply was: {reply_excerpt}"
        )),
    }
}

async fn execute_check_conflicts(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let intentions = load_intentions(db, workspace_id).await?;
    let produced = produced_outputs(db, workspace_id).await;

    // Optional filter, accepted as an agent name or id.
    let only = match input.get("agent_id").and_then(|v| v.as_str()) {
        Some(_) => resolve_agent_id(input, "agent_id", ctx)
            .await
            .ok()
            .and_then(|id| {
                intentions
                    .iter()
                    .find(|i| i.agent_id == id.to_string())
                    .map(|i| i.agent_name.clone())
            }),
        None => None,
    };

    let conflicts = crate::intentions::detect_conflicts(&intentions, &produced, only.as_deref());
    let grounding = crate::intentions::Grounding::of(&intentions);
    serde_json::to_string_pretty(&json!({
        "signal": crate::intentions::overall_signal(&conflicts),
        "conflicts": conflicts,
        "checked": intentions.len(),
        // A CLEAR signal means two different things depending on this, and
        // until mig-218 the caller could not tell them apart: "the team's
        // stated plans do not collide" versus "nobody has stated a plan and
        // the map is your own reading of a transcript".
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
        "note": if intentions.iter().any(|i| i.embedding.is_none()) {
            Some("Some intentions carry no embedding; duplication detection is \
                  incomplete for those. Resource and dependency signals are unaffected.")
        } else { None },
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_get_intention_map(ctx: &ToolContext) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let intentions = load_intentions(db, workspace_id).await?;
    let entries: Vec<serde_json::Value> = intentions
        .iter()
        .map(|i| {
            json!({
                "agent": i.agent_name,
                "action_type": i.action_type,
                "tool": i.tool,
                "description": i.description,
                "targets": i.targets,
                "depends_on": i.depends_on,
                "has_embedding": i.embedding.is_some(),
                // Whose plan this is, and who said so (mig-218). Without these
                // a map the coordinator wrote entirely by itself is
                // indistinguishable from one the team filled in.
                "source": i.source.as_str(),
                "first_hand": i.source.is_first_hand(),
                "declared_by": i.declared_by,
            })
        })
        .collect();
    let grounding = crate::intentions::Grounding::of(&intentions);
    serde_json::to_string_pretty(&json!({
        "workspace_id": workspace_id,
        "active": entries.len(),
        "intentions": entries,
        "grounding": grounding,
        "grounding_reading": grounding.reading(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_clear_intention(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let agent_id = resolve_agent_id(input, "agent_id", ctx).await?;
    let status = input
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("completed");
    if !matches!(status, "completed" | "cancelled" | "superseded") {
        return Err(format!("unknown status: {status}"));
    }

    let n = sqlx::query(
        "UPDATE workspace_intentions
            SET status = $3, resolved_at = NOW()
          WHERE workspace_id = $1 AND agent_id = $2 AND status = 'active'",
    )
    .bind(workspace_id)
    .bind(agent_id)
    .bind(status)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to clear intention: {e}"))?
    .rows_affected();

    serde_json::to_string_pretty(&json!({
        "cleared": n,
        "status": status,
        // `completed` intentions' targets become satisfied dependencies for
        // everyone else, so clearing is what unblocks a DEPENDENCY_WAIT.
        "note": if status == "completed" {
            "Targets of this intention now count as produced outputs."
        } else {
            "Removed from conflict checks without marking its targets produced."
        },
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_suggest_differentiation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let a_name = input
        .get("agent_a")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent_a is required".to_string())?;
    let b_name = input
        .get("agent_b")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent_b is required".to_string())?;

    let intentions = load_intentions(db, workspace_id).await?;
    let find = |n: &str| intentions.iter().find(|i| i.agent_name == n);
    let (Some(a), Some(b)) = (find(a_name), find(b_name)) else {
        return Err(format!(
            "both agents must have an active intention; have: {}",
            intentions
                .iter()
                .map(|i| i.agent_name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };

    // Report the overlap; do not prescribe the split.
    //
    // The card's own constraint is "structural, not prescriptive: name the
    // pattern, do not prescribe the fix", and it is right for a reason beyond
    // style — this tool has the two descriptions and nothing about the
    // workspace's goal, so any concrete division of labour it invented would be
    // a guess dressed as advice. The agents have the context; give them the
    // facts.
    let shared_targets: Vec<&String> = a.targets.iter().filter(|t| b.targets.contains(t)).collect();
    let similarity = match (&a.embedding, &b.embedding) {
        (Some(_), Some(_)) => {
            crate::intentions::detect_conflicts(&[a.clone(), b.clone()], &[], None)
                .into_iter()
                .find_map(|c| match c {
                    crate::intentions::Conflict::Duplication { similarity, .. } => Some(similarity),
                    _ => None,
                })
        }
        _ => None,
    };

    serde_json::to_string_pretty(&json!({
        "agent_a": {
            "name": a.agent_name, "intent": a.description, "targets": a.targets,
            "source": a.source.as_str(), "first_hand": a.source.is_first_hand(),
        },
        "agent_b": {
            "name": b.agent_name, "intent": b.description, "targets": b.targets,
            "source": b.source.as_str(), "first_hand": b.source.is_first_hand(),
        },
        "shared_targets": shared_targets,
        "description_similarity": similarity,
        // The caveat has to travel with the suggestion. Telling two agents to
        // divide work on the strength of two sentences the coordinator wrote
        // about them is the failure mode this whole column exists to name.
        "grounding_caveat": match (a.source.is_first_hand(), b.source.is_first_hand()) {
            (true, true) => None,
            (false, false) => Some(
                "NEITHER intention is first-hand. Both descriptions are your own \
                 reading, so their similarity measures your paraphrasing and not \
                 these agents' plans. Solicit both plans before asking anyone to \
                 differentiate."
            ),
            _ => Some(
                "One of these intentions is your inference rather than the \
                 agent's own statement. Say which when you raise the overlap."
            ),
        },
        "guidance": "These two intentions overlap on the axes above. Decide the                      split yourselves — you have the workspace goal and this tool                      does not. State the division explicitly in the conversation                      so the other agent can rely on it.",
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_emit_coherence_signal(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let (workspace_id, db) = intention_ctx(ctx)?;
    let relation_type = input
        .get("relation_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !matches!(relation_type, "IntentionAligns" | "IntentionConflicts") {
        return Err("relation_type must be IntentionAligns or IntentionConflicts".to_string());
    }
    let strength = input
        .get("strength")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let rationale = input.get("rationale").and_then(|v| v.as_str());

    let resolve = |key: &'static str| async move {
        let v = input.get(key).and_then(|x| x.as_str()).unwrap_or("");
        sqlx::query_scalar::<_, Uuid>("SELECT agent_id FROM agents WHERE agent_name = $1")
            .bind(v)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| format!("{key} does not name a known agent: {v}"))
    };
    let agent_a = resolve("agent_a").await?;
    let agent_b = resolve("agent_b").await?;

    sqlx::query(
        "INSERT INTO workspace_intention_signals
           (workspace_id, relation_type, agent_a, agent_b, strength, rationale)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(workspace_id)
    .bind(relation_type)
    .bind(agent_a)
    .bind(agent_b)
    .bind(strength)
    .bind(rationale)
    .execute(db)
    .await
    .map_err(|e| format!("Failed to record signal: {e}"))?;

    // Post it into the conversation as well, because that is what actually
    // reaches coherence: `ConversationObserver::observe` builds the TEC graph
    // from workspace messages. A row in a table nothing reads would be the
    // deferred-work pattern again.
    let a_name = input.get("agent_a").and_then(|v| v.as_str()).unwrap_or("?");
    let b_name = input.get("agent_b").and_then(|v| v.as_str()).unwrap_or("?");
    let body = match rationale {
        Some(r) => {
            format!("**{relation_type}** — {a_name} ↔ {b_name} (strength {strength:.2}): {r}")
        }
        None => format!("**{relation_type}** — {a_name} ↔ {b_name} (strength {strength:.2})"),
    };
    let posted = sqlx::query(
        "INSERT INTO workspace_messages
           (message_id, workspace_id, sender_type, sender_id, sender_name, content, message_type)
         VALUES (gen_random_uuid(), $1, 'system', 'intention_coordinator',
                 'Intention Coordinator', $2, 'intention_signal')",
    )
    .bind(workspace_id)
    .bind(&body)
    .execute(db)
    .await;
    if let Err(e) = &posted {
        tracing::warn!(error = %e, "intention signal recorded but not posted to the conversation");
    }

    serde_json::to_string_pretty(&json!({
        "relation_type": relation_type,
        "strength": strength,
        "recorded": true,
        "posted_to_conversation": posted.is_ok(),
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

async fn execute_record_coordination_observation(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or_else(|| {
        "record_coordination_observation must be called inside a workspace".to_string()
    })?;
    let db = ctx
        .db
        .as_ref()
        .ok_or_else(|| "record_coordination_observation requires a database context".to_string())?;
    let caller = ctx
        .current_agent_id
        .ok_or_else(|| "caller identity unavailable".to_string())?;

    // Gate 1 — only this workspace's coordination strategist.
    let strategist: Option<Uuid> =
        sqlx::query_scalar("SELECT coordination_strategist_id FROM teams WHERE id = $1")
            .bind(workspace_id)
            .fetch_optional(db)
            .await
            .map_err(|e| format!("Failed to read workspace: {e}"))?
            .flatten();
    if strategist != Some(caller) {
        return Err(
            "Only the workspace's coordination strategist may write coordination \
             observations into member memory."
                .to_string(),
        );
    }

    let target = resolve_agent_id(input, "agent_id", ctx).await?;

    // Gate 2 — target must be a member of this workspace.
    let observation = input
        .get("observation")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "observation is required".to_string())?;
    let session_summary = input
        .get("session_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // The episode write itself lives in `fermi::coordination_note`, shared with
    // the platform-side delivery in `handlers::workspace::coherence`. One
    // implementation, per §3.4 — and the reason there are two callers at all is
    // that this one, the model-invoked one, produced 0 of 3,576 episodes for the
    // life of the feature. The platform now delivers the brief as a floor and
    // this remains the better path: a note targeted at one member about its own
    // behaviour.
    //
    // `since: None` — a targeted note is never a duplicate of itself. The
    // duplicate check exists so the platform's generic delivery yields to this
    // call, not the other way round.
    let delivery = crate::coordination_note::deliver(
        db,
        &ctx.memory_store,
        ctx.embedder.as_ref(),
        workspace_id,
        caller,
        target,
        observation,
        session_summary,
        None,
    )
    .await;

    match delivery {
        crate::coordination_note::Delivery::Written { episode_id } => {
            serde_json::to_string_pretty(&json!({
                "episode_id": episode_id,
                "agent_id": target,
                "workspace_id": workspace_id,
                "status": "recorded",
                "message": "Observation written to the member's episodic memory. It will be \
                            consolidated into a semantic rule on that agent's next dreaming cycle.",
            }))
            .map_err(|e| format!("Serialization error: {e}"))
        }
        crate::coordination_note::Delivery::NotAMember => Err(format!(
            "Agent {target} is not a member of this workspace; refusing to write \
             into its memory."
        )),
        crate::coordination_note::Delivery::AlreadyTargeted => Err(
            "A coordination observation for this member already exists for this \
             run. Unreachable from this path, which passes no cutoff."
                .to_string(),
        ),
        crate::coordination_note::Delivery::Failed { error } => {
            Err(format!("Failed to write observation: {error}"))
        }
    }
}

async fn execute_propose_composition_change(
    input: &serde_json::Value,
    ctx: &ToolContext,
) -> Result<String, String> {
    let workspace_id = ctx.workspace_id.ok_or_else(|| {
        "propose_composition_change must be called inside a workspace".to_string()
    })?;

    let diff_summary = input
        .get("diff_summary")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "diff_summary is required".to_string())?;
    let rationale = input
        .get("rationale")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "rationale is required".to_string())?;
    let homophily = input
        .get("homophily_detected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Same prefix convention the HTTP proposal route uses, so the owner sees
    // one vocabulary regardless of which path raised the proposal.
    let summary = if homophily {
        format!("[homophily detected] {diff_summary}")
    } else {
        diff_summary.to_string()
    };

    let version = agent_bestiary_memory::CompositionVersion {
        composition_version_id: Uuid::new_v4(),
        workspace_id,
        version_number: 0, // assigned by create_composition_version
        mission: None,
        coordination_strategist_id: ctx.current_agent_id,
        member_agent_ids: None,
        member_weights: None,
        diff_summary: Some(summary),
        proposed_by: Some("cohere_and_coordinate".to_string()),
        accepted_by: None,
        rejected_by: None,
        rejection_note: Some(rationale.to_string()),
        created_at: chrono::Utc::now(),
    };

    let version_id = ctx
        .memory_store
        .create_composition_version(&version)
        .await
        .map_err(|e| format!("Failed to create composition version: {e}"))?;

    serde_json::to_string_pretty(&json!({
        "version_id": version_id,
        "workspace_id": workspace_id,
        "status": "pending",
        "message": "Composition change proposed — the workspace owner must accept or reject it.",
    }))
    .map_err(|e| format!("Serialization error: {e}"))
}

// ─── declare_intention ────────────────────────────────────────────────────────

struct DeclareIntention;

#[async_trait]
impl PlatformTool for DeclareIntention {
    fn name(&self) -> &'static str {
        "declare_intention"
    }

    fn description(&self) -> &'static str {
        "Register what you or another workspace agent plans to do next. Writes a row to the intention map and immediately checks for conflicts. Use action_type to classify the work: tool_call, research, synthesis, writing, review, or idle. Specify targets (outputs you will produce) and depends_on (outputs you need from others) so the conflict checker can reason about ordering and duplication.\n\nNOTE: If you are declaring for another agent rather than yourself, the row is marked as second-hand (your inference, not their statement). Use solicit_agent_plan to ask them directly and record a first-hand declaration."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent declaring the intention. Defaults to the calling agent."
                },
                "action_type": {
                    "type": "string",
                    "description": "What kind of work: tool_call | research | synthesis | writing | review | idle",
                    "enum": ["tool_call", "research", "synthesis", "writing", "review", "idle"]
                },
                "description": {
                    "type": "string",
                    "description": "Plain-language description of what the agent intends to do."
                },
                "tool": {
                    "type": "string",
                    "description": "If action_type is tool_call, the tool name."
                },
                "targets": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Named outputs this intention will produce (used for dependency resolution)."
                },
                "depends_on": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Named outputs from others that this intention requires."
                }
            },
            "required": ["agent_id", "description"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_declare_intention(input, ctx).await
    }
}

// ─── solicit_agent_plan ───────────────────────────────────────────────────────

struct SolicitAgentPlan;

#[async_trait]
impl PlatformTool for SolicitAgentPlan {
    fn name(&self) -> &'static str {
        "solicit_agent_plan"
    }

    fn description(&self) -> &'static str {
        "Ask a workspace member what it intends to do next, record its answer as a first-hand intention, and return the conflict check. Unlike declare_intention (which records your inference about another agent), this tool asks the agent directly and marks the result as solicited — the strongest provenance short of the agent calling declare_intention itself."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the workspace agent to solicit."
                },
                "context": {
                    "type": "string",
                    "description": "Optional context to give the agent before asking for its plan."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    fn is_delegation(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_solicit_agent_plan(input, ctx).await
    }
}

// ─── check_conflicts ──────────────────────────────────────────────────────────

struct CheckConflicts;

#[async_trait]
impl PlatformTool for CheckConflicts {
    fn name(&self) -> &'static str {
        "check_conflicts"
    }

    fn description(&self) -> &'static str {
        "Run conflict detection over the workspace intention map. Returns a signal (CLEAR, RESOURCE_CONFLICT, DEPENDENCY_WAIT, DUPLICATION) and the list of detected conflicts. Optionally filter to one agent's intentions."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Optional: UUID or name of an agent to filter the conflict check to."
                }
            },
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_check_conflicts(input, ctx).await
    }
}

// ─── get_intention_map ────────────────────────────────────────────────────────

struct GetIntentionMap;

#[async_trait]
impl PlatformTool for GetIntentionMap {
    fn name(&self) -> &'static str {
        "get_intention_map"
    }

    fn description(&self) -> &'static str {
        "Return the full intention map for this workspace — all active intention rows, with source and grounding metadata."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        let _ = input;
        execute_get_intention_map(ctx).await
    }
}

// ─── clear_intention ──────────────────────────────────────────────────────────

struct ClearIntention;

#[async_trait]
impl PlatformTool for ClearIntention {
    fn name(&self) -> &'static str {
        "clear_intention"
    }

    fn description(&self) -> &'static str {
        "Mark an agent's active intention as completed, cancelled, or superseded. Completed intentions make their targets available as produced outputs, unblocking any agent that depends on them."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the agent whose intention to clear."
                },
                "status": {
                    "type": "string",
                    "description": "How to resolve the intention: completed | cancelled | superseded",
                    "enum": ["completed", "cancelled", "superseded"],
                    "default": "completed"
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_clear_intention(input, ctx).await
    }
}

// ─── suggest_differentiation ─────────────────────────────────────────────────

struct SuggestDifferentiation;

#[async_trait]
impl PlatformTool for SuggestDifferentiation {
    fn name(&self) -> &'static str {
        "suggest_differentiation"
    }

    fn description(&self) -> &'static str {
        "Compare two agents' active intentions and surface shared targets and description similarity. Structural, not prescriptive: names the pattern, does not prescribe the fix. Both agents must have declared an active intention first."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_a": {
                    "type": "string",
                    "description": "Name of the first agent."
                },
                "agent_b": {
                    "type": "string",
                    "description": "Name of the second agent."
                }
            },
            "required": ["agent_a", "agent_b"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_suggest_differentiation(input, ctx).await
    }
}

// ─── emit_coherence_signal ────────────────────────────────────────────────────

struct EmitCoherenceSignal;

#[async_trait]
impl PlatformTool for EmitCoherenceSignal {
    fn name(&self) -> &'static str {
        "emit_coherence_signal"
    }

    fn description(&self) -> &'static str {
        "Record an IntentionAligns or IntentionConflicts signal between two agents and post it to the workspace conversation. The signal is picked up by ConversationObserver and woven into the TEC coherence graph."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "relation_type": {
                    "type": "string",
                    "description": "IntentionAligns or IntentionConflicts",
                    "enum": ["IntentionAligns", "IntentionConflicts"]
                },
                "agent_a": {
                    "type": "string",
                    "description": "Name of the first agent."
                },
                "agent_b": {
                    "type": "string",
                    "description": "Name of the second agent."
                },
                "strength": {
                    "type": "number",
                    "description": "Signal strength from 0.0 to 1.0 (default: 0.5).",
                    "minimum": 0.0,
                    "maximum": 1.0
                },
                "rationale": {
                    "type": "string",
                    "description": "Optional: why you assessed this relation."
                }
            },
            "required": ["relation_type", "agent_a", "agent_b"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_emit_coherence_signal(input, ctx).await
    }
}

// ─── record_coordination_observation ─────────────────────────────────────────

struct RecordCoordinationObservation;

#[async_trait]
impl PlatformTool for RecordCoordinationObservation {
    fn name(&self) -> &'static str {
        "record_coordination_observation"
    }

    fn description(&self) -> &'static str {
        "Write a targeted coordination observation into a member agent's episodic memory. Only the workspace's registered coordination_strategist may call this. The target must be a current member of this workspace. The observation will be consolidated into a semantic rule on the member's next dreaming cycle."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "UUID or name of the target agent to write the observation into."
                },
                "observation": {
                    "type": "string",
                    "description": "The coordination observation about this agent's behaviour."
                },
                "session_summary": {
                    "type": "string",
                    "description": "Optional: summary of the session that prompted this observation."
                }
            },
            "required": ["agent_id", "observation"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_record_coordination_observation(input, ctx).await
    }
}

// ─── propose_composition_change ───────────────────────────────────────────────

struct ProposeCompositionChange;

#[async_trait]
impl PlatformTool for ProposeCompositionChange {
    fn name(&self) -> &'static str {
        "propose_composition_change"
    }

    fn description(&self) -> &'static str {
        "Propose a structural change to the workspace composition. Creates a pending composition_versions row for the workspace owner to accept or reject. Use ONLY when dreaming has identified a persistent structural issue — valence homophily, chronic destructive incoherence, or a role gap. Provide diff_summary and rationale. Do NOT specify which agent to add; that is the owner's decision."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "diff_summary": {
                    "type": "string",
                    "description": "Plain-language description of what should change and why."
                },
                "rationale": {
                    "type": "string",
                    "description": "Which episodes, principle patterns and valence distribution drove this."
                },
                "homophily_detected": {
                    "type": "boolean",
                    "description": "True when the valence audit found arousal or valence spread < 0.25."
                }
            },
            "required": ["diff_summary", "rationale"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Coordination
    }

    fn requires_workspace(&self) -> bool {
        true
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_propose_composition_change(input, ctx).await
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
    fn all_categories_are_coordination() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Coordination,
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
    fn tool_count_is_nine() {
        assert_eq!(tools().len(), 9);
    }

    #[test]
    fn all_require_workspace() {
        for tool in tools() {
            assert!(
                tool.requires_workspace(),
                "tool `{}` should require workspace",
                tool.name()
            );
        }
    }

    #[test]
    fn solicit_agent_plan_is_delegation() {
        let tool = tools()
            .into_iter()
            .find(|t| t.name() == "solicit_agent_plan")
            .expect("solicit_agent_plan missing");
        assert!(tool.is_delegation());
    }
}
