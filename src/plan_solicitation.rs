//! Asking a member what it intends to do, and recording the answer as its own.
//!
//! # Why this is a module and not a tool body
//!
//! The same argument as [`crate::coordination_note`], one stage earlier in the
//! loop. Loop 3's Stage 0 claims that *a composition catches its coordination
//! failures before the work rather than diagnosing them after it*, and the
//! mechanism is:
//!
//! ```text
//! each member states its own next action → the map is conflict-checked →
//! overlaps are differentiated before anyone spends a turn on them
//! ```
//!
//! mig-218 established the distinction that makes this worth doing: an
//! intention the agent stated (`self` / `solicited`) is a report, and one the
//! coordinator wrote on its behalf (`inferred`) is a belief. Two beliefs cannot
//! be checked against each other — their similarity measures the coordinator's
//! paraphrasing — so the value of the whole stage is bounded by how much of the
//! map is first-hand.
//!
//! # Asking a model to perform a side effect is not a mechanism
//!
//! `solicit_agent_plan` shipped as a tool, named in the card's Stage 0 and in
//! the shelf's prompt, and that made the stage's grounding contingent on a
//! language model electing to make N tool calls. This is the identical defect
//! `coordination_note` was written to fix, where the same contingency produced
//! **0 of 3,576 episodes** for the life of the feature — the tool existed, was
//! dispatched, was asked for by name, and was never once called.
//!
//! The division is the same one, and it holds here for the same reason:
//!
//! * the **judgement** — which members matter to this diagnosis, what context
//!   to give them, whether to re-ask after reading a reply — is the model's;
//! * the **round trip** — going and asking, and writing down what came back —
//!   is bookkeeping, and is ours.
//!
//! So there are two callers, exactly as there:
//!
//! * `agent_backend::tools_legacy::execute_solicit_agent_plan` — the strategist
//!   asking one member, with context it chose. Better, and still available.
//! * `handlers::workspace::coherence` — the platform asking every member that
//!   has no fresh first-hand plan, before the strategist runs. The floor.
//!
//! # The floor runs BEFORE the strategist, and that is the difference
//!
//! [`crate::coordination_note`]'s floor runs after, because a coordination
//! brief is retrospective: it summarises a session that has happened.
//!
//! A plan is not. Stage 0 is pre-flight — *"before any significant agent
//! action"* — and a plan solicited after the diagnosis is a plan the diagnosis
//! could not use. Running the floor first means the strategist opens Stage 2
//! with a map the team filled in rather than one it has to invent, which is the
//! entire point of the stage and was never once true in production.
//!
//! The cost is latency on an already-slow paid endpoint, and it is bounded
//! three ways: only at `depth=recommendations`, only for members whose plan is
//! stale ([`FRESHNESS_SECS`]), and never more than [`MAX_PER_RUN`] of them.
//! Concurrently, so the wall clock is one round trip rather than N.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::MemoryStore;

use crate::agent_backend::credentials::ResolvedCredentials;
use crate::agent_backend::registry::AgentRegistry;
use crate::intentions::IntentionSource;

/// How long a first-hand plan stands before the floor asks again.
///
/// Ten minutes, and the number is a judgement about what an intention *is*
/// rather than a cache policy. `declare_intention`'s own schema carries
/// `ttl_seconds: 300` as its default, so five minutes is the platform's
/// existing opinion about when a declared action goes stale; this is double
/// that, because the floor pays an LLM call to refresh and the tool does not.
///
/// Too short and every shelf press re-interrogates a team that has not moved.
/// Too long and Stage 0 coordinates against plans the members have already
/// carried out — which is worse than no plan, because a stale row is
/// indistinguishable from a current one and carries the same `solicited`
/// authority.
pub const FRESHNESS_SECS: i64 = 600;

