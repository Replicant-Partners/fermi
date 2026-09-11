//! Adjudicating a semantic rule against the outcomes of runs that used it.
//!
//! # What this is, and the thing it must not be mistaken for
//!
//! `semantic_rules.verification_status` has existed since migration 010 with
//! four readers and no production writer. Every one of the 264 real rules on
//! this deployment sits at `pending`, so `verified_rules` is structurally 0,
//! `verification_rate` is not a low rate but the absence of a rate, and
//! [`crate::consolidation::extractor_self_knowledge`]'s verified-first ordering
//! has never had anything to put first.
//!
//! Three cheaper signals were measured before any of this was built, and none
//! held: near-duplicate rules to reject, **0** pairs at cosine ≥ 0.95;
//! corroboration from an independent episode cluster, **6 of 265**; a
//! human-correction corpus to adjudicate against, **none**. Rules are well
//! grounded — 265 of 265 carry a source cluster, averaging 12.2 episodes — and
//! are not near-copies of one another. There was simply no evidence in stored
//! data that adjudicated them.
//!
//! `rule_retrievals` (migration 235) is that evidence: one row per rule per
//! prompt it was injected into. This module reads it and compares a rule's runs
//! against the agent's own base rate over the same window.
//!
//! ## It is correlation, and it says so in the database
//!
//! A rule being in the prompt of runs that succeeded does not mean the rule
//! caused the success. `outcome_trust`'s own statement of the limit:
//!
//! > Nothing about whether retrieval changed the agent's output. That is the
//! > measurement this platform does not have and cannot take from stored data:
//! > it needs a control arm, and forming one means suppressing rule injection
//! > for a turn, which nothing does.
//!
//! That ceiling is real and this module does not pretend to clear it. So
//! `verified` here means **earned its place in the prompt**, not *true* — and
//! the distinction is written into the row rather than left in a doc comment.
//! Every adjudication stamps `verification_method = "outcome_correlation:v1"`
//! and the full evidence into `verification_details`, a jsonb column that had
//! no references anywhere before this. A reader who finds a verified rule can
//! see exactly what verified it and how weak that is.
//!
//! Naming the method is the whole defence against laundering a correlation into
//! a constraint. If a stronger verifier ever exists, it stamps a different
//! method and the two remain distinguishable forever.
//!
//! ## Why the base rate and not a fixed threshold
//!
//! A fixed pass mark would encode a success rate as a quality bar, and this
//! platform's agents differ enormously in how often they fail. The comparison
//! that carries information is against the agent's own baseline over the same
//! window — the same shape as `brier_skill_score`, which is 1 − brier/baseline
//! precisely because a raw score against a lopsided outcome set says nothing.
//!
//! ## `running` is not evidence
//!
//! Production carries four `execution_status` values, and `running` is not in
//! [`crate::types::ExecutionStatus`]: 3,452 `success`, 290 `failure`, 5
//! `running`, 2 `partial`. An unresolved run is excluded from both numerator
//! and denominator rather than counted as a failure — the same distinction
//! between *unmeasured* and *bad* that the rest of this codebase makes.
//! `partial` counts as not-success, because it means the run did not do what
//! was asked.

use crate::Result;
use serde_json::json;
use uuid::Uuid;

/// Runs a rule must have appeared in before it can be adjudicated at all.
///
/// Ten is a judgement, and a soft one. Below it a single unlucky run moves the
/// rate by ten points and the comparison against a base rate is noise. It is
/// deliberately not tuned against this deployment's data, because there is no
/// data yet: `rule_retrievals` starts empty and every rule reports
/// `Pending(TooFewRuns)` until it fills. That is the correct initial state and
/// not a failure to report.
pub const MIN_RUNS: i64 = 10;

/// How far below its agent's base rate a rule must fall to be rejected.
///
/// Asymmetric with verification on purpose. Rejection retires a rule from every
/// future prompt, so it should require a margin rather than merely losing a
/// coin-flip against the baseline; verification only grants ordering
/// preference, which is cheap to be wrong about and cheap to reverse.
pub const REJECT_MARGIN: f64 = 0.15;

/// The method string stamped on every rule this module adjudicates.
///
/// Versioned so a stronger verifier is distinguishable from this one in the
/// same column, forever, without a migration.
pub const METHOD: &str = "outcome_correlation:v1";

/// One rule's record across the runs that used it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleOutcome {
    pub rule_id: Uuid,
    /// Runs that used this rule and have resolved. Excludes `running`.
    pub runs: i64,
    /// Of those, how many succeeded. `partial` is not a success.
    pub successes: i64,
}

