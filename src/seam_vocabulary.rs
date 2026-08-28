//! Every closed token set a column will accept, declared once and checked both
//! ways against the live schema.
//!
//! # The seam
//!
//! A **seam** is a boundary where two sides each hold an opinion about the same
//! vocabulary. Postgres holds one in a `CHECK` constraint; Rust holds one in a
//! string literal at the write site. Each is independently correct and nothing
//! compares them, so they drift, and the drift is silent in both directions:
//!
//! | direction | symptom | instance |
//! |---|---|---|
//! | Rust has a token the schema rejects | the write is refused, in a spawned task, with the error logged | `severity = "L1"` against `('info','warning','critical')` — Loop 2's seed, rejected for the life of the feature |
//! | the schema has a token Rust never writes | a feature was widened for a producer nobody finished | migration 200 added `'grounding'` to `anomaly_events.kind` and no `AnomalyKind` variant was ever added |
//!
//! Neither is visible by reading either side. Both are visible in one query.
//!
//! # This is an index, not a copy
//!
//! Where a declaration already exists it is **referenced**, not restated:
//! `anomaly_events.kind` points at [`crate::anomaly_vocabulary::KINDS`], and
//! both `semantic_rules.provenance_floor` and
//! `assertion_verifications.verdict` point at the same
//! [`crate::grounding_trust::PROVENANCE_VALUES`] — which is the useful part,
//! because it makes visible that two columns share one ladder.
//!
//! Restating them here would be the §3.4 violation this module exists to
//! prevent: *a trust calculation must have exactly one implementation, and the
//! layer that owns the vocabulary must own the arithmetic.* A registry that
//! copies its entries is a second answer to the same question.
//!
//! The sets declared here for the first time are the ones that had no Rust
//! declaration at all — they were bare string literals at the write site, which
//! is the `L1` setup exactly.
//!
//! # Four checks, and the third is the one with no substitute
//!
//! `tests/seam_vocabulary_contract.rs`:
//!
//! 1. every declared token is accepted by the constraint;
//! 2. every token the constraint accepts is declared;
//! 3. **every value actually present in the column is declared;**
//! 4. every typed variant binds and reads back as itself.
//!
//! The third catches what the other two cannot: a value written before the
//! `CHECK` existed, and — more importantly — drift on a column that has *no*
//! constraint, where the data is the only authority there is.
//!
//! The fourth is the only one that *binds* anything. The first three compare
//! declarations; none of them would notice that the types below cannot reach
//! the server at all.
//!
//! # A constant fixes the spelling; a type closes the slot
//!
//! The first pass at this module replaced the bare literals with `pub const`
//! strings. That removes the typo and leaves the hole. `sqlx::query(..).bind(x)`
//! is untyped — it takes anything that encodes — so nothing stopped a *second
//! severity scheme* being invented at the writer, which is precisely what
//! `severity = "L1"` was, and no constant would have prevented it.
//!
//! The four vocabularies this registry **owns** — the ones that had no Rust
//! owner at all before this work — are therefore types, not constants:
//! [`DeltaDirection`], [`ResolutionMode`], [`EvaluatorTier`], [`ActorKind`].
//! The value that reaches the bind can only be a variant. Inventing a new token
//! now means adding a variant here, in the file the contract test reads, rather
//! than typing it at a write site the contract test cannot see.
//!
//! The remaining entries have upstream owners and stay as references; a type
//! defined here for a vocabulary owned elsewhere would be the copy this module
//! exists to prevent.

use crate::anomaly_vocabulary;
use crate::gate_trust;
use crate::grounding_trust;

// ── Sets declared here for the first time ───────────────────────────────
//
// Each of these was a bare string literal at the write site with no Rust
// declaration anywhere, which is the shape the `L1` defect had.

/// A token no variant of a closed vocabulary spells.
///
/// Carries what would have been accepted, because the only useful thing to say
/// about a rejected token is what to write instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownToken {
    /// The type that refused it.
    pub vocabulary: &'static str,
    /// What was offered.
    pub got: String,
    /// What it would have taken.
    pub expected: &'static [&'static str],
}

