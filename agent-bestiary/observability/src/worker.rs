//! `ObservabilityWorker` — scans timeline entries since the last
//! checkpoint, then persists the results. Three passes, in order:
//!
//! 1. **Drift** — per (entry, persona_version) embedding drift.
//! 2. **Social** — fold each dyad-scoped entry into that dyad's running
//!    rapport / trust / reciprocity, flagging ruptures.
//! 3. **Anomaly** — detect over the window, including the rupture flags
//!    written by pass 2.
//!
//! Pass 2 must precede pass 3: the anomaly detector surfaces ruptures by
//! reading `rupture:<dyad_id>` flags off the entries rather than
//! recomputing them.
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
use crate::social::{InteractionObservation, SocialInteractionTracker};

/// Marker flag written onto a timeline entry once its episode has been
/// folded into the dyad's running social state. Makes the social pass
/// idempotent across re-scans.
pub const SOCIAL_OBSERVED_FLAG: &str = "social:observed";

/// Summary of a scan run — surfaced to operators in the
/// per-agent observability page (Phase 4).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanReport {
    pub agent_id: Uuid,
    pub entries_scanned: usize,
    pub anomalies_detected: usize,
    pub drift_computations: usize,
    /// Number of timeline entries that produced a `dyad_state` update.
    pub dyad_updates: usize,
    /// Number of those updates whose rolling-rapport window tripped the
    /// rupture threshold.
    pub ruptures_detected: usize,
    pub duration_ms: u64,
}

/// Summary of a social backfill run.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackfillReport {
    pub agent_id: Uuid,
    /// Dyad-scoped episodes folded into relationship state.
    pub episodes_replayed: usize,
    /// Distinct relationships rebuilt.
    pub dyads_rebuilt: usize,
    pub ruptures_detected: usize,
    /// Dyads skipped because their id could not be parsed.
    pub skipped: usize,
    pub duration_ms: u64,
}

/// Background scanner. Cheap to construct; share via `Arc::clone`.
#[derive(Clone)]
pub struct ObservabilityWorker {
    store: Arc<MemoryStore>,
    drift: PersonaDriftMonitor,
    anomaly: AnomalyDetector,
    social: SocialInteractionTracker,
    /// Maximum number of entries to process in a single scan.
    pub batch_size: i64,
}

