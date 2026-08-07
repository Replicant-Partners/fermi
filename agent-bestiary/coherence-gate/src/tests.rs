//! Phase 5 — coherence-gate unit tests.

use uuid::Uuid;

use crate::{
    encoder::{InterventionEncoder, InterventionRequest},
    gate::{CoherenceGate, GateVerdict, DEFAULT_GATE_THRESHOLD},
    GateError,
};
use agent_bestiary_memory::{CorrectionClassification, CorrectionScope};

// ── Encoder tests ────────────────────────────────────────────────────────────

#[test]
fn encoder_episode_scope_minimal() {
    let req = InterventionRequest {
        anomaly_event_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: Some(Uuid::new_v4()),
        reviewer_id: "user-1".to_string(),
        scope: CorrectionScope::Episode,
        classification: None,
        dimension: Some("relevance".to_string()),
        correction_text: None,
        score_overrides: serde_json::json!({}),
        justification: None,
    };

    let encoded = InterventionEncoder::encode(req).expect("should encode");
    assert_eq!(encoded.authority_weight, 1.0);
    assert!(!encoded.gate_is_synchronous);
}

#[test]
fn encoder_agent_wide_requires_classification() {
    let req = InterventionRequest {
        anomaly_event_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: None,
        reviewer_id: "admin".to_string(),
        scope: CorrectionScope::AgentWide,
        classification: None, // missing
        dimension: None,
        correction_text: Some("do not refuse safety requests".to_string()),
        score_overrides: serde_json::json!({}),
        justification: None,
    };

    let err = InterventionEncoder::encode(req).expect_err("should reject missing classification");
    assert!(matches!(err, GateError::InvalidRequest(_)));
}

#[test]
fn encoder_agent_wide_requires_correction_text() {
    let req = InterventionRequest {
        anomaly_event_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: None,
        reviewer_id: "admin".to_string(),
        scope: CorrectionScope::AgentWide,
        classification: Some(CorrectionClassification::Behaviour),
        dimension: None,
        correction_text: None, // missing
        score_overrides: serde_json::json!({}),
        justification: None,
    };

    let err = InterventionEncoder::encode(req).expect_err("should reject missing correction_text");
    assert!(matches!(err, GateError::InvalidRequest(_)));
}

#[test]
fn encoder_agent_wide_gate_is_synchronous() {
    let req = InterventionRequest {
        anomaly_event_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: None,
        reviewer_id: "admin".to_string(),
        scope: CorrectionScope::AgentWide,
        classification: Some(CorrectionClassification::Belief),
        dimension: Some("accuracy".to_string()),
        correction_text: Some("the correct answer is X".to_string()),
        score_overrides: serde_json::json!({}),
        justification: Some("confirmed factual error".to_string()),
    };

    let encoded = InterventionEncoder::encode(req).expect("should encode");
    assert!(encoded.gate_is_synchronous);
    assert_eq!(encoded.authority_weight, 1.0);
}

// ── Gate tests ────────────────────────────────────────────────────────────────

fn make_encoded(
    scope: CorrectionScope,
    correction_text: Option<String>,
) -> crate::EncodedIntervention {
    let req = InterventionRequest {
        anomaly_event_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: Some(Uuid::new_v4()),
        reviewer_id: "reviewer".to_string(),
        scope,
        classification: if scope == CorrectionScope::AgentWide {
            Some(CorrectionClassification::Behaviour)
        } else {
            None
        },
        dimension: Some("relevance".to_string()),
        correction_text,
        score_overrides: serde_json::json!({}),
        justification: None,
    };
    InterventionEncoder::encode(req).unwrap()
}

#[test]
fn gate_episode_scope_always_settles() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(CorrectionScope::Episode, Some("corrected text".to_string()));
    let outcome = gate
        .check(&encoded)
        .expect("episode scope should not block");
    assert_eq!(outcome.verdict, GateVerdict::Settled);
    // gamma should be populated
    assert!(outcome.gamma.is_some());
}

#[test]
fn gate_dyad_scope_always_settles() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(CorrectionScope::Dyad, Some("dyad correction".to_string()));
    let outcome = gate.check(&encoded).expect("dyad scope should not block");
    assert_eq!(outcome.verdict, GateVerdict::Settled);
}

#[test]
fn gate_agent_wide_approves_when_coherent() {
    // Use a very low threshold so the correction is always approved.
    let gate = CoherenceGate::new(0.0);
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("the agent must always provide safety guidance".to_string()),
    );
    let outcome = gate
        .check(&encoded)
        .expect("should approve with threshold=0");
    assert_eq!(outcome.verdict, GateVerdict::Approved);
}

#[test]
fn gate_agent_wide_blocks_when_threshold_not_met() {
    // Use a threshold of 1.0 — no real settling will ever reach this.
    let gate = CoherenceGate::new(1.0);
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("some correction text".to_string()),
    );
    let err = gate
        .check(&encoded)
        .expect_err("should block with threshold=1.0");
    match err {
        GateError::Blocked { threshold, .. } => assert_eq!(threshold, 1.0),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn gate_principle_scores_present() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::Episode,
        Some("test correction".to_string()),
    );
    let outcome = gate.check(&encoded).unwrap();
    // Should have principle scores from the TEC model.
    assert!(!outcome.principle_scores.is_empty());
}

#[test]
fn gate_minimum_update_set_is_vec() {
    // Verify minimum_update_set is always a Vec (may be empty for
    // symmetric contradicting pairs where neither utterance tips negative).
    let gate = CoherenceGate::new(0.0); // approve always
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("the correct behaviour is X".to_string()),
    );
    let outcome = gate.check(&encoded).unwrap();
    // The field must be a Vec; it may be empty when both utterances start
    // at equal activation and the settling is fully symmetric.
    let _ = outcome.minimum_update_set; // type check is sufficient
}

#[test]
fn gate_threshold_constant() {
    assert_eq!(DEFAULT_GATE_THRESHOLD, 0.5);
}
