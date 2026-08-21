//! Phase 5 — coherence-gate unit tests.

use uuid::Uuid;

use crate::{
    encoder::{InterventionEncoder, InterventionRequest},
    gate::{CoherenceGate, GateVerdict, WorldModel, WorldNode, DEFAULT_GATE_THRESHOLD},
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
        justification: Some("reviewer cited GBIF occurrence 1234".to_string()),
    };
    InterventionEncoder::encode(req).unwrap()
}

// ── World-model fixtures ────────────────────────────────────────
//
// An agent that has observed four things and concluded three rules from them.
// The two fixtures differ only in *which* observation the correction overturns,
// which is the whole point: the same correction text against the same agent
// must be judged differently depending on how much rests on the thing it
// contradicts.

fn obs(id: &str, text: &str) -> WorldNode {
    WorldNode {
        id: id.to_string(),
        text: text.to_string(),
        grounded: true,
        derived_from: vec![],
    }
}

fn rule(id: &str, text: &str, from: Vec<usize>) -> WorldNode {
    WorldNode {
        id: id.to_string(),
        text: text.to_string(),
        grounded: false,
        derived_from: from,
    }
}

fn base_nodes() -> Vec<WorldNode> {
    vec![
        obs(
            "e0",
            "observed: GBIF returned Antaxius beieri, family Tettigoniidae",
        ),
        obs(
            "e1",
            "observed: occurrence records cluster in alpine meadow",
        ),
        obs("e2", "observed: no genome assembly exists for this species"),
        obs(
            "e3",
            "observed: a single unrelated sighting near a car park",
        ),
        // Three rules, all resting on e0 — it is load-bearing.
        rule("r0", "bush-crickets in this genus are alpine", vec![0, 1]),
        rule(
            "r1",
            "identify Tettigoniidae before describing morphology",
            vec![0],
        ),
        rule(
            "r2",
            "do not assert genome size without an assembly",
            vec![0, 2],
        ),
    ]
}

/// The correction overturns `e3` — one stray observation nothing was built on.
fn peripheral_world() -> WorldModel {
    WorldModel {
        nodes: base_nodes(),
        target: Some(3),
    }
}

/// The correction overturns `e0` — the observation all three rules rest on.
fn load_bearing_world() -> WorldModel {
    WorldModel {
        nodes: base_nodes(),
        target: Some(0),
    }
}

// ── The tests that were missing ───────────────────────────────────
//
// Every gate test below runs at `DEFAULT_GATE_THRESHOLD`. The previous suite
// tested only at 0.0 and 1.0, which is why nobody noticed that the production
// threshold could never be met: the gate blocked every agent-wide correction
// for arithmetic reasons, and the one control that mattered — two independent
// reviewers — sat downstream of it and was therefore unreachable.
//
// Rule 5.1: a check that has never failed has not been tested. These are the
// pair — one that must pass and one that must block, both at 0.5.

#[test]
fn a_correction_to_a_peripheral_belief_is_approved_at_the_production_threshold() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("that car park sighting was a misidentification".to_string()),
    );
    let outcome = gate
        .check_against(&encoded, &peripheral_world())
        .expect("a correction to a belief nothing rests on must be absorbable");

    assert_eq!(outcome.verdict, GateVerdict::Approved);
    let gamma = outcome.gamma.expect("a settled system must score");
    assert!(
        gamma >= DEFAULT_GATE_THRESHOLD,
        "gamma {gamma} below the production threshold"
    );
}

