//! Was the gate right? The one question no counter can answer.
//!
//! # Why this module exists, arithmetically
//!
//! [`crate::gate_trust`] counts what each gate approved, refused and could not
//! decide, and [`crate::gate_api`] turns those counts into a reading. That
//! machinery catches the Γ bug's signature exactly — `refuses_everything`: asked,
//! and approved nothing — and it is genuinely the shape that hid a coherence gate
//! rejecting 100% of agent-wide interventions.
//!
//! It cannot catch the next one. A gate that approves 90% of what it sees and
//! refuses the other 10% **wrongly** reads `discriminating`, which the surface
//! renders as the healthy state, and every counter in the system agrees with it.
//! There is no arrangement of approve/refuse totals that distinguishes a correct
//! refusal from an incorrect one, because correctness is not a property of a
//! count. It is a judgement about the subject, and only a reviewer holds it.
//!
//! So this is not a dashboard feature bolted onto a read-only surface. It is the
//! only path by which "is this gate refusing the right things" becomes answerable
//! at all, and until migration 216 the answer was structurally unavailable rather
//! than merely unmeasured.
//!
//! # It does not override anything, and that is deliberate
//!
//! `gate_api::GATE_DOORS` was empty, with a comment saying the emptiness was
//! itself the finding — and also that it was *not obviously wrong*, because a
//! gate a person can wave through is not much of a gate. Both halves survive
//! here. A review is a **judgement recorded after the fact**; nothing in this
//! module lets a decision be re-run, reversed, or retried. `Overturned` changes
//! no behaviour. What it does is make a wrong refusal visible to the person who
//! can change the code, which is the step that was missing.
//!
//! # One implementation of the rule, and it is Postgres's
//!
//! `overturned` requires a rationale. That rule lives in **exactly one place**:
//! `gate_decision_reviews_rationale_check`. This module deliberately does *not*
//! pre-validate it in Rust before the insert, and the temptation to is worth
//! naming, because a Rust guard would read as defensive good practice and be a
//! §3.4 violation: two implementations of one trust rule, drifting, with the
//! weaker one in front. The predictable end state is a Rust check that is
//! narrower than the constraint, an insert that fails anyway, and a 500 in the
//! one place a reviewer was told their finding had been recorded.
//!
//! Instead [`classify_write_error`] **translates the database's verdict**. The
//! constraint refuses; this names which constraint refused so the API can answer
//! 400 with the reason rather than 500 with a stack. Translation is not a second
//! implementation — it holds no opinion about when a rationale is required, only
//! about what `gate_decision_reviews_rationale_check` is called.
//!
//! # Append-only, and current state is derived
//!
//! Migration 205's reasoning, applied again: the latest row per `decision_id` is
//! the current verdict, computed rather than stored. Two reviewers disagreeing
//! about one refusal is the most informative row this table can hold, and a
//! mutable `verdict` column would erase the disagreement and the earlier
//! reviewer's name with it.

use crate::panel_absence::Reading;
use crate::seam_vocabulary::{ActorKind, GateReviewVerdict, UnknownToken};

// ── the queries ──────────────────────────────────────────────────────────

/// Record one review. `$1..$7` in declaration order.
///
/// The only INSERT into `gate_decision_reviews`. `gate` is denormalised from the
/// decision row the caller has already fetched rather than passed in by a
/// client — a client-supplied gate would let a review be filed against the wrong
/// gate's standing, which is the one field here whose corruption is silent.
pub const REVIEW_INSERT_SQL: &str = "INSERT INTO gate_decision_reviews \
                                       (decision_id, gate, verdict, rationale, \
                                        actor, actor_kind, evidence) \
                                     VALUES ($1, $2, $3, $4, $5, $6, $7) \
                                     RETURNING review_id, created_at";

/// The decision being reviewed, and its gate. `$1` is the decision id.
///
/// Read before the insert so the gate is the ledger's own and a decision that
/// does not exist is a 404 rather than a foreign-key 500.
pub const DECISION_LOOKUP_SQL: &str = "SELECT gate::text, decision::text, reason, subject \
                                         FROM gate_decisions WHERE id = $1";

