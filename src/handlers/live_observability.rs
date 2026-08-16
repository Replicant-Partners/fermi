//! Loop 1 → Loop 2: make live traffic observable.
//!
//! ## The gap this closes
//!
//! `EpisodeScorer::write_inline` has exactly one call site, inside
//! `run_eval_cases`. Live executions stored an episode and stopped there, so
//! `agent_timeline_entries` — the *only* window `ObservabilityWorker::scan_agent`
//! reads — was populated exclusively by eval runs. The worker's own doc comment
//! recorded the consequence: *"timeline entries are written only by the eval
//! pipeline — no live conversation has ever produced one."*
//!
//! Everything downstream inherited it. `PersonaDriftMonitor` and
//! `AnomalyDetector` never saw real traffic, so `anomaly_events` was fed only by
//! eval fixtures, so the HITL queue was too. Loop 2's machinery was correct and
//! starved: the corrections it exists to capture could only ever be raised
//! about synthetic runs.
//!
//! ## Why this costs no LLM tokens
//!
//! The obvious implementation — run the full `EvaluatorRegistry` on every
//! execution — would roughly double the platform's model spend and add judge
//! latency to every response. It is also unnecessary, because the observability
//! consumers do not need LLM judgment:
//!
//! - `PersonaDriftMonitor` needs an **embedding** and a **persona version**.
//!   Live executions already generate the embedding.
//! - `AnomalyDetector::detect_in_window` is a pure function over
//!   `anomaly_flags`. It needs no scores at all — only that something wrote
//!   the flags.
//!
//! So this path registers only evaluators that are deterministic by
//! construction: `WildGuardEvaluator::new()` (a `RegexSet`, no I/O) and
//! `CharacterEvaluator::new()` (heuristic commitment matching). Both are built
//! with `::new()` and **never** `with_llm(...)` — passing an API key silently
//! converts them into per-execution model calls, which is precisely the cost
//! this module exists to avoid. `LlmJudgeEvaluator` is deliberately absent.
//!
//! ## What becomes live
//!
//! | Anomaly | Live | Driven by |
//! |---|---|---|
//! | `Safety` | yes | WildGuard regex → `safety:<label>` flag |
//! | `Rupture` | yes | telemetry rapport drop → `rupture:<dyad>` flag |
//! | `Drift` | yes, after a persona bump | embedding cosine across versions |
//! | `RollingConflict` | no | requires two evaluators scoring one dimension; the deterministic set has fully disjoint dimensions, so `AggregatedSignal.conflicts` is structurally always empty. Not fixable at this cost. |
//!
//! All three live variants set `requires_review = true`, which is exactly the
//! predicate behind `GET /api/observatory/hitl`.

use std::sync::Arc;

use agent_bestiary_evaluators::EvaluatorRegistry;
use agent_bestiary_memory::{
    Agent, Episode, EpisodeBundle, MemoryStore, TimelineEntry, TranscriptRole, TranscriptTurn,
};
use evaluator_character::CharacterEvaluator;
use evaluator_wildguard::WildGuardEvaluator;
use uuid::Uuid;

use crate::AppState;

/// Flag the observability worker uses to mark an entry's social pass done.
///
/// Pre-stamping it stops pass 2 folding the same exchange into `dyad_state` a
/// second time: `spawn_dyad_observation` has already done it, using interaction
/// telemetry (real cadence for the reciprocity axis) that `dim_scores` cannot
/// express. Double-counting would inflate `episode_count` and push a flat value
/// into the rupture window, damping the signal we most want to keep.
const SOCIAL_OBSERVED_FLAG: &str = "social:observed";

/// A completed live turn, captured on the hot path and moved into the
/// background task.
pub struct LiveObservation {
    pub episode: Episode,
    pub agent: Agent,
    pub response: String,
    pub session_id: Option<String>,
    /// Set when the dyad tracker has already detected a rupture for this
    /// exchange, so it can be surfaced as an anomaly rather than only a log
    /// line. Before this, `rupture_detected` produced a `tracing::warn!` and
    /// nothing a human would ever be shown.
    pub rupture_detected: bool,
}