impl RuleOutcome {
    /// `None` when nothing has resolved — never `0.0`, which would read as
    /// "failed every time".
    pub fn rate(&self) -> Option<f64> {
        (self.runs > 0).then(|| self.successes as f64 / self.runs as f64)
    }
}

/// Why a rule is still `pending`. Three different situations, and a surface
/// that collapses them tells an operator to do the wrong thing.
#[derive(Debug, Clone, PartialEq)]
pub enum Why {
    /// Not enough resolved runs yet. Time fixes this; nothing else needs to.
    TooFewRuns { runs: i64, needed: i64 },
    /// The agent has no resolved runs in the window, so there is nothing to
    /// compare against. A property of the agent, not of the rule.
    NoBaseline,
    /// Enough evidence, and it points nowhere: below the baseline but not by
    /// the rejection margin. A real verdict of "we looked and cannot say".
    Inconclusive { rate: f64, baseline: f64 },
}

/// The verdict. `Pending` leaves the stored status untouched.
#[derive(Debug, Clone, PartialEq)]
pub enum Adjudication {
    Verified { rate: f64, baseline: f64 },
    Rejected { rate: f64, baseline: f64 },
    Pending(Why),
}

impl Adjudication {
    /// The evidence, for `verification_details`. Written for every outcome
    /// including `Pending`, so "we looked and could not say" is on the row
    /// rather than indistinguishable from never having looked.
    pub fn details(&self, o: &RuleOutcome) -> serde_json::Value {
        let base = json!({
            "method": METHOD,
            "runs": o.runs,
            "successes": o.successes,
            "min_runs": MIN_RUNS,
            "reject_margin": REJECT_MARGIN,
            "caveat": "Correlation over runs that had this rule in the prompt. \
                       No control arm exists, so this is not evidence the rule \
                       caused the outcome.",
        });
        let mut v = base;
        let m = v.as_object_mut().expect("json object");
        match self {
            Self::Verified { rate, baseline } | Self::Rejected { rate, baseline } => {
                m.insert("rate".into(), json!(rate));
                m.insert("baseline".into(), json!(baseline));
            }
            Self::Pending(Why::Inconclusive { rate, baseline }) => {
                m.insert("rate".into(), json!(rate));
                m.insert("baseline".into(), json!(baseline));
                m.insert("outcome".into(), json!("inconclusive"));
            }
            Self::Pending(Why::TooFewRuns { needed, .. }) => {
                m.insert("outcome".into(), json!("too_few_runs"));
                m.insert("needed".into(), json!(needed));
            }
            Self::Pending(Why::NoBaseline) => {
                m.insert("outcome".into(), json!("no_baseline"));
            }
        }
        v
    }
}

/// The decision, as a pure function over the evidence.
///
/// Pure so it can be falsified without a database — the thresholds are the
/// part worth testing, and a test that needs Postgres to exercise a comparison
/// is a test nobody runs.
pub fn adjudicate(o: &RuleOutcome, baseline: Option<f64>) -> Adjudication {
    if o.runs < MIN_RUNS {
        return Adjudication::Pending(Why::TooFewRuns {
            runs: o.runs,
            needed: MIN_RUNS,
        });
    }
    let (Some(rate), Some(baseline)) = (o.rate(), baseline) else {
        return Adjudication::Pending(Why::NoBaseline);
    };
    if rate <= baseline - REJECT_MARGIN {
        return Adjudication::Rejected { rate, baseline };
    }
    if rate >= baseline {
        return Adjudication::Verified { rate, baseline };
    }
    Adjudication::Pending(Why::Inconclusive { rate, baseline })
}

/// The agent's own success rate over resolved runs in the window.
pub async fn baseline_for_agent(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    window_days: i64,
) -> Result<Option<f64>> {
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT count(*)::bigint,
                count(*) FILTER (WHERE execution_status = 'success')::bigint
           FROM episodes
          WHERE agent_id = $1
            AND execution_status <> 'running'
            AND timestamp_created >= NOW() - ($2 || ' days')::interval",
    )
    .bind(agent_id)
    .bind(window_days.to_string())
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some((n, ok)) if n > 0 => Some(ok as f64 / n as f64),
        _ => None,
    })
}

