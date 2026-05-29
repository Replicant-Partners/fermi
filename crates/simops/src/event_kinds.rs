//! Standard SimOps workspace event kinds — spec 36a §C.3.
//!
//! kask's `SimOpsLog.append` already supports fold-from-events for agent
//! observations (C5.6.11). This module defines the canonical event kind
//! schema for the remaining derived state types so kask can fold them
//! deterministically and eliminate module-scope `let _X` variables.
//!
//! ## Pattern (existing, correct — C5.6.11)
//!
//! ```text
//! SimOpsLog.append(workspaceId, {
//!   kind: "agent_observation.received",
//!   stage_id: "...",
//!   observation: { ... }
//! })
//! ```
//!
//! kask folds `_agentObservationsByStage` from these events on mount.
//! Rejected-stays-rejected across reloads.
//!
//! ## New event kinds (Phase 5)
//!
//! Each kind maps to one `let _X` in simops-page.js §C.3 that should
//! become a fold over ABW events instead.
//!
//! ### Sensor proposals
//!
//! ```json,ignore
//! {
//!   "kind": "sensor.proposal.received",
//!   "stage_id": "primary_fermentation",
//!   "proposal_id": "uuid",
//!   "source_agent": "sensor_advisor",
//!   "proposal": { "sensor_id": "...", "observes": "...", "sosa_uri": "..." }
//! }
//! ```
//! ```json,ignore
//! {
//!   "kind": "sensor.proposal.dismissed",
//!   "proposal_id": "uuid",
//!   "dismissed_by": "user_id",
//!   "reason": "..." (optional)
//! }
//! ```
//!
//! Folds to: `_sensorProposals: Map<stage_id, ProposalRecord[]>`.
//! Rule: dismissed proposals are excluded from the fold output.
//!
//! ### Energy proposals
//!
//! ```json,ignore
//! {
//!   "kind": "energy.proposal.received",
//!   "stage_id": "...",
//!   "proposal_id": "uuid",
//!   "source_agent": "energy_advisor",
//!   "proposal": {
//!     "power_kwh_per_input_kg": 0.12,
//!     "labor_hours_per_input_kg": 0.001,
//!     "carbon_intensity": { "mode": "synthetic", "value": 0.05 },
//!     "rationale": "...",
//!     "confidence": 0.75,
//!     "typical_range": [0.08, 0.18]
//!   }
//! }
//! ```
//! ```json,ignore
//! { "kind": "energy.proposal.accepted", "proposal_id": "uuid", "accepted_by": "user_id" }
//! ```
//! ```json,ignore
//! { "kind": "energy.proposal.overridden", "proposal_id": "uuid",
//!   "overridden_value": 0.15, "overridden_by": "user_id" }
//! ```
//!
//! Folds to: `_energyProposals: Map<stage_id, ProposalRecord>`.
//! Rule: latest-wins per stage_id (accepted/overridden replaces received).
//!
//! ### Twin projection
//!
//! ```json,ignore
//! {
//!   "kind": "twin.projection.computed",
//!   "twin_id": "primary",
//!   "projection_id": "uuid",
//!   "model_uris": ["kask:dynamics/kombucha_fermentation@v1"],
//!   "trajectories": { ... },
//!   "derived_quantities": [...],
//!   "valid_until": "2026-05-30T02:00:00Z",
//!   "computed_at": "2026-05-30T00:00:00Z"
//! }
//! ```
//!
//! Folds to: `_twinSnapshot` + `_twinFetchedAt`. Rule: most recent event
//! by `computed_at` wins. `valid_until` drives TTL without a client timer.
//!
//! ### Rule evaluation
//!
//! ```json,ignore
//! {
//!   "kind": "rule.evaluated",
//!   "rule_id": "harvest_ready",
//!   "twin_id": "primary",
//!   "instance_id": "vessel_A",
//!   "fired": true,
//!   "evidence": { "brix": 1.2, "days_elapsed": 14 },
//!   "evaluated_at": "2026-05-30T00:00:00Z"
//! }
//! ```
//!
//! Folds to: `_expectedFires: Map<rule_id, EvaluationRecord[]>`.
//! `_lastEvaluations: Map<rule_id, EvaluationRecord>` takes last per rule.
//!
//! ### Calibration result
//!
//! ```json,ignore
//! {
//!   "kind": "calibration.result.received",
//!   "rule_id": "harvest_ready",
//!   "twin_id": "primary",
//!   "result": {
//!     "calibrated_threshold": 1.5,
//!     "confidence": 0.82,
//!     "sample_size": 14,
//!     "source_agent": "calibration_advisor"
//!   },
//!   "received_at": "2026-05-30T00:00:00Z"
//! }
//! ```
//!
//! Folds to: `_calibrationResults: Map<rule_id, ResultRecord>`.
//! Rule: latest-wins per rule_id.
//!
//! ## Fold invariant (shared across all kinds)
//!
//! The C5.6.11 pattern applies to every kind above:
//! 1. On workspace mount, fetch all workspace messages where `kind ∈ known_kinds`.
//! 2. Replay in `created_at` order.
//! 3. Each kind's fold function produces a deterministic in-memory state.
//! 4. Side-effect writers become `SimOpsLog.append` callers.
//! 5. Derived state survives page reload (it's reconstructed from events).
//!
//! ## ABW contract
//!
//! ABW accepts any workspace message with these `kind` values without
//! special handling — they are stored in `workspace_messages` like any
//! other event-type message. ABW does NOT process or validate their
//! payload beyond JSON well-formedness. kask owns the fold logic.
//!
//! The schemas above are the authoritative contract. kask's `simops-folds.js`
//! implements the fold functions; ABW stores the events.

