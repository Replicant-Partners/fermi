//! Delivering a coordination finding into a member agent's memory.
//!
//! # Why this is a module and not a tool body
//!
//! Loop 3's claim is that *a composition notices its own incoherence and
//! coordinates out of it within a session*, and the mechanism is:
//!
//! ```text
//! coherence measured → strategist writes a brief → the brief reaches each
//! member's episodic memory → the member dreams on it → a semantic rule
//! ```
//!
//! The card is explicit about why the memory step is the one that matters:
//! *"An agent does not read the brief — it dreams on its episodes."* Writing a
//! file to workspace git changes nothing about any agent's behaviour.
//!
//! **And it has never happened.** `coordinator_observation` stands at 0 of 3,576
//! episodes. The tool exists, is dispatched, is exposed to the strategist, and
//! both the card's Stage 3 and the handler's prompt ask for it by name.
//!
//! # The mechanism was asking a model to perform a side effect
//!
//! That is the defect, and it is structural rather than a prompt problem. The
//! *content* of a coordination finding is a judgement and belongs to the model.
//! The *delivery* of it is bookkeeping and belongs to the platform. Loop 3 asked
//! the model to do both, so the loop's terminal half was contingent on a
//! language model electing to make a tool call — and `awaiting_agent` is the
//! honest reading of what happens when it does not.
//!
//! So delivery moves here, with two callers:
//!
//! * `agent_backend::tools_legacy::execute_record_coordination_observation` —
//!   the strategist writing a **targeted** note about one member. Better, and
//!   still available.
//! * `handlers::workspace::coherence` — the platform delivering the brief to
//!   every member after the run, for any member the strategist did not write to.
//!   The floor.
//!
//! One implementation of the episode write, per §3.4. The tool keeps its own
//! authorisation check — *is the caller really this workspace's strategist* —
//! because that question only arises for a model-invoked call; the platform
//! knows which strategist it just ran.

use sqlx::PgPool;
use uuid::Uuid;

use agent_bestiary_memory::embeddings::EmbeddingGenerator;
use agent_bestiary_memory::MemoryStore;

/// Why a delivery did not happen.
///
/// An enum rather than a `String` for the same reason
/// [`crate::claim_outcome::ClaimOutcome`] is: the caller has to be able to tell
/// "already done, better, by the model" from "refused" from "failed", and a
/// message cannot be branched on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// Written. The member will consolidate it on its next dreaming cycle.
    Written { episode_id: Uuid },
    /// The strategist already wrote a targeted note for this member during this
    /// run, so the platform's generic one would be a duplicate.
    ///
    /// **Not a failure.** This is the outcome to hope for: it means the model
    /// did the better thing and the floor was not needed.
    AlreadyTargeted,
    /// The agent is not a member of this workspace.
    ///
    /// Refused rather than skipped. Writing into the memory of an agent that
    /// was never in the room is not a coordination observation, it is an
    /// injection.
    NotAMember,
    /// The write was attempted and failed.
    Failed { error: String },
}