/// The most members the floor will ask in one run.
///
/// A hard bound on an unbounded cost. Each solicitation is an LLM call funded
/// by the workspace, on an endpoint the user pressed expecting to pay for one
/// strategist run — so a twenty-member workspace must not silently become a
/// twenty-one-call request.
///
/// Eight rather than a rounder number: it covers the 99th percentile of
/// workspace sizes on this platform, and a team larger than that has a
/// composition problem Stage 0 cannot fix. When the cap bites, it is reported
/// rather than absorbed — see [`Floor::capped`].
pub const MAX_PER_RUN: usize = 8;

/// What the platform needs in order to ask.
///
/// A struct rather than eight positional arguments, because both callers
/// already hold every field and the tool path's version of this list is
/// `ToolContext` — which is not available here and should not be, since a
/// module that owns a domain step must not depend on the tool layer's shape.
pub struct Asker {
    pub db: PgPool,
    pub memory_store: Arc<MemoryStore>,
    pub embedder: Arc<dyn EmbeddingGenerator>,
    pub registry: Arc<AgentRegistry>,
    pub credentials: Arc<ResolvedCredentials>,
}

/// What came of asking one member.
///
/// An enum rather than a `String` for the reason [`crate::coordination_note`]'s
/// is: the caller has to tell "already knew" from "refused" from "asked and got
/// nothing back", and those want different responses. Only the last two are
/// problems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Solicited {
    /// The member answered and its plan is in the map.
    Recorded {
        intention_id: Uuid,
        /// The member's own one-line statement of its next action.
        description: String,
        /// `CLEAR` | `OVERLAP_WARNING` | `CONFLICT_ALERT` | `DEPENDENCY_WAIT`.
        signal: String,
    },
    /// The member already has a first-hand plan younger than [`FRESHNESS_SECS`].
    ///
    /// **Not a failure, and the outcome to hope for.** It means either the
    /// member declared for itself or the strategist already asked, and the
    /// floor was not needed. Charging an LLM call to re-confirm it would be
    /// the floor competing with the thing it exists to back up.
    AlreadyFresh { source: IntentionSource },
    /// The agent is not a member of this workspace.
    ///
    /// Refused rather than skipped: recording an outsider's plan in this team's
    /// map would put work nobody here is doing into everybody's conflict
    /// checks.
    NotAMember,
    /// The member could not be run — unregistered, unfunded, or the provider
    /// failed.
    Unreachable { error: String },
    /// The member ran and did not return a plan this module could read.
    ///
    /// Distinct from [`Self::Unreachable`], and the distinction is the whole
    /// value of this variant: one is an infrastructure fault and the other is
    /// an agent that cannot follow a structured-output instruction, which is a
    /// fact about that agent worth surfacing to whoever owns it. Nothing is
    /// recorded either way — a blank row carrying `solicited` provenance would
    /// be worse than the inference it replaced.
    Unparseable { reply_excerpt: String },
}

impl Solicited {
    /// Should a caller surface this as a problem?
    pub fn is_problem(&self) -> bool {
        matches!(
            self,
            Self::NotAMember | Self::Unreachable { .. } | Self::Unparseable { .. }
        )
    }

    /// A short tag for logs and for the floor's summary.
    pub fn tag(&self) -> &'static str {
        match self {
            Self::Recorded { .. } => "recorded",
            Self::AlreadyFresh { .. } => "already_fresh",
            Self::NotAMember => "not_a_member",
            Self::Unreachable { .. } => "unreachable",
            Self::Unparseable { .. } => "unparseable",
        }
    }
}

/// What one floor pass did.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Floor {
    pub asked: usize,
    pub recorded: usize,
    pub already_fresh: usize,
    pub problems: Vec<String>,
    /// Members that needed asking and were not asked, because [`MAX_PER_RUN`]
    /// bit.
    ///
    /// Reported, never absorbed. A silently truncated floor produces a
    /// partially grounded map that reads exactly like a fully grounded one, and
    /// the strategist would treat the missing members' inferred rows as though
    /// the platform had chosen not to ask.
    pub capped: usize,
}