impl std::fmt::Display for UnknownToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` is not a {}; the column accepts {:?}",
            self.got, self.vocabulary, self.expected
        )
    }
}

impl std::error::Error for UnknownToken {}

/// Declare a closed vocabulary once: the type, and the array the registry
/// indexes.
///
/// # Why the array is generated
///
/// An array maintained *next to* a type is the same drift one layer down — the
/// exact failure this module is about, moved from the Rust/Postgres seam to a
/// Rust/Rust one, where nothing at all would compare the two. So there is one
/// list of variants and everything comes off it: the enum, `ALL`, `as_str`,
/// `FromStr`, `Display`, the sqlx encoding, and the `&[&str]` the `VOCABULARIES`
/// table points at. There is no second place to edit.
///
/// Not exported. A vocabulary declared outside this file would be invisible to
/// `VOCABULARIES`, and an unregistered vocabulary is what the whole module is
/// for.
///
/// # Why the tokens are spelled and not derived from the variant names
///
/// `rename_all = "snake_case"` would produce the right string for all four of
/// these today, and would silently produce a different one the first time a
/// variant is named something whose snake case is not the token — a rename in
/// Rust becoming a schema change nobody wrote a migration for. So the token is
/// written out, once, and both `as_str` and `#[sqlx(rename)]` are generated
/// from it: they cannot disagree because there is only one of them.
///
/// What is *not* guaranteed by construction is that sqlx honours the attribute
/// at all — drop it, misspell it, or have a future version change its meaning,
/// and the derive falls back to the **variant name**, so `AnyReading` goes on
/// the wire and the column refuses it. That is what
/// `the_wire_form_is_the_declared_token` checks, by encoding each variant and
/// reading the bytes back.
macro_rules! closed_vocabulary {
    (
        $(#[$enum_doc:meta])*
        enum $name:ident;
        $(#[$array_doc:meta])*
        const $array:ident;
        $(
            $(#[$variant_doc:meta])*
            $variant:ident => $token:literal,
        )+
    ) => {
        $(#[$enum_doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            sqlx::Type, serde::Serialize, serde::Deserialize,
        )]
        #[sqlx(type_name = "text")]
        pub enum $name {
            $(
                $(#[$variant_doc])*
                #[sqlx(rename = $token)]
                #[serde(rename = $token)]
                $variant,
            )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The wire form — the string the column stores.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $token),+
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = UnknownToken;
            fn from_str(s: &str) -> ::std::result::Result<$name, UnknownToken> {
                match s {
                    $($token => Ok($name::$variant),)+
                    _ => Err(UnknownToken {
                        vocabulary: stringify!($name),
                        got: s.to_string(),
                        expected: $array,
                    }),
                }
            }
        }

        $(#[$array_doc])*
        pub const $array: &[&str] = &[$($name::$variant.as_str()),+];
    };
}

closed_vocabulary! {
    /// `process_spacetime.delta_direction` — which side of the reading the
    /// model fell on.
    ///
    /// Named variants rather than positional access. An earlier pass wrote
    /// `DELTA_DIRECTION[2] // exact`, which reintroduces the whole problem in
    /// miniature: reorder the array and every call site silently changes
    /// meaning, with the comment still claiming otherwise. A variant cannot be
    /// reordered into a different meaning.
    enum DeltaDirection;
    /// The wire forms of [`DeltaDirection`], generated from it.
    const DELTA_DIRECTION;
    /// The model read high.
    Over => "over",
    /// The model read low.
    Under => "under",
    /// Within float tolerance.
    Exact => "exact",
}

closed_vocabulary! {
    /// `process_spacetime.resolution_mode` — why this measurement was scored.
    enum ResolutionMode;
    /// The wire forms of [`ResolutionMode`], generated from it.
    const RESOLUTION_MODE;
    /// Every real reading that matches a prediction.
    AnyReading => "any_reading",
    /// A reading at a configured sample interval.
    SamplePoint => "sample_point",
    /// A reading whose relative error breached the threshold.
    AnomalyDelta => "anomaly_delta",
}

closed_vocabulary! {
    /// `eval_signals.evaluator_tier` — how the score was arrived at.
    ///
    /// Was a bare `'dimensional'` inside three separate SQL string literals, in
    /// three files, none of which references the others.
    enum EvaluatorTier;
    /// The wire forms of [`EvaluatorTier`], generated from it.
    const EVALUATOR_TIER;
    /// A cheap deterministic screen.
    PreFilter => "pre_filter",
    /// A scored dimension.
    Dimensional => "dimensional",
}

closed_vocabulary! {
    /// `assertion_verifications.actor_kind` — who settled the assertion.
    ///
    /// Not the same vocabulary as the activity feed's `actor_kind`
    /// (`user` / `agent` / `system`, `handlers::collab`), which names a
    /// different column on a different table. Two columns with one name and two
    /// vocabularies is why this registry is keyed on `(table, column)` rather
    /// than on the column name.
    enum ActorKind;
    /// The wire forms of [`ActorKind`], generated from it.
    const ACTOR_KIND;
    /// A tool call or automated checker.
    Tool => "tool",
    /// A person, who must cite evidence — see migration 205's CHECK.
    Human => "human",
    /// The platform itself, with no external check behind it.
    Platform => "platform",
}

closed_vocabulary! {
    /// `gate_decision_reviews.verdict` — was the gate right?
    ///
    /// The only vocabulary in the platform that can distinguish a **correct**
    /// refusal from an incorrect one. `gate_trust`'s readings are computed from
    /// approve/refuse counts, and `refuses_everything` catches only the extreme:
    /// asked, and approved nothing. A gate that approves 90% and refuses the
    /// other 10% wrongly reads `discriminating`, which the surface renders as
    /// healthy. No counter can tell those apart, because correctness is a
    /// judgement about the subject rather than a property of the count.
    enum GateReviewVerdict;
    /// The wire forms of [`GateReviewVerdict`], generated from it.
    const GATE_REVIEW_VERDICT;
    /// The gate was right.
    ///
    /// Requires no rationale, and the asymmetry is load-bearing rather than
    /// lenient: making the cheap confirmation as expensive as the finding means
    /// nobody reviews the routine decisions, and then the denominator is
    /// unknown. "3 overturned" and "3 overturned of 400 reviewed" are different
    /// findings and only the second is actionable.
    Upheld => "upheld",
    /// The gate was wrong. **Requires a rationale**, enforced by
    /// `gate_decision_reviews_rationale_check` in migration 216.
    ///
    /// This is the row that says the platform was wrong and should cause an
    /// engineering change. An uncited overturn is a complaint; the citation is
    /// what makes it followable, which is migration 205's argument for the
    /// `human_sourced` citation CHECK, applied to the verdict that carries the
    /// same weight.
    Overturned => "overturned",
    /// The record does not contain enough to judge the decision from.
    ///
    /// A first-class verdict, not a missing one. `gate_decisions.reason` is free
    /// text truncated at the writer, and forcing a reviewer to pick upheld or
    /// overturned when it says too little manufactures agreement. *The ledger
    /// does not record enough to review its own decisions* is a finding about
    /// the ledger, reported by `gate_review::Standing::Inconclusive`, and it
    /// would be invisible if this token did not exist.
    Unclear => "unclear",
}

/// Expand `$body!(Type, ARRAY)` once for every vocabulary this registry owns.
///
/// # Why this exists rather than five lists
///
/// The four checks below each held their own hand-written list of the types:
/// `the_wire_form_is_the_declared_token`,
/// `every_owned_vocabulary_binds_as_text`,
/// `every_declared_token_parses_back_to_its_variant`, and
/// `every_registry_owned_vocabulary_is_generated_from_a_type`. Adding a fifth
/// vocabulary meant editing four lists, and the failure mode of a missed edit is
/// the worst available: the new type is simply **not checked**, every list still
/// agrees with itself, and the suite is green. That is this module's own subject
/// matter — two sides holding an opinion about one vocabulary with nothing
/// comparing them — relocated from the Rust/Postgres seam to a Rust/Rust one.
///
/// It was found by adding the fifth. Three of the four lists would have silently
/// skipped [`GateReviewVerdict`]; the fourth failed, and only because it happened
/// to assert a count.
///
/// A macro rather than an array because two of the four checks need the *type*
/// and not a value: `<T as Type<Postgres>>::type_info()` and `wire_form::<T>` are
/// resolved at compile time, so a `Vec` of anything cannot carry them.
macro_rules! for_each_owned_vocabulary {
    ($body:ident) => {
        $body!(DeltaDirection, DELTA_DIRECTION);
        $body!(ResolutionMode, RESOLUTION_MODE);
        $body!(EvaluatorTier, EVALUATOR_TIER);
        $body!(ActorKind, ACTOR_KIND);
        $body!(GateReviewVerdict, GATE_REVIEW_VERDICT);
    };
}

/// `episodes.provenance` — how the episode came to be believed.
///
/// The authority is `agent_bestiary_memory::Provenance`; this array is the
/// wire form, and `every_episode_provenance_round_trips` holds the two
/// together by parsing each token back into the enum. A `Display`
/// implementation cannot be enumerated at compile time, so the alternative was
/// no declaration at all.
pub const EPISODE_PROVENANCE: &[&str] = &[
    "auto_pass",
    "auto_fail",
    "human_approved",
    "human_relabeled",
    "human_corrected",
    "synthetic_correction",
    "coordinator_observation",
];

/// `episodes.cost_basis` — how confidently the run's cost is known.
pub const COST_BASIS: &[&str] = &[
    "measured_split",
    "assumed_split",
    "unknown_model",
    "no_charge",
];

/// One column's closed vocabulary.
#[derive(Debug, Clone, Copy)]
pub struct Vocabulary {
    pub table: &'static str,
    pub column: &'static str,
    /// The Rust side. A reference to the owning declaration wherever one exists.
    pub tokens: &'static [&'static str],
    /// The `CHECK` constraint, or `None` when the column has no constraint and
    /// the data is the only authority.
    pub constraint: Option<&'static str>,
    /// Which module owns the tokens, when it is not this registry.
    ///
    /// `None` means the set is declared here because it had no Rust owner at
    /// all — it was bare literals at the write site, which is the `L1` setup.
    /// Those tokens are fenced: spelling one outside this module fails the
    /// build.
    ///
    /// `Some(path)` means an upstream module is the authority and this registry
    /// only indexes it. Its tokens are **not** fenced, because the owner and
    /// its legitimate users spell them constantly and a scan that flags those
    /// fires on correct behaviour — §5.2, and a check that cries wolf gets
    /// deleted, with the deletion looking like cleanup.
    pub owned_by: Option<&'static str>,
    /// Who writes this column. Named so a drift report points at a file.
    pub producers: &'static str,
    /// What goes wrong when the two sides disagree. Specific to this column,
    /// not a restatement of the module docs.
    pub why: &'static str,
}

/// Every closed vocabulary at a Rust/Postgres seam.
///
/// Rule for adding one: **if a column has a `CHECK ... IN (...)` and a Rust
/// site writes it, it belongs here.**
pub const VOCABULARIES: &[Vocabulary] = &[
    Vocabulary {
        table: "gate_decisions",
        column: "decision",
        tokens: gate_trust::DECISIONS,
        constraint: Some("gate_decisions_decision_check"),
        owned_by: Some("src/gate_trust.rs"),
        producers: "gate_trust::flush, draining the queue filled by gate_trust::decided",
        why: "The record of what the platform refused, which it did not have at \
              all before migration 214. A token the column rejects means the \
              refusal is not written — and the writer is non-fatal by design, so \
              the loss would be counted as a write failure and nothing else. \
              `undetermined` is the token most likely to be dropped by a second \
              implementation, because two-state thinking about gates is the \
              default.",
    },
    Vocabulary {
        table: "gate_decisions",
        column: "gate",
        tokens: gate_trust::GATE_IDS,
        constraint: Some("gate_decisions_gate_check"),
        owned_by: Some("src/gate_trust.rs"),
        producers: "gate_trust::flush",
        why: "Adding a gate to `gate_trust::GATES` without widening this \
              constraint makes every decision by the new gate unwritable, in a \
              batch insert whose error is swallowed by design. \
              `gate_ids_match_the_declared_gates` pins the Rust side against \
              GATES; this pins it against Postgres.",
    },
    Vocabulary {
        table: "gate_decision_reviews",
        column: "verdict",
        tokens: GATE_REVIEW_VERDICT,
        constraint: Some("gate_decision_reviews_verdict_check"),
        owned_by: None,
        producers: "gate_review::record, from handlers::loops::review_gate_decision_handler",
        why: "The only judgement in the platform that can call a gate wrong. A \
              token the column rejects means the reviewer pressed the button, \
              saw a success, and the finding was never written — and the finding \
              is the whole point: `gate_trust` counts decisions and cannot tell \
              a correct refusal from an incorrect one. `unclear` is the token \
              most likely to be dropped by a second implementation, for the same \
              reason `undetermined` is on `gate_decisions.decision`: two-state \
              thinking about a judgement is the default, and it turns `the \
              ledger does not say enough` into a fabricated verdict.",
    },
    Vocabulary {
        table: "gate_decision_reviews",
        column: "gate",
        tokens: gate_trust::GATE_IDS,
        constraint: Some("gate_decision_reviews_gate_check"),
        owned_by: Some("src/gate_trust.rs"),
        producers: "gate_review::record, denormalised from the reviewed gate_decisions row",
        why: "Denormalised from `gate_decisions.gate` so the per-gate standing \
              is one index scan rather than a join. Two CHECKs over one \
              vocabulary is exactly the drift this registry is for: widening \
              `gate_trust::GATES` and migration 214's constraint while leaving \
              216's alone makes the new gate's decisions recordable and its \
              reviews unwritable, which is the worse half — the decision is \
              logged, the reviewer is told nothing, and the gate reads \
              unreviewed forever.",
    },
    Vocabulary {
        table: "gate_decision_reviews",
        column: "actor_kind",
        tokens: ACTOR_KIND,
        constraint: Some("gate_decision_reviews_actor_kind_check"),
        owned_by: None,
        producers: "gate_review::record",
        why: "The same three-token set as `assertion_verifications.actor_kind` \
              and the same reason for existing: `reviewed` with no actor kind is \
              how a queue becomes a rubber stamp. Sharing the type rather than \
              the spelling is what stops the second table inventing a fourth \
              actor — which is how `severity = 'L1'` happened, one table over.",
    },
    Vocabulary {
        table: "anomaly_events",
        column: "kind",
        tokens: anomaly_vocabulary::KINDS,
        constraint: Some("anomaly_events_kind_check"),
        owned_by: Some("src/anomaly_vocabulary.rs"),
        producers: "handlers::execution (grounding), observability::AnomalyDetector::persist",
        why: "Loop 2's only input. Migration 200 widened this for `grounding` and \
              the detector enum was never given the variant, so for two hundred \
              migrations the only kind actually written was the one no enum could \
              express.",
    },
    Vocabulary {
        table: "anomaly_events",
        column: "severity",
        tokens: anomaly_vocabulary::SEVERITIES,
        constraint: Some("anomaly_events_severity_check"),
        owned_by: Some("src/anomaly_vocabulary.rs"),
        producers: "handlers::execution, observability::AnomalyDetector::persist",
        why: "The column the `L1` defect was written against. A second severity \
              scheme invented at the writer, rejected by the constraint, and \
              swallowed by a spawned task — the whole audit's emblem.",
    },
    Vocabulary {
        table: "semantic_rules",
        column: "provenance_floor",
        tokens: grounding_trust::PROVENANCE_VALUES,
        constraint: Some("semantic_rules_provenance_floor_check"),
        owned_by: Some("src/grounding_trust.rs"),
        producers: "provenance_oracle, consolidation (extraction floor)",
        why: "The provenance ladder as stored on a distilled rule. A token the \
              column rejects means the rule is written ungraded, and an ungraded \
              rule is retrieved and injected into a prompt exactly like a graded \
              one.",
    },
    Vocabulary {
        table: "assertion_verifications",
        column: "verdict",
        tokens: grounding_trust::PROVENANCE_VALUES,
        constraint: Some("assertion_verifications_verdict_check"),
        owned_by: Some("src/grounding_trust.rs"),
        producers: "the verification queue (automated route + owner review)",
        why: "The same ladder as `semantic_rules.provenance_floor`, and \
              registering both against one array is the point: two columns \
              sharing a vocabulary is exactly where a copy would be made and \
              would drift.",
    },
    Vocabulary {
        table: "assertion_verifications",
        column: "actor_kind",
        tokens: ACTOR_KIND,
        constraint: Some("assertion_verifications_actor_kind_check"),
        owned_by: None,
        producers: "the verification queue",
        why: "Distinguishes a tool check from a person's. `human_sourced` scores \
              level with `tool_verified`, so which actor settled an assertion is \
              load-bearing for how much the verdict is worth.",
    },
    Vocabulary {
        table: "process_spacetime",
        column: "delta_direction",
        tokens: DELTA_DIRECTION,
        constraint: Some("process_spacetime_delta_direction_check"),
        owned_by: None,
        producers: "handlers::simops_benchmark::resolve_against_projection",
        why: "Loop 5.A (projection accuracy) resolution rows. This site had \
              no Rust declaration and no logging on failure of any kind — \
              the `L1` setup with the alarm removed as well.",
    },
    Vocabulary {
        table: "process_spacetime",
        column: "resolution_mode",
        tokens: RESOLUTION_MODE,
        constraint: Some("process_spacetime_resolution_mode_check"),
        owned_by: None,
        producers: "handlers::simops_benchmark::resolve_against_projection",
        why: "As above. Three modes chosen per reading, all three written as bare \
              literals a few lines apart.",
    },
    Vocabulary {
        table: "eval_signals",
        column: "evaluator_tier",
        tokens: EVALUATOR_TIER,
        constraint: Some("eval_signals_evaluator_tier_check"),
        owned_by: None,
        producers: "handlers::eval, handlers::consolidation, handlers::forecasts",
        why: "Was a bare `'dimensional'` inside three separate SQL string \
              literals, in three files, none of which references the others. \
              Now `EvaluatorTier`, bound at all three. The residual hop is \
              `EvalSignal.evaluator_tier`, still a `String` because \
              `agent-bestiary-memory` cannot depend on this crate — one \
              conversion, in `handlers::eval::tier_label`, and nowhere else.",
    },
    Vocabulary {
        table: "episodes",
        column: "provenance",
        tokens: EPISODE_PROVENANCE,
        constraint: Some("episodes_provenance_check"),
        owned_by: Some("agent-bestiary/memory/src/types.rs"),
        producers: "every episode writer; `Provenance` in agent-bestiary-memory",
        why: "Decides an episode's authority weight in consolidation. A rejected \
              value fails the episode write outright, which at least fails \
              loudly — unlike most of this table.",
    },
    Vocabulary {
        table: "episodes",
        column: "cost_basis",
        tokens: COST_BASIS,
        constraint: Some("episodes_cost_basis_valid"),
        owned_by: Some("src/agent_backend/rate_card.rs"),
        producers: "agent_backend::rate_card",
        why: "How confidently a run's cost is known. Already pinned by a unit \
              test in `rate_card`; registered here so the pin is discoverable \
              from the seam side rather than only from the producer's.",
    },
];

/// Parse the token set out of a `CHECK (... = ANY (ARRAY['a'::text, ...]))`.
///
/// Quoted literals sit at the odd indices of a split on `'`. Shared with the
/// live tier so the parser is exercised by unit tests that do not need a
/// database.
pub fn tokens_in_constraint(def: &str) -> Vec<String> {
    def.split('\'')
        .skip(1)
        .step_by(2)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::str::FromStr;

    /// What sqlx would put on the wire for this value, as text.
    ///
    /// Not a re-implementation of the encoder — it *is* the encoder, run into a
    /// buffer and read back. The point is that `as_str` and `#[sqlx(rename)]`
    /// are two renderings of one token, written a few characters apart, and
    /// nothing but this compares them.
    fn wire_form<T>(v: &T) -> String
    where
        T: for<'q> sqlx::Encode<'q, sqlx::Postgres>,
    {
        let mut buf = sqlx::postgres::PgArgumentBuffer::default();
        let is_null =
            <T as sqlx::Encode<'_, sqlx::Postgres>>::encode_by_ref(v, &mut buf).expect("encode");
        assert!(
            matches!(is_null, sqlx::encode::IsNull::No),
            "a closed vocabulary encoded itself as NULL"
        );
        String::from_utf8(buf.to_vec()).expect("a token is not utf8")
    }

    /// The string this module claims and the string sqlx sends.
    ///
    /// The two are generated from one literal, so they cannot drift. What can
    /// go is the *binding* between them: sqlx's derive falls back to the
    /// variant name whenever it does not see a `rename`, so removing the
    /// attribute from `closed_vocabulary!` puts `Over` on the wire while every
    /// other check in this file still reads `over` and passes. Verified by
    /// removing it: this test, and only this test, goes red with
    /// `left: "Over", right: "over"`.
    ///
    /// That failure is on the write path, refused by the CHECK, and swallowed
    /// — the `L1` shape exactly, one layer further in than the constants
    /// reached.
    #[test]
    fn the_wire_form_is_the_declared_token() {
        let mut checked = 0;
        let mut expected = 0;
        macro_rules! encodes_its_token {
            ($t:ident, $arr:ident) => {
                for v in $t::ALL {
                    assert_eq!(wire_form(v), v.as_str());
                    checked += 1;
                }
                expected += $arr.len();
            };
        }
        for_each_owned_vocabulary!(encodes_its_token);
        assert_eq!(
            checked, expected,
            "a variant exists that this test does not encode"
        );
        // The count is over the macro's own expansion, so it cannot catch a
        // vocabulary missing from `for_each_owned_vocabulary!` — only a variant
        // whose `ALL` and array disagree, which the macro makes impossible. The
        // list itself is held by
        // `every_registry_owned_vocabulary_is_generated_from_a_type`, which
        // compares it against `VOCABULARIES` in both directions.
        assert!(checked > 0, "the expansion produced nothing");
    }

    /// Every column these types bind is `text`.
    ///
    /// `#[sqlx(type_name = ..)]` is resolved by name at bind time
    /// (`SELECT $1::regtype::oid`), so a typo here is not a compile error and
    /// not a wrong value — it is `type "txt" does not exist` on the first
    /// write, at runtime, on paths that swallow their errors.
    ///
    /// **This shares a finding with**
    /// `seam_vocabulary_contract::every_owned_vocabulary_round_trips_through_postgres`,
    /// which is normally the thing to avoid: one state, two reds. Kept
    /// deliberately, and the split is stated so the second red is read as an
    /// echo rather than a second problem. This tier runs with no database and
    /// is the only one that can fail *before* a deploy; the live tier owns the
    /// behaviour, and catches what a declaration check cannot — a column that
    /// is not text, or a future sqlx that resolves the name differently.
    #[test]
    fn every_owned_vocabulary_binds_as_text() {
        use sqlx::{Postgres, Type, TypeInfo};
        macro_rules! binds_as_text {
            ($t:ident, $arr:ident) => {
                assert_eq!(
                    <$t as Type<Postgres>>::type_info()
                        .name()
                        .to_ascii_lowercase(),
                    "text",
                    "{} does not bind as text, so its first write fails at \
                     runtime with `type does not exist` on a path that swallows \
                     the error",
                    stringify!($t)
                );
            };
        }
        for_each_owned_vocabulary!(binds_as_text);
    }

    /// Wire → Rust, for every token, and a rejection that proves the round trip
    /// is doing work.
    #[test]
    fn every_declared_token_parses_back_to_its_variant() {
        macro_rules! round_trips {
            ($t:ident, $arr:ident) => {
                for v in $t::ALL {
                    assert_eq!($t::from_str(v.as_str()).as_ref(), Ok(v));
                }
            };
        }
        for_each_owned_vocabulary!(round_trips);
        // The severity that started all of this, offered to a vocabulary that
        // has no room for it. If this parsed, everything above would prove
        // nothing.
        let e = EvaluatorTier::from_str("L1").expect_err("`L1` is not a tier");
        assert_eq!(e.vocabulary, "EvaluatorTier");
        assert_eq!(e.expected, EVALUATOR_TIER);
        assert!(e.to_string().contains("dimensional"), "{e}");
    }

    /// A vocabulary this registry owns must be backed by a type.
    ///
    /// `owned_by: None` means there is no upstream authority, so the authority
    /// is the type — and a hand-written array is the shape that was just
    /// removed. Registering one again would compile, pass the live contract,
    /// and leave the write site free to spell anything.
    #[test]
    fn every_registry_owned_vocabulary_is_generated_from_a_type() {
        let mut generated: Vec<(&str, &[&str])> = Vec::new();
        macro_rules! collect {
            ($t:ident, $arr:ident) => {
                generated.push((stringify!($t), $arr));
            };
        }
        for_each_owned_vocabulary!(collect);

        for v in VOCABULARIES {
            if v.owned_by.is_some() {
                continue;
            }
            assert!(
                generated.iter().any(|(_, g)| *g == v.tokens),
                "{}.{} has no upstream owner and no type either — its tokens are \
                 a hand-written array. Declare it with `closed_vocabulary!` and \
                 point `tokens` at the generated const.",
                v.table,
                v.column
            );
        }

        // And the other direction, which is the half with no substitute: a type
        // no `VOCABULARIES` entry points at is never compared against a live
        // CHECK, so the type and the column can disagree freely.
        //
        // This used to be `assert_eq!(backed, generated.len())`, which is a
        // *proxy* for the property and stops being one the first time a single
        // type governs two columns. `ActorKind` now does —
        // `assertion_verifications.actor_kind` and
        // `gate_decision_reviews.actor_kind` are one vocabulary over two tables,
        // which is the whole point of sharing the type — and the count assertion
        // failed on that correct state. A check that fires on the behaviour it
        // wants is §5.2's road to deletion, so it is stated directly instead.
        for (name, tokens) in &generated {
            assert!(
                VOCABULARIES
                    .iter()
                    .any(|v| v.owned_by.is_none() && v.tokens == *tokens),
                "`{name}` is a registry-owned type with no entry in \
                 `VOCABULARIES`, so nothing compares it against a live CHECK — \
                 the one check here with no substitute. Either register the \
                 column it governs, or delete the type."
            );
        }
        assert!(
            !generated.is_empty(),
            "`for_each_owned_vocabulary!` expanded to nothing, so both \
             directions above are vacuous"
        );
    }

    #[test]
    fn every_vocabulary_names_its_producers_and_its_stake() {
        let mut seen = HashSet::new();
        for v in VOCABULARIES {
            assert!(
                seen.insert((v.table, v.column)),
                "{}.{} is declared twice",
                v.table,
                v.column
            );
            assert!(
                !v.tokens.is_empty(),
                "{}.{} declares no tokens",
                v.table,
                v.column
            );
            assert!(
                v.producers.contains("::") || v.producers.contains(' '),
                "{}.{}: `producers` must name code",
                v.table,
                v.column
            );
            assert!(
                v.why.len() > 60,
                "{}.{}: say what a disagreement costs, specifically",
                v.table,
                v.column
            );
        }
    }

    #[test]
    fn no_vocabulary_repeats_a_token() {
        for v in VOCABULARIES {
            let uniq: HashSet<_> = v.tokens.iter().collect();
            assert_eq!(
                uniq.len(),
                v.tokens.len(),
                "{}.{} lists a token twice",
                v.table,
                v.column
            );
        }
    }

    /// Two columns, one ladder.
    ///
    /// Compared by content, not by pointer. `PROVENANCE_VALUES` is a `const`,
    /// so every use site gets its own copy and `ptr::eq` is always false — a
    /// first version of this test asserted pointer identity and failed for that
    /// reason rather than for a real one. Content equality is the weaker claim
    /// and the true one: it says these two columns accept the same ladder, which
    /// is what a reader needs to know.
    #[test]
    fn the_provenance_ladder_is_registered_once_and_shared() {
        let ladder: Vec<_> = VOCABULARIES
            .iter()
            .filter(|v| v.tokens == grounding_trust::PROVENANCE_VALUES)
            .map(|v| format!("{}.{}", v.table, v.column))
            .collect();
        assert_eq!(
            ladder,
            vec![
                "semantic_rules.provenance_floor".to_string(),
                "assertion_verifications.verdict".to_string()
            ],
            "the provenance ladder is no longer shared by exactly the two \
             columns that store it — either a column was added without \
             registering, or the array was copied"
        );
    }

    /// The wire form and the enum must agree, in both directions.
    #[test]
    fn every_episode_provenance_round_trips() {
        use agent_bestiary_memory::Provenance;
        use std::str::FromStr;
        for t in EPISODE_PROVENANCE {
            let p = Provenance::from_str(t)
                .unwrap_or_else(|e| panic!("`{t}` is declared but the enum rejects it: {e}"));
            assert_eq!(
                &p.to_string(),
                t,
                "`{t}` parses to a variant that renders as something else"
            );
        }
        // And nothing the enum can produce is missing here. Checked through the
        // one path that enumerates the variants: parsing every declared token
        // above covers Rust -> wire; this covers wire -> Rust for a value the
        // enum would accept but the registry omits.
        assert!(
            Provenance::from_str("not_a_provenance").is_err(),
            "the enum accepts an unknown token, so the round-trip above proves \
             nothing"
        );
    }

    #[test]
    fn the_constraint_parser_reads_both_shapes() {
        // Plain.
        let plain = "CHECK ((kind = ANY (ARRAY['drift'::text, 'safety'::text])))";
        assert_eq!(tokens_in_constraint(plain), vec!["drift", "safety"]);

        // Nullable, which is how the two provenance columns are written.
        let nullable = "CHECK (((cost_basis IS NULL) OR (cost_basis = ANY \
                        (ARRAY['no_charge'::text]))))";
        assert_eq!(tokens_in_constraint(nullable), vec!["no_charge"]);

        // A constraint with no literals must yield nothing rather than
        // something wrong — the live tier treats an empty parse as a failure to
        // read, not as an empty vocabulary.
        assert!(tokens_in_constraint("CHECK ((score >= 0.0))").is_empty());
    }
}
