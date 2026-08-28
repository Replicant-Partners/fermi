//! Enqueuing a contracted field for verification.
//!
//! # The queue that had a schema and no writer
//!
//! `assertion_verifications` has existed since migration 205. It is keyed to both
//! the assertion and the episode, its `actor_kind` maps one-to-one onto
//! *pending-tool* versus *pending-human*, and it carries the CHECK that makes a
//! human verdict cost something — `human_sourced` requires a citation. The UX
//! team's own audit of it concluded: *"It needs a writer, not a schema."*
//!
//! It has held **0 rows** for its whole life. This is the writer.
//!
//! # Why the content comes from fields and not from prose
//!
//! The obvious source is `episodes.assertions[]`, and it cannot work. All 94
//! assertions in production are 75 `Multiplier` and 19 `Probability` — **zero**
//! `Quantity` — and [`crate::assertions::Assertion::route`] sends a
//! non-verifiable kind to [`crate::assertions::Route::InheritFromBasis`],
//! correctly, because *you cannot verify a multiplier.* No amount of improving
//! the prose extractor produces a queue item.
//!
//! A **contracted field** purports to be a retrieval, so it is checkable, and the
//! field contract already names the tool that could settle it. That is the whole
//! content of the routing decision and it is derived rather than declared a
//! second time: `Grounding::Sourced { tool }` present means a tool can answer,
//! absent means a person must.
//!
//! # The row is written by the platform, and says so
//!
//! `actor_kind = platform` and `actor = "grounding_contract"` on enqueue, with
//! `verdict = pending_tool_check | pending_human_check`. Not `tool` or `human`:
//! at enqueue time **nobody has acted**, and recording the intended actor as
//! though it had would make "queued for a person" and "checked by a person" the
//! same row. The routing lives in the verdict, where it belongs, and the actor
//! records who actually decided — which at this moment is the platform, with no
//! external check behind it, exactly as `ActorKind::Platform` is documented.
//!
//! A later verification appends its own row with its own actor. Current state is
//! the latest row per `assertion_id`, derived rather than stored, so a claim
//! queued and then settled reads as both events rather than only the last.
//!
//! # Non-fatal, and counted
//!
//! An execute must not fail because its verification queue could not be written.
//! But a lost enqueue is a claim nobody will ever check, and the table's whole
//! problem has been that its emptiness was unexplained — so every attempt goes
//! through [`crate::write_accounting`] and a failure is a number rather than a
//! silence.

use sqlx::PgPool;
use uuid::Uuid;

use crate::assertions::{Assertion, NotEnqueued, Route};
use crate::grounding_trust::GradedField;
use crate::seam_vocabulary::ActorKind;
use crate::write_accounting::{self, Sink};

/// Who the platform records as the author of an enqueue.
///
/// A constant rather than a literal at the write site: it is the value a reader
/// filters the queue by to find *"rows nobody has acted on yet"*, and a second
/// spelling would silently split that set.
pub const ENQUEUED_BY: &str = "grounding_contract";

/// One row per claim awaiting verification. `$1..$6` in declaration order.
///
/// `source_citation` is deliberately absent from the column list: migration 205's
/// CHECK only requires one for `human_sourced`, and a `pending_*` row has nothing
/// to cite yet. Writing an empty string to satisfy a constraint that does not
/// apply is how a citation requirement becomes decorative.
pub const ENQUEUE_SQL: &str = "INSERT INTO assertion_verifications \
                                 (assertion_id, episode_id, verdict, actor, actor_kind, evidence) \
                               VALUES ($1, $2, $3, $4, $5, $6) \
                               RETURNING verification_id";

