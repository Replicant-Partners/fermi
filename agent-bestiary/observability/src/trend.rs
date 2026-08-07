//! `TrendAnalyzer` — on-demand rolling means / std-devs per dimension
//! over an agent's timeline.
//!
//! Q5: Phase 3 ships only the function. Snapshot caching lands in
//! Phase 4 once we know what shape the dashboard wants.

use std::collections::BTreeMap;
use std::sync::Arc;

use uuid::Uuid;

use agent_bestiary_memory::{MemoryStore, TimelineEntry};

use crate::error::ObservabilityError;

/// Window size for the rolling stats. Concrete value tunable per call.
#[derive(Debug, Clone, Copy)]
pub struct TrendWindow {
    pub size: usize,
}

impl Default for TrendWindow {
    fn default() -> Self {
        Self { size: 50 }
    }
}

/// One dimension's rolling-window summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrendSeries {
    pub dimension: String,
    pub mean: f64,
    pub std_dev: f64,
    /// Min / max within the window — gives operators a quick range read.
    pub min: f64,
    pub max: f64,
    /// Sample count actually used (≤ window size when window not full).
    pub n: usize,
    /// Most-recent value in the window (the latest scored episode's mean
    /// for this dimension).
    pub latest: Option<f64>,
}

/// Output of one trend computation. Keys are dimension names; values
/// are summaries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TrendReport {
    pub agent_id: Uuid,
    pub window: usize,
    pub series: BTreeMap<String, TrendSeries>,
}

#[derive(Clone)]
pub struct TrendAnalyzer {
    store: Arc<MemoryStore>,
}

impl TrendAnalyzer {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    pub async fn compute(
        &self,
        agent_id: Uuid,
        window: TrendWindow,
    ) -> Result<TrendReport, ObservabilityError> {
        let entries = self
            .store
            .list_timeline_entries(agent_id, window.size as i64)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        let series = compute_series(&entries);

        Ok(TrendReport {
            agent_id,
            window: window.size,
            series,
        })
    }
}

/// Pure helper — computes the per-dimension series from a slice of
/// timeline entries. Newest-first or oldest-first input both work; the
/// summaries are order-independent except for `latest` which uses the
/// max-time entry.
pub fn compute_series(entries: &[TimelineEntry]) -> BTreeMap<String, TrendSeries> {
    let mut buckets: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut latest_by_dim: BTreeMap<String, (chrono::DateTime<chrono::Utc>, f64)> = BTreeMap::new();

    for e in entries {
        let Some(obj) = e.dim_scores.as_object() else {
            continue;
        };
        for (dim, v) in obj {
            let Some(score) = v.as_f64() else {
                continue;
            };
            buckets.entry(dim.clone()).or_default().push(score);

            // Track the most-recent value per dimension by timestamp.
            latest_by_dim
                .entry(dim.clone())
                .and_modify(|cur| {
                    if e.created_at > cur.0 {
                        *cur = (e.created_at, score);
                    }
                })
                .or_insert((e.created_at, score));
        }
    }

    let mut out = BTreeMap::new();
    for (dim, vals) in buckets {
        if vals.is_empty() {
            continue;
        }
        let n = vals.len();
        let mean = vals.iter().sum::<f64>() / n as f64;
        let variance = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n as f64;
        let std_dev = variance.sqrt();
        let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let latest = latest_by_dim.get(&dim).map(|(_, v)| *v);
        out.insert(
            dim.clone(),
            TrendSeries {
                dimension: dim,
                mean,
                std_dev,
                min,
                max,
                n,
                latest,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn entry_at(t: chrono::DateTime<chrono::Utc>, dims: serde_json::Value) -> TimelineEntry {
        TimelineEntry {
            entry_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            episode_id: None,
            run_id: None,
            persona_version: 1,
            dyad_id: None,
            session_id: None,
            provenance: "auto_pass".into(),
            dim_scores: dims,
            drift_norm: None,
            within_version_cosine: None,
            anomaly_flags: serde_json::json!([]),
            created_at: t,
        }
    }

    #[test]
    fn empty_entries_empty_series() {
        let out = compute_series(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn mean_and_stddev_basic() {
        let now = Utc::now();
        let entries = vec![
            entry_at(now, serde_json::json!({ "rapport": 0.5 })),
            entry_at(now, serde_json::json!({ "rapport": 0.7 })),
            entry_at(now, serde_json::json!({ "rapport": 0.9 })),
        ];
        let out = compute_series(&entries);
        let s = out.get("rapport").unwrap();
        assert_eq!(s.n, 3);
        assert!((s.mean - 0.7).abs() < 1e-9);
        assert!((s.min - 0.5).abs() < 1e-9);
        assert!((s.max - 0.9).abs() < 1e-9);
        // Variance = ((0.2)^2 + 0 + (0.2)^2) / 3 = 0.08/3
        // stddev ≈ 0.16329931...
        assert!((s.std_dev - 0.163299316_f64).abs() < 1e-6);
    }

    #[test]
    fn latest_uses_max_timestamp() {
        let t1 = Utc::now();
        let t2 = t1 + chrono::Duration::seconds(10);
        let entries = vec![
            entry_at(t1, serde_json::json!({ "rapport": 0.3 })),
            entry_at(t2, serde_json::json!({ "rapport": 0.8 })),
        ];
        let out = compute_series(&entries);
        let s = out.get("rapport").unwrap();
        assert!((s.latest.unwrap() - 0.8).abs() < 1e-9);
    }

    #[test]
    fn dimensions_are_independent() {
        let now = Utc::now();
        let entries = vec![
            entry_at(now, serde_json::json!({ "rapport": 0.5 })),
            entry_at(
                now,
                serde_json::json!({ "rapport": 0.7, "forecast_calibration": 0.9 }),
            ),
        ];
        let out = compute_series(&entries);
        assert_eq!(out.get("rapport").unwrap().n, 2);
        assert_eq!(out.get("forecast_calibration").unwrap().n, 1);
    }
}
