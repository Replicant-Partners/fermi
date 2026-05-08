//! `PersonaDriftMonitor` — embedding-based drift between consecutive
//! `persona_version` baselines.
//!
//! Drift is `1.0 - cosine_similarity(mean_embedding_v_n,
//! mean_embedding_v_n+1)`. Higher = more drift.
//!
//! Per Q2 — Phase 3 ships **infrastructure** for adaptive thresholds
//! and uses **static** thresholds in production until we have enough
//! data for adaptive to be meaningful. The `DriftThreshold` enum lets
//! callers swap modes via configuration without touching the monitor's
//! interior.

use std::sync::Arc;
use uuid::Uuid;

use agent_bestiary_memory::MemoryStore;

use crate::error::ObservabilityError;

/// Default static drift threshold — chosen as a placeholder per Q2.
/// Per-agent overrides go on `agents.capability_gates.drift_threshold`.
pub const DEFAULT_DRIFT_THRESHOLD: f64 = 0.20;

/// How the [`PersonaDriftMonitor`] decides whether a drift value is
/// anomalous.
///
/// Phase 3 default is [`DriftThreshold::Static`]. The [`Adaptive`]
/// branch is wired into the same call path so flipping later is a
/// configuration change rather than a refactor (per Q2 plan).
#[derive(Debug, Clone)]
pub enum DriftThreshold {
    /// Fixed cutoff — `drift_norm > value` triggers the anomaly.
    Static(f64),
    /// Compare against rolling-mean drift over a sliding window. The
    /// anomaly fires when `drift_norm > mean + sigma_multiplier *
    /// stddev` and the window has at least `min_samples` data points.
    Adaptive {
        window: usize,
        sigma_multiplier: f64,
        min_samples: usize,
    },
}

impl Default for DriftThreshold {
    fn default() -> Self {
        DriftThreshold::Static(DEFAULT_DRIFT_THRESHOLD)
    }
}

impl DriftThreshold {
    /// Build the production default — read per-agent override from
    /// the agent card if present, otherwise fall back to the static
    /// constant.
    pub fn from_agent_capability_gates(gates: &serde_json::Value) -> Self {
        if let Some(override_value) = gates
            .get("drift_threshold")
            .and_then(|v| v.as_f64())
        {
            return DriftThreshold::Static(override_value.clamp(0.0, 1.0));
        }
        DriftThreshold::Static(DEFAULT_DRIFT_THRESHOLD)
    }

    /// Decide whether `drift_norm` against `recent_norms` (most-recent
    /// last) is anomalous.
    pub fn is_anomalous(&self, drift_norm: f64, recent_norms: &[f64]) -> bool {
        match self {
            DriftThreshold::Static(t) => drift_norm > *t,
            DriftThreshold::Adaptive {
                window,
                sigma_multiplier,
                min_samples,
            } => {
                if recent_norms.len() < *min_samples {
                    return false;
                }
                let take_from = recent_norms.len().saturating_sub(*window);
                let sample = &recent_norms[take_from..];
                if sample.len() < *min_samples {
                    return false;
                }
                let mean = sample.iter().sum::<f64>() / sample.len() as f64;
                let variance =
                    sample.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                        / sample.len() as f64;
                let std_dev = variance.sqrt();
                drift_norm > mean + sigma_multiplier * std_dev
            }
        }
    }
}

/// Result of one drift computation.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftVector {
    /// `1.0 - cosine_similarity` between the two version means.
    pub norm: f64,
    /// Cosine similarity itself, surfaced separately for the dashboard.
    pub cosine_similarity: f64,
    /// The two version numbers compared.
    pub prev_persona_version: i32,
    pub curr_persona_version: i32,
    /// Whether the threshold judged this drift anomalous.
    pub anomalous: bool,
}

#[derive(Clone)]
pub struct PersonaDriftMonitor {
    store: Arc<MemoryStore>,
    /// How many episodes' embeddings to average per persona_version.
    pub baseline_window: i64,
    pub threshold: DriftThreshold,
}