/// How this gate's reviews break down by verdict. `$1` is the gate id.
///
/// Grouped rather than summed into three columns on purpose: a verdict token the
/// Rust side does not recognise comes back as its own row and
/// [`tally_from_counts`] refuses it, instead of being folded into a bucket. A
/// column whose CHECK is wider than the type that writes it is the drift
/// `seam_vocabulary` exists to catch, and this is where it would show up as data.
pub const STANDING_SQL: &str = "SELECT verdict::text, count(*)::bigint \
                                  FROM gate_decision_reviews \
                                 WHERE gate = $1 \
                                 GROUP BY verdict";

/// The current verdict on each of this gate's recently reviewed decisions.
///
/// `$1` is the gate id. `DISTINCT ON` over the append-only log is the derived
/// current state: newest row per `decision_id`, so a decision upheld and later
/// overturned reads as overturned without the earlier row having been touched.
///
/// **Overturned first**, for `gate_api::LEDGER_SQL`'s reason: a reader opening
/// this is asking what the platform got wrong, and an upheld stream is the wrong
/// thing to make them page through.
///
/// `$2` is [`GateReviewVerdict::Overturned`], **bound rather than spelled.** The
/// sibling query `gate_api::LEDGER_SQL` writes `(decision = 'refused')` inline
/// and is allowed to because `gate_trust` owns that vocabulary upstream; this one
/// is registry-owned, so the token is fenced and the fence caught it here. Which
/// is the right outcome twice over: binding the enum means the priority ordering
/// and the CHECK cannot disagree about how the word is spelled, and a rename of
/// the variant moves both.
pub const LATEST_REVIEWS_SQL: &str = "SELECT decision_id, verdict::text, rationale, actor, \
                                             actor_kind::text, created_at \
                                        FROM ( \
                                          SELECT DISTINCT ON (decision_id) \
                                                 decision_id, verdict, rationale, actor, \
                                                 actor_kind, created_at \
                                            FROM gate_decision_reviews \
                                           WHERE gate = $1 \
                                           ORDER BY decision_id, created_at DESC \
                                        ) latest \
                                       ORDER BY (verdict = $2) DESC, created_at DESC \
                                       LIMIT 200";

// ── the tally ────────────────────────────────────────────────────────────

/// How a gate's reviews break down.
///
/// The three counts are the state; there is no separate `reviewed` field and
/// [`ReviewTally::reviewed`] derives it. A stored total is a fourth number that
/// can disagree with the three it summarises, and `gate_api::tally` needs a whole
/// separate partition test for exactly that reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct ReviewTally {
    pub upheld: i64,
    pub overturned: i64,
    /// Reviewed, and the ledger did not say enough to judge it.
    pub unclear: i64,
}

impl ReviewTally {
    /// Derived, never stored. See the struct docs.
    pub fn reviewed(&self) -> i64 {
        self.upheld + self.overturned + self.unclear
    }
}

/// Fold `(verdict, count)` rows into a tally, refusing a token no variant spells.
///
/// The refusal is the point. Folding an unrecognised verdict into the nearest
/// bucket would make a CHECK that is wider than [`GateReviewVerdict`] invisible
/// in the one place it shows up as data, and a widened CHECK with no matching
/// variant is half of `severity = 'L1'` — the other half being a variant with no
/// matching CHECK. `seam_vocabulary` catches both against the schema; this
/// catches the first against the rows.
pub fn tally_from_counts(counts: &[(String, i64)]) -> Result<ReviewTally, UnknownToken> {
    let mut t = ReviewTally::default();
    for (token, n) in counts {
        match token.parse::<GateReviewVerdict>()? {
            GateReviewVerdict::Upheld => t.upheld += n,
            GateReviewVerdict::Overturned => t.overturned += n,
            GateReviewVerdict::Unclear => t.unclear += n,
        }
    }
    Ok(t)
}

// ── the standing ─────────────────────────────────────────────────────────