#[test]
fn a_correction_to_a_load_bearing_belief_is_blocked_at_the_production_threshold() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("this species is a longhorn beetle, not a bush-cricket".to_string()),
    );
    let err = gate
        .check_against(&encoded, &load_bearing_world())
        .expect_err("overturning the belief three rules rest on must not pass silently");

    match err {
        GateError::Blocked {
            gamma, threshold, ..
        } => {
            assert_eq!(threshold, DEFAULT_GATE_THRESHOLD);
            // Blocked while gamma is comfortably ABOVE the threshold. That is
            // the finding, not an anomaly: the system stayed coherent by
            // rejecting the correction, so a gate reading only gamma would
            // have waved this through.
            assert!(
                gamma >= DEFAULT_GATE_THRESHOLD,
                "expected a healthy gamma alongside a rejected correction, got {gamma}"
            );
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

/// The two fixtures differ only in the target index. If the verdict did not
/// depend on *which* belief is overturned, the gate would not be reading the
/// world model at all — which was the original defect.
#[test]
fn the_verdict_depends_on_which_belief_is_corrected() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("identical correction text".to_string()),
    );
    let peripheral = gate.check_against(&encoded, &peripheral_world());
    let load_bearing = gate.check_against(&encoded, &load_bearing_world());

    assert!(
        peripheral.is_ok() && load_bearing.is_err(),
        "same text, same agent, same threshold produced the same verdict for a \
         peripheral and a load-bearing belief — the gate is ignoring the world model"
    );
}

/// Γ is reported, but it is **not** what decides an agent-wide correction, and
/// this pins the measurement that established why.
///
/// A settled system that rejects a contradicting proposition stays perfectly
/// coherent. Γ comes out identical for a correction the agent absorbs and one
/// it throws out; what differs is the correction's own activation. Anyone
/// tempted to simplify the gate back to "Γ < threshold" has to delete this test
/// first, and the deletion will not look like cleanup.
#[test]
fn gamma_alone_cannot_tell_the_two_cases_apart() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::Episode, // settler mode: never blocks, so both return Ok
        Some("identical correction text".to_string()),
    );
    let p = gate.check_against(&encoded, &peripheral_world()).unwrap();
    let l = gate.check_against(&encoded, &load_bearing_world()).unwrap();

    let (pg, lg) = (p.gamma.unwrap(), l.gamma.unwrap());
    assert!(
        (pg - lg).abs() < 0.01,
        "gamma unexpectedly discriminates ({pg} vs {lg}) — if this now holds, the \
         gate could be simplified, but check why before doing it"
    );

    assert!(
        p.correction_activation.is_some_and(|a| a >= 0.0),
        "a correction to a peripheral belief should be absorbed"
    );
    assert!(
        l.correction_activation.is_some_and(|a| a < 0.0),
        "a correction overturning a load-bearing belief should be rejected by the model"
    );
}

/// Silence is not a verdict. An agent with nothing recorded has neither passed
/// a coherence check nor failed one, and the gate must refuse to pretend
/// otherwise in either direction.
#[test]
fn an_absent_world_model_is_undetermined_not_approved() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(
        CorrectionScope::AgentWide,
        Some("correction against an agent we know nothing about".to_string()),
    );
    let outcome = gate
        .check(&encoded)
        .expect("an empty world model is not an error");
    assert_eq!(outcome.verdict, GateVerdict::Undetermined);
    assert!(outcome.gamma.is_none());
}

#[test]
fn gate_episode_scope_always_settles() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(CorrectionScope::Episode, Some("corrected text".to_string()));
    let outcome = gate
        .check_against(&encoded, &peripheral_world())
        .expect("episode scope should not block");
    assert_eq!(outcome.verdict, GateVerdict::Settled);
    // gamma should be populated
    assert!(outcome.gamma.is_some());
}

#[test]
fn gate_dyad_scope_always_settles() {
    let gate = CoherenceGate::default();
    let encoded = make_encoded(CorrectionScope::Dyad, Some("dyad correction".to_string()));
    let outcome = gate
        .check_against(&encoded, &peripheral_world())
        .expect("dyad scope should not block");
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
        .check_against(&encoded, &peripheral_world())
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
        .check_against(&encoded, &peripheral_world())
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
    let outcome = gate.check_against(&encoded, &peripheral_world()).unwrap();
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
    let outcome = gate.check_against(&encoded, &peripheral_world()).unwrap();
    // The field must be a Vec; it may be empty when both utterances start
    // at equal activation and the settling is fully symmetric.
    let _ = outcome.minimum_update_set; // type check is sufficient
}

#[test]
fn gate_threshold_constant() {
    assert_eq!(DEFAULT_GATE_THRESHOLD, 0.5);
}