impl PersonaDriftMonitor {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self {
            store,
            baseline_window: 50,
            threshold: DriftThreshold::default(),
        }
    }

    pub fn with_threshold(mut self, threshold: DriftThreshold) -> Self {
        self.threshold = threshold;
        self
    }

    pub fn with_baseline_window(mut self, window: i64) -> Self {
        self.baseline_window = window.max(1);
        self
    }

    /// Compute drift between the agent's `prev_persona_version` and
    /// `curr_persona_version`. Returns `Inapplicable` when either
    /// baseline is empty (no embeddings yet).
    pub async fn compute(
        &self,
        agent_id: Uuid,
        prev_persona_version: i32,
        curr_persona_version: i32,
        recent_drift_norms: &[f64],
    ) -> Result<DriftVector, ObservabilityError> {
        if prev_persona_version >= curr_persona_version {
            return Err(ObservabilityError::Invalid(format!(
                "prev ({}) must be < curr ({})",
                prev_persona_version, curr_persona_version
            )));
        }

        let prev = self
            .store
            .mean_embedding_for_persona_version(
                agent_id,
                prev_persona_version,
                self.baseline_window,
            )
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;
        let curr = self
            .store
            .mean_embedding_for_persona_version(
                agent_id,
                curr_persona_version,
                self.baseline_window,
            )
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        let (prev, curr) = match (prev, curr) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                return Err(ObservabilityError::Inapplicable(format!(
                    "no embeddings for v{} or v{}",
                    prev_persona_version, curr_persona_version
                )));
            }
        };

        let cos = cosine_similarity(&prev, &curr).ok_or_else(|| {
            ObservabilityError::Embedding(format!(
                "dimension mismatch: prev {} vs curr {}",
                prev.len(),
                curr.len()
            ))
        })?;
        let norm = 1.0 - cos;
        let anomalous = self.threshold.is_anomalous(norm, recent_drift_norms);

        Ok(DriftVector {
            norm,
            cosine_similarity: cos,
            prev_persona_version,
            curr_persona_version,
            anomalous,
        })
    }
}

/// Cosine similarity. Returns `None` on dimension mismatch.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f64> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let mut dot = 0.0_f64;
    let mut na = 0.0_f64;
    let mut nb = 0.0_f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        na += xf * xf;
        nb += yf * yf;
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < f64::EPSILON {
        return Some(0.0);
    }
    Some(dot / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identity_is_one() {
        let v: Vec<f32> = (0..8).map(|i| i as f32).collect();
        assert!((cosine_similarity(&v, &v).unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0_f32, 0.0, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn cosine_dim_mismatch_is_none() {
        assert!(cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]).is_none());
    }

    #[test]
    fn static_threshold_basic() {
        let t = DriftThreshold::Static(0.20);
        assert!(!t.is_anomalous(0.10, &[]));
        assert!(!t.is_anomalous(0.20, &[]));
        assert!(t.is_anomalous(0.21, &[]));
    }

    #[test]
    fn adaptive_threshold_warmup_is_not_anomalous() {
        let t = DriftThreshold::Adaptive {
            window: 5,
            sigma_multiplier: 2.0,
            min_samples: 5,
        };
        // Only 3 samples → still warming up
        assert!(!t.is_anomalous(0.99, &[0.1, 0.1, 0.1]));
    }

    #[test]
    fn adaptive_threshold_flags_after_warmup() {
        let t = DriftThreshold::Adaptive {
            window: 5,
            sigma_multiplier: 2.0,
            min_samples: 5,
        };
        // Stable at ~0.10 — mean=0.10, stddev≈0
        let recent = vec![0.10, 0.10, 0.10, 0.10, 0.10];
        assert!(t.is_anomalous(0.50, &recent));
    }

    #[test]
    fn capability_gates_override_default_threshold() {
        let gates = serde_json::json!({ "drift_threshold": 0.40 });
        let t = DriftThreshold::from_agent_capability_gates(&gates);
        match t {
            DriftThreshold::Static(v) => assert!((v - 0.40).abs() < 1e-9),
            _ => panic!("expected Static"),
        }
    }

    #[test]
    fn capability_gates_default_when_unset() {
        let gates = serde_json::json!({});
        let t = DriftThreshold::from_agent_capability_gates(&gates);
        match t {
            DriftThreshold::Static(v) => {
                assert!((v - DEFAULT_DRIFT_THRESHOLD).abs() < 1e-9)
            }
            _ => panic!("expected Static"),
        }
    }
}