/// What happened when a document's contracted fields were queued.
///
/// Every count is carried, including the ones that are not failures, because the
/// question this surface has never been able to answer is *why is the queue
/// empty* — and "nothing was checkable", "everything was already verified" and
/// "the writes were refused" are three different answers with three different
/// remedies.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Enqueued {
    /// Rows written. The queue grew by this much.
    pub queued: usize,
    /// Routed to a tool that the contract names.
    pub to_tool: usize,
    /// Routed to a person, because no tool can settle the field.
    ///
    /// Per the paper this is **not only** a work item: it is a prioritised
    /// request for the data integration that would close it, which is the same
    /// gap seen from the other side.
    pub to_human: usize,
    /// Already reproducible, so deliberately not queued.
    ///
    /// A queue that contains everything is not a queue.
    pub already_settled: usize,
    /// Not a checkable proposition; verification routes to its basis instead.
    pub inherits_from_basis: usize,
    /// Fields the queue could not represent, with the reason each was refused.
    pub not_representable: Vec<String>,
    /// Writes attempted and refused by the database.
    pub failed: usize,
}

impl Enqueued {
    /// Is there anything here a reader should act on?
    ///
    /// `not_representable` counts: a field the queue cannot carry is a hole in
    /// its coverage, and the whole reason it is returned rather than logged is
    /// that an empty queue for want of representable claims reads identically to
    /// an empty queue for want of problems.
    pub fn is_problem(&self) -> bool {
        self.failed > 0 || !self.not_representable.is_empty()
    }
}

/// Queue every contracted field of one episode that needs checking.
///
/// `fields` must come from [`crate::grounding_trust::graded_fields`] over the
/// document **as the agent produced it** — before `enforce` nulled anything —
/// because the claimed value is the evidence and a nulled field has none.
///
/// Returns rather than erroring, and never propagates: see the module docs on why
/// this write is allowed to be swallowed and what is paid for that.
pub async fn enqueue(
    pool: &PgPool,
    episode_id: Uuid,
    agent_id: &str,
    fields: &[GradedField],
) -> Enqueued {
    let mut out = Enqueued::default();
    let (assertions, skipped) = crate::assertions::from_graded_fields(agent_id, fields);
    out.not_representable = skipped
        .iter()
        .map(|NotEnqueued { path, why }| format!("{path}: {why}"))
        .collect();

    for a in &assertions {
        // The field this assertion came from, so the route can read the
        // contract's own answer about which tool could settle it. Matched on the
        // path because that is what `from_graded_field` recorded, and a
        // positional match would silently repoint if either list were reordered
        // — the same reason `Assertion::assertion_id` is minted rather than
        // derived from array position.
        let Some(field) = fields.iter().find(|f| Some(f.path) == field_path_of(a)) else {
            continue;
        };
        match a.route(field.settleable_by.is_some()) {
            Route::None => {
                out.already_settled += 1;
                continue;
            }
            Route::InheritFromBasis => {
                out.inherits_from_basis += 1;
                continue;
            }
            Route::Automated => out.to_tool += 1,
            Route::Human => out.to_human += 1,
        }
        let Some(verdict) = a.route(field.settleable_by.is_some()).pending_verdict() else {
            continue;
        };

        let written = sqlx::query_scalar::<_, Uuid>(ENQUEUE_SQL)
            .bind(a.assertion_id)
            .bind(episode_id)
            .bind(verdict)
            .bind(ENQUEUED_BY)
            .bind(ActorKind::Platform)
            .bind(serde_json::json!({
                "path": field.path,
                // The claim, verbatim. The reason the whole chain retains it:
                // it is the only evidence that could ever answer which model
                // fabricates what, and a null cannot be labelled.
                "claimed": field.value,
                "block_provenance": field.provenance,
                "settleable_by": field.settleable_by,
            }))
            .fetch_one(pool)
            .await;

        if write_accounting::observe(Sink::AssertionVerifications, written).is_some() {
            out.queued += 1;
        } else {
            out.failed += 1;
        }
    }
    out
}

// ── settling one ─────────────────────────────────────────────────────────

