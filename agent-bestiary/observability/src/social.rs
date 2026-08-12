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

/// Observed target value per axis. `None` leaves that axis unchanged.
#[derive(Debug, Clone, Copy, Default)]
pub struct AxisTargets {
    pub rapport: Option<f64>,
    pub trust: Option<f64>,
    pub reciprocity: Option<f64>,
}

/// How the agent performed on one live exchange, in terms every episode
/// already records — no evaluator, no LLM, no rubric required.
///
/// ## What each axis actually measures
///
/// These are **behavioural proxies**, and it is worth being precise about
/// what they do and do not capture:
///
/// - `trust` ← **reliability**. Did the agent deliver a confident, non-failing
///   answer? A user's trust tracks whether the thing works; failures and
///   low-confidence hedging erode it fast, which is the behaviour we want.
/// - `rapport` ← **engagement depth**. How much the human invested in the
///   turn (message length, saturating), discounted hard when the agent
///   failed them. Long considered questions to an agent that answers well
///   is what a warming relationship looks like from the outside.
/// - `reciprocity` ← **return cadence**. Time since the last exchange, decaying
///   exponentially. Coming back tomorrow is the single strongest signal a
///   companion is working; silence for a month is the strongest signal it
///   is not.
///
/// What they are *not*: a semantic read of whether the conversation felt
/// good. `evaluator-sotopia` is the instrument for that, and when it starts
/// producing signals its scores flow through [`SocialInteractionTracker::observe`]
/// into the same axes. Until then these keep the loop closed and moving
/// rather than frozen at 0.5.
#[derive(Debug, Clone)]
pub struct InteractionObservation {
    /// Execution succeeded outright.
    pub succeeded: bool,
    /// Execution partially succeeded (e.g. below confidence threshold).
    pub partial: bool,
    /// Agent's self-reported confidence, 0..1.
    pub confidence: f64,
    /// Characters the human wrote in this turn — the engagement proxy.
    pub user_chars: usize,
    /// When the exchange happened. Wall-clock for a live turn; the episode
    /// timestamp when replaying history. Reciprocity is measured as the gap
    /// between this and the dyad's previous exchange, so replaying with real
    /// timestamps reproduces the cadence the relationship actually had.
    pub occurred_at: chrono::DateTime<chrono::Utc>,
}

/// Engagement saturates: past a point, a longer message does not mean a
/// deeper relationship. `1 - e^(-chars/250)` reaches ~0.63 at 250 chars and
/// ~0.91 at 600, so ordinary questions register without a single essay
/// pinning the axis at 1.0.
const ENGAGEMENT_SCALE: f64 = 250.0;

/// Rapport floor for an exchange that simply went well.
///
/// Message length alone is a bad *absolute* scale for rapport: a crisp
/// 60-character question is a perfectly healthy interaction, but raw
/// saturation scores it ~0.2, which reads as a failing relationship. So a
/// successful exchange starts from this floor and earns depth on top,
/// rather than having to buy its way up from zero with verbosity.
const RAPPORT_SUCCESS_FLOOR: f64 = 0.65;

/// How much of the rapport range depth can add above the floor.
const RAPPORT_DEPTH_WEIGHT: f64 = 0.35;

/// Reciprocity half-life. `e^(-days/14)` → 1.0 same-day, ~0.49 at 10 days,
/// ~0.12 at 30. Tuned so a weekly user stays healthy and a lapsed one
/// visibly decays.
const RECIPROCITY_DECAY_DAYS: f64 = 14.0;