/// Per-rule outcomes for one agent, over the window.
///
/// The correlation is `query_sha` plus time order, for the reason migration 235
/// gives: retrieval happens before the episode exists, so there is no episode
/// id to record. `DISTINCT` on the episode keeps a rule retrieved twice for the
/// same run from counting that run twice.
pub async fn outcomes_for_agent(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    window_days: i64,
) -> Result<Vec<RuleOutcome>> {
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        "WITH paired AS (
           SELECT DISTINCT rr.rule_id, e.episode_id, e.execution_status
             FROM rule_retrievals rr
             JOIN LATERAL (
               SELECT ep.episode_id, ep.execution_status
                 FROM episodes ep
                WHERE ep.agent_id = rr.agent_id
                  AND 'sha256:' || encode(sha256(convert_to(ep.query, 'UTF8')), 'hex')
                      = rr.query_sha
                  AND ep.timestamp_created >= rr.created_at
                ORDER BY ep.timestamp_created
                LIMIT 1
             ) e ON true
            WHERE rr.agent_id = $1
              AND rr.created_at >= NOW() - ($2 || ' days')::interval
              AND e.execution_status <> 'running'
         )
         SELECT rule_id,
                count(*)::bigint,
                count(*) FILTER (WHERE execution_status = 'success')::bigint
           FROM paired
          GROUP BY rule_id",
    )
    .bind(agent_id)
    .bind(window_days.to_string())
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(rule_id, runs, successes)| RuleOutcome {
            rule_id,
            runs,
            successes,
        })
        .collect())
}

/// Counts from one adjudication pass. These are what the consolidation cycle
/// reports as `rules_verified` / `rules_rejected`, which were hardcoded 0.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub verified: usize,
    pub rejected: usize,
    pub still_pending: usize,
}

