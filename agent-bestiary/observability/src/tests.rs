//! Integration smoke tests — exercise the pure-function paths of the
//! Plane C algorithms together. Storage-bound paths (worker, scorer,
//! store-backed drift) are covered by handler-level tests once Phase 4
//! ships an HTTP surface.

use chrono::Utc;
use uuid::Uuid;

use crate::anomaly::{detect_in_window_with_window, AnomalyKind};
use crate::drift::{cosine_similarity, DriftThreshold};
use crate::social::detect_rupture;
use crate::trend::compute_series;
use agent_bestiary_memory::TimelineEntry;

fn entry(
    persona_version: i32,
    dim_scores: serde_json::Value,
    flags: serde_json::Value,
    drift_norm: Option<f64>,
) -> TimelineEntry {
    TimelineEntry {
        entry_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        episode_id: Some(Uuid::new_v4()),
        run_id: None,
        persona_version,
        dyad_id: None,
        session_id: None,
        provenance: "auto_pass".into(),
        dim_scores,
        drift_norm,
        within_version_cosine: None,
        anomaly_flags: flags,
        created_at: Utc::now(),
    }
}

#[test]
fn cosine_drift_above_threshold_is_anomalous() {
    let a = vec![1.0_f32, 0.0, 0.0, 0.0];
    let b = vec![0.0_f32, 1.0, 0.0, 0.0];
    let cos = cosine_similarity(&a, &b).unwrap();
    let drift = 1.0 - cos;
    assert!((drift - 1.0).abs() < 1e-9);

    let t = DriftThreshold::Static(0.20);
    assert!(t.is_anomalous(drift, &[]));
}

#[test]
fn series_then_anomaly_pipeline() {
    let agent_id = Uuid::new_v4();
    let entries = vec![
        entry(
            1,
            serde_json::json!({ "rapport": 0.6 }),
            serde_json::json!(["conflict:rapport"]),
            None,
        ),
        entry(
            1,
            serde_json::json!({ "rapport": 0.5 }),
            serde_json::json!(["conflict:rapport"]),
            None,
        ),
        entry(
            2,
            serde_json::json!({ "rapport": 0.45 }),
            serde_json::json!(["conflict:rapport", "drift:anomalous"]),
            Some(0.32),
        ),
    ];

    // Trend should compute a single dimension across all three.
    let series = compute_series(&entries);
    let s = series.get("rapport").expect("rapport series present");
    assert_eq!(s.n, 3);
    assert!(s.mean > 0.4 && s.mean < 0.6);

    // Anomalies should fire: rolling_conflict on rapport + drift.
    let found = detect_in_window_with_window(agent_id, &entries, 3);
    let kinds: Vec<_> = found.iter().map(|a| a.kind).collect();
    assert!(kinds.contains(&AnomalyKind::RollingConflict));
    assert!(kinds.contains(&AnomalyKind::Drift));
}

#[test]
fn rupture_detector_independent_of_other_signals() {
    let history = vec![0.85, 0.82, 0.50];
    let (rupture, drop) = detect_rupture(&history);
    assert!(rupture);
    assert!(drop > 0.30);
}
