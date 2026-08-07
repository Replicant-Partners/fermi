//! `SocialInteractionTracker` — per-(agent, human) running rapport,
//! trust, reciprocity.
//!
//! Q1 (a): only updates state when `dyad_id` is non-null. Background
//! / agent-to-agent / system invocations are silently skipped.
//!
//! Per Q3, a "rupture" event is detected when rapport drops by
//! `RUPTURE_DROP_THRESHOLD` within `RUPTURE_WINDOW_LEN` consecutive
//! episodes for the same dyad.

use std::sync::Arc;
use uuid::Uuid;

use agent_bestiary_evaluators::AggregatedSignal;
use agent_bestiary_memory::{DyadState, MemoryStore};

use crate::error::ObservabilityError;

/// Bounded rolling-rapport history per dyad — used by the rupture
/// detector. Persisted in `dyad_state.recent_rapport`.
pub const RUPTURE_WINDOW_LEN: usize = 5;

/// Q3 default — rapport drop > this within the window flags a rupture.
pub const RUPTURE_DROP_THRESHOLD: f64 = 0.20;

/// Smoothing coefficient for the running averages. `α` close to 1.0
/// makes the running state highly responsive to the most recent
/// observation; close to 0.0 makes it sticky. We start at `0.3` —
/// enough to react in ~3 episodes, stable enough not to thrash.
pub const SMOOTHING_ALPHA: f64 = 0.3;

/// Result of one social update — the new dyad state plus whether a
/// rupture was detected.
#[derive(Debug, Clone)]
pub struct SocialUpdate {
    pub state: DyadState,
    /// True when the rolling-rapport window saw a drop > the threshold.
    pub rupture_detected: bool,
    /// Magnitude of the largest rapport drop within the window.
    pub max_rapport_drop: f64,
}

#[derive(Clone)]
pub struct SocialInteractionTracker {
    store: Arc<MemoryStore>,
}

impl SocialInteractionTracker {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }

    /// Apply one observation to the dyad's running state. Returns
    /// `Inapplicable` when there is no `dyad_id` to scope to.
    ///
    /// Mapping from signal → axes (placeholder pending Track B
    /// evaluators that score these dimensions explicitly):
    /// - `rapport`         ← `signal.dim_scores.rapport` if present
    /// - `trust`           ← `signal.dim_scores.persona_fidelity`
    ///                       (trust = "do I get the same agent each time?")
    /// - `reciprocity`     ← mean of `social_capital` + `goal_completion`
    ///                       when present, fallback to running value
    pub async fn observe(
        &self,
        agent_id: Uuid,
        dyad_id: Option<&str>,
        human_id: Option<&str>,
        signal: &AggregatedSignal,
    ) -> Result<SocialUpdate, ObservabilityError> {
        let dyad_id = dyad_id
            .ok_or_else(|| ObservabilityError::Inapplicable("no dyad_id on episode".into()))?;
        let human_id = human_id
            .ok_or_else(|| ObservabilityError::Inapplicable("no human_id provided".into()))?;

        let prev = self
            .store
            .get_dyad_state(dyad_id)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?
            .unwrap_or_else(|| DyadState {
                dyad_id: dyad_id.to_string(),
                agent_id,
                human_id: human_id.to_string(),
                rapport: 0.5,
                trust: 0.5,
                reciprocity: 0.5,
                episode_count: 0,
                recent_rapport: serde_json::json!([]),
                last_updated_at: chrono::Utc::now(),
                created_at: chrono::Utc::now(),
            });

        let dim_value = |name: &str| -> Option<f64> {
            signal
                .per_dimension
                .iter()
                .find(|d| d.dimension.as_str() == name)
                .map(|d| d.mean)
        };

        let new_rapport = match dim_value("rapport") {
            Some(v) => smooth(prev.rapport, v),
            None => prev.rapport,
        };
        let new_trust = match dim_value("persona_fidelity") {
            Some(v) => smooth(prev.trust, v),
            None => prev.trust,
        };
        let new_reciprocity = match (dim_value("social_capital"), dim_value("goal_completion")) {
            (Some(a), Some(b)) => smooth(prev.reciprocity, (a + b) / 2.0),
            (Some(a), None) | (None, Some(a)) => smooth(prev.reciprocity, a),
            (None, None) => prev.reciprocity,
        };

        // Update rolling rapport history.
        let mut history: Vec<f64> = prev
            .recent_rapport
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
            .unwrap_or_default();
        history.push(new_rapport);
        if history.len() > RUPTURE_WINDOW_LEN {
            history.drain(0..(history.len() - RUPTURE_WINDOW_LEN));
        }
        let recent_rapport_json = serde_json::to_value(&history)
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        // Rupture: largest peak-to-trough drop within the window.
        let (rupture_detected, max_drop) = detect_rupture(&history);

        let new_state = DyadState {
            dyad_id: dyad_id.to_string(),
            agent_id,
            human_id: human_id.to_string(),
            rapport: new_rapport.clamp(0.0, 1.0),
            trust: new_trust.clamp(0.0, 1.0),
            reciprocity: new_reciprocity.clamp(0.0, 1.0),
            episode_count: prev.episode_count + 1,
            recent_rapport: recent_rapport_json,
            last_updated_at: chrono::Utc::now(),
            created_at: prev.created_at,
        };

        self.store
            .upsert_dyad_state(&new_state)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        Ok(SocialUpdate {
            state: new_state,
            rupture_detected,
            max_rapport_drop: max_drop,
        })
    }
}

fn smooth(prev: f64, observed: f64) -> f64 {
    SMOOTHING_ALPHA * observed + (1.0 - SMOOTHING_ALPHA) * prev
}

/// Detect a rupture in the rolling rapport history. Returns
/// `(detected, max_drop)`.
///
/// Definition: max(peak) - min(trough_after_peak) > RUPTURE_DROP_THRESHOLD,
/// i.e. the largest drop from any earlier value to any later value
/// exceeds the threshold.
pub fn detect_rupture(history: &[f64]) -> (bool, f64) {
    if history.len() < 2 {
        return (false, 0.0);
    }
    let mut max_drop = 0.0;
    for i in 0..history.len() {
        for j in (i + 1)..history.len() {
            let drop = history[i] - history[j];
            if drop > max_drop {
                max_drop = drop;
            }
        }
    }
    (max_drop > RUPTURE_DROP_THRESHOLD, max_drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rupture_empty_history_no_rupture() {
        assert_eq!(detect_rupture(&[]), (false, 0.0));
        assert_eq!(detect_rupture(&[0.7]), (false, 0.0));
    }

    #[test]
    fn rupture_steady_history_no_rupture() {
        assert!(!detect_rupture(&[0.7, 0.71, 0.69, 0.72, 0.70]).0);
    }

    #[test]
    fn rupture_sharp_drop_detected() {
        // 0.85 → 0.50 = 0.35 drop, > 0.20 threshold
        let (rupture, drop) = detect_rupture(&[0.85, 0.83, 0.50]);
        assert!(rupture);
        assert!((drop - 0.35).abs() < 1e-9);
    }

    #[test]
    fn rupture_gradual_decline_below_threshold_no_rupture() {
        // 0.70 → 0.55 = 0.15 drop
        let (rupture, _) = detect_rupture(&[0.70, 0.65, 0.60, 0.55]);
        assert!(!rupture);
    }

    #[test]
    fn smoothing_moves_toward_observation() {
        let r = smooth(0.5, 1.0);
        // α=0.3 → 0.3*1.0 + 0.7*0.5 = 0.65
        assert!((r - 0.65).abs() < 1e-9);
    }
}
