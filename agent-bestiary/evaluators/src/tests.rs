//! Integration tests — registry + aggregator + reference impls
//! exercised end-to-end against synthetic bundles.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::prelude::*;
use agent_bestiary_memory::{Episode, ExecutionStatus};

// ─── Fixtures ────────────────────────────────────────────────────

fn bundle_for(agent_id: Uuid) -> EpisodeBundle {
    let ep = Episode {
        response_text: None,
        assertions: None,
        episode_id: Uuid::new_v4(),
        agent_id,
        timestamp_ref: Utc::now(),
        query: "Test query for registry".into(),
        context: serde_json::json!({}),
        execution_status: ExecutionStatus::Success,
        error_details: None,
        execution_time_ms: 0,
        tokens_used: None,
        cost_usd: None,
        input_tokens: None,
        output_tokens: None,
        cost_basis: None,
        cost_rate_key: None,
        parent_episode_id: None,
        embedding: None,
        consolidated: false,
        tags: vec![],
        provenance: Provenance::AutoPass,
        authority_weight: 0.5,
        dyad_id: None,
        persona_version_at_write: None,
        provider_used: None,
        model_used: None,
    };
    EpisodeBundle::from_episode(&ep)
}

// ─── Synthetic test evaluators ─────────────────────────────────

struct FixedScorer {
    name: &'static str,
    tier: EvalTier,
    dim: Dimension,
    score: f64,
}

#[async_trait]
impl EvalModel for FixedScorer {
    fn name(&self) -> &'static str {
        self.name
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn tier(&self) -> EvalTier {
        self.tier
    }

    fn epistemic(&self) -> crate::model::Epistemic {
        crate::model::Epistemic::Computed
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![self.dim.clone()]
    }
    async fn evaluate(&self, _bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        Ok(EvalResult::new(self.name, "test").with_score(self.dim.clone(), self.score))
    }
}

struct AlwaysFail(&'static str);

#[async_trait]
impl EvalModel for AlwaysFail {
    fn name(&self) -> &'static str {
        self.0
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn epistemic(&self) -> crate::model::Epistemic {
        crate::model::Epistemic::Computed
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("ignored")]
    }
    async fn evaluate(&self, _bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        Err(EvalError::Provider("simulated provider failure".into()))
    }
}

/// A pre-filter whose provider is down.
struct FailingPreFilter;

#[async_trait]
impl EvalModel for FailingPreFilter {
    fn name(&self) -> &'static str {
        "failing_guard"
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn tier(&self) -> EvalTier {
        EvalTier::PreFilter
    }

    fn epistemic(&self) -> crate::model::Epistemic {
        crate::model::Epistemic::Computed
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("safety")]
    }
    async fn evaluate(&self, _bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        Err(EvalError::Provider("provider outage".into()))
    }
}

/// A pre-filter that does not apply to this bundle.
struct InapplicablePreFilter;

#[async_trait]
impl EvalModel for InapplicablePreFilter {
    fn name(&self) -> &'static str {
        "inapplicable_guard"
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn tier(&self) -> EvalTier {
        EvalTier::PreFilter
    }

    fn epistemic(&self) -> crate::model::Epistemic {
        crate::model::Epistemic::Computed
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("safety")]
    }
    async fn evaluate(&self, _bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        Err(EvalError::Inapplicable("not a chat bundle".into()))
    }
}

struct AlwaysInapplicable(&'static str);

#[async_trait]
impl EvalModel for AlwaysInapplicable {
    fn name(&self) -> &'static str {
        self.0
    }
    fn version(&self) -> &'static str {
        "test"
    }
    fn tier(&self) -> EvalTier {
        EvalTier::Dimensional
    }

    fn epistemic(&self) -> crate::model::Epistemic {
        crate::model::Epistemic::Computed
    }
    fn dimensions(&self) -> Vec<Dimension> {
        vec![Dimension::new("ignored")]
    }
    async fn evaluate(&self, _bundle: &EpisodeBundle) -> Result<EvalResult, EvalError> {
        Err(EvalError::Inapplicable("missing inputs".into()))
    }
}

// ─── Tests ──────────────────────────────────────────────────────

#[tokio::test]
async fn registry_runs_two_dimensional_evaluators_and_aggregates() {
    let agent_id = Uuid::new_v4();
    let bundle = bundle_for(agent_id);

    let mut reg = EvaluatorRegistry::new();
    reg.register(Arc::new(FixedScorer {
        name: "alpha",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.70,
    }));
    reg.register(Arc::new(FixedScorer {
        name: "beta",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.78,
    }));

    let outcome = reg.run(&bundle).await;
    assert!(!outcome.prefilter_blocked);
    assert_eq!(outcome.results.len(), 2);
    assert_eq!(outcome.signal.per_dimension.len(), 1);
    let d = &outcome.signal.per_dimension[0];
    assert_eq!(d.dimension.as_str(), "rapport");
    assert!(!d.conflict);
    assert!((d.mean - 0.74).abs() < 1e-9);
}

