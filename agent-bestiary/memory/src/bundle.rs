//! `EpisodeBundle` — the normalized signal consumed by the Plane B
//! evaluator registry.
//!
//! See `docs/architecture/social_agent_observability_architecture.html`
//! (Plane A — Output interface): the bundle carries
//!     transcript · goal_spec · persona_version · context snapshot · source enum
//! so that every `EvalModel` implementation reads the same shape regardless
//! of which capability (LLM judge, Brier, coherence agent, episodic memory)
//! produced the underlying episode.
//!
//! Phase 0 just defines the type. Phase 1 introduces the `EvalModel` trait
//! and starts consuming bundles; Phase 2 wires the existing eval pipeline
//! to construct bundles from real episode runs.

use crate::{Agent, Episode, Provenance};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Normalized projection of an episode + persona context, suitable as
/// input to any `EvalModel`.
///
/// Construct via [`EpisodeBundle::from_parts`] or
/// [`EpisodeBundle::from_episode`]. The bundle is intentionally
/// serializable so it can be passed across crate boundaries (the
/// evaluator registry will live in a sibling crate).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeBundle {
    /// Stable id of the underlying episode (one bundle per episode).
    pub episode_id: Uuid,
    pub agent_id: Uuid,
    /// Snapshot of `agents.persona_version` at the moment this bundle
    /// was constructed. Drift monitor compares embeddings across
    /// versions; evaluators that care about persona consistency read
    /// this field.
    pub persona_version: i32,
    /// (agent_id, human_id) dyad identifier when known. Wiring deferred
    /// per Phase 0 Q4 — eval-run executions leave this `None`.
    pub dyad_id: Option<String>,
    /// Wall-clock time of the underlying episode.
    pub timestamp_ref: DateTime<Utc>,

    /// The original prompt / query that opened the episode.
    pub query: String,
    /// Full conversation transcript when available. For single-turn
    /// agents this is just `[(user, query), (agent, response)]`. The
    /// shape is intentionally lightweight — richer multi-turn workspace
    /// transcripts join via `context`.
    #[serde(default)]
    pub transcript: Vec<TranscriptTurn>,
    /// Optional structured goal spec (Sotopia-style social goal,
    /// forecasting target, etc.). Provided by callers that know the
    /// task semantics; evaluators that need goals will short-circuit
    /// when missing.
    pub goal_spec: Option<serde_json::Value>,
    /// Pass-through of the underlying episode's `context` JSONB.
    pub context: serde_json::Value,

    /// Source enum — see [`Provenance`].
    pub provenance: Provenance,
    /// 1.0 = HumanAuthority. Defaulted from the episode field.
    pub authority_weight: f64,

    /// Optional handles to make `EvalModel`s self-sufficient — agent
    /// type, system prompt at execution time, model used. Filled in
    /// when the bundle is built from an `Agent`.
    pub agent_card: Option<AgentCardSnapshot>,
}

/// Single turn in the bundled transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptTurn {
    pub role: TranscriptRole,
    pub content: String,
    /// Optional speaker identifier (workspace user_id, agent_id, etc.).
    pub speaker_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptRole {
    User,
    Agent,
    System,
    Tool,
}

/// Lightweight snapshot of agent metadata captured into the bundle so
/// that downstream evaluators don't need to hit the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCardSnapshot {
    pub agent_id: Uuid,
    pub agent_name: String,
    pub agent_type: String,
    pub model: String,
    pub system_prompt: Option<String>,
    pub temperature: f64,
}

impl AgentCardSnapshot {
    pub fn from_agent(agent: &Agent) -> Self {
        Self {
            agent_id: agent.agent_id,
            agent_name: agent.agent_name.clone(),
            agent_type: agent.agent_type.clone(),
            model: agent.model.clone(),
            system_prompt: agent.system_prompt.clone(),
            temperature: agent.temperature,
        }
    }
}

impl EpisodeBundle {
    /// Build a bundle from already-loaded parts. The most direct
    /// constructor — useful from the eval pipeline where the agent
    /// is already in scope and we'd just be re-reading it otherwise.
    pub fn from_parts(
        episode: &Episode,
        agent: &Agent,
        transcript: Vec<TranscriptTurn>,
        goal_spec: Option<serde_json::Value>,
    ) -> Self {
        Self {
            episode_id: episode.episode_id,
            agent_id: episode.agent_id,
            persona_version: episode.persona_version_at_write.unwrap_or(agent.persona_version),
            dyad_id: episode.dyad_id.clone(),
            timestamp_ref: episode.timestamp_ref,
            query: episode.query.clone(),
            transcript,
            goal_spec,
            context: episode.context.clone(),
            provenance: episode.provenance,
            authority_weight: episode.authority_weight,
            agent_card: Some(AgentCardSnapshot::from_agent(agent)),
        }
    }

    /// Convenience constructor for callers that only have an `Episode`
    /// and don't yet have an `Agent` — leaves the agent-card snapshot
    /// empty and synthesises a minimal transcript from the query.
    pub fn from_episode(episode: &Episode) -> Self {
        let transcript = vec![TranscriptTurn {
            role: TranscriptRole::User,
            content: episode.query.clone(),
            speaker_id: None,
        }];

        Self {
            episode_id: episode.episode_id,
            agent_id: episode.agent_id,
            persona_version: episode.persona_version_at_write.unwrap_or(1),
            dyad_id: episode.dyad_id.clone(),
            timestamp_ref: episode.timestamp_ref,
            query: episode.query.clone(),
            transcript,
            goal_spec: None,
            context: episode.context.clone(),
            provenance: episode.provenance,
            authority_weight: episode.authority_weight,
            agent_card: None,
        }
    }

    /// True when this bundle should bypass the evaluator registry
    /// entirely — synthetic corrections at HumanAuthority weight are
    /// already canonical and do not need re-scoring.
    pub fn is_canonical(&self) -> bool {
        self.provenance.is_human_authority()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExecutionStatus;

    fn dummy_episode() -> Episode {
        Episode {
            episode_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            timestamp_ref: Utc::now(),
            query: "What is 2+2?".into(),
            context: serde_json::json!({}),
            execution_status: ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 42,
            tokens_used: Some(100),
            cost_usd: None,
            embedding: None,
            consolidated: false,
            tags: vec![],
            provenance: Provenance::AutoPass,
            authority_weight: 0.5,
            dyad_id: None,
            persona_version_at_write: Some(3),
            provider_used: None,
            model_used: None,
        }
    }

    #[test]
    fn from_episode_sets_persona_version_from_snapshot() {
        let ep = dummy_episode();
        let bundle = EpisodeBundle::from_episode(&ep);
        assert_eq!(bundle.persona_version, 3);
        assert_eq!(bundle.episode_id, ep.episode_id);
        assert_eq!(bundle.transcript.len(), 1);
        assert!(matches!(bundle.transcript[0].role, TranscriptRole::User));
    }

    #[test]
    fn synthetic_correction_is_canonical() {
        let mut ep = dummy_episode();
        ep.provenance = Provenance::SyntheticCorrection;
        ep.authority_weight = 1.0;
        let bundle = EpisodeBundle::from_episode(&ep);
        assert!(bundle.is_canonical());
    }

    #[test]
    fn auto_pass_is_not_canonical() {
        let ep = dummy_episode();
        let bundle = EpisodeBundle::from_episode(&ep);
        assert!(!bundle.is_canonical());
    }
}
