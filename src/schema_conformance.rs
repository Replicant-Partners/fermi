//! # Did the member's document conform, and is it getting better?
//!
//! ## The third consumer
//!
//! A schema verdict at the delegation hop has three audiences, and they need
//! different things:
//!
//! ```text
//!   coordinator   should I trust THIS document, right now?
//!                 -> envelope.validation, read per hop
//!   platform      is this gate ever refusing anything?
//!                 -> gate_trust counters, aggregate, catches inert contracts
//!   loops         is this member getting better or worse?
//!                 -> here
//! ```
//!
//! The first two landed already. Neither answers the third, and the reason is
//! structural rather than an oversight: `envelope.validation` is discarded when
//! the tool result is consumed, and `gate_trust`'s counters are process-local
//! `AtomicU64`s that reset on deploy and are not per-agent. Nothing accrues.
//!
//! ## Why `eval_signals` and not `gate_decisions`
//!
//! `gate_decisions` is the obvious home and the wrong one. Promoting
//! `Gate::OutputSchema` to `Retention::Recorded` would write **one row per
//! delegation hop**, which is what migration 214 explicitly declined for the
//! rate limiter: "one row per refused request turns a control into a load
//! generator". Worse, ~98% of those rows would be `undetermined`, because that
//! is how many curated cards declare no schema. A table that is mostly noise
//! gets ignored, and then the signal inside it does too.
//!
//! `eval_signals` already is what this needs: per agent, per episode, a score
//! in `[0,1]`, indexed on `(agent_id, dimension, created_at DESC)` for exactly
//! the trend question being asked.
//!
//! ## The rule that keeps this honest
//!
//! **An unverified document produces NO signal.**
//!
//! `score` is `NOT NULL CHECK (score >= 0 AND score <= 1)`, so an unverified
//! document has no honest value to write:
//!
//! ```text
//!   1.0   the `unverified means pass` defect, which is the whole thing this
//!         line of work exists to remove
//!   0.0   blames an agent for having no schema, which is a different failure
//!         with a different owner
//!   0.5   invents a measurement
//! ```
//!
//! So nothing is written. The absence is not a gap: `gate_trust` counts the
//! `undetermined` verdicts, so "how often could we not check" is answered
//! there, and this table answers "when we could check, how did it do". Two
//! surfaces, two questions, neither pretending to answer the other's.
//!
//! That split is also why the loop stage below counts rows rather than
//! averaging scores. A stage asks "is anything flowing"; the score is for
//! whoever reads the trend.

use sqlx::PgPool;
use uuid::Uuid;

/// `eval_signals.evaluator_name` for this check.
pub const EVALUATOR: &str = "schema_conformance";

/// `eval_signals.dimension`.
pub const DIMENSION: &str = "schema_conformance";

/// Deterministic and structural, so `PreFilter` rather than `Dimensional`: no
/// model ran, nothing was judged, and the answer does not vary between two
/// evaluations of the same document.
///
/// The typed variant rather than the string. `eval_signals.evaluator_tier`
/// carries a CHECK constraint registered in `seam_vocabulary`, and
/// `no_declared_token_is_re_spelled_as_a_bare_literal` refuses a literal here
/// — correctly: a literal is invisible to the seam contract, so a token the
/// column rejects would be refused at runtime, swallowed by the write path,
/// and surface as an empty table. This test caught exactly that in the first
/// version of this module.
pub const TIER: crate::seam_vocabulary::EvaluatorTier =
    crate::seam_vocabulary::EvaluatorTier::PreFilter;

/// The score for a validation status, or `None` when nothing was checked.
///
/// Separated from the write so the rule is testable without a database, which
/// matters because the rule is the part that can be got wrong.
pub fn score_for(validation_status: &str) -> Option<f64> {
    match validation_status {
        "valid" => Some(1.0),
        "invalid" => Some(0.0),
        // Every `unverified_*`, and anything unrecognised. A catch-all on
        // purpose: a new status must not silently acquire a score.
        _ => None,
    }
}