/// Record a verdict on a queued claim. `$1..$6` in declaration order.
///
/// **Appends; never updates.** Current state is the latest row per
/// `assertion_id`, derived rather than stored, so a claim queued and then settled
/// reads as both events and two reviewers who disagree read as a disagreement.
/// Migration 205's reasoning, and the reason a mutable `verdict` column would be
/// worse than useless: it would erase the earlier reviewer's name along with the
/// verdict.
///
/// `episode_id` is carried from the pending row rather than supplied by the
/// caller — a settle that could name a different episode would attribute the
/// verdict to the wrong artifact, and nothing downstream would notice.
pub const SETTLE_SQL: &str = "INSERT INTO assertion_verifications                                 (assertion_id, episode_id, verdict, source_citation,                                  actor, actor_kind, evidence)                               SELECT $1, v.episode_id, $2, $3, $4, $5, $6                                 FROM assertion_verifications v                                WHERE v.assertion_id = $1                                ORDER BY v.created_at DESC LIMIT 1                               RETURNING verification_id, episode_id";

/// Why a settle was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettleRefusal {
    /// No pending row for that assertion, so there is nothing to settle.
    ///
    /// A 404 rather than an insert: settling a claim nobody queued would put a
    /// verdict in the ledger with no record of what was asked.
    NotQueued,
    /// `human_sourced` with no citation. Postgres's judgement, translated.
    CitationRequired,
    /// A verdict the column refuses.
    UnknownVerdict,
    /// Anything else the database said.
    Rejected { error: String },
}

/// Is this refusal the caller's fault?
pub fn settle_is_client_error(r: &SettleRefusal) -> bool {
    match r {
        SettleRefusal::NotQueued | SettleRefusal::CitationRequired => true,
        // A verdict the typed path cannot produce means the ladder and the CHECK
        // have drifted, which is ours.
        SettleRefusal::UnknownVerdict | SettleRefusal::Rejected { .. } => false,
    }
}

/// Translate a Postgres error on the settle insert.
///
/// Pure, and on the constraint **name** rather than the message: message text is
/// a locale-and-version artifact, a constraint name is something migration 205
/// chose. Same reasoning as `gate_review::classify_write_error`, and the same
/// reason it is a translation rather than a Rust pre-check — the citation rule
/// has exactly one implementation and it is the CHECK.
pub fn classify_settle_error(constraint: Option<&str>, message: &str) -> SettleRefusal {
    match constraint {
        Some("assertion_verifications_citation_check") => SettleRefusal::CitationRequired,
        Some("assertion_verifications_verdict_check") => SettleRefusal::UnknownVerdict,
        _ => SettleRefusal::Rejected {
            error: message.to_string(),
        },
    }
}

/// Which verdicts may settle a queued claim.
///
/// A subset of `grounding_trust::PROVENANCE_VALUES`, and the exclusions are the
/// point. `pending_tool_check` and `pending_human_check` are what a claim is
/// queued *as*, so offering them as a settlement would let a reviewer "resolve"
/// an item by re-queueing it and the queue would never drain. The retrieval
/// verdicts (`tool_verified`, `derived`) are the platform's to write from an
/// actual tool call, not a person's to assert.
///
/// What is left is what a reviewer can honestly conclude: they followed a source
/// (`human_sourced`, which the CHECK requires a citation for), they formed a
/// judgement without one (`human_endorsed`, deliberately available at the
/// strength of a model inference — requiring a citation for every judgement
/// pushes reviewers to paste a plausible URL, which is worse than an admitted
/// opinion), or the claim is wrong (`rejected`).
pub const SETTLEABLE_BY_A_REVIEWER: &[&str] = &[
    crate::grounding_trust::PROV_HUMAN_SOURCED,
    crate::grounding_trust::PROV_HUMAN_ENDORSED,
    crate::grounding_trust::PROV_REJECTED,
];

/// May a reviewer write this verdict?
///
/// Checked before the insert, and this is **not** a second implementation of the
/// CHECK: the column accepts every ladder value including `pending_*` and
/// `tool_verified`, and this narrows that to what a *person* may assert. Two
/// different rules, and the narrower one has no home in the database because the
/// same column is written by the platform's own enqueue.
pub fn reviewer_may_write(verdict: &str) -> bool {
    SETTLEABLE_BY_A_REVIEWER.contains(&verdict)
}