/// Adjudicate every rule of one agent and write the verdicts.
///
/// Only `Verified` and `Rejected` change `verification_status`. A `Pending`
/// verdict still writes `verification_method` and `verification_details`, so
/// "adjudicated and inconclusive" is distinguishable from "never adjudicated" —
/// the distinction the whole Observatory panel turned on.
///
/// A rejected rule is also deactivated: `is_active = false` removes it from
/// retrieval, which is the only thing rejection can usefully mean. Retrieval
/// filters on `is_active`, so a rejected-but-active rule would keep reaching
/// prompts and the verdict would be decoration.
pub async fn adjudicate_agent(
    pool: &sqlx::PgPool,
    agent_id: Uuid,
    window_days: i64,
) -> Result<Applied> {
    let baseline = baseline_for_agent(pool, agent_id, window_days).await?;
    let outcomes = outcomes_for_agent(pool, agent_id, window_days).await?;

    let mut applied = Applied::default();
    for o in &outcomes {
        let verdict = adjudicate(o, baseline);
        let details = verdict.details(o);
        match verdict {
            Adjudication::Verified { .. } => {
                sqlx::query(
                    "UPDATE semantic_rules
                        SET verification_status = 'verified',
                            verification_method = $2,
                            verification_details = $3
                      WHERE rule_id = $1",
                )
                .bind(o.rule_id)
                .bind(METHOD)
                .bind(&details)
                .execute(pool)
                .await?;
                applied.verified += 1;
            }
            Adjudication::Rejected { .. } => {
                sqlx::query(
                    "UPDATE semantic_rules
                        SET verification_status = 'rejected',
                            verification_method = $2,
                            verification_details = $3,
                            is_active = false
                      WHERE rule_id = $1",
                )
                .bind(o.rule_id)
                .bind(METHOD)
                .bind(&details)
                .execute(pool)
                .await?;
                applied.rejected += 1;
            }
            Adjudication::Pending(_) => {
                sqlx::query(
                    "UPDATE semantic_rules
                        SET verification_method = $2,
                            verification_details = $3
                      WHERE rule_id = $1",
                )
                .bind(o.rule_id)
                .bind(METHOD)
                .bind(&details)
                .execute(pool)
                .await?;
                applied.still_pending += 1;
            }
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(runs: i64, successes: i64) -> RuleOutcome {
        RuleOutcome {
            rule_id: Uuid::nil(),
            runs,
            successes,
        }
    }

    /// An empty table verifies nothing, and that is the correct initial state.
    ///
    /// `rule_retrievals` ships empty. If this returned `Verified` on no
    /// evidence, every rule on the platform would be promoted the moment the
    /// verifier ran, which is the failure mode that makes a quality signal
    /// worse than no signal.
    #[test]
    fn no_evidence_promotes_nothing() {
        assert_eq!(
            adjudicate(&o(0, 0), Some(0.9)),
            Adjudication::Pending(Why::TooFewRuns {
                runs: 0,
                needed: MIN_RUNS
            })
        );
        assert_eq!(
            adjudicate(&o(MIN_RUNS - 1, MIN_RUNS - 1), Some(0.0)),
            Adjudication::Pending(Why::TooFewRuns {
                runs: MIN_RUNS - 1,
                needed: MIN_RUNS
            }),
            "a perfect record over too few runs is still too few runs"
        );
    }

    /// No baseline is not a pass.
    ///
    /// An agent with no resolved runs in the window gives nothing to compare
    /// against. Treating a missing baseline as 0.0 would verify every rule of
    /// every silent agent.
    #[test]
    fn a_missing_baseline_is_not_a_zero_baseline() {
        assert_eq!(
            adjudicate(&o(20, 1), None),
            Adjudication::Pending(Why::NoBaseline)
        );
    }

    /// The comparison is against the agent, not against a fixed bar.
    ///
    /// The same rate is a rejection under a strong agent and a verification
    /// under a weak one, and that is the point: a fixed pass mark would encode
    /// one agent's difficulty as everyone's quality bar.
    ///
    /// # This test was vacuous first
    ///
    /// It used a 40% rule as 70% and baselines of 0.95 and 0.60. Replacing
    /// `rate >= baseline` with `rate >= 0.65` — the exact defect it claims to
    /// catch — left BOTH assertions passing, because 70% clears 0.65 anyway.
    /// `scripts/break_rule_verification.py` reported it green under its own
    /// mutation while a different test went red in its place.
    ///
    /// So the rate now sits BELOW any plausible fixed bar and the weak agent's
    /// baseline sits below the rate. Nothing but a comparison against the
    /// agent can return `Verified` here.
    #[test]
    fn the_same_rate_is_judged_against_the_agent_it_belongs_to() {
        let rule = o(20, 8); // 40% — under any fixed pass mark worth writing

        assert!(
            matches!(adjudicate(&rule, Some(0.95)), Adjudication::Rejected { .. }),
            "40% against a 95% agent is a rejection"
        );
        assert!(
            matches!(adjudicate(&rule, Some(0.35)), Adjudication::Verified { .. }),
            "40% against a 35% agent is doing better than the agent does, and \
             only a baseline comparison can say so"
        );
    }

    /// Rejection needs a margin; verification does not.
    ///
    /// Rejection retires a rule from every future prompt and deactivates it.
    /// Verification only grants ordering preference. The asymmetry is
    /// deliberate and this pins it: exactly at the baseline is a pass, and
    /// just below it is neither.
    #[test]
    fn rejection_requires_a_margin_and_verification_does_not() {
        assert!(matches!(
            adjudicate(&o(100, 90), Some(0.90)),
            Adjudication::Verified { .. }
        ));

        // One point below the baseline: worse, and nowhere near the margin.
        let near = adjudicate(&o(100, 89), Some(0.90));
        assert!(
            matches!(near, Adjudication::Pending(Why::Inconclusive { .. })),
            "a rule one point under its baseline must not be retired: {near:?}"
        );

        // Exactly at the margin rejects; a hair inside it does not.
        assert!(matches!(
            adjudicate(&o(100, 75), Some(0.90)),
            Adjudication::Rejected { .. }
        ));
        assert!(matches!(
            adjudicate(&o(100, 76), Some(0.90)),
            Adjudication::Pending(Why::Inconclusive { .. })
        ));
    }

    /// Every verdict carries the method and the caveat, including the ones that
    /// change nothing.
    ///
    /// The method string is the only thing standing between "verified" and a
    /// reader concluding the rule is true. If a verdict could be written
    /// without it, the column would be back to meaning whatever anyone assumed.
    #[test]
    fn every_verdict_names_its_method_and_its_limit() {
        for (out, base) in [
            (o(100, 95), Some(0.90)),
            (o(100, 50), Some(0.90)),
            (o(100, 89), Some(0.90)),
            (o(2, 2), Some(0.90)),
            (o(100, 95), None),
        ] {
            let v = adjudicate(&out, base);
            let d = v.details(&out);
            assert_eq!(d["method"], json!(METHOD), "verdict {v:?} lost its method");
            assert!(
                d["caveat"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("control arm"),
                "verdict {v:?} dropped the causation caveat"
            );
            assert_eq!(d["runs"], json!(out.runs));
        }
    }

    /// `rate()` distinguishes "nothing resolved" from "failed everything".
    #[test]
    fn an_unresolved_rule_has_no_rate_rather_than_a_zero_one() {
        assert_eq!(o(0, 0).rate(), None);
        assert_eq!(o(4, 0).rate(), Some(0.0));
    }
}
