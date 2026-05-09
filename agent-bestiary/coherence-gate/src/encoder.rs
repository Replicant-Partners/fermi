//! Step 2 — Intervention encoder.
//!
//! Converts a raw HITL reviewer action into a structured
//! [`EncodedIntervention`] that the coherence gate and two-write memory
//! pattern can consume.
//!
//! Per architecture doc step 2:
//!   dimension · scope · correction · authority_weight=1.0
//!   provenance: HumanAuthority
//!
//! Decision D10 (OBSERVABILITY_IMPL.md):
//!   - `AgentWide` scope → synchronous gate (blocks on Γ(C) check)
//!   - `Episode` / `Dyad` scope → settler mode (gate advises but does not block)

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use agent_bestiary_memory::{CorrectionClassification, CorrectionScope, Provenance};

use crate::error::GateError;

/// Raw inputs from the HITL action that triggered the intervention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionRequest {
    /// The anomaly event that triggered the review.
    pub anomaly_event_id: Uuid,
    /// The agent being corrected.
    pub agent_id: Uuid,
    /// The episode associated with the anomaly (may be `None` for dyad/drift
    /// anomalies that don't point to a single episode).
    pub episode_id: Option<Uuid>,
    /// Reviewer identity (user_id string).
    pub reviewer_id: String,

    /// Scope of the correction.
    pub scope: CorrectionScope,
    /// Belief vs. behavioural classification. Required for `AgentWide`;
    /// optional for `Episode`/`Dyad`.
    pub classification: Option<CorrectionClassification>,

    /// The dimension being corrected (e.g. "social_capital", "persona_fidelity").
    pub dimension: Option<String>,
    /// Free-text description of what the corrected response should have been.
    pub correction_text: Option<String>,
    /// Score overrides for relabelling (carried through for audit).
    #[serde(default)]
    pub score_overrides: serde_json::Value,
    /// Human-readable justification.
    pub justification: Option<String>,
}

/// Structured, validated corrective signal ready to be passed to the
/// coherence gate and two-write memory pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodedIntervention {
    pub anomaly_event_id: Uuid,
    pub agent_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub reviewer_id: String,

    pub scope: CorrectionScope,
    pub classification: Option<CorrectionClassification>,

    pub dimension: Option<String>,
    pub correction_text: Option<String>,
    pub score_overrides: serde_json::Value,
    pub justification: Option<String>,

    /// Always `1.0` for human-originated interventions (HumanAuthority).
    pub authority_weight: f64,
    /// Always `HumanCorrected` for the original episode annotation write.
    pub provenance: Provenance,

    /// Whether the gate should block on Γ(C) < threshold (D10).
    /// `true` for `AgentWide`; `false` (settler mode) for `Episode`/`Dyad`.
    pub gate_is_synchronous: bool,
}

/// Encodes and validates an [`InterventionRequest`] into an
/// [`EncodedIntervention`].
pub struct InterventionEncoder;

impl InterventionEncoder {
    /// Validate and encode the request.
    ///
    /// Returns `Err(GateError::InvalidRequest)` when:
    /// - `AgentWide` scope is used without a `classification`
    /// - `AgentWide` scope is used without `correction_text`
    pub fn encode(req: InterventionRequest) -> Result<EncodedIntervention, GateError> {
        // AgentWide is the most destructive scope — enforce richer inputs.
        if req.scope == CorrectionScope::AgentWide {
            if req.classification.is_none() {
                return Err(GateError::InvalidRequest(
                    "agent_wide intervention requires a classification (belief | behaviour)"
                        .to_string(),
                ));
            }
            if req.correction_text.is_none() {
                return Err(GateError::InvalidRequest(
                    "agent_wide intervention requires correction_text describing the desired behaviour"
                        .to_string(),
                ));
            }
        }

        let gate_is_synchronous = req.scope == CorrectionScope::AgentWide;

        Ok(EncodedIntervention {
            anomaly_event_id: req.anomaly_event_id,
            agent_id: req.agent_id,
            episode_id: req.episode_id,
            reviewer_id: req.reviewer_id,
            scope: req.scope,
            classification: req.classification,
            dimension: req.dimension,
            correction_text: req.correction_text,
            score_overrides: req.score_overrides,
            justification: req.justification,
            authority_weight: 1.0,
            provenance: Provenance::HumanCorrected,
            gate_is_synchronous,
        })
    }
}