/// The contracted path an assertion was minted from, if it was minted from one.
///
/// `ExtractionPath::TypedField` records it; `Prose` does not, and a prose
/// assertion has no contracted field behind it by construction.
fn field_path_of(a: &Assertion) -> Option<&str> {
    match &a.extraction {
        crate::assertions::ExtractionPath::TypedField { field_path, .. } => Some(field_path),
        crate::assertions::ExtractionPath::Prose { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty queue has three causes and they must not read the same.
    ///
    /// The table has held 0 rows since migration 205 and nothing could say why.
    /// `Enqueued` carries every count so the answer is a reading rather than a
    /// guess: nothing checkable, everything already settled, or writes refused.
    #[test]
    fn the_reasons_a_queue_stays_empty_are_kept_apart() {
        let nothing_checkable = Enqueued {
            inherits_from_basis: 9,
            ..Default::default()
        };
        let all_settled = Enqueued {
            already_settled: 9,
            ..Default::default()
        };
        let refused = Enqueued {
            failed: 9,
            ..Default::default()
        };
        assert_eq!(nothing_checkable.queued, 0);
        assert_eq!(all_settled.queued, 0);
        assert_eq!(refused.queued, 0);

        assert!(
            !nothing_checkable.is_problem(),
            "a document of unverifiable judgements is not a fault: you cannot \
             verify a multiplier, and saying so is the correct outcome"
        );
        assert!(
            !all_settled.is_problem(),
            "everything already reproducible is the best case, and a caller that \
             warned on it would fill the log on exactly the runs that went well"
        );
        assert!(
            refused.is_problem(),
            "a refused write is a claim nobody will ever check, and the table's \
             whole problem has been that its emptiness was unexplained"
        );
    }

    /// A field the queue cannot represent is a finding, not a skip.
    ///
    /// `taxonomy.order = "Coleoptera"` is the canonical case — the claim most
    /// worth verifying, and `Assertion::value` is a `Spread` so it cannot be
    /// carried. If that were merely skipped, the queue would be empty and look
    /// healthy while the checkable claims went unchecked.
    #[test]
    fn a_field_the_queue_cannot_carry_is_reported_as_a_problem() {
        let e = Enqueued {
            not_representable: vec!["taxonomy.order: not numeric".to_string()],
            ..Default::default()
        };
        assert!(e.is_problem());
        assert_eq!(e.failed, 0, "it is a coverage gap, not a write failure");
    }

    /// A prose assertion has no contracted field, and must not borrow one.
    ///
    /// The lookup is by path, and matching positionally would let a prose
    /// assertion pick up whichever field happened to sit at its index — silently
    /// giving it a settling tool it has no claim to, which would route a
    /// multiplier to a tool that cannot verify one.
    #[test]
    fn a_prose_assertion_names_no_contracted_field() {
        use crate::assertions::{AssertionKind, Claim, ExtractionPath, Spread};
        let prose = Assertion {
            assertion_id: Uuid::new_v4(),
            kind: AssertionKind::Multiplier,
            value: Claim::Numeric(Spread {
                p5: 1.0,
                p50: 1.1,
                p95: 1.2,
            }),
            basis: vec![],
            extraction: ExtractionPath::Prose {
                pattern: "multiplier_v2".into(),
            },
            target_hint: None,
            raw: None,
        };
        assert_eq!(field_path_of(&prose), None);

        let typed = Assertion {
            extraction: ExtractionPath::TypedField {
                schema: "contract:genome_profiler".into(),
                field_path: "genome.estimated_size_mb".into(),
            },
            ..prose
        };
        assert_eq!(field_path_of(&typed), Some("genome.estimated_size_mb"));
    }

    /// The enqueue names the platform, not the actor it is waiting for.
    ///
    /// At enqueue time nobody has acted. Recording `actor_kind = human` because a
    /// person is *expected* to act would make "queued for a person" and "checked
    /// by a person" the same row, and the queue could never be filtered down to
    /// what still needs doing.
    #[test]
    fn the_enqueue_records_who_wrote_it_and_not_who_should_act() {
        assert_eq!(ActorKind::Platform.as_str(), "platform");
        assert!(
            ENQUEUE_SQL.contains("actor_kind"),
            "the actor kind must be written; `reviewed` with no actor is how a \
             queue becomes a rubber stamp"
        );
        assert!(
            !ENQUEUE_SQL.contains("source_citation"),
            "a pending row has nothing to cite, and writing an empty string to \
             satisfy a constraint that does not apply is how migration 205's \
             citation requirement becomes decorative"
        );
    }

    /// A reviewer cannot settle a claim by re-queueing it.
    ///
    /// `pending_tool_check` and `pending_human_check` are what a claim is queued
    /// AS. Offering them as settlements would let an item be "resolved" into the
    /// state it is already in, and the queue would never drain while every item
    /// showed recent activity.
    #[test]
    fn the_pending_tier_is_not_a_settlement() {
        for v in [
            crate::grounding_trust::PROV_PENDING_TOOL,
            crate::grounding_trust::PROV_PENDING_HUMAN,
        ] {
            assert!(
                !reviewer_may_write(v),
                "`{v}` is what a claim is queued as; accepting it as a verdict \
                 lets a reviewer resolve an item by re-queueing it"
            );
        }
    }

    /// A person cannot assert a retrieval.
    ///
    /// `tool_verified` and `derived` mean *run the tool, or apply the transform,
    /// and you land on the same value*. They are the platform's to write from an
    /// actual call. A reviewer asserting one by hand puts strength 2 on the
    /// ladder with nothing reproducible behind it, which is exactly the laundering
    /// migration 205's citation CHECK exists to stop, one rung higher.
    #[test]
    fn a_reviewer_cannot_assert_a_retrieval() {
        for v in [
            crate::grounding_trust::PROV_TOOL,
            crate::grounding_trust::PROV_DERIVED,
        ] {
            assert!(
                !reviewer_may_write(v),
                "`{v}` is a reproducible claim and a person cannot make one by \
                 saying so"
            );
        }
        // And what a reviewer CAN honestly conclude is available.
        for v in SETTLEABLE_BY_A_REVIEWER {
            assert!(reviewer_may_write(v));
        }
        assert_eq!(SETTLEABLE_BY_A_REVIEWER.len(), 3);
    }

    /// The citation rule stays Postgres's, and this only names it.
    #[test]
    fn a_missing_citation_is_translated_and_not_reinvented() {
        assert_eq!(
            classify_settle_error(Some("assertion_verifications_citation_check"), "..."),
            SettleRefusal::CitationRequired
        );
        assert!(settle_is_client_error(&SettleRefusal::CitationRequired));

        let other = classify_settle_error(None, "deadlock detected");
        assert_eq!(
            other,
            SettleRefusal::Rejected {
                error: "deadlock detected".into()
            }
        );
        assert!(
            !settle_is_client_error(&other),
            "an unrecognised database error is ours, not the reviewer's"
        );
    }

    /// The settle carries the episode from the pending row, not from the caller.
    ///
    /// A caller-supplied episode could attribute a verdict to a different
    /// artifact, and nothing downstream would notice: the rejection rate would
    /// move on an agent that never made the claim.
    #[test]
    fn the_settle_takes_its_episode_from_the_row_it_settles() {
        assert!(
            SETTLE_SQL.contains("v.episode_id"),
            "the settle must read the episode from the pending row"
        );
        assert!(
            SETTLE_SQL.contains("ORDER BY v.created_at DESC"),
            "it must settle against the LATEST row for the assertion, or a \
             re-queued claim would be settled against a stale one"
        );
        assert!(
            SETTLE_SQL.trim_start().starts_with("INSERT"),
            "append-only: an UPDATE would erase the earlier verdict and the \
             earlier reviewer's name with it"
        );
        assert!(
            !SETTLE_SQL.contains("UPDATE"),
            "append-only, see migration 205"
        );
    }
}