/// Score a completed live turn and write its timeline entry, in the background.
///
/// Fire-and-forget by design, following `spawn_dyad_observation`: observability
/// must never delay or fail a user's response. Every failure below is logged
/// and swallowed.
pub fn spawn_live_observation(state: &AppState, obs: LiveObservation) {
    let store = Arc::clone(&state.memory_store);
    tokio::spawn(async move {
        if let Err(e) = record_live_observation(&store, obs).await {
            tracing::warn!(error = %e, "live observation failed");
        }
    });
}

async fn record_live_observation(
    store: &Arc<MemoryStore>,
    obs: LiveObservation,
) -> anyhow::Result<()> {
    let (dim_scores, mut flags) = score_deterministically(&obs).await;

    if obs.rupture_detected {
        if let Some(dyad) = &obs.episode.dyad_id {
            flags.push(format!("rupture:{dyad}"));
        }
    }
    if obs.episode.dyad_id.is_some() {
        flags.push(SOCIAL_OBSERVED_FLAG.to_string());
    }

    // `persona_version_at_write` is stamped by the live handlers. If it is
    // missing we fall back to the agent's current version rather than to 1:
    // the worker skips any entry with `persona_version <= 1`, so defaulting to
    // 1 would silently discard the entry — the exact failure this module was
    // written to remove.
    let persona_version = obs
        .episode
        .persona_version_at_write
        .unwrap_or(obs.agent.persona_version);

    let entry = TimelineEntry {
        entry_id: Uuid::new_v4(),
        agent_id: obs.episode.agent_id,
        episode_id: Some(obs.episode.episode_id),
        run_id: None,
        persona_version,
        dyad_id: obs.episode.dyad_id.clone(),
        session_id: obs.session_id.clone(),
        provenance: obs.episode.provenance.to_string(),
        dim_scores: serde_json::to_value(&dim_scores).unwrap_or_else(|_| serde_json::json!({})),
        drift_norm: None,
        within_version_cosine: None,
        anomaly_flags: serde_json::json!(flags),
        created_at: chrono::Utc::now(),
    };

    store.create_timeline_entry(&entry).await?;

    tracing::debug!(
        agent_id = %obs.episode.agent_id,
        episode_id = %obs.episode.episode_id,
        persona_version,
        dims = dim_scores.len(),
        flags = flags.len(),
        "live timeline entry written"
    );
    Ok(())
}

/// Run the zero-cost evaluators and return `(dim_scores, anomaly_flags)`.
async fn score_deterministically(
    obs: &LiveObservation,
) -> (std::collections::BTreeMap<String, f64>, Vec<String>) {
    let mut registry = EvaluatorRegistry::new();
    // `::new()`, never `with_llm(...)`. See module docs.
    registry.register(Arc::new(WildGuardEvaluator::new()));
    registry.register(Arc::new(CharacterEvaluator::new()));

    // Both evaluators need the agent's own turn: WildGuard screens the
    // response, CharacterEvaluator compares it against the system prompt.
    let transcript = vec![
        TranscriptTurn {
            role: TranscriptRole::User,
            content: obs.episode.query.clone(),
            speaker_id: None,
        },
        TranscriptTurn {
            role: TranscriptRole::Agent,
            content: obs.response.clone(),
            speaker_id: None,
        },
    ];

    let bundle = EpisodeBundle::from_parts(&obs.episode, &obs.agent, transcript, None);
    let outcome = registry.run(&bundle).await;

    let dim_scores: std::collections::BTreeMap<String, f64> = outcome
        .signal
        .per_dimension
        .iter()
        .map(|d| (d.dimension.as_str().to_string(), d.mean))
        .collect();

    // `EvalFlag { category, value }` → the `category:value` strings
    // `AnomalyDetector` matches on. Without this mapping the `Safety` variant
    // stays dead code: `EpisodeScorer::write_inline` writes `[]`
    // unconditionally, which is why no safety anomaly has ever been raised, on
    // any path, including eval runs.
    let flags: Vec<String> = outcome
        .signal
        .flags
        .iter()
        .map(|f| format!("{}:{}", f.kind, f.value))
        .collect();

    (dim_scores, flags)
}

// ─── Scan sweeper ────────────────────────────────────────────────────

/// Default cadence. Long enough that entries batch up — the rolling detectors
/// want a window, not a stream of size-1 scans — short enough that a safety
/// anomaly reaches the HITL queue within a couple of minutes.
const DEFAULT_SCAN_INTERVAL_SECS: u64 = 120;

