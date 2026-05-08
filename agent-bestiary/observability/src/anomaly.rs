//! `AnomalyDetector` — detects four kinds of anomalies and writes to
//! `anomaly_events`.
//!
//! Per Q3 defaults:
//! - **drift**            — `PersonaDriftMonitor` flagged the latest drift vector
//! - **rolling_conflict** — same dimension flagged as conflict in 3+ consecutive entries
//! - **rupture**          — `SocialInteractionTracker` flagged a rupture
//! - **safety**           — any timeline entry's anomaly_flags contains `safety:*`
//!
//! Per Q4 (c) — the detector is invoked by the background scanner, not
//! inline. It batches over the timeline-entry window since the last
//! checkpoint.

use std::sync::Arc;
use uuid::Uuid;

use agent_bestiary_memory::{AnomalyEvent, MemoryStore, TimelineEntry};

use crate::error::ObservabilityError;

/// Default rolling window length for the rolling-conflict detector
/// (per Q3).
pub const ROLLING_CONFLICT_WINDOW: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyKind {
    Drift,
    RollingConflict,
    Rupture,
    Safety,
}

impl AnomalyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AnomalyKind::Drift => "drift",
            AnomalyKind::RollingConflict => "rolling_conflict",
            AnomalyKind::Rupture => "rupture",
            AnomalyKind::Safety => "safety",
        }
    }
}

/// In-memory representation of an anomaly the detector found before
/// it gets persisted as `AnomalyEvent`.
#[derive(Debug, Clone)]
pub struct DetectedAnomaly {
    pub kind: AnomalyKind,
    pub severity: &'static str, // "info" | "warning" | "critical"
    pub agent_id: Uuid,
    pub episode_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub dyad_id: Option<String>,
    pub payload: serde_json::Value,
    pub requires_review: bool,
}

#[derive(Clone)]
pub struct AnomalyDetector {
    store: Arc<MemoryStore>,
    pub rolling_conflict_window: usize,
}

impl AnomalyDetector {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self {
            store,
            rolling_conflict_window: ROLLING_CONFLICT_WINDOW,
        }
    }

    pub fn with_rolling_conflict_window(mut self, n: usize) -> Self {
        self.rolling_conflict_window = n.max(1);
        self
    }

    /// Scan a window of timeline entries (chronological order — oldest
    /// first) and emit anomalies. Caller has already loaded the entries
    /// since the worker's last checkpoint. Pure function — no I/O.
    pub fn detect_in_window(
        &self,
        agent_id: Uuid,
        entries: &[TimelineEntry],
    ) -> Vec<DetectedAnomaly> {
        detect_in_window_with_window(agent_id, entries, self.rolling_conflict_window)
    }
}

/// Pure detection helper that doesn't need a store reference. Allows
/// unit tests to exercise the algorithm without constructing a
/// `MemoryStore`. The instance method [`AnomalyDetector::detect_in_window`]
/// delegates here.
pub fn detect_in_window_with_window(
    agent_id: Uuid,
    entries: &[TimelineEntry],
    rolling_conflict_window: usize,
) -> Vec<DetectedAnomaly> {
    let mut found = Vec::new();
    {

        // ─── Safety: any entry with a safety:* anomaly_flag ──
        for e in entries {
            if let Some(arr) = e.anomaly_flags.as_array() {
                for f in arr {
                    if let Some(s) = f.as_str() {
                        if s.starts_with("safety:") {
                            found.push(DetectedAnomaly {
                                kind: AnomalyKind::Safety,
                                severity: "critical",
                                agent_id,
                                episode_id: e.episode_id,
                                run_id: e.run_id,
                                dyad_id: e.dyad_id.clone(),
                                payload: serde_json::json!({
                                    "flag": s,
                                    "entry_id": e.entry_id,
                                }),
                                requires_review: true,
                            });
                        }
                    }
                }
            }
        }

        // ─── Drift: any entry with `drift:anomalous` flag ──
        // The flag is written by ObservabilityWorker after running
        // PersonaDriftMonitor. We surface it here with full payload
        // detail (drift_norm, threshold).
        for e in entries {
            if let Some(arr) = e.anomaly_flags.as_array() {
                let drift_flag = arr.iter().any(|v| {
                    v.as_str().map(|s| s == "drift:anomalous").unwrap_or(false)
                });
                if drift_flag {
                    found.push(DetectedAnomaly {
                        kind: AnomalyKind::Drift,
                        severity: "warning",
                        agent_id,
                        episode_id: e.episode_id,
                        run_id: e.run_id,
                        dyad_id: e.dyad_id.clone(),
                        payload: serde_json::json!({
                            "drift_norm": e.drift_norm,
                            "persona_version": e.persona_version,
                            "entry_id": e.entry_id,
                        }),
                        requires_review: true,
                    });
                }
            }
        }

        // ─── Rolling conflict: same dimension flagged in N consecutive ──
        // Phase 3 source: timeline entries' `dim_scores` carry the
        // mean per dimension. The conflict signal isn't on the entry
        // directly (it's on `eval_runs.conflict_flags`); we approximate
        // by counting `anomaly_flags` containing `conflict:<dim>`
        // strings written by the scorer when the registry detected one.
        // The simpler implementation: any N-of-last-N window where the
        // same `conflict:<dim>` appears flags it.
        let window_n = rolling_conflict_window;
        if entries.len() >= window_n {
            let tail = &entries[entries.len() - window_n..];
            // Collect dim names that appear in EVERY entry's flags as `conflict:<dim>`.
            use std::collections::HashSet;
            let mut common: Option<HashSet<String>> = None;
            for e in tail {
                let mut dims_in_entry: HashSet<String> = HashSet::new();
                if let Some(arr) = e.anomaly_flags.as_array() {
                    for f in arr {
                        if let Some(s) = f.as_str() {
                            if let Some(dim) = s.strip_prefix("conflict:") {
                                dims_in_entry.insert(dim.to_string());
                            }
                        }
                    }
                }
                common = Some(match common {
                    None => dims_in_entry,
                    Some(prev) => prev.intersection(&dims_in_entry).cloned().collect(),
                });
            }
            if let Some(dims) = common {
                for dim in dims {
                    let last = tail.last().unwrap();
                    found.push(DetectedAnomaly {
                        kind: AnomalyKind::RollingConflict,
                        severity: "warning",
                        agent_id,
                        episode_id: last.episode_id,
                        run_id: last.run_id,
                        dyad_id: last.dyad_id.clone(),
                        payload: serde_json::json!({
                            "dimension": dim,
                            "window_len": window_n,
                            "entry_ids": tail.iter().map(|e| e.entry_id).collect::<Vec<_>>(),
                        }),
                        requires_review: true,
                    });
                }
            }
        }

        // ─── Rupture: written by SocialInteractionTracker ──
        // The tracker emits a separate `rupture:<dyad_id>` flag on the
        // entry when it sees one; we surface it here.
        for e in entries {
            if let Some(arr) = e.anomaly_flags.as_array() {
                for f in arr {
                    if let Some(s) = f.as_str() {
                        if let Some(dyad) = s.strip_prefix("rupture:") {
                            found.push(DetectedAnomaly {
                                kind: AnomalyKind::Rupture,
                                severity: "warning",
                                agent_id,
                                episode_id: e.episode_id,
                                run_id: e.run_id,
                                dyad_id: Some(dyad.to_string()),
                                payload: serde_json::json!({
                                    "entry_id": e.entry_id,
                                }),
                                requires_review: true,
                            });
                        }
                    }
                }
            }
        }
    }

    found
}