impl ObservabilityWorker {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        let drift = PersonaDriftMonitor::new(store.clone());
        let anomaly = AnomalyDetector::new(store.clone());
        let social = SocialInteractionTracker::new(store.clone());
        Self {
            store,
            drift,
            anomaly,
            social,
            batch_size: 200,
        }
    }

    /// Rebuild every dyad's relationship state for an agent by replaying its
    /// episode history.
    ///
    /// Replays from `episodes` rather than `agent_timeline_entries` because
    /// timeline entries are written only by the eval pipeline — no live
    /// conversation has ever produced one — and because episodes carry the
    /// real timestamps the reciprocity axis needs to reconstruct cadence.
    ///
    /// Deterministic and idempotent: each dyad is folded from a fresh
    /// initial state in chronological order, so re-running produces an
    /// identical result rather than compounding.
    ///
    /// Deliberately does **not** re-run drift or anomaly detection.
    /// `AnomalyDetector::persist` inserts a fresh event per call with no
    /// dedupe, so replaying that pass would duplicate every historical
    /// anomaly. Ruptures found here are reported in the return value and
    /// will be raised as events by the next ordinary scan.
    pub async fn backfill_social(
        &self,
        agent_id: Uuid,
    ) -> Result<BackfillReport, ObservabilityError> {
        let start = Instant::now();

        let interactions = self
            .store
            .list_dyad_interactions(agent_id)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;
        let episodes_replayed = interactions.len();

        // Group by dyad, preserving the oldest-first ordering from SQL.
        let mut by_dyad: std::collections::BTreeMap<String, Vec<InteractionObservation>> =
            std::collections::BTreeMap::new();
        for (dyad_id, occurred_at, status, chars) in interactions {
            let succeeded = status.eq_ignore_ascii_case("success");
            let partial = status.eq_ignore_ascii_case("partial");
            by_dyad
                .entry(dyad_id)
                .or_default()
                .push(InteractionObservation {
                    succeeded,
                    partial,
                    // Episodes do not persist the agent's self-reported
                    // confidence, so replay uses a neutral value and lets
                    // execution outcome carry the trust signal. Live
                    // observations pass the real confidence.
                    confidence: 0.5,
                    user_chars: chars,
                    occurred_at,
                });
        }

        let mut dyads_rebuilt = 0usize;
        let mut ruptures_detected = 0usize;
        let mut skipped = 0usize;

        for (dyad_id, observations) in &by_dyad {
            match self
                .social
                .replay_dyad(agent_id, dyad_id, observations)
                .await
            {
                Ok(u) => {
                    dyads_rebuilt += 1;
                    if u.rupture_detected {
                        ruptures_detected += 1;
                    }
                }
                Err(e) if e.is_inapplicable() => skipped += 1,
                Err(e) => return Err(e),
            }
        }

        Ok(BackfillReport {
            agent_id,
            episodes_replayed,
            dyads_rebuilt,
            ruptures_detected,
            skipped,
            duration_ms: start.elapsed().as_millis() as u64,
        })
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
        let mut dyad_updates = 0usize;
        let mut ruptures_detected = 0usize;

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

        // ── Pass 2: per-dyad social state ──
        //
        // Entries arrive oldest-first, which matters: the tracker applies
        // exponential smoothing, so replaying them out of order would
        // produce a different rapport/trust/reciprocity than the
        // conversation actually had.
        //
        // Entries without a `dyad_id` (system-spawned agents, agent-to-agent
        // work) are skipped by design — there is no human to have a
        // relationship with.
        //
        // The `social:observed` marker makes this pass idempotent: if a scan
        // fails after updating some dyads but before advancing the
        // checkpoint, a re-run will not double-count those episodes into the
        // running averages.
        for entry in &entries {
            let Some(dyad_id) = entry.dyad_id.as_deref() else {
                continue;
            };

            let mut flags: Vec<serde_json::Value> =
                entry.anomaly_flags.as_array().cloned().unwrap_or_default();
            let already_observed = flags
                .iter()
                .filter_map(|f| f.as_str())
                .any(|s| s == SOCIAL_OBSERVED_FLAG);
            if already_observed {
                continue;
            }

            let update = self
                .social
                .observe_dim_scores(agent_id, Some(dyad_id), None, &entry.dim_scores)
                .await;

            match update {
                Ok(u) => {
                    dyad_updates += 1;
                    flags.push(serde_json::Value::String(SOCIAL_OBSERVED_FLAG.into()));
                    if u.rupture_detected {
                        ruptures_detected += 1;
                        flags.push(serde_json::Value::String(format!("rupture:{}", dyad_id)));
                    }
                    let updated_flags = serde_json::Value::Array(flags);
                    // Preserve drift fields written by pass 1 — this UPDATE
                    // sets all three columns unconditionally.
                    self.store
                        .update_timeline_drift_anomaly(
                            entry.entry_id,
                            entry.drift_norm,
                            entry.within_version_cosine,
                            &updated_flags,
                        )
                        .await
                        .map_err(|e| ObservabilityError::Storage(e.to_string()))?;
                }
                Err(e) if e.is_inapplicable() => {
                    // Malformed dyad_id, or no human recoverable from it.
                }
                Err(e) => return Err(e),
            }
        }

        // ── Pass 3: anomaly detection over the (now-updated) window ──
        //
        // Re-fetch entries so flags written in passes 1 and 2 are visible
        // to the detector — in particular the `rupture:<dyad_id>` flags,
        // which is how ruptures become anomaly events. This is cheap
        // (single index scan); the alternative — mutating local copies —
        // risks divergence.
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
            dyad_updates,
            ruptures_detected,
            duration_ms,
        })
    }
}