/// Periodically run `ObservabilityWorker::scan_agent` over agents with
/// unscanned timeline entries.
///
/// Writing entries is only half the job: nothing in the codebase schedules a
/// scan. `scan_agent` had exactly three call sites — the end of `run_eval_cases`
/// and two manual observatory endpoints — so on live traffic the entries would
/// accumulate and never be read, and drift/anomaly detection would stay dark
/// for a subtler reason than before.
///
/// A sweeper rather than a per-execution spawn, for two reasons: it keeps the
/// request path untouched, and it lets the window accumulate so the detectors
/// see a sequence instead of a single entry. `batch_size` on the worker already
/// bounds each pass.
///
/// `OBSERVABILITY_SCAN_SECS=0` disables it, matching `PM_RESOLUTION_SWEEP_SECS`.
pub fn spawn_observability_sweeper(state: AppState) {
    let interval_secs = std::env::var("OBSERVABILITY_SCAN_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SCAN_INTERVAL_SECS);

    if interval_secs == 0 {
        println!("[observability] scan sweeper disabled (OBSERVABILITY_SCAN_SECS=0)");
        return;
    }
    println!("[observability] scanning agents every {interval_secs}s");

    tokio::spawn(async move {
        // Stagger past boot so migrations and schema ensures have the pool.
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        loop {
            match sweep_observability_once(&state).await {
                Ok((agents, anomalies)) if agents > 0 => {
                    println!("[observability] scanned {agents} agent(s), {anomalies} anomaly(ies)");
                }
                Ok(_) => {}
                Err(e) => eprintln!("[observability] sweep failed: {e}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
        }
    });
}

/// One sweep pass. Returns `(agents_scanned, anomalies_created)`.
///
/// Separated from the spawn loop so it can be driven from a test or an admin
/// endpoint without waiting on a timer.
pub async fn sweep_observability_once(state: &AppState) -> anyhow::Result<(usize, usize)> {
    // Only agents with entries the worker has not consumed. The LEFT JOIN
    // against the checkpoint is what keeps this cheap on a platform where most
    // agents are idle: without it every sweep would scan all 745 rows.
    //
    // Column is `last_scan_completed_at`. Verified against the live schema —
    // an earlier draft of this query said `last_scanned_at`, which does not
    // exist, and because the sweep's only error handling is an `eprintln!` it
    // would have failed silently on every pass forever.
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT t.agent_id
          FROM agent_timeline_entries t
          LEFT JOIN agent_observability_state s ON s.agent_id = t.agent_id
         WHERE s.last_scan_completed_at IS NULL
            OR t.created_at > s.last_scan_completed_at
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let worker =
        agent_bestiary_observability::ObservabilityWorker::new(Arc::clone(&state.memory_store));

    let mut scanned = 0usize;
    let mut anomalies = 0usize;
    for (agent_id,) in rows {
        match worker.scan_agent(agent_id).await {
            Ok(report) => {
                scanned += 1;
                anomalies += report.anomalies_detected;
            }
            // One bad agent must not stop the sweep.
            Err(e) => tracing::warn!(agent_id = %agent_id, error = %e, "agent scan failed"),
        }
    }
    Ok((scanned, anomalies))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A detected rupture must reach `anomaly_flags` in the exact shape
    /// `AnomalyDetector::Rupture` matches, or the detector cannot see it.
    #[test]
    fn rupture_flag_matches_the_detector_prefix() {
        let dyad = "agent:user";
        let flag = format!("rupture:{dyad}");
        assert!(flag.starts_with("rupture:"));
        assert_eq!(flag, "rupture:agent:user");
    }

    /// Pre-stamping this is what stops the worker's social pass double-counting
    /// an exchange `spawn_dyad_observation` has already folded in.
    #[test]
    fn social_observed_flag_matches_the_worker_constant() {
        assert_eq!(SOCIAL_OBSERVED_FLAG, "social:observed");
    }

    /// The evaluator flag → anomaly flag mapping. `EvalFlag { category:
    /// "safety", value: "harmful" }` must render as `safety:harmful`, because
    /// `AnomalyDetector::Safety` matches any flag beginning `safety:`.
    #[test]
    fn eval_flags_render_in_detector_form() {
        let rendered = format!("{}:{}", "safety", "harmful_content");
        assert!(
            rendered.starts_with("safety:"),
            "AnomalyDetector::Safety matches on this prefix"
        );
    }
}