impl AnomalyDetector {
    /// Persist a detected anomaly. Returns the new event_id.
    pub async fn persist(
        &self,
        anomaly: &DetectedAnomaly,
    ) -> Result<Uuid, ObservabilityError> {
        let event = AnomalyEvent {
            event_id: Uuid::new_v4(),
            agent_id: anomaly.agent_id,
            episode_id: anomaly.episode_id,
            run_id: anomaly.run_id,
            dyad_id: anomaly.dyad_id.clone(),
            kind: anomaly.kind.as_str().to_string(),
            severity: anomaly.severity.to_string(),
            payload: anomaly.payload.clone(),
            requires_review: anomaly.requires_review,
            resolved_at: None,
            resolved_by: None,
            created_at: chrono::Utc::now(),
        };
        self.store
            .create_anomaly_event(&event)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry(flags: serde_json::Value) -> TimelineEntry {
        TimelineEntry {
            entry_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            episode_id: Some(Uuid::new_v4()),
            run_id: None,
            persona_version: 1,
            dyad_id: None,
            session_id: None,
            provenance: "auto_pass".into(),
            dim_scores: serde_json::json!({}),
            drift_norm: None,
            within_version_cosine: None,
            anomaly_flags: flags,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn safety_flag_detected() {
        let agent_id = Uuid::new_v4();
        let entries = vec![entry(serde_json::json!(["safety:violence"]))];
        let found = detect_in_window_with_window(agent_id, &entries, 3);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, AnomalyKind::Safety);
    }

    #[test]
    fn rolling_conflict_three_consecutive_same_dim() {
        let agent_id = Uuid::new_v4();
        let entries = vec![
            entry(serde_json::json!(["conflict:rapport"])),
            entry(serde_json::json!(["conflict:rapport"])),
            entry(serde_json::json!(["conflict:rapport", "conflict:retention"])),
        ];
        let found = detect_in_window_with_window(agent_id, &entries, 3);
        let kinds: Vec<_> = found.iter().map(|a| a.kind).collect();
        assert!(kinds.contains(&AnomalyKind::RollingConflict));
        let dims: Vec<String> = found
            .iter()
            .filter(|a| a.kind == AnomalyKind::RollingConflict)
            .map(|a| {
                a.payload["dimension"]
                    .as_str()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        assert!(dims.contains(&"rapport".to_string()));
        assert!(!dims.contains(&"retention".to_string()));
    }

    #[test]
    fn rupture_flag_detected_with_dyad_id() {
        let agent_id = Uuid::new_v4();
        let entries = vec![entry(serde_json::json!(["rupture:dyad-abc"]))];
        let found = detect_in_window_with_window(agent_id, &entries, 3);
        let rupture: Vec<_> = found
            .iter()
            .filter(|a| a.kind == AnomalyKind::Rupture)
            .collect();
        assert_eq!(rupture.len(), 1);
        assert_eq!(rupture[0].dyad_id.as_deref(), Some("dyad-abc"));
    }

    #[test]
    fn drift_anomalous_flag_detected() {
        let agent_id = Uuid::new_v4();
        let mut e = entry(serde_json::json!(["drift:anomalous"]));
        e.drift_norm = Some(0.32);
        e.persona_version = 4;
        let found = detect_in_window_with_window(agent_id, &[e], 3);
        assert!(found.iter().any(|a| a.kind == AnomalyKind::Drift));
    }

    #[test]
    fn no_anomalies_when_no_flags() {
        let agent_id = Uuid::new_v4();
        let entries = vec![entry(serde_json::json!([]))];
        let found = detect_in_window_with_window(agent_id, &entries, 3);
        assert!(found.is_empty());
    }
}