/// Where a gate stands with its reviewers.
///
/// Five states, because "nothing to review", "nothing reviewed" and "reviewed
/// and nothing judgeable" are three different observations and only one of them
/// says anything about the gate. Collapsing them into "0 overturned" would
/// render all three as a pass, which is the reading this whole surface exists to
/// refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "standing", rename_all = "snake_case")]
pub enum Standing {
    /// The gate's ledger is empty, so there is nothing to have reviewed.
    ///
    /// Says nothing about the gate. A `Retention::Counted` gate never writes a
    /// ledger row at all and will sit here permanently by design.
    NothingToReview,
    /// Decisions on file and none reviewed.
    ///
    /// **Not a pass.** This is where every gate starts, and it is the state the
    /// platform was in for its entire life: a full ledger nobody had read.
    Unreviewed { decisions: i64 },
    /// Reviewed, and **not one review could reach a verdict.**
    ///
    /// A finding about the ledger rather than about the gate:
    /// `gate_decisions.reason` is free text truncated at the writer, and if every
    /// reviewer said `unclear` then the record does not carry enough to review a
    /// decision from. Worth fixing at the writer, and invisible without this
    /// state.
    ///
    /// Deliberately the *strict* form — every review unclear, not most of them.
    /// A ratio here would need a cutoff, and a cutoff is the threshold-as-target
    /// this codebase does not allow. The counts ride along in every state so a
    /// surface can show the ratio without anyone having picked a number.
    Inconclusive { unclear: i64, reviewed: i64 },
    /// Reviewed, nothing overturned, at least one judged.
    Upheld { tally: ReviewTally },
    /// **At least one decision was wrong.** The finding.
    Overturned { tally: ReviewTally },
}

/// Classify one gate's standing.
///
/// `decisions` is the gate's ledger count, from `gate_api::LEDGER_COUNT_SQL`. It
/// is needed and not derivable: a gate with 400 decisions and no reviews and a
/// gate with no decisions at all both have an empty tally, and they are the two
/// states this function exists to keep apart.
pub fn standing(decisions: i64, tally: ReviewTally) -> Standing {
    if decisions <= 0 {
        return Standing::NothingToReview;
    }
    let reviewed = tally.reviewed();
    if reviewed == 0 {
        return Standing::Unreviewed { decisions };
    }
    // Worst first, as `gate_api::read` orders its own readings: an overturned
    // decision outranks an unreadable ledger, which outranks a clean sheet.
    if tally.overturned > 0 {
        return Standing::Overturned { tally };
    }
    if tally.unclear == reviewed {
        return Standing::Inconclusive {
            unclear: tally.unclear,
            reviewed,
        };
    }
    Standing::Upheld { tally }
}

/// The three-word reading for a standing, and the token beneath it.
///
/// Three readings and five tokens, for `gate_api::read`'s reason: several states
/// share `unknown` and mean different things, and a client that wants the
/// distinction should not have to reconstruct it from the counts.
///
/// # The tokens are not the verdicts, and they used to look like they were
///
/// `has_overturned` and `all_upheld` rather than `overturned` and `upheld`. A
/// standing is a statement about a **gate**; a verdict is a statement about one
/// **decision**, and they are different objects that happened to share a word.
/// The first version reused the verdict's spelling and the token fence in
/// `tests/seam_vocabulary_coverage.rs` flagged it as a re-spelled vocabulary
/// token — correctly, and for a better reason than the one it checks: a client
/// branching on `token == "overturned"` could not tell whether it had been handed
/// a gate's standing or a decision's verdict, and the two arrive on the same
/// surface.
pub fn reading(s: Standing) -> (Reading, &'static str) {
    match s {
        // A wrong refusal is a fault in the gate, full stop.
        Standing::Overturned { .. } => (Reading::Fault, "has_overturned"),
        // Reviewed and unreadable: a fault in the ledger, not in the gate, and
        // `unknown` rather than `fault` because nothing here says the gate did
        // anything wrong — only that nobody can tell.
        Standing::Inconclusive { .. } => (Reading::Unknown, "inconclusive"),
        // Not a pass. An unread ledger and a clean one look identical from the
        // counters, which is the whole reason this module exists.
        Standing::Unreviewed { .. } => (Reading::Unknown, "unreviewed"),
        Standing::NothingToReview => (Reading::Unknown, "nothing_to_review"),
        // The only state here that is a pass, and it is a narrow one: it means
        // the decisions *someone looked at* were right. See
        // `gate_api::GATE_CAVEATS`.
        Standing::Upheld { .. } => (Reading::Idle, "all_upheld"),
    }
}

// ── writing one ──────────────────────────────────────────────────────────

