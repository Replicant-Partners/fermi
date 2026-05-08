//! Aggregator — merges per-evaluator results into a single
//! `AggregatedSignal` with per-dimension means and conflict flags.
//!
//! The architecture-doc mock detects a "conflict" on a dimension when
//! two or more evaluators score it and `max - min > 0.2`. We adopt the
//! same threshold here, configurable via `Aggregator::conflict_threshold`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::result::{Dimension, EvalFlag, RegistryResult};

/// Architecture-doc default: a dimension is "in conflict" when
/// `max(scores) - min(scores) > 0.2`.
pub const DEFAULT_CONFLICT_THRESHOLD: f64 = 0.20;

/// Per-dimension aggregated view: the mean across active evaluators,
/// plus the individual contributions for transparency / debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionAggregate {
    pub dimension: Dimension,
    /// Confidence-weighted mean across contributing evaluators.
    pub mean: f64,
    /// `(evaluator_name, score)` pairs — preserved for HITL surfaces
    /// so a reviewer can see exactly who scored what.
    pub contributions: Vec<(String, f64)>,
    /// True when this dimension is flagged for conflict
    /// (`max - min > threshold`).
    pub conflict: bool,
    /// `max - min` across contributors. Useful for ranking the
    /// "loudest" disagreements when more than one dimension is in
    /// conflict.
    pub spread: f64,
}

/// Conflict flag suitable for surfacing in the HITL queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictFlag {
    pub dimension: Dimension,
    pub spread: f64,
    /// Names of evaluators whose disagreement triggered the flag.
    pub evaluators: Vec<String>,
}

/// What the aggregator emits. Phase 2 stores this on `eval_runs` /
/// `eval_signals`; Phase 3 surfaces it on the timeline; Phase 4 routes
/// to the HITL queue when conflicts trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedSignal {
    pub per_dimension: Vec<DimensionAggregate>,
    pub conflicts: Vec<ConflictFlag>,
    /// Aggregated flags from all evaluators (pre-filter `safety:*`,
    /// dimensional `groundedness:*`, etc.).
    pub flags: Vec<EvalFlag>,
    /// Names of evaluators that contributed at least one dimension.
    pub active_evaluators: Vec<String>,
    /// Names of evaluators that returned `Inapplicable`.
    pub inapplicable_evaluators: Vec<String>,
    /// Names of evaluators that errored. The registry never aborts on
    /// a single evaluator's failure; failures land here for visibility.
    pub failed_evaluators: Vec<String>,
}

/// Aggregator with a configurable conflict threshold.
#[derive(Debug, Clone)]
pub struct Aggregator {
    pub conflict_threshold: f64,
}

impl Default for Aggregator {
    fn default() -> Self {
        Self {
            conflict_threshold: DEFAULT_CONFLICT_THRESHOLD,
        }
    }
}

impl Aggregator {
    pub fn new(conflict_threshold: f64) -> Self {
        Self {
            conflict_threshold,
        }
    }

    /// Merge per-evaluator outcomes into an aggregated signal.
    /// `Inapplicable` evaluators are silently skipped; failed ones are
    /// recorded in `failed_evaluators` but do not contribute scores.
    pub fn aggregate(&self, results: &[RegistryResult]) -> AggregatedSignal {
        let mut per_dim_map: HashMap<Dimension, Vec<(String, f64, f64)>> = HashMap::new();
        let mut all_flags: Vec<EvalFlag> = Vec::new();
        let mut active_evaluators: Vec<String> = Vec::new();
        let mut inapplicable_evaluators: Vec<String> = Vec::new();
        let mut failed_evaluators: Vec<String> = Vec::new();

        for r in results {
            match &r.outcome {
                Ok(eval) => {
                    if !eval.dimension_scores.is_empty() {
                        active_evaluators.push(r.evaluator_name.clone());
                    }
                    for (dim, score) in &eval.dimension_scores {
                        per_dim_map
                            .entry(dim.clone())
                            .or_default()
                            .push((r.evaluator_name.clone(), *score, eval.confidence));
                    }
                    for f in &eval.flags {
                        all_flags.push(f.clone());
                    }
                }
                Err(e) if e.is_inapplicable() => {
                    inapplicable_evaluators.push(r.evaluator_name.clone());
                }
                Err(_) => {
                    failed_evaluators.push(r.evaluator_name.clone());
                }
            }
        }

        let mut per_dimension: Vec<DimensionAggregate> = Vec::new();
        let mut conflicts: Vec<ConflictFlag> = Vec::new();

        for (dim, contribs) in per_dim_map.iter() {
            let (mean, spread) = mean_and_spread(contribs);
            let conflict = contribs.len() > 1 && spread > self.conflict_threshold;
            if conflict {
                conflicts.push(ConflictFlag {
                    dimension: dim.clone(),
                    spread,
                    evaluators: contribs.iter().map(|(n, _, _)| n.clone()).collect(),
                });
            }
            per_dimension.push(DimensionAggregate {
                dimension: dim.clone(),
                mean,
                contributions: contribs.iter().map(|(n, s, _)| (n.clone(), *s)).collect(),
                conflict,
                spread,
            });
        }

        // Stable ordering for deterministic test output.
        per_dimension.sort_by(|a, b| a.dimension.0.cmp(&b.dimension.0));
        conflicts.sort_by(|a, b| a.dimension.0.cmp(&b.dimension.0));
        active_evaluators.sort();
        inapplicable_evaluators.sort();
        failed_evaluators.sort();

        AggregatedSignal {
            per_dimension,
            conflicts,
            flags: all_flags,
            active_evaluators,
            inapplicable_evaluators,
            failed_evaluators,
        }
    }
}