#[tokio::test]
async fn registry_detects_conflict_when_evaluators_disagree() {
    let bundle = bundle_for(Uuid::new_v4());

    let mut reg = EvaluatorRegistry::new();
    reg.register(Arc::new(FixedScorer {
        name: "low",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("persona_fidelity"),
        score: 0.30,
    }));
    reg.register(Arc::new(FixedScorer {
        name: "high",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("persona_fidelity"),
        score: 0.85,
    }));

    let outcome = reg.run(&bundle).await;
    assert_eq!(outcome.signal.conflicts.len(), 1);
    let c = &outcome.signal.conflicts[0];
    assert_eq!(c.dimension.as_str(), "persona_fidelity");
    assert!((c.spread - 0.55).abs() < 1e-9);
}

#[tokio::test]
async fn prefilter_short_circuits_dimensional_when_below_threshold() {
    let bundle = bundle_for(Uuid::new_v4());

    let mut reg = EvaluatorRegistry::new().with_prefilter_block_threshold(Some(0.5));
    // Pre-filter scores 0.10 → below 0.5 → block.
    reg.register(Arc::new(FixedScorer {
        name: "guard",
        tier: EvalTier::PreFilter,
        dim: Dimension::new("safety"),
        score: 0.10,
    }));
    // Dimensional evaluator that should NOT run.
    reg.register(Arc::new(FixedScorer {
        name: "dim",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.90,
    }));

    let outcome = reg.run(&bundle).await;
    assert!(outcome.prefilter_blocked);
    assert_eq!(outcome.results.len(), 1, "dimensional was not skipped");
    assert_eq!(outcome.results[0].evaluator_name, "guard");
}

#[tokio::test]
async fn prefilter_pass_lets_dimensional_run() {
    let bundle = bundle_for(Uuid::new_v4());

    let mut reg = EvaluatorRegistry::new().with_prefilter_block_threshold(Some(0.5));
    reg.register(Arc::new(FixedScorer {
        name: "guard",
        tier: EvalTier::PreFilter,
        dim: Dimension::new("safety"),
        score: 0.95,
    }));
    reg.register(Arc::new(FixedScorer {
        name: "dim",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.80,
    }));

    let outcome = reg.run(&bundle).await;
    assert!(!outcome.prefilter_blocked);
    assert_eq!(outcome.results.len(), 2);
}

#[tokio::test]
async fn registry_records_inapplicable_and_failure_separately() {
    let bundle = bundle_for(Uuid::new_v4());

    let mut reg = EvaluatorRegistry::new();
    reg.register(Arc::new(FixedScorer {
        name: "ok",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.72,
    }));
    reg.register(Arc::new(AlwaysInapplicable("skipped")));
    reg.register(Arc::new(AlwaysFail("broken")));

    let outcome = reg.run(&bundle).await;
    assert_eq!(outcome.signal.active_evaluators, vec!["ok"]);
    assert_eq!(outcome.signal.inapplicable_evaluators, vec!["skipped"]);
    assert_eq!(outcome.signal.failed_evaluators, vec!["broken"]);
}

#[tokio::test]
async fn reference_impls_run_through_registry() {
    use crate::judge::{JudgeOutcome, LlmJudgeEvaluator, NoopLlmJudge};
    use crate::scoring::{BrierEvaluator, BrierLookup, BrierObservation};

    struct StubBrier(Option<BrierObservation>);
    #[async_trait]
    impl BrierLookup for StubBrier {
        async fn latest_for_agent(
            &self,
            _agent_id: Uuid,
        ) -> Result<Option<BrierObservation>, EvalError> {
            Ok(self.0.clone())
        }
    }

    let bundle = bundle_for(Uuid::new_v4());

    let judge = Arc::new(NoopLlmJudge::new(JudgeOutcome {
        relevance: 4.0,
        accuracy: 4.0,
        completeness: 5.0,
        overall: 4.3,
        reasoning: Some("good".into()),
    }));
    let llm_judge = Arc::new(LlmJudgeEvaluator::new(judge));

    let brier_lookup = Arc::new(StubBrier(Some(BrierObservation {
        brier_score: 0.12,
        n_forecasts: Some(25),
        computed_at: None,
    })));
    let brier = Arc::new(BrierEvaluator::new(brier_lookup));

    let mut reg = EvaluatorRegistry::new();
    reg.register(llm_judge);
    reg.register(brier);

    let outcome = reg.run(&bundle).await;

    // Both dimensional, both succeed.
    assert!(!outcome.prefilter_blocked);
    assert_eq!(outcome.results.len(), 2);
    assert!(outcome.results.iter().all(|r| r.is_success()));

    // We expect 4 distinct dimensions: relevance, accuracy,
    // completeness, forecast_calibration.
    assert_eq!(outcome.signal.per_dimension.len(), 4);
    let dim_names: std::collections::HashSet<&str> = outcome
        .signal
        .per_dimension
        .iter()
        .map(|d| d.dimension.as_str())
        .collect();
    assert!(dim_names.contains("relevance"));
    assert!(dim_names.contains("accuracy"));
    assert!(dim_names.contains("completeness"));
    assert!(dim_names.contains("forecast_calibration"));

    // No conflicts because each dimension has only one contributor.
    assert!(outcome.signal.conflicts.is_empty());
}