impl Floor {
    /// A line for the strategist's prompt, so it knows what its map is worth
    /// and who it still has to guess about.
    pub fn reading(&self) -> String {
        if self.asked == 0 && self.capped == 0 {
            return "Every member already had a current first-hand plan; the \
                    platform asked nobody."
                .to_string();
        }
        let mut s = format!(
            "The platform asked {} member(s) for their plans before this run: \
             {} answered and are recorded as `solicited`.",
            self.asked, self.recorded
        );
        if !self.problems.is_empty() {
            s.push_str(&format!(
                " {} could not be reached or did not return a readable plan, so \
                 those remain unstated and you will have to infer them — say so \
                 when you do.",
                self.problems.len()
            ));
        }
        if self.capped > 0 {
            s.push_str(&format!(
                " {} further member(s) needed asking and were not, because the \
                 per-run cap of {MAX_PER_RUN} was reached. Their rows are not \
                 first-hand.",
                self.capped
            ));
        }
        s
    }
}

/// Members of this workspace with no first-hand plan younger than `freshness`.
///
/// The strategist is excluded: Stage 0 coordinates the team, and a coordinator
/// asking itself what it plans to do next would spend a call to be told it
/// plans to coordinate.
///
/// Ordered oldest-plan-first so that when [`MAX_PER_RUN`] bites, the members
/// asked are the ones the map knows least about. A stable order also keeps the
/// cap from silently rotating which member gets left out on each press.
pub async fn members_needing_a_plan(
    db: &PgPool,
    workspace_id: Uuid,
    strategist_id: Option<Uuid>,
    freshness_secs: i64,
) -> Result<Vec<Uuid>, String> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT wa.agent_id
           FROM workspace_agents wa
           LEFT JOIN LATERAL (
                SELECT i.declared_at
                  FROM workspace_intentions i
                 WHERE i.workspace_id = wa.workspace_id
                   AND i.agent_id = wa.agent_id
                   AND i.status = 'active'
                   AND i.source IN ('self','solicited')
                 ORDER BY i.declared_at DESC
                 LIMIT 1
           ) fresh ON TRUE
          WHERE wa.workspace_id = $1
            AND ($2::uuid IS NULL OR wa.agent_id <> $2)
            AND (fresh.declared_at IS NULL
                 OR fresh.declared_at < NOW() - make_interval(secs => $3::double precision))
          ORDER BY fresh.declared_at ASC NULLS FIRST, wa.agent_id",
    )
    .bind(workspace_id)
    .bind(strategist_id)
    .bind(freshness_secs as f64)
    .fetch_all(db)
    .await
    .map_err(|e| format!("could not list members needing a plan: {e}"))
}