/// Record one delegation hop's schema verdict against the producing agent.
///
/// Never fails the caller. A missing trend row is worth a log line and not
/// worth failing a delegation that otherwise succeeded — the same stance
/// `record_delegated_episode` takes one call up.
pub async fn record(
    db: &PgPool,
    agent_db_id: Uuid,
    episode_id: Uuid,
    validation_status: &str,
    declared_type: Option<&str>,
) {
    let Some(score) = score_for(validation_status) else {
        // The common path, and deliberately silent at info level: 98 of 101
        // cards declare no schema, so a warning here would be noise that
        // trains people to filter the log.
        tracing::debug!(
            status = validation_status,
            "[schema_conformance] nothing was checked, so nothing is scored"
        );
        return;
    };

    let rationale = match validation_status {
        "valid" => format!(
            "Document conformed to its declared type {}.",
            declared_type.unwrap_or("(unnamed)")
        ),
        _ => format!(
            "Document contradicted its declared type {}. See the delegation \
             envelope's validation.violations for the paths.",
            declared_type.unwrap_or("(unnamed)")
        ),
    };

    let res = sqlx::query(
        "INSERT INTO eval_signals \
            (episode_id, agent_id, evaluator_name, evaluator_tier, \
             dimension, score, confidence, rationale) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(episode_id)
    .bind(agent_db_id)
    .bind(EVALUATOR)
    .bind(TIER)
    .bind(DIMENSION)
    .bind(score)
    // 1.0 without apology: a schema check either ran or it did not, and when
    // it ran the answer is not a matter of degree. Contrast an LLM judge,
    // where a confidence below 1 is the honest default.
    .bind(1.0_f64)
    .bind(rationale)
    .execute(db)
    .await;

    // Counted, not logged.
    //
    // The first version of this was `if let Err(e) = res { tracing::warn!() }`,
    // and `write_accounting_coverage` refused it as an uninstrumented
    // swallowed write. It was right, and the irony is the point: this module
    // exists because a signal nobody counts does not exist, and it was
    // swallowing its own write failure into a log line nobody reads. A failed
    // insert here makes `loop4.conformed` under-report, which reads as "few
    // documents were checked" rather than as "the writer is broken" — the
    // same indistinguishability one layer down.
    crate::write_accounting::observe(crate::write_accounting::Sink::EvalSignals, res);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conforming_document_scores_one_and_a_contradiction_scores_zero() {
        assert_eq!(score_for("valid"), Some(1.0));
        assert_eq!(score_for("invalid"), Some(0.0));
    }

    /// **The load-bearing rule.** Nothing unverified gets a score.
    ///
    /// Each of the three wrong answers fails differently and all three are
    /// tempting, so the absence is asserted rather than assumed.
    #[test]
    fn an_unverified_document_is_not_scored_at_all() {
        for status in [
            "unverified_no_schema",
            "unverified_no_payload",
            "unverified_unsupported_schema",
        ] {
            assert_eq!(
                score_for(status),
                None,
                "`{status}` was given a score. 1.0 is the `unverified means \
                 pass` defect, 0.0 blames an agent for having no schema, and \
                 0.5 invents a measurement."
            );
        }
    }

    /// A status nobody anticipated must not acquire a score by falling through
    /// a match arm. `unverified_no_schema` is the majority case today, so the
    /// permissive reading is also the frequent one.
    #[test]
    fn an_unrecognised_status_is_never_scored() {
        for weird in ["", "ok", "VALID", "unverified_something_new", "skipped"] {
            assert_eq!(score_for(weird), None, "`{weird}` acquired a score");
        }
    }

    #[test]
    fn the_evaluator_vocabulary_is_stable() {
        assert_eq!(EVALUATOR, "schema_conformance");
        assert_eq!(DIMENSION, "schema_conformance");
            assert_eq!(
            TIER,
            crate::seam_vocabulary::EvaluatorTier::PreFilter,
            "a schema check is deterministic and structural: nothing was \
             judged, so it is not a `Dimensional` score"
        );
    }

    /// `EVALUATOR` is written into three places: the INSERT above, the loop
    /// probe in `loop_model.rs`, and the per-agent probe in `loop_api.rs`.
    /// Three copies of one string is the drift this repo keeps finding, and
    /// the failure mode is quiet — rename the constant and the loop stage
    /// silently reads zero for ever, which is indistinguishable from "nothing
    /// has been checked yet".
    #[test]
    fn the_loop_probes_query_the_evaluator_this_module_writes() {
        let stage = crate::loop_model::LOOPS
            .iter()
            .find(|l| l.id == "loop4")
            .expect("loop4")
            .stages
            .iter()
            .find(|s| s.id == "conformed")
            .expect("loop4.conformed exists");

        assert!(
            stage.sink_sql.contains(EVALUATOR),
            "the loop4 probe does not query `{EVALUATOR}`, so it counts rows \
             this module never writes: {}",
            stage.sink_sql
        );

        let (_, _, scope) = crate::loop_api::SUBJECT_SCOPES
            .iter()
            .find(|(l, s, _)| *l == "loop4" && *s == "conformed")
            .expect("loop4.conformed declares a subject scope");

        match scope {
            crate::loop_api::SubjectScope::PerAgent { sql } => {
                assert!(
                    sql.contains(EVALUATOR),
                    "the per-agent probe does not query `{EVALUATOR}`: {sql}"
                );
                assert!(
                    sql.contains("agent_id = $1"),
                    "the whole reason this lives in `eval_signals` rather than \
                     `gate_decisions` is that it has an agent dimension"
                );
            }
            crate::loop_api::SubjectScope::Platform { .. } => panic!(
                "loop4.conformed is declared Platform-scoped, which throws away \
                 the per-agent answer that is the entire point of the stage"
            ),
        }
    }

    /// The stage names the gate that decides it, which is what connects this
    /// to `loop_model::diagnose`: a gate refusing everything becomes a named
    /// stall reason (`gate_refuses_everything`) instead of an unexplained zero
    /// three stages downstream.
    #[test]
    fn the_stage_names_the_gate_that_decides_it() {
        let stage = crate::loop_model::LOOPS
            .iter()
            .find(|l| l.id == "loop4")
            .unwrap()
            .stages
            .iter()
            .find(|s| s.id == "conformed")
            .unwrap();
        assert_eq!(
            stage.gated_by,
            Some(crate::gate_trust::Gate::OutputSchema),
            "without this the loop cannot say WHY it stalled here"
        );
    }

    /// It must be the FIRST stage of loop 4. The loop's claim is that
    /// composition changes in response to measured per-agent contribution, and
    /// whether a member's document was well-formed is upstream of whether its
    /// claim should have been retained at all. Ordering is load-bearing:
    /// `evaluate` walks stages in order and stops at the first empty one, so
    /// the position decides which stage gets blamed for a stall.
    #[test]
    fn conformance_comes_before_the_claim_it_qualifies() {
        let stages = crate::loop_model::LOOPS
            .iter()
            .find(|l| l.id == "loop4")
            .unwrap()
            .stages;
        let pos = |id: &str| stages.iter().position(|s| s.id == id);
        assert!(
            pos("conformed") < pos("claims"),
            "a malformed document's claim should not be counted as a\
             contribution before anyone has asked whether it was well-formed"
        );
    }
}