impl InteractionObservation {
    fn targets(&self, prev: &DyadState) -> AxisTargets {
        // ── Reliability → trust ──
        // Success floors at 0.6 and scales with confidence, so a hedged
        // answer still counts as an answer. Outright failure targets 0.0 so
        // repeated breakage pulls trust down decisively.
        let reliability = if self.succeeded {
            0.6 + 0.4 * self.confidence.clamp(0.0, 1.0)
        } else if self.partial {
            0.35
        } else {
            0.0
        };

        // ── Engagement → rapport ──
        // A good exchange is worth the floor; the depth of the human's turn
        // earns the remainder. A failed exchange is not rapport-building
        // however much they wrote — arguably the opposite — so it collapses
        // to a low target and drags the running average down.
        let engagement = 1.0 - (-(self.user_chars as f64) / ENGAGEMENT_SCALE).exp();
        let rapport = if self.succeeded {
            RAPPORT_SUCCESS_FLOOR + RAPPORT_DEPTH_WEIGHT * engagement
        } else if self.partial {
            0.35 + 0.15 * engagement
        } else {
            0.10
        };

        // ── Return cadence → reciprocity ──
        // The first exchange has no gap to measure, so leave the axis at its
        // neutral default rather than inventing a value from a single point.
        let reciprocity = if prev.episode_count == 0 {
            None
        } else {
            let gap_days =
                (self.occurred_at - prev.last_updated_at).num_seconds() as f64 / 86_400.0;
            Some((-gap_days.max(0.0) / RECIPROCITY_DECAY_DAYS).exp())
        };

        AxisTargets {
            rapport: Some(rapport.clamp(0.0, 1.0)),
            trust: Some(reliability.clamp(0.0, 1.0)),
            reciprocity: reciprocity.map(|v| v.clamp(0.0, 1.0)),
        }
    }
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
        self.observe_with(agent_id, dyad_id, human_id, &|name| {
            signal
                .per_dimension
                .iter()
                .find(|d| d.dimension.as_str() == name)
                .map(|d| d.mean)
        })
        .await
    }

    /// Same update, sourced from a persisted `agent_timeline_entries.dim_scores`
    /// JSON object (`{ dimension: mean }`) instead of a live `AggregatedSignal`.
    ///
    /// This is what the background [`crate::worker::ObservabilityWorker`] uses:
    /// by scan time the original signal is long gone, but the per-dimension
    /// means were persisted inline by [`crate::scorer::EpisodeScorer`].
    pub async fn observe_dim_scores(
        &self,
        agent_id: Uuid,
        dyad_id: Option<&str>,
        human_id: Option<&str>,
        dim_scores: &serde_json::Value,
    ) -> Result<SocialUpdate, ObservabilityError> {
        self.observe_with(agent_id, dyad_id, human_id, &|name| {
            dim_scores.get(name).and_then(|v| v.as_f64())
        })
        .await
    }

    /// Fold one live interaction into the dyad's running state.
    ///
    /// This is the path real conversations take. It exists because the
    /// evaluator-derived path cannot serve them: `rapport`,
    /// `social_capital` and `goal_completion` are produced only by
    /// `evaluator-sotopia`, which runs inside the eval pipeline and needs a
    /// per-test-case rubric — neither of which a live chat turn has.
    /// Waiting on that would leave every real relationship pinned at its
    /// 0.5 defaults forever.
    ///
    /// Instead the three axes are derived from interaction telemetry that
    /// every episode already carries, at zero additional LLM cost. See
    /// [`InteractionObservation`] for the derivation and its limits.
    pub async fn observe_interaction(
        &self,
        agent_id: Uuid,
        dyad_id: &str,
        obs: &InteractionObservation,
    ) -> Result<SocialUpdate, ObservabilityError> {
        self.observe_axes(agent_id, Some(dyad_id), None, &|prev| obs.targets(prev))
            .await
    }

    /// Recompute a dyad's state from its full interaction history, replacing
    /// whatever was stored.
    ///
    /// Used to backfill relationships whose episodes predate the social pass.
    /// Folding from a fresh [`initial_dyad_state`] in chronological order
    /// makes this **deterministic and idempotent**: running it twice yields
    /// the same state, and the result matches what live accumulation would
    /// have produced had the tracker been wired from the start. That is why
    /// it needs no "already processed" marker.
    ///
    /// `observations` must be ordered oldest-first.
    pub async fn replay_dyad(
        &self,
        agent_id: Uuid,
        dyad_id: &str,
        observations: &[InteractionObservation],
    ) -> Result<SocialUpdate, ObservabilityError> {
        let human_id = agent_bestiary_memory::human_id_from_dyad(dyad_id)
            .ok_or_else(|| ObservabilityError::Inapplicable("unparseable dyad_id".into()))?;
        let first_at = observations
            .first()
            .map(|o| o.occurred_at)
            .unwrap_or_else(chrono::Utc::now);

        let mut state = initial_dyad_state(dyad_id, agent_id, human_id, first_at);
        let mut rupture_detected = false;
        let mut max_drop = 0.0_f64;

        for obs in observations {
            let (next, ruptured, drop) =
                apply_targets(&state, obs.targets(&state), obs.occurred_at);
            state = next;
            // Report a rupture if one occurred anywhere in the history, and
            // carry the largest drop seen.
            rupture_detected |= ruptured;
            max_drop = max_drop.max(drop);
        }

        self.store
            .upsert_dyad_state(&state)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?;

        Ok(SocialUpdate {
            state,
            rupture_detected,
            max_rapport_drop: max_drop,
        })
    }

    /// Shared core keyed on dimension names. Leaves an axis untouched when
    /// its dimension is absent.
    async fn observe_with(
        &self,
        agent_id: Uuid,
        dyad_id: Option<&str>,
        human_id: Option<&str>,
        dim_value: &(dyn Fn(&str) -> Option<f64> + Sync),
    ) -> Result<SocialUpdate, ObservabilityError> {
        self.observe_axes(agent_id, dyad_id, human_id, &|_prev| AxisTargets {
            rapport: dim_value("rapport"),
            trust: dim_value("persona_fidelity"),
            reciprocity: match (dim_value("social_capital"), dim_value("goal_completion")) {
                (Some(a), Some(b)) => Some((a + b) / 2.0),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            },
        })
        .await
    }

    /// Shared core. `targets` maps the previous state to the observed value
    /// for each axis; `None` leaves that axis unchanged. Everything after
    /// this point — smoothing, rolling history, rupture detection, persist
    /// — is identical regardless of where the targets came from.
    async fn observe_axes(
        &self,
        agent_id: Uuid,
        dyad_id: Option<&str>,
        human_id: Option<&str>,
        targets: &(dyn Fn(&DyadState) -> AxisTargets + Sync),
    ) -> Result<SocialUpdate, ObservabilityError> {
        let dyad_id = dyad_id
            .ok_or_else(|| ObservabilityError::Inapplicable("no dyad_id on episode".into()))?;
        // Fall back to parsing the human out of the dyad id when the caller
        // does not have it to hand — the id is constructed to carry it.
        let human_id = human_id
            .or_else(|| agent_bestiary_memory::human_id_from_dyad(dyad_id))
            .ok_or_else(|| ObservabilityError::Inapplicable("no human_id provided".into()))?;

        let prev = self
            .store
            .get_dyad_state(dyad_id)
            .await
            .map_err(|e| ObservabilityError::Storage(e.to_string()))?
            .unwrap_or_else(|| initial_dyad_state(dyad_id, agent_id, human_id, chrono::Utc::now()));

        let (new_state, rupture_detected, max_drop) =
            apply_targets(&prev, targets(&prev), chrono::Utc::now());

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

/// The pure fold: previous state + observed targets → next state.
///
/// Deliberately free of I/O and of `Utc::now()` so that replaying a
/// dyad's history produces *exactly* the state live accumulation would
/// have produced. `observed_at` is the moment the exchange happened —
/// wall-clock for a live turn, the episode timestamp during a replay.
///
/// Returns `(next_state, rupture_detected, max_rapport_drop)`.
pub fn apply_targets(
    prev: &DyadState,
    targets: AxisTargets,
    observed_at: chrono::DateTime<chrono::Utc>,
) -> (DyadState, bool, f64) {
    let new_rapport = match targets.rapport {
        Some(v) => smooth(prev.rapport, v),
        None => prev.rapport,
    };
    let new_trust = match targets.trust {
        Some(v) => smooth(prev.trust, v),
        None => prev.trust,
    };
    let new_reciprocity = match targets.reciprocity {
        Some(v) => smooth(prev.reciprocity, v),
        None => prev.reciprocity,
    };

    // Rolling rapport history, bounded to the rupture window.
    let mut history: Vec<f64> = prev
        .recent_rapport
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();
    history.push(new_rapport);
    if history.len() > RUPTURE_WINDOW_LEN {
        history.drain(0..(history.len() - RUPTURE_WINDOW_LEN));
    }

    let (rupture_detected, max_drop) = detect_rupture(&history);

    let next = DyadState {
        dyad_id: prev.dyad_id.clone(),
        agent_id: prev.agent_id,
        human_id: prev.human_id.clone(),
        rapport: new_rapport.clamp(0.0, 1.0),
        trust: new_trust.clamp(0.0, 1.0),
        reciprocity: new_reciprocity.clamp(0.0, 1.0),
        episode_count: prev.episode_count + 1,
        recent_rapport: serde_json::Value::Array(
            history
                .iter()
                .filter_map(|v| serde_json::Number::from_f64(*v).map(serde_json::Value::Number))
                .collect(),
        ),
        last_updated_at: observed_at,
        created_at: prev.created_at,
    };

    (next, rupture_detected, max_drop)
}

/// A fresh, unobserved dyad. Axes start neutral at 0.5.
pub fn initial_dyad_state(
    dyad_id: &str,
    agent_id: Uuid,
    human_id: &str,
    at: chrono::DateTime<chrono::Utc>,
) -> DyadState {
    DyadState {
        dyad_id: dyad_id.to_string(),
        agent_id,
        human_id: human_id.to_string(),
        rapport: 0.5,
        trust: 0.5,
        reciprocity: 0.5,
        episode_count: 0,
        recent_rapport: serde_json::json!([]),
        last_updated_at: at,
        created_at: at,
    }
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

    fn state_at(episode_count: i32, last_seen: chrono::DateTime<chrono::Utc>) -> DyadState {
        DyadState {
            dyad_id: "dyad:a:b".into(),
            agent_id: Uuid::nil(),
            human_id: "b".into(),
            rapport: 0.5,
            trust: 0.5,
            reciprocity: 0.5,
            episode_count,
            recent_rapport: serde_json::json!([]),
            last_updated_at: last_seen,
            created_at: last_seen,
        }
    }

    fn obs(succeeded: bool, confidence: f64, user_chars: usize) -> InteractionObservation {
        obs_at(succeeded, confidence, user_chars, chrono::Utc::now())
    }

    fn obs_at(
        succeeded: bool,
        confidence: f64,
        user_chars: usize,
        occurred_at: chrono::DateTime<chrono::Utc>,
    ) -> InteractionObservation {
        InteractionObservation {
            succeeded,
            partial: false,
            confidence,
            user_chars,
            occurred_at,
        }
    }

    /// The whole point of the interaction path: successive exchanges must
    /// produce *different* numbers. A frozen 0.5 is the bug it exists to fix.
    #[test]
    fn interaction_targets_move_off_the_default() {
        let prev = state_at(3, chrono::Utc::now());
        let t = obs(true, 0.9, 400).targets(&prev);
        assert!(
            t.trust.unwrap() > 0.5,
            "trust should rise on a confident success"
        );
        assert!(
            t.rapport.unwrap() > 0.5,
            "rapport should rise on an engaged turn"
        );
        assert!(
            t.reciprocity.unwrap() > 0.9,
            "same-day return should be near 1.0"
        );
    }

    #[test]
    fn failure_drives_trust_and_rapport_down() {
        let prev = state_at(3, chrono::Utc::now());
        let good = obs(true, 0.9, 400).targets(&prev);
        let bad = obs(false, 0.0, 400).targets(&prev);
        assert!(bad.trust.unwrap() < good.trust.unwrap());
        assert!(bad.rapport.unwrap() < good.rapport.unwrap());
        assert_eq!(bad.trust.unwrap(), 0.0);
    }

    #[test]
    fn reciprocity_decays_with_absence() {
        let now = chrono::Utc::now();
        let recent = obs(true, 0.8, 200).targets(&state_at(3, now));
        let lapsed = obs(true, 0.8, 200).targets(&state_at(3, now - chrono::Duration::days(30)));
        assert!(
            lapsed.reciprocity.unwrap() < recent.reciprocity.unwrap(),
            "a month of silence must score below a same-day return"
        );
        assert!(lapsed.reciprocity.unwrap() < 0.2);
    }

    #[test]
    fn first_interaction_leaves_reciprocity_unset() {
        let t = obs(true, 0.8, 200).targets(&state_at(0, chrono::Utc::now()));
        assert_eq!(
            t.reciprocity, None,
            "no prior visit means no cadence to measure yet"
        );
    }

    #[test]
    fn engagement_saturates_rather_than_pinning() {
        let prev = state_at(3, chrono::Utc::now());
        let short = obs(true, 0.8, 50).targets(&prev).rapport.unwrap();
        let long = obs(true, 0.8, 600).targets(&prev).rapport.unwrap();
        let essay = obs(true, 0.8, 20_000).targets(&prev).rapport.unwrap();
        assert!(short < long, "longer turns should register");
        assert!(essay <= 1.0);
        assert!(
            essay - long < 0.2,
            "a 20k-char essay must not dwarf a 600-char message"
        );
    }

    /// Fold a history the way `replay_dyad` does, without touching storage.
    fn fold(observations: &[InteractionObservation]) -> DyadState {
        let start = observations
            .first()
            .map(|o| o.occurred_at)
            .unwrap_or_else(chrono::Utc::now);
        let mut state = initial_dyad_state("dyad:a:b", Uuid::nil(), "b", start);
        for o in observations {
            let (next, _, _) = apply_targets(&state, o.targets(&state), o.occurred_at);
            state = next;
        }
        state
    }

    /// A daily user getting good answers should end up visibly healthier
    /// than the 0.5 the relationship started at. This is the property the
    /// whole exercise is for.
    #[test]
    fn engaged_history_produces_a_healthy_moving_relationship() {
        let t0 = chrono::Utc::now() - chrono::Duration::days(10);
        let history: Vec<_> = (0..10)
            .map(|i| obs_at(true, 0.85, 350, t0 + chrono::Duration::days(i)))
            .collect();
        let s = fold(&history);

        assert_eq!(s.episode_count, 10);
        assert!(
            s.trust > 0.8,
            "consistent success should build trust, got {}",
            s.trust
        );
        assert!(
            s.rapport > 0.6,
            "sustained engagement should build rapport, got {}",
            s.rapport
        );
        assert!(
            s.reciprocity > 0.8,
            "daily return should keep reciprocity high, got {}",
            s.reciprocity
        );
    }

    /// Calibration guard, pinned against a real replayed history (xaman_ek,
    /// 64 successful exchanges of 50-70 chars over five weeks).
    ///
    /// An earlier derivation scored short-but-successful turns at ~0.19
    /// rapport, which rendered a perfectly healthy relationship as failing.
    /// Ordinary concise questions must land in a believable band.
    #[test]
    fn concise_successful_exchanges_score_as_healthy() {
        let t0 = chrono::Utc::now() - chrono::Duration::days(35);
        let history: Vec<_> = (0..64)
            .map(|i| obs_at(true, 0.5, 60, t0 + chrono::Duration::hours(i * 12)))
            .collect();
        let s = fold(&history);
        assert!(
            s.rapport > 0.6 && s.rapport < 0.85,
            "short successful turns should read as healthy, not failing: {}",
            s.rapport
        );
        let health = (s.rapport + s.trust + s.reciprocity) / 3.0;
        assert!(
            health > 0.7,
            "overall health should be strong, got {}",
            health
        );
    }

    /// The same history must never depend on when it is replayed, or how
    /// many times — that is what makes the backfill safe to re-run.
    #[test]
    fn replay_is_deterministic() {
        let t0 = chrono::Utc::now() - chrono::Duration::days(20);
        let history: Vec<_> = (0..12)
            .map(|i| {
                obs_at(
                    i % 4 != 0,
                    0.7,
                    100 + i as usize * 20,
                    t0 + chrono::Duration::days(i),
                )
            })
            .collect();

        let a = fold(&history);
        let b = fold(&history);
        assert_eq!(a.rapport, b.rapport);
        assert_eq!(a.trust, b.trust);
        assert_eq!(a.reciprocity, b.reciprocity);
        assert_eq!(a.episode_count, b.episode_count);
    }

    /// A relationship that degrades must score below one that does not,
    /// otherwise the signal carries no information.
    #[test]
    fn degrading_history_scores_below_healthy_history() {
        let t0 = chrono::Utc::now() - chrono::Duration::days(10);
        let healthy: Vec<_> = (0..10)
            .map(|i| obs_at(true, 0.9, 300, t0 + chrono::Duration::days(i)))
            .collect();
        // Starts well, then the agent begins failing.
        let degrading: Vec<_> = (0..10)
            .map(|i| obs_at(i < 4, 0.9, 300, t0 + chrono::Duration::days(i)))
            .collect();

        let h = fold(&healthy);
        let d = fold(&degrading);
        assert!(d.trust < h.trust, "repeated failure must erode trust");
        assert!(d.rapport < h.rapport, "repeated failure must erode rapport");
    }

    /// Rapport falling away over the window is exactly the rupture case.
    #[test]
    fn collapse_after_good_run_trips_rupture() {
        let t0 = chrono::Utc::now() - chrono::Duration::days(10);
        let mut history: Vec<_> = (0..5)
            .map(|i| obs_at(true, 0.9, 800, t0 + chrono::Duration::days(i)))
            .collect();
        // Then a run of hard failures on short, frustrated messages.
        history.extend((5..10).map(|i| obs_at(false, 0.0, 20, t0 + chrono::Duration::days(i))));

        let start = history[0].occurred_at;
        let mut state = initial_dyad_state("dyad:a:b", Uuid::nil(), "b", start);
        let mut sawrupture = false;
        for o in &history {
            let (next, ruptured, _) = apply_targets(&state, o.targets(&state), o.occurred_at);
            state = next;
            sawrupture |= ruptured;
        }
        assert!(
            sawrupture,
            "a collapse from high to low rapport should trip the detector"
        );
    }

    /// Smoothing means one bad turn dents the relationship without erasing it.
    #[test]
    fn single_failure_does_not_erase_accumulated_trust() {
        let established = 0.9;
        let after_one_failure = smooth(established, 0.0);
        assert!(
            after_one_failure > 0.55,
            "one failure should not wipe trust"
        );
        assert!(after_one_failure < established, "but it must register");
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