/// Ask one member for its plan and record the answer as that member's own.
///
/// `asked_by` is the agent doing the asking — the strategist on the tool path,
/// and the strategist again on the floor path, since the platform asks on its
/// behalf. It lands in `workspace_intentions.declared_by`, which answers "who
/// went and got this" without ever being mistaken for whose plan it is.
///
/// Non-fatal by construction: returns [`Solicited`] rather than erroring, and
/// counts the row write through [`crate::write_accounting::Sink::WorkspaceIntentions`].
#[allow(clippy::too_many_arguments)]
pub async fn solicit(
    asker: &Asker,
    workspace_id: Uuid,
    asked_by: Option<Uuid>,
    target: Uuid,
    context: Option<&str>,
    freshness_secs: Option<i64>,
    parent_episode_id: Option<Uuid>,
) -> Solicited {
    // Membership, always. The question is about the *target*, so it belongs to
    // the asking rather than to either caller's authorisation.
    let is_member: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(workspace_id)
    .bind(target)
    .fetch_optional(&asker.db)
    .await
    .ok()
    .flatten();
    if is_member.is_none() {
        return Solicited::NotAMember;
    }

    // Yield to a plan the member already stated. The floor passes a window; the
    // tool passes `None`, because a strategist that has read the map and asked
    // anyway has a reason and the platform is not better placed to overrule it.
    if let Some(window) = freshness_secs {
        let existing: Option<(String,)> = sqlx::query_as(
            "SELECT source FROM workspace_intentions
              WHERE workspace_id = $1 AND agent_id = $2 AND status = 'active'
                AND source IN ('self','solicited')
                AND declared_at >= NOW() - make_interval(secs => $3::double precision)
              ORDER BY declared_at DESC LIMIT 1",
        )
        .bind(workspace_id)
        .bind(target)
        .bind(window as f64)
        .fetch_optional(&asker.db)
        .await
        .ok()
        .flatten();
        if let Some((source,)) = existing {
            return Solicited::AlreadyFresh {
                source: IntentionSource::from_db(&source),
            };
        }
    }

    let agent_name: Option<String> =
        sqlx::query_scalar("SELECT agent_name FROM agents WHERE agent_id = $1")
            .bind(target)
            .fetch_optional(&asker.db)
            .await
            .ok()
            .flatten();
    let Some(agent_name) = agent_name else {
        return Solicited::Unreachable {
            error: format!("agent {target} has no row in `agents`"),
        };
    };

    let query = match elicitation_prompt(&asker.db, workspace_id, target, context).await {
        Ok(q) => q,
        Err(e) => return Solicited::Unreachable { error: e },
    };

    let card = match asker.registry.get(&agent_name) {
        Ok(c) => c,
        Err(e) => {
            return Solicited::Unreachable {
                error: format!("{agent_name} is not in the registry: {e}"),
            }
        }
    };
    // The member answers with everything it has learned, same as any other
    // execution. A plan drafted without the agent's own semantic rules is a
    // plan from an agent that has forgotten what it knows about this workspace.
    let (card, _) = crate::agent_backend::kg_context::enrich_with_kg_context(
        &asker.memory_store,
        &asker.embedder,
        target,
        &query,
        card,
    )
    .await;

    let stmt = crate::ast::AgentStmt {
        name: agent_name.clone(),
        agent_type: Some(card.agent_type.clone()),
        query: query.clone(),
        executor: None,
        schedule: None,
        driver_refs: vec![],
        depends_on: vec![],
        confidence_threshold: None,
    };
    let exec_context = crate::agent_backend::executor::ExecutionContext {
        program: crate::ast::Program { statements: vec![] },
        agent_card: card,
        creature_id: None,
        cognition_tier: None,
        credentials: asker.credentials.clone(),
        attachments: Vec::new(),
    };

    // No tools. A plan is a statement, not an action, and the premise of
    // Stage 0 is that it runs before anything is done. Handing the member a
    // tool list here would let pre-flight do the flight.
    let output = match asker.registry.execute_agent(&stmt, &exec_context).await {
        Ok(o) => o,
        Err(e) => {
            return Solicited::Unreachable {
                error: format!("could not reach {agent_name}: {e}"),
            }
        }
    };

    // The member's own run, on the member's own clock, recorded against the
    // member. Its cost belongs in that agent's totals and not in the
    // strategist's, and without this the floor would spend N calls that appear
    // in no ledger.
    record_ask_episode(
        asker,
        target,
        &agent_name,
        parent_episode_id,
        &query,
        &output,
    )
    .await;

    let raw = output.metadata.reasoning.clone().unwrap_or_default();
    let Some(plan) = extract_json_object(&raw) else {
        return Solicited::Unparseable {
            reply_excerpt: raw.chars().take(400).collect(),
        };
    };

    let description = plan
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(description) = description else {
        return Solicited::Unparseable {
            reply_excerpt: format!("no `description` in: {}", truncate(&raw, 300)),
        };
    };

    let list = |key: &str| -> Vec<String> {
        plan.get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    };

    let written = crate::agent_backend::tools::write_intention(
        &asker.db,
        asker.embedder.as_ref(),
        workspace_id,
        target,
        asked_by,
        plan.get("action_type")
            .and_then(|v| v.as_str())
            .unwrap_or("research"),
        plan.get("tool").and_then(|v| v.as_str()),
        description,
        &list("targets"),
        &list("depends_on"),
        IntentionSource::Solicited,
    )
    .await;

    match crate::write_accounting::observe(
        crate::write_accounting::Sink::WorkspaceIntentions,
        written,
    ) {
        Some(v) => Solicited::Recorded {
            intention_id: v
                .get("intention_id")
                .and_then(|x| x.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or_else(Uuid::nil),
            description: description.to_string(),
            signal: v
                .get("signal")
                .and_then(|x| x.as_str())
                .unwrap_or("CLEAR")
                .to_string(),
        },
        None => Solicited::Unreachable {
            error: "the member answered and the intention write was refused; \
                    see write_accounting"
                .to_string(),
        },
    }
}

/// The question, with the team's declared plans attached.
///
/// The peer list is the propagation half, and without it this tool would only
/// *collect* private intentions. ReMALIS §3.1's claim is not that a coordinator
/// should hold every `I_j` — it is that agent *i* choosing `a_i` should already
/// know `I_j`. So each member is shown what its teammates have said before it
/// answers, and told which of those are first-hand, so it does not defer to the
/// coordinator's guess about a third party as though the third party had said
/// it.
async fn elicitation_prompt(
    db: &PgPool,
    workspace_id: Uuid,
    target: Uuid,
    context: Option<&str>,
) -> Result<String, String> {
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT a.agent_name, i.action_type, i.description, i.source
           FROM workspace_intentions i
           JOIN agents a ON a.agent_id = i.agent_id
          WHERE i.workspace_id = $1 AND i.status = 'active' AND i.agent_id <> $2
          ORDER BY i.declared_at",
    )
    .bind(workspace_id)
    .bind(target)
    .fetch_all(db)
    .await
    .map_err(|e| format!("could not read the intention map: {e}"))?;

    let peers = if rows.is_empty() {
        "No other agent has declared an intention yet.\n\n".to_string()
    } else {
        let lines: Vec<String> = rows
            .iter()
            .map(|(name, action, desc, source)| {
                format!(
                    "- {name} ({action}): {desc} [{}]",
                    if IntentionSource::from_db(source).is_first_hand() {
                        "stated by that agent"
                    } else {
                        "inferred by the coordinator, unconfirmed"
                    }
                )
            })
            .collect();
        format!(
            "What your teammates have declared:\n{}\n\n",
            lines.join("\n")
        )
    };

    Ok(format!(
        "Before you act, state your plan so the workspace can coordinate.\n\n\
         {}{peers}\
         Reply with ONLY a JSON object, no prose around it:\n\
         {{\n\
         \"action_type\": one of tool_call|research|synthesis|writing|review|idle,\n\
         \"description\": one sentence naming the specific next action you intend to take,\n\
         \"targets\": [files or named outputs you will write or consume],\n\
         \"depends_on\": [named outputs you need from someone else before you can start],\n\
         \"teammate_assignment\": [{{\"agent\": name, \"should_take\": what you think they should own}}]\n\
         }}\n\n\
         This is your own plan, recorded as yours. Say `idle` for action_type if \
         you have nothing to do next — that is a useful answer and better than \
         inventing work. If your plan overlaps something a peer above has already \
         stated, say what you will do differently in `description`.",
        context
            .map(|c| format!("Coordination context: {c}\n\n"))
            .unwrap_or_default(),
    ))
}