use serde::{Deserialize, Serialize};

/// The complete set of standard SimOps event kinds.
/// These are stored as workspace messages with `message_type: "event_append"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimOpsEventKind {
    // ── Existing (C5.6.11) ────────────────────────────────────────────────────
    AgentObservationReceived,
    AgentObservationDismissed,
    InsightArchived,
    InsightAccepted,
    InsightSnoozed,

    // ── Sensor proposals (Phase 5) ────────────────────────────────────────────
    SensorProposalReceived,
    SensorProposalDismissed,
    SensorProposalAccepted,

    // ── Energy proposals (Phase 5) ────────────────────────────────────────────
    EnergyProposalReceived,
    EnergyProposalAccepted,
    EnergyProposalOverridden,

    // ── Twin projections (Phase 5) ────────────────────────────────────────────
    TwinProjectionComputed,

    // ── Rule evaluation (Phase 5) ─────────────────────────────────────────────
    RuleEvaluated,

    // ── Calibration (Phase 5) ─────────────────────────────────────────────────
    CalibrationResultReceived,

    // ── Process lifecycle (existing) ──────────────────────────────────────────
    CascadeRan,
    ProcessSaved,
    VariationForked,
}

impl SimOpsEventKind {
    /// The string value as used in `workspace_messages.kind`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentObservationReceived => "agent_observation.received",
            Self::AgentObservationDismissed => "agent_observation.dismissed",
            Self::InsightArchived => "insight.archived",
            Self::InsightAccepted => "insight.accepted",
            Self::InsightSnoozed => "insight.snoozed",
            Self::SensorProposalReceived => "sensor.proposal.received",
            Self::SensorProposalDismissed => "sensor.proposal.dismissed",
            Self::SensorProposalAccepted => "sensor.proposal.accepted",
            Self::EnergyProposalReceived => "energy.proposal.received",
            Self::EnergyProposalAccepted => "energy.proposal.accepted",
            Self::EnergyProposalOverridden => "energy.proposal.overridden",
            Self::TwinProjectionComputed => "twin.projection.computed",
            Self::RuleEvaluated => "rule.evaluated",
            Self::CalibrationResultReceived => "calibration.result.received",
            Self::CascadeRan => "cascade.ran",
            Self::ProcessSaved => "process.saved",
            Self::VariationForked => "variation.forked",
        }
    }

    /// All Phase 5 event kinds (the new ones kask needs to fold from events).
    pub fn phase5_kinds() -> &'static [&'static str] {
        &[
            "sensor.proposal.received",
            "sensor.proposal.dismissed",
            "sensor.proposal.accepted",
            "energy.proposal.received",
            "energy.proposal.accepted",
            "energy.proposal.overridden",
            "twin.projection.computed",
            "rule.evaluated",
            "calibration.result.received",
        ]
    }

    /// All known SimOps event kinds (for workspace message filtering).
    pub fn all_kinds() -> &'static [&'static str] {
        &[
            "agent_observation.received",
            "agent_observation.dismissed",
            "insight.archived",
            "insight.accepted",
            "insight.snoozed",
            "sensor.proposal.received",
            "sensor.proposal.dismissed",
            "sensor.proposal.accepted",
            "energy.proposal.received",
            "energy.proposal.accepted",
            "energy.proposal.overridden",
            "twin.projection.computed",
            "rule.evaluated",
            "calibration.result.received",
            "cascade.ran",
            "process.saved",
            "variation.forked",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_phase5_kinds_have_dot_separator() {
        for kind in SimOpsEventKind::phase5_kinds() {
            assert!(kind.contains('.'), "Phase 5 event kind must use dot separator: {kind}");
        }
    }

    #[test]
    fn no_duplicate_kind_strings() {
        let all = SimOpsEventKind::all_kinds();
        let unique: std::collections::HashSet<_> = all.iter().collect();
        assert_eq!(all.len(), unique.len(), "Duplicate event kind strings found");
    }

    #[test]
    fn sensor_proposal_round_trips() {
        let kind = SimOpsEventKind::SensorProposalReceived;
        assert_eq!(kind.as_str(), "sensor.proposal.received");
    }
}