/// Deliver one coordination finding into one member's episodic memory.
///
/// `since` bounds the duplicate check: a note the strategist wrote *during this
/// run* suppresses the platform's floor, and one from a previous session does
/// not. Without it, the second coherence evaluation on a workspace would deliver
/// nothing because the first one had.
///
/// Non-fatal by construction — it returns [`Delivery`] rather than erroring —
/// and counted through `write_accounting::Sink::Episodes`, because a lost
/// coordination note is a dreaming cycle that will not happen and nothing else
/// would record its absence.
#[allow(clippy::too_many_arguments)]
pub async fn deliver(
    pool: &PgPool,
    store: &MemoryStore,
    embedder: &dyn EmbeddingGenerator,
    workspace_id: Uuid,
    strategist_id: Uuid,
    target: Uuid,
    observation: &str,
    session_summary: &str,
    since: Option<chrono::DateTime<chrono::Utc>>,
) -> Delivery {
    // Membership, always. The tool checked this and so does the platform: the
    // question is about the *target*, not about the caller, so it belongs to
    // the delivery rather than to either caller's authorisation.
    let is_member: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2",
    )
    .bind(workspace_id)
    .bind(target)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if is_member.is_none() {
        return Delivery::NotAMember;
    }

    // Did the strategist already write a targeted note for this member during
    // this run? `context->>'workspace_id'` rather than a column, because
    // `episodes` has no workspace dimension — which is itself Loop 4's first
    // empty link, one loop over.
    if let Some(cutoff) = since {
        let already: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM episodes \
              WHERE agent_id = $1 \
                AND provenance = 'coordinator_observation' \
                AND context->>'workspace_id' = $2 \
                AND timestamp_ref >= $3",
        )
        .bind(target)
        .bind(workspace_id.to_string())
        .bind(cutoff)
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        if already > 0 {
            return Delivery::AlreadyTargeted;
        }
    }

    let query = format!("Coordination observation from this workspace: {observation}");
    let body = if session_summary.is_empty() {
        observation.to_string()
    } else {
        format!("{observation}\n\nSession context: {session_summary}")
    };

    let provenance = match embedder
        .generate_provenanced(&format!("{query} {body}"))
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return Delivery::Failed {
                error: format!("could not embed the observation: {e}"),
            }
        }
    };

    let episode = agent_bestiary_memory::Episode {
        episode_id: Uuid::new_v4(),
        agent_id: target,
        timestamp_ref: chrono::Utc::now(),
        query,
        context: serde_json::json!({
            "kind": "coordination_observation",
            "workspace_id": workspace_id,
            "strategist_agent_id": strategist_id,
        }),
        execution_status: agent_bestiary_memory::ExecutionStatus::Success,
        error_details: None,
        execution_time_ms: 0,
        tokens_used: None,
        cost_usd: None,
        input_tokens: None,
        output_tokens: None,
        cost_basis: None,
        cost_rate_key: None,
        parent_episode_id: None,
        response_text: Some(body),
        assertions: None,
        embedding: None, // set from `provenance` by the storing call below
        consolidated: false,
        tags: vec![
            "coordination_observation".to_string(),
            "dreaming_material".to_string(),
        ],
        provenance: agent_bestiary_memory::Provenance::CoordinatorObservation,
        // Above an ordinary episode (0.5) so it survives the top-30 extraction
        // budget in a busy agent, well below a human correction (1.0). The
        // strategist is an LLM making a second-order judgement about behaviour,
        // not ground truth, and should not outrank what the agent actually did.
        authority_weight: 0.6,
        dyad_id: None,
        persona_version_at_write: None,
        provider_used: None,
        model_used: None,
    };

    let source_ref = serde_json::json!({
        "kind": "coordination_observation",
        "workspace_id": workspace_id,
        "strategist_agent_id": strategist_id,
    });

    let written = store
        .store_episode_with_provenance(episode, Some(&provenance), Some(source_ref))
        .await;

    // Counted, not merely returned. Loop 3's terminal half has produced zero
    // rows for the life of the feature; when it starts, "how many did we lose"
    // has to be answerable, and a `Delivery::Failed` handed back to a spawned
    // caller is not a record of it.
    match crate::write_accounting::observe(crate::write_accounting::Sink::Episodes, written) {
        Some(episode_id) => Delivery::Written { episode_id },
        None => Delivery::Failed {
            error: "the episode write was refused; see write_accounting".to_string(),
        },
    }
}

/// Is this outcome one a caller should surface as a problem?
///
/// [`Delivery::AlreadyTargeted`] is the outcome to hope for and must not be
/// logged as a failure — a caller that warns on it would train its readers to
/// ignore the warning that matters.
pub fn is_problem(d: &Delivery) -> bool {
    matches!(d, Delivery::NotAMember | Delivery::Failed { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outcome to hope for is not a problem.
    ///
    /// The floor exists to be *unnecessary*. If a strategist writes targeted
    /// notes for every member, every delivery returns `AlreadyTargeted` and the
    /// platform did nothing — which is success, and a caller that logged it as
    /// a warning would make the log useless on exactly the runs that went best.
    #[test]
    fn a_note_the_model_already_wrote_is_not_a_problem() {
        assert!(!is_problem(&Delivery::AlreadyTargeted));
        assert!(!is_problem(&Delivery::Written {
            episode_id: Uuid::nil()
        }));
        assert!(is_problem(&Delivery::NotAMember));
        assert!(is_problem(&Delivery::Failed {
            error: "e".to_string()
        }));
    }

    /// Not-a-member is a refusal, not a skip.
    ///
    /// Stated as a test because the two read alike in a loop over members and
    /// the difference matters: writing a coordination observation into the
    /// memory of an agent that was never in the room is not coordination, it is
    /// an injection into an unrelated agent's dreaming material.
    #[test]
    fn writing_into_a_non_members_memory_is_refused_and_reported() {
        assert!(
            is_problem(&Delivery::NotAMember),
            "a non-member delivery must be surfaced, not quietly skipped"
        );
        assert_ne!(Delivery::NotAMember, Delivery::AlreadyTargeted);
    }
}