/// Confidence-weighted mean and unweighted spread (max - min).
fn mean_and_spread(contribs: &[(String, f64, f64)]) -> (f64, f64) {
    if contribs.is_empty() {
        return (0.0, 0.0);
    }

    // Confidence-weighted mean. Falls back to unweighted when all
    // confidences are zero (defensive).
    let total_w: f64 = contribs.iter().map(|(_, _, w)| *w).sum();
    let mean = if total_w > f64::EPSILON {
        contribs
            .iter()
            .map(|(_, s, w)| s * w)
            .sum::<f64>()
            / total_w
    } else {
        contribs.iter().map(|(_, s, _)| *s).sum::<f64>() / contribs.len() as f64
    };

    let max = contribs
        .iter()
        .map(|(_, s, _)| *s)
        .fold(f64::NEG_INFINITY, f64::max);
    let min = contribs
        .iter()
        .map(|(_, s, _)| *s)
        .fold(f64::INFINITY, f64::min);
    (mean, max - min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::EvalResult;
    use crate::tier::EvalTier;
    use crate::EvalError;

    fn ok_result(
        name: &str,
        version: &str,
        dim_scores: &[(&str, f64)],
    ) -> RegistryResult {
        let mut eval = EvalResult::new(name, version);
        for (d, s) in dim_scores {
            eval = eval.with_score(*d, *s);
        }
        RegistryResult {
            evaluator_name: name.to_string(),
            tier: EvalTier::Dimensional,
            outcome: Ok(eval),
            latency_ms: 1,
        }
    }

    fn inapplicable(name: &str) -> RegistryResult {
        RegistryResult {
            evaluator_name: name.to_string(),
            tier: EvalTier::Dimensional,
            outcome: Err(EvalError::Inapplicable("no goal_spec".into())),
            latency_ms: 0,
        }
    }

    fn failed(name: &str) -> RegistryResult {
        RegistryResult {
            evaluator_name: name.to_string(),
            tier: EvalTier::Dimensional,
            outcome: Err(EvalError::Provider("boom".into())),
            latency_ms: 0,
        }
    }

    #[test]
    fn agreement_within_threshold_does_not_flag_conflict() {
        let agg = Aggregator::default();
        let results = vec![
            ok_result("a", "1", &[("rapport", 0.70)]),
            ok_result("b", "1", &[("rapport", 0.80)]),
        ];
        let signal = agg.aggregate(&results);
        assert!(signal.conflicts.is_empty());
        assert_eq!(signal.per_dimension.len(), 1);
        let d = &signal.per_dimension[0];
        assert!((d.mean - 0.75).abs() < 1e-9);
        assert!(!d.conflict);
        assert!((d.spread - 0.10).abs() < 1e-9);
    }

    #[test]
    fn disagreement_above_threshold_flags_conflict() {
        let agg = Aggregator::default();
        let results = vec![
            ok_result("a", "1", &[("rapport", 0.30)]),
            ok_result("b", "1", &[("rapport", 0.85)]),
        ];
        let signal = agg.aggregate(&results);
        assert_eq!(signal.conflicts.len(), 1);
        let c = &signal.conflicts[0];
        assert_eq!(c.dimension.as_str(), "rapport");
        assert!((c.spread - 0.55).abs() < 1e-9);
        assert!(c.evaluators.contains(&"a".to_string()));
        assert!(c.evaluators.contains(&"b".to_string()));
    }

    #[test]
    fn single_evaluator_never_conflicts() {
        let agg = Aggregator::default();
        let results = vec![ok_result("a", "1", &[("rapport", 0.10)])];
        let signal = agg.aggregate(&results);
        assert!(signal.conflicts.is_empty());
    }

    #[test]
    fn inapplicable_does_not_count_as_failure() {
        let agg = Aggregator::default();
        let results = vec![
            ok_result("a", "1", &[("rapport", 0.70)]),
            inapplicable("sotopia"),
            failed("brier"),
        ];
        let signal = agg.aggregate(&results);
        assert_eq!(signal.active_evaluators, vec!["a"]);
        assert_eq!(signal.inapplicable_evaluators, vec!["sotopia"]);
        assert_eq!(signal.failed_evaluators, vec!["brier"]);
    }

    #[test]
    fn confidence_weights_the_mean() {
        let agg = Aggregator::default();
        let mut a_eval = EvalResult::new("a", "1").with_score("x", 0.0).with_confidence(0.0);
        // confidence 0.0 should fall back to unweighted mean
        a_eval.confidence = 0.0;
        let a = RegistryResult {
            evaluator_name: "a".into(),
            tier: EvalTier::Dimensional,
            outcome: Ok(a_eval),
            latency_ms: 0,
        };
        let b_eval = EvalResult::new("b", "1").with_score("x", 1.0).with_confidence(1.0);
        let b = RegistryResult {
            evaluator_name: "b".into(),
            tier: EvalTier::Dimensional,
            outcome: Ok(b_eval),
            latency_ms: 0,
        };
        // both confidences zero would force unweighted; non-zero +
        // zero gives full weight to the non-zero one.
        let signal = agg.aggregate(&[a, b]);
        let d = &signal.per_dimension[0];
        // confidence-weighted: 0*0 + 1*1 / 1 = 1.0
        assert!((d.mean - 1.0).abs() < 1e-9);
    }
}