/// SPEC 36 R0.7 done-when: a pre-filter that returns `Err` causes zero
/// dimensional evaluators to run, and the signal is `Undetermined`, naming
/// the failed filter. Before, the error read as "not below block" and every
/// dimensional evaluator scored the unfiltered content.
#[tokio::test]
async fn prefilter_error_is_undetermined_and_runs_no_dimensional() {
    use crate::SignalStatus;
    let bundle = bundle_for(Uuid::new_v4());
    let mut reg = EvaluatorRegistry::new().with_prefilter_block_threshold(Some(0.5));
    reg.register(Arc::new(FailingPreFilter));
    reg.register(Arc::new(FixedScorer {
        name: "dim",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.90,
    }));

    let outcome = reg.run(&bundle).await;
    assert_eq!(outcome.results.len(), 1, "no dimensional evaluator may run");
    assert_eq!(outcome.results[0].evaluator_name, "failing_guard");
    assert_eq!(outcome.signal.status, SignalStatus::Undetermined);
    assert_eq!(
        outcome.signal.failed_evaluators,
        vec!["failing_guard".to_string()]
    );
    assert!(
        outcome.signal.per_dimension.is_empty(),
        "nothing was scored"
    );
    assert!(
        !outcome.prefilter_blocked,
        "undetermined is not a block verdict"
    );
}

#[tokio::test]
async fn inapplicable_prefilter_lets_scoring_proceed() {
    use crate::SignalStatus;
    let bundle = bundle_for(Uuid::new_v4());
    let mut reg = EvaluatorRegistry::new().with_prefilter_block_threshold(Some(0.5));
    reg.register(Arc::new(InapplicablePreFilter));
    reg.register(Arc::new(FixedScorer {
        name: "dim",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.90,
    }));
    let outcome = reg.run(&bundle).await;
    assert_eq!(outcome.results.len(), 2);
    assert_eq!(outcome.signal.status, SignalStatus::Scored);
}

#[tokio::test]
async fn status_reports_block_and_scored() {
    use crate::SignalStatus;
    let bundle = bundle_for(Uuid::new_v4());
    let mut blocked = EvaluatorRegistry::new().with_prefilter_block_threshold(Some(0.5));
    blocked.register(Arc::new(FixedScorer {
        name: "guard",
        tier: EvalTier::PreFilter,
        dim: Dimension::new("safety"),
        score: 0.10,
    }));
    assert_eq!(
        blocked.run(&bundle).await.signal.status,
        SignalStatus::Blocked
    );
    let mut fine = EvaluatorRegistry::new();
    fine.register(Arc::new(FixedScorer {
        name: "dim",
        tier: EvalTier::Dimensional,
        dim: Dimension::new("rapport"),
        score: 0.9,
    }));
    assert_eq!(fine.run(&bundle).await.signal.status, SignalStatus::Scored);
}

/// A stored payload from before the field existed is not read as scored.
#[test]
fn legacy_payload_without_status_is_undetermined() {
    let legacy = serde_json::json!({
        "per_dimension": [], "conflicts": [], "flags": [],
        "active_evaluators": [], "inapplicable_evaluators": [], "failed_evaluators": []
    });
    let s: crate::AggregatedSignal = serde_json::from_value(legacy).unwrap();
    assert_eq!(s.status, crate::SignalStatus::Undetermined);
}

/// A registry that scored nothing (none registered, or all inapplicable)
/// has no verdict to call `Scored`.
#[tokio::test]
async fn nothing_scored_is_undetermined() {
    use crate::SignalStatus;
    let bundle = bundle_for(Uuid::new_v4());
    assert_eq!(
        EvaluatorRegistry::new().run(&bundle).await.signal.status,
        SignalStatus::Undetermined
    );
    let mut only_inapplicable = EvaluatorRegistry::new();
    only_inapplicable.register(Arc::new(AlwaysInapplicable("na")));
    assert_eq!(
        only_inapplicable.run(&bundle).await.signal.status,
        SignalStatus::Undetermined
    );
}