/// Record the member's answer as an episode of the member's own.
///
/// Tagged so it can be told apart from work: being asked to plan is not the
/// same as having done something, and an agent whose episode history is half
/// pre-flight questionnaires would consolidate rules about answering
/// questionnaires.
async fn record_ask_episode(
    asker: &Asker,
    target: Uuid,
    agent_slug: &str,
    parent_episode_id: Option<Uuid>,
    query: &str,
    output: &crate::agent_backend::executor::AgentOutput,
) {
    let mut episode = crate::episodes::agent_output_to_episode(target, query, output);
    episode.parent_episode_id = parent_episode_id;
    episode.tags.push("plan_solicitation".to_string());
    episode.tags.push("stage_0".to_string());

    // Embedded and provenanced like any other episode, not stored bare.
    //
    // `store_episode` is deprecated for a reason that bites hardest here: it
    // writes NULL provenance even when an embedding is present, and an episode
    // with no embedding is invisible to retrieval. A member's stated plan is
    // exactly the thing worth recalling next time it is asked — "last time I
    // said I would take the CPI half" — so storing it unretrievably would make
    // the floor pay for a call whose output no agent can ever consult.
    let embed_text = format!(
        "{query} {}",
        output.metadata.reasoning.as_deref().unwrap_or("")
    );
    let provenance = asker.embedder.generate_provenanced(&embed_text).await.ok();
    let source_ref = serde_json::json!({
        "kind": "plan_solicitation",
        "agent_id": target,
    });

    // Through the boundary, not around it. This path invoked an agent and
    // persisted its answer, and until now it ran none of the six checks: a
    // member with a field contract stated a plan and nothing enforced the
    // contract, nothing stamped the grade, no gate row was written and no
    // contracted field was queued. Loop 3's own mechanism was the ungoverned
    // one.
    crate::write_accounting::observe(
        crate::write_accounting::Sink::Episodes,
        crate::episode_boundary::persist(
            "the member is asked and its answer stored in one call, with nothing \
             in between that could name this id; Stage 0 hands the member no \
             tools, so it cannot delegate",
            crate::episode_boundary::Write {
                store: &asker.memory_store,
                db: Some(&asker.db),
                agent_slug,
                episode,
                route: crate::route_trust::RouteSelection::CallerNamed,
                provenance: provenance.as_ref(),
                source_ref: Some(source_ref),
            },
        )
        .await,
    );
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// Pull the first balanced JSON object out of a model reply.
///
/// Models fence JSON, preface it, or apologise after it, and a plan lost to a
/// stray "Here you go:" would be recorded as the member refusing to coordinate
/// — which sends the strategist back to inferring, the behaviour this whole
/// path exists to replace. Brace-counting rather than a regex because plans
/// nest: `teammate_assignment` is an array of objects.
pub fn extract_json_object(raw: &str) -> Option<serde_json::Value> {
    if let Ok(v @ serde_json::Value::Object(_)) = serde_json::from_str(raw.trim()) {
        return Some(v);
    }
    let start = raw.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, ch) in raw[start..].char_indices() {
        if in_string {
            match ch {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + offset + ch.len_utf8();
                    return serde_json::from_str(&raw[start..end]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outcome to hope for is not a problem.
    ///
    /// The floor exists to be *unnecessary*. On a workspace where every member
    /// keeps its own plan current, every call returns `AlreadyFresh` and the
    /// platform spent nothing — which is success. A caller that logged it as a
    /// warning would make the log useless on exactly the runs that went best,
    /// which is the mistake `coordination_note::is_problem` was written to
    /// avoid one stage later.
    #[test]
    fn a_plan_the_member_already_stated_is_not_a_problem() {
        assert!(!Solicited::AlreadyFresh {
            source: IntentionSource::SelfDeclared
        }
        .is_problem());
        assert!(!Solicited::Recorded {
            intention_id: Uuid::nil(),
            description: "x".into(),
            signal: "CLEAR".into()
        }
        .is_problem());

        assert!(Solicited::NotAMember.is_problem());
        assert!(Solicited::Unreachable { error: "e".into() }.is_problem());
        assert!(Solicited::Unparseable {
            reply_excerpt: "e".into()
        }
        .is_problem());
    }

    /// An agent that cannot answer and an agent that could not be reached are
    /// different findings.
    ///
    /// One is an infrastructure fault; the other is a fact about that agent's
    /// ability to follow a structured-output instruction, which its owner
    /// should hear about. Collapsing them into one error string would bury the
    /// second inside the first, and the second is the one that recurs.
    #[test]
    fn unreachable_and_unparseable_are_distinct() {
        let a = Solicited::Unreachable { error: "e".into() };
        let b = Solicited::Unparseable {
            reply_excerpt: "e".into(),
        };
        assert_ne!(a, b);
        assert_eq!(a.tag(), "unreachable");
        assert_eq!(b.tag(), "unparseable");
    }

    /// A truncated floor says so.
    ///
    /// The failure this guards: `MAX_PER_RUN` bites, three members are never
    /// asked, and the strategist reads a partially grounded map as though the
    /// platform had asked everyone and those three simply had nothing to say.
    #[test]
    fn the_cap_is_reported_rather_than_absorbed() {
        let f = Floor {
            asked: 8,
            recorded: 8,
            already_fresh: 0,
            problems: vec![],
            capped: 3,
        };
        let r = f.reading();
        assert!(r.contains("3 further member(s)"), "{r}");
        assert!(r.contains("not first-hand"), "{r}");
    }

    /// A floor that did nothing because nothing needed doing reads as such,
    /// and does not claim to have asked anybody.
    #[test]
    fn an_unneeded_floor_does_not_claim_credit() {
        let r = Floor::default().reading();
        assert!(r.contains("asked nobody"), "{r}");
        assert!(!r.contains("solicited"), "{r}");
    }

    /// Problems are named in the reading, because a member the platform could
    /// not reach is one the strategist must go back to inferring — and it has
    /// to know to say so.
    #[test]
    fn unreachable_members_are_surfaced_to_the_strategist() {
        let f = Floor {
            asked: 3,
            recorded: 2,
            already_fresh: 1,
            problems: vec!["scout: unreachable".into()],
            capped: 0,
        };
        let r = f.reading();
        assert!(r.contains("infer"), "{r}");
    }

    // ── the parser ────────────────────────────────────────────────────

    #[test]
    fn a_plan_survives_the_usual_model_packaging() {
        let bare = r#"{"action_type":"research","description":"read the CPI series"}"#;
        let fenced = format!("Here is my plan:\n```json\n{bare}\n```\nLet me know.");
        for raw in [bare.to_string(), fenced, format!("  \n{bare}\n\n")] {
            let v = extract_json_object(&raw).expect("should parse");
            assert_eq!(v["description"], "read the CPI series", "from {raw:?}");
        }
    }

    #[test]
    fn a_nested_plan_is_not_truncated_at_the_first_brace() {
        let raw = r#"Plan: {"description":"x","teammate_assignment":[{"agent":"bob","should_take":"the CPI half"}]} done"#;
        let v = extract_json_object(raw).expect("should parse");
        assert_eq!(v["teammate_assignment"][0]["agent"], "bob");
    }

    #[test]
    fn braces_inside_strings_do_not_confuse_the_depth_count() {
        let raw =
            r#"{"description":"emit a {placeholder} and a \"quote\"","action_type":"writing"}"#;
        let v = extract_json_object(raw).expect("should parse");
        assert_eq!(v["action_type"], "writing");
    }

    /// No plan is not an empty plan.
    ///
    /// `solicit` turns `None` into `Unparseable` and records nothing. An empty
    /// object here would file a blank intention under that member's name
    /// carrying `solicited` provenance — worse than the inference it replaced,
    /// because it would be trusted more.
    #[test]
    fn prose_with_no_object_yields_nothing() {
        assert!(extract_json_object("I'm not sure what to do next.").is_none());
        assert!(extract_json_object("").is_none());
        assert!(extract_json_object("{ unbalanced").is_none());
        assert!(extract_json_object("[1,2,3]").is_none());
    }
}