/// Why a review was not recorded.
///
/// An enum rather than a `String` for [`crate::claim_outcome::ClaimOutcome`]'s
/// reason: the caller has to turn these into different HTTP statuses, and a
/// message cannot be branched on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// No `gate_decisions` row with that id. A 404, not a 500.
    NoSuchDecision,
    /// `overturned` with no rationale.
    ///
    /// Postgres's judgement, translated. This module holds no opinion about when
    /// a rationale is required — see the module docs on why a Rust pre-check
    /// would be a §3.4 violation rather than defensive programming.
    RationaleRequired,
    /// A token the column's CHECK refuses.
    ///
    /// Should be unreachable through the typed path, and is kept because
    /// "unreachable" is what `severity = 'L1'` was: the write site held a
    /// vocabulary the column did not, and the rejection was swallowed for the
    /// life of the feature.
    UnknownToken { column: &'static str },
    /// Anything else the database said.
    Rejected { error: String },
}

/// Translate a Postgres error on the review insert into a [`Refusal`].
///
/// Takes the constraint name and the message rather than an `sqlx::Error`, so it
/// is a pure function the falsification registry can put worlds in front of.
/// `sqlx::Error` cannot be constructed with an arbitrary constraint name from
/// outside the crate, and a decision that can only be exercised against a live
/// database is a decision nothing checks.
///
/// The mapping is on the **constraint name**, not on the message text. Message
/// text is a Postgres locale-and-version artifact; a constraint name is
/// something migration 216 chose and a test can pin.
pub fn classify_write_error(constraint: Option<&str>, message: &str) -> Refusal {
    match constraint {
        Some("gate_decision_reviews_rationale_check") => Refusal::RationaleRequired,
        Some("gate_decision_reviews_verdict_check") => Refusal::UnknownToken { column: "verdict" },
        Some("gate_decision_reviews_gate_check") => Refusal::UnknownToken { column: "gate" },
        Some("gate_decision_reviews_actor_kind_check") => Refusal::UnknownToken {
            column: "actor_kind",
        },
        // The FK. Reachable when a decision is deleted between the lookup and
        // the insert, which is a race rather than a client error.
        Some("gate_decision_reviews_decision_id_fkey") => Refusal::NoSuchDecision,
        _ => Refusal::Rejected {
            error: message.to_string(),
        },
    }
}

/// Is this refusal the client's fault?
///
/// The HTTP question, answered once here rather than in the handler, so a new
/// variant cannot default to 500 by being forgotten in a `match`.
pub fn is_client_error(r: &Refusal) -> bool {
    match r {
        Refusal::NoSuchDecision | Refusal::RationaleRequired => true,
        // A token the column refuses is *our* bug: the typed path cannot produce
        // one, so if it happens the CHECK and the type have drifted and the
        // client did nothing wrong.
        Refusal::UnknownToken { .. } | Refusal::Rejected { .. } => false,
    }
}

