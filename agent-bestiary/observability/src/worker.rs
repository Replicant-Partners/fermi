//! `ObservabilityWorker` — scans timeline entries since the last
//! checkpoint, runs drift + anomaly detection + dyad updates, then
//! persists the results.
//!
//! Per Q4 (c) — hybrid scheduling: the **inline** scorer
//! (`EpisodeScorer::write_inline`) is what's hot-path; this worker
//! runs on demand. Cadence mirrors `ConsolidationWorker`'s on-demand
//! pattern (HTTP trigger or post-eval-run hook), not a periodic timer.

use std::sync::Arc;
use std::time::Instant;

use uuid::Uuid;

use agent_bestiary_memory::MemoryStore;

use crate::anomaly::{AnomalyDetector, DetectedAnomaly};
use crate::drift::{DriftThreshold, PersonaDriftMonitor};
use crate::error::ObservabilityError;

/// Summary of a scan run — surfaced to operators in the
/// per-agent observability page (Phase 4).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanReport {
    pub agent_id: Uuid,
    pub entries_scanned: usize,
    pub anomalies_detected: usize,
    pub drift_computations: usize,
    pub duration_ms: u64,
}

/// Background scanner. Cheap to construct; share via `Arc::clone`.
#[derive(Clone)]
pub struct ObservabilityWorker {
    store: Arc<MemoryStore>,
    drift: PersonaDriftMonitor,
    anomaly: AnomalyDetector,
    /// Maximum number of entries to process in a single scan.
    pub batch_size: i64,
}

impl ObservabilityWorker {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        let drift = PersonaDriftMonitor::new(store.clone());
        let anomaly = AnomalyDetector::new(store.clone());
        Self {
            store,
            drift,
            anomaly,
            batch_size: 200,
        }
    }

    pub fn with_drift_threshold(mut self, threshold: DriftThreshold) -> Self {
        self.drift = self.drift.with_threshold(threshold);
        self
    }

    pub fn with_batch_size(mut self, n: i64) -> Self {
        self.batch_size = n.max(1);
        self
    }

    /// Scan one agent's timeline since its last checkpoint. Updates
    /// drift fields on entries, runs anomaly detection over the
    /// resulting window, persists found anomalies, and advances the
    /// checkpoint.
    pub async fn scan_agent(&self, agent_id: Uuid) -> Result<ScanReport, ObservabilityError> {
        let start = Instant::now();

        let mut state = self
            .store
            .get_agent_observability_state(agent_id)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?
            .unwrap_or_else(|| agent_bestiary_memory::AgentObservabilityState {
                agent_id,
                last_scanned_entry_id: None,
                last_scan_started_at: None,
                last_scan_completed_at: None,
                last_scan_duration_ms: None,
                timeline_entry_count: 0,
                anomaly_event_count: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            });

        state.last_scan_started_at = Some(chrono::Utc::now());

        // Pull entries since checkpoint, oldest-first.
        let entries = self
            .store
            .list_timeline_entries_since(agent_id, state.last_scanned_entry_id, self.batch_size)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        let entries_scanned = entries.len();
        let mut drift_computations = 0usize;
        let mut anomalies_detected = 0usize;

        // ── Pass 1: compute drift per (entry, persona_version) ──
        //
        // For each entry, compare its persona_version's mean
        // embedding to the previous persona_version's mean. We only
        // recompute drift when persona_version changes within the
        // window or when the entry's drift_norm is null (i.e. the
        // inline scorer wrote it but we haven't filled drift yet).
        let mut recent_drift_norms: Vec<f64> = Vec::new();
        for entry in &entries {
            if entry.drift_norm.is_some() {
                if let Some(d) = entry.drift_norm {
                    recent_drift_norms.push(d);
                }
                continue;
            }

            // No previous baseline → drift undefined for v1.
            if entry.persona_version <= 1 {
                continue;
            }

            let drift = self
                .drift
                .compute(
                    agent_id,
                    entry.persona_version - 1,
                    entry.persona_version,
                    &recent_drift_norms,
                )
                .await;

            match drift {
                Ok(v) => {
                    drift_computations += 1;
                    let mut flags: Vec<serde_json::Value> =
                        entry.anomaly_flags.as_array().cloned().unwrap_or_default();
                    if v.anomalous {
                        flags.push(serde_json::Value::String("drift:anomalous".into()));
                    }
                    let updated_flags = serde_json::Value::Array(flags);
                    self.store
                        .update_timeline_drift_anomaly(
                            entry.entry_id,
                            Some(v.norm),
                            None,
                            &updated_flags,
                        )
                        .await
                        .map_err(|e| ObservabilityError::Storage(e.to_string()))?;
                    recent_drift_norms.push(v.norm);
                }
                Err(e) if e.is_inapplicable() => {
                    // No baseline yet — leave drift_norm null.
                }
                Err(e) => return Err(e),
            }
        }

        // ── Pass 2: anomaly detection over the (now-updated) window ──
        //
        // Re-fetch entries so flags written in pass 1 are visible to
        // the detector. This is cheap (single index scan); the
        // alternative — mutating local copies — risks divergence.
        let refreshed = self
            .store
            .list_timeline_entries_since(agent_id, state.last_scanned_entry_id, self.batch_size)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        let detected: Vec<DetectedAnomaly> = self.anomaly.detect_in_window(agent_id, &refreshed);

        for a in &detected {
            self.anomaly
                .persist(a)
                .await
                .map_err(|e| ObservabilityError::Storage(e.to_string()))?;
            anomalies_detected += 1;
        }

        // ── Advance checkpoint ──
        if let Some(last) = refreshed.last() {
            state.last_scanned_entry_id = Some(last.entry_id);
        }
        let duration_ms = start.elapsed().as_millis() as u64;
        state.last_scan_completed_at = Some(chrono::Utc::now());
        state.last_scan_duration_ms = Some(duration_ms as i64);
        state.timeline_entry_count += entries_scanned as i32;
        state.anomaly_event_count += anomalies_detected as i32;
        state.updated_at = chrono::Utc::now();

        self.store
            .upsert_agent_observability_state(&state)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        Ok(ScanReport {
            agent_id,
            entries_scanned,
            anomalies_detected,
            drift_computations,
            duration_ms,
        })
    }
}
