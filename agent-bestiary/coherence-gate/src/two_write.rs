//! Step 4 — Two-write memory pattern.
//!
//! For every approved intervention we perform exactly two writes to episodic
//! memory, as specified by the architecture doc:
//!
//! **Write 1 — Annotation (immutable)**
//!   The original episode stays untouched (DB trigger blocks UPDATE).
//!   We append a row to `episode_corrections` with the reviewer's decision,
//!   scope, classification, coherence-gate outcome, and minimum update set.
//!   `provenance` on the correction: `HumanCorrected`.
//!
//! **Write 2 — Synthetic corrected episode**
//!   A new `Episode` row that is structurally identical to the original but
//!   carries the corrected query/response, `provenance = SyntheticCorrection`,
//!   and `authority_weight = 1.0` (HumanAuthority — cannot be averaged down
//!   by lower-confidence subsequent observations).
//!   The `episode_corrections.synthetic_episode_id` field is back-filled after
//!   this write so the audit trail points to the synthetic episode.
//!
//! For `AgentWide` scope the method also calls
//! `MemoryStore::bump_persona_version` so the drift monitor has a new baseline.

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agent_bestiary_memory::{
    CorrectionScope, Episode, EpisodeCorrection, ExecutionStatus, MemoryStore, Provenance,
    ReviewerAction,
};

use crate::encoder::EncodedIntervention;
use crate::error::GateError;
use crate::gate::GateOutcome;

/// Receipt returned after a successful two-write operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TwoWriteReceipt {
    /// The correction record id (`episode_corrections.correction_id`).
    pub correction_id: Uuid,
    /// The synthetic corrected episode id (`episodes.episode_id`).
    pub synthetic_episode_id: Uuid,
    /// Whether the agent's `persona_version` was bumped (only for
    /// `AgentWide` scope).
    pub persona_version_bumped: bool,
    /// The new persona_version after the bump (if any).
    pub new_persona_version: Option<i32>,
}

/// Executes the two-write memory pattern (step 4).
pub struct TwoWriteMemory {
    store: Arc<MemoryStore>,
}

impl TwoWriteMemory {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    /// Run both writes.
    ///
    /// `original_episode` is the episode that triggered the anomaly.  
    /// When the anomaly has no linked episode (e.g. a drift event not
    /// tied to a single episode), `original_episode` may be `None` and
    /// we synthesise a minimal stub episode instead.
    pub async fn execute(
        &self,
        intervention: &EncodedIntervention,
        gate_outcome: &GateOutcome,
        original_episode: Option<Episode>,
    ) -> Result<TwoWriteReceipt, GateError> {
        // ── Write 2 first so we have the synthetic_episode_id for Write 1 ──

        let synthetic_episode =
            self.build_synthetic_episode(intervention, original_episode.as_ref())?;
        let synthetic_episode_id = synthetic_episode.episode_id;
        self.store.store_episode(synthetic_episode).await?;

        // ── Write 1 — annotation on the original episode ────────────────

        let correction_id = Uuid::new_v4();
        let episode_id_for_correction = intervention
            .episode_id
            .or_else(|| original_episode.as_ref().map(|e| e.episode_id))
            .unwrap_or_else(|| {
                // No linked episode — use the synthetic episode as the
                // anchor (best-effort; the real episode may not exist).
                synthetic_episode_id
            });

        let coherence_check_json = serde_json::to_value(gate_outcome)
            .unwrap_or(serde_json::Value::Null);

        let minimum_update_set_json = serde_json::to_value(&gate_outcome.minimum_update_set)
            .unwrap_or(serde_json::Value::Null);

        let tensions_json =
            serde_json::to_value(&gate_outcome.tensions).unwrap_or(serde_json::Value::Null);

        let correction = EpisodeCorrection {
            correction_id,
            episode_id: episode_id_for_correction,
            agent_id: intervention.agent_id,
            reviewer_id: intervention.reviewer_id.clone(),
            reviewer_action: ReviewerAction::Intervene,
            scope: intervention.scope,
            classification: intervention.classification,
            dimension: intervention.dimension.clone(),
            correction_text: intervention.correction_text.clone(),
            score_overrides: intervention.score_overrides.clone(),
            coherence_check: Some(coherence_check_json),
            minimum_update_set: Some(minimum_update_set_json),
            tensions_flagged: Some(tensions_json),
            synthetic_episode_id: Some(synthetic_episode_id),
            authority_weight: intervention.authority_weight,
            // persona_version_bump is filled after we bump the version.
            persona_version_bump: None,
            justification: intervention.justification.clone(),
            created_at: Utc::now(),
        };

        self.store.create_episode_correction(&correction).await?;

        // ── Persona version bump for AgentWide scope ─────────────────────

        let (persona_version_bumped, new_persona_version) =
            if intervention.scope == CorrectionScope::AgentWide {
                let new_version = self
                    .store
                    .bump_persona_version(intervention.agent_id)
                    .await?;
                (true, Some(new_version))
            } else {
                (false, None)
            };

        Ok(TwoWriteReceipt {
            correction_id,
            synthetic_episode_id,
            persona_version_bumped,
            new_persona_version,
        })
    }

    // ── Internal ────────────────────────────────────────────────────

    fn build_synthetic_episode(
        &self,
        intervention: &EncodedIntervention,
        original: Option<&Episode>,
    ) -> Result<Episode, GateError> {
        let corrected_text = intervention
            .correction_text
            .clone()
            .unwrap_or_else(|| "corrected response (no text provided)".to_string());

        // Build the synthetic episode by copying the original where possible.
        Ok(Episode {
            episode_id: Uuid::new_v4(),
            agent_id: intervention.agent_id,
            timestamp_ref: Utc::now(),

            // The query stays the same — the corrected *response* is the
            // change, encoded in `context.corrected_response`.
            query: original
                .map(|e| e.query.clone())
                .unwrap_or_else(|| format!("corrected by reviewer {}", intervention.reviewer_id)),

            context: serde_json::json!({
                "corrected_response": corrected_text,
                "original_episode_id": intervention.episode_id,
                "reviewer_id": intervention.reviewer_id,
                "scope": intervention.scope.to_string(),
                "classification": intervention.classification.map(|c| c.to_string()),
                "dimension": intervention.dimension,
                "correction_type": "synthetic_correction",
            }),

            execution_status: ExecutionStatus::Success,
            error_details: None,
            execution_time_ms: 0,
            tokens_used: None,
            cost_usd: None,
            embedding: None, // will be re-embedded by the consolidation worker
            consolidated: false,
            tags: vec![
                "synthetic_correction".to_string(),
                "hitl_intervention".to_string(),
                intervention.scope.to_string(),
            ],

            // Phase 0 observability fields — HumanAuthority settings.
            provenance: Provenance::SyntheticCorrection,
            authority_weight: intervention.authority_weight, // 1.0
            dyad_id: original.and_then(|e| e.dyad_id.clone()),
            persona_version_at_write: original
                .and_then(|e| e.persona_version_at_write),
            provider_used: original.and_then(|e| e.provider_used.clone()),
            model_used: original.and_then(|e| e.model_used.clone()),
        })
    }
}