/// The parsed body of a review request, with the vocabulary already closed.
///
/// The types are the validation. A handler that took `verdict: String` would have
/// to compare it against something, and whatever it compared it against would be
/// the second implementation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReviewRequest {
    pub verdict: GateReviewVerdict,
    #[serde(default)]
    pub rationale: Option<String>,
    #[serde(default)]
    pub actor_kind: Option<ActorKind>,
    #[serde(default)]
    pub evidence: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(upheld: i64, overturned: i64, unclear: i64) -> ReviewTally {
        ReviewTally {
            upheld,
            overturned,
            unclear,
        }
    }

    /// A full ledger nobody has read is not a pass.
    ///
    /// The state the platform was in for its entire life, and the one a
    /// "0 overturned" header would render green. `Unreviewed` carries the
    /// decision count so a surface can say *400 decisions, none reviewed*, which
    /// is a queue rather than a clean sheet.
    #[test]
    fn an_unread_ledger_and_a_clean_one_are_different_states() {
        assert_eq!(
            standing(400, tally(0, 0, 0)),
            Standing::Unreviewed { decisions: 400 }
        );
        assert_eq!(standing(0, tally(0, 0, 0)), Standing::NothingToReview);
        assert_eq!(
            reading(standing(400, tally(0, 0, 0))).0,
            Reading::Unknown,
            "an unreviewed ledger read as idle would be the over-reading this \
             whole surface refuses"
        );
    }

    /// One wrong refusal outranks any number of right ones.
    #[test]
    fn a_single_overturned_decision_is_a_fault() {
        let s = standing(50, tally(40, 1, 3));
        assert_eq!(
            s,
            Standing::Overturned {
                tally: tally(40, 1, 3)
            }
        );
        assert_eq!(reading(s), (Reading::Fault, "has_overturned"));
    }

    /// Every review `unclear` is a finding about the ledger, not the gate.
    ///
    /// And it is the *strict* form: one judgeable review is enough to leave this
    /// state, because the alternative is a ratio and a ratio needs a cutoff.
    #[test]
    fn a_ledger_nobody_can_judge_from_is_its_own_finding() {
        assert_eq!(
            standing(9, tally(0, 0, 9)),
            Standing::Inconclusive {
                unclear: 9,
                reviewed: 9
            }
        );
        assert_eq!(
            reading(standing(9, tally(0, 0, 9))),
            (Reading::Unknown, "inconclusive"),
            "`unclear` says nobody could tell, which is not the same as saying \
             the gate was wrong"
        );
        // One judged review and it is no longer inconclusive, however lopsided.
        assert_eq!(
            standing(9, tally(1, 0, 8)),
            Standing::Upheld {
                tally: tally(1, 0, 8)
            }
        );
    }

    /// The total is derived, so it cannot disagree with its parts.
    #[test]
    fn the_tally_partitions_by_construction() {
        assert_eq!(tally(3, 4, 5).reviewed(), 12);
        assert_eq!(ReviewTally::default().reviewed(), 0);
    }

    /// A verdict no variant spells is refused rather than bucketed.
    ///
    /// The failure this prevents: a CHECK widened without a variant added, which
    /// is half of the `severity = 'L1'` shape. Folding the unknown token into
    /// `unclear` would make the widening invisible in the only place it appears
    /// as data.
    #[test]
    fn an_undeclared_verdict_in_the_column_is_refused() {
        // Through `as_str`, not spelled. This test is about the fold, and a
        // literal here would be a second place the wire form is written down —
        // which is what the token fence in `tests/seam_vocabulary_coverage.rs`
        // exists to stop, including in tests.
        let ok = tally_from_counts(&[
            (GateReviewVerdict::Upheld.as_str().to_string(), 2),
            (GateReviewVerdict::Overturned.as_str().to_string(), 1),
        ]);
        assert_eq!(ok, Ok(tally(2, 1, 0)));

        let bad = tally_from_counts(&[
            (GateReviewVerdict::Upheld.as_str().to_string(), 2),
            ("needs_more_thought".into(), 1),
        ]);
        let err = bad.expect_err("an undeclared verdict was folded into a bucket");
        assert_eq!(err.got, "needs_more_thought");
        assert!(
            err.expected.contains(&"unclear"),
            "the refusal must say what would have been accepted: {err}"
        );
    }

    /// The rationale rule is Postgres's, and this only names it.
    ///
    /// Pinned on the constraint *name*, because that is what migration 216 chose
    /// and what a test can hold; message text is a locale-and-version artifact.
    /// If 216's constraint is ever renamed, this is what says the translation
    /// stopped working — otherwise a missing rationale would start arriving as a
    /// 500 with a Postgres string in it, at the moment a reviewer was told their
    /// finding had been filed.
    #[test]
    fn a_missing_rationale_is_translated_and_not_reinvented() {
        assert_eq!(
            classify_write_error(Some("gate_decision_reviews_rationale_check"), "..."),
            Refusal::RationaleRequired
        );
        assert!(is_client_error(&Refusal::RationaleRequired));

        // Anything unrecognised keeps the database's own words and is ours, not
        // the client's.
        let other = classify_write_error(None, "deadlock detected");
        assert_eq!(
            other,
            Refusal::Rejected {
                error: "deadlock detected".into()
            }
        );
        assert!(!is_client_error(&other));
    }

    /// A token the column refuses is our defect, not the caller's.
    #[test]
    fn a_rejected_token_is_a_five_hundred_because_the_typed_path_cannot_produce_one() {
        let r = classify_write_error(Some("gate_decision_reviews_verdict_check"), "...");
        assert_eq!(r, Refusal::UnknownToken { column: "verdict" });
        assert!(
            !is_client_error(&r),
            "a CHECK the typed path cannot violate means the CHECK and the type \
             have drifted, which is a platform bug"
        );
    }
}
