//! **One field, two clocks** — what it can be trusted about, and what happened
//! to it this time.
//!
//! # The ambition this serves
//!
//! * **The contract shapes the pulse.** Prospective: declared before the run,
//!   it decides what fields exist and what each one can be trusted about.
//! * **The gates verify the pulse against the contract.** Retrospective: after
//!   the model has written. (A separate class of gates — `credit`,
//!   `rate_limit`, `attachment` — are *preconditions* on the run rather than
//!   verification of the artifact. `gate_trust`'s
//!   `decides_before_the_artifact` is the platform's own name for that split.)
//! * **Composition moves pulses between agents.** Ports say two agents *can*
//!   connect; these states say whether what flows is worth consuming.
//!
//! Those are three questions on three clocks and they were being answered in
//! four vocabularies.
//!
//! # The collision this removes
//!
//! On one football_analyst pulse, three surfaces described the same thirteen
//! fields:
//!
//! ```text
//! specimen   6 resolved · 4 pending · 1 derived · 1 inferred · 1 narrative
//! trace top  1 from a tool · 1 from judgement · 11 at zero
//! trace low  graded 13 claim(s), 1 unsourced
//! ```
//!
//! Nothing contradicted. Every number reconciles: of the eleven at zero, five
//! were a tool asked and empty, four were correctly absent, one was owed, one
//! derived. But **`unsourced` meant two opposite things on adjacent panels** —
//! on the specimen a declared kind, four of them, explicitly *"a standing
//! request for an integration, not a defect"*; on the trace a violation, *"1
//! unsourced claim was removed"*.
//!
//! And `refused` did not mean refused. `Decision::Refused` is the ledger's word
//! for *this gate acted*; nothing was declined, one field was nulled and the
//! answer was delivered whole. A reader asking "what exactly was refused?" got
//! no answer from the page.
//!
//! # The rule that makes it structural
//!
//! [`tests::the_two_vocabularies_share_no_token`] asserts the two token sets
//! are disjoint. One word cannot mean a declared kind on one surface and a
//! violation on another, because the build fails. That is the specific defect,
//! made impossible rather than corrected.
//!
//! # Derived, never re-derived
//!
//! Both states are computed from what the platform already produced —
//! `grounding_trust::Grounding`, `GradedField`, the enforcement `Report`, and
//! `completeness::Assessment`. This module adds no judgement of its own.
//!
//! That is the whole point. The trace strip once re-derived in JavaScript the
//! gate it was drawing, and the page computed question three from the values
//! because no checkpoint computed it. Both were fixed by having one producer of
//! the verdict and everything else read it. Four surfaces each deciding what a
//! field's state is, is the same defect spread wider.

use crate::completeness::Assessment;
use crate::grounding_trust::{GradedField, Grounding, GroundingKind, Report};

/// **What this field can be trusted about**, from the contract. The static
/// clock: true before the agent runs and after it finishes.
///
/// Token strings are the specimen page's existing vocabulary, kept deliberately
/// — that surface reads best of the three and there is no reason to churn a
/// working word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Declared {
    /// A tool is named and the platform can dispatch it.
    Resolved,
    /// A tool is named and the platform **cannot** dispatch it, so nothing can
    /// ever settle this field. The one state here that is a defect.
    Unresolvable,
    /// No tool exists yet. The field must be null; a value is the violation.
    /// A standing request for an integration, not a fault.
    Pending,
    /// Platform code computes it from other fields.
    Derived,
    /// A judgement the agent is commissioned to make.
    Inferred,
    /// Prose. No proposition to settle.
    Narrative,
}

/// **What happened to this field on this pulse.** The run clock.
///
/// Deliberately disjoint from [`Declared`]: no token appears in both, so a
/// reader who learns one word learns one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// A value came back.
    Filled,
    /// Empty, and the contract requires that. The contract working, not a gap.
    AbsentByContract,
    /// The named tool was asked and had nothing. A capability gap in the
    /// world, and nobody's fault.
    ToolEmpty,
    /// Empty and owed: a named tool was never called, or commissioned work did
    /// not come back. The agent's.
    Owed,
    /// The model wrote a value the contract forbids and the platform removed
    /// it before the response left.
    ///
    /// **This is the word `refused` was standing in for.** The gate recorded
    /// `Decision::Refused`, the artifact was delivered in full, and one field
    /// was nulled. `Stripped` says that; `refused` said something that did not
    /// happen.
    Stripped,
}

impl Declared {
    pub fn token(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            // `error`, not `unresolvable`, because five places in
            // `specimen.html` and `compile_error_count` already consume that
            // string. This change exists to stop tokens meaning two things;
            // renaming a working one to a better one would be churn wearing
            // the same coat.
            //
            // Noted rather than hidden: on the specimen page `error` is also
            // the severity for non-field rows ("does not name its type"), so
            // it carries two senses there already. That is pre-existing, out of
            // scope here, and worth fixing when the page's row grammar is next
            // touched — the variant is named `Unresolvable` so the Rust side
            // is unambiguous in the meantime.
            Self::Unresolvable => "error",
            Self::Pending => "pending",
            Self::Derived => "derived",
            Self::Inferred => "inferred",
            Self::Narrative => "narrative",
        }
    }

    /// Said once, keyed by the token the row prints.
    pub fn why(self) -> &'static str {
        match self {
            Self::Resolved => "a tool is named and the platform can run it",
            Self::Unresolvable => {
                "the contract names a tool the platform cannot dispatch, so \
                 nothing can ever settle this field"
            }
            Self::Pending => {
                "no tool exists for this yet. The field must be null and a value \
                 here is the violation — a standing request for an integration, \
                 not a defect"
            }
            Self::Derived => {
                "computed by the platform from other fields, so it is \
                 reproducible by construction"
            }
            Self::Inferred => {
                "a judgement the agent is commissioned to make. An endorsement \
                 is the terminal verdict, not a weak citation"
            }
            Self::Narrative => "prose. There is no proposition to settle",
        }
    }

    /// From the contract, plus whether the platform can dispatch the named tool.
    ///
    /// `dispatchable` is asked rather than assumed: a contract naming a tool
    /// that no longer exists is the difference between `Resolved` and
    /// `Unresolvable`, and it is invisible in the contract alone.
    pub fn of(grounding: &Grounding, dispatchable: impl Fn(&str) -> bool) -> Self {
        match grounding {
            Grounding::Sourced { tool, .. } => {
                if dispatchable(tool) {
                    Self::Resolved
                } else {
                    Self::Unresolvable
                }
            }
            Grounding::Unsourced => Self::Pending,
            Grounding::Derived { .. } => Self::Derived,
            Grounding::Inferred { .. } => Self::Inferred,
            Grounding::Narrative => Self::Narrative,
        }
    }
}

impl Observed {
    pub fn token(self) -> &'static str {
        match self {
            Self::Filled => "filled",
            Self::AbsentByContract => "absent_by_contract",
            Self::ToolEmpty => "tool_empty",
            Self::Owed => "owed",
            Self::Stripped => "stripped",
        }
    }

    pub fn why(self) -> &'static str {
        match self {
            Self::Filled => {
                "a value came back. What it is worth depends on the contract                  state beside it — a filled `resolved` field came from a tool,                  a filled `inferred` one is the agent's judgement"
            }
            Self::AbsentByContract => {
                "empty, and the contract requires that. This is the contract \
                 working rather than a gap"
            }
            Self::ToolEmpty => {
                "the named tool was asked and had nothing. A capability gap in \
                 the world, and nobody's fault"
            }
            Self::Owed => {
                "empty and owed — a named tool was never called, or \
                 commissioned work did not come back. This one is the agent's"
            }
            Self::Stripped => {
                "the model wrote a value the contract forbids and the platform \
                 removed it before the response left. The answer was still \
                 delivered; this field was nulled"
            }
        }
    }

    /// Whose gap this is, for a reader deciding what to do about it.
    ///
    /// The distinction `completeness` was built to make, carried here so a
    /// surface does not have to re-derive it from the token.
    pub fn whose(self) -> &'static str {
        match self {
            Self::Filled | Self::AbsentByContract => "nobody's",
            Self::ToolEmpty => "the world's",
            Self::Owed | Self::Stripped => "the agent's",
        }
    }

    /// What happened to one field on one pulse.
    ///
    /// Ordered worst-first, like every other reading on this platform: a field
    /// that was stripped AND empty is reported as stripped, because the removal
    /// is the thing a reader must know.
    pub fn of(field: &GradedField, report: &Report, completeness: Option<&Assessment>) -> Self {
        if report.violations.iter().any(|v| v.path == field.path) {
            return Self::Stripped;
        }
        // The SAME predicate `completeness` uses for its `filled` count. A
        // second opinion here would let this row say `filled` while the
        // summary above it says the field is empty.
        if crate::completeness::has_value(&field.value) {
            return Self::Filled;
        }
        // Empty. Whose gap is it? `completeness` already decided; read it
        // rather than re-deciding from the kind.
        if let Some(a) = completeness {
            if a.owed.iter().any(|g| g.path == field.path) {
                return Self::Owed;
            }
            if a.no_data.contains(&field.path) {
                return Self::ToolEmpty;
            }
        }
        match field.kind {
            // The contract requires null here, so an absence is compliance.
            GroundingKind::Unsourced | GroundingKind::Derived => Self::AbsentByContract,
            // Prose is the one kind where both a value and its lack are
            // ordinary — see `GroundingKind::Narrative`.
            GroundingKind::Narrative => Self::AbsentByContract,
            // A sourced or inferred field with no value and no completeness
            // verdict: the agent had the means and produced nothing.
            GroundingKind::Sourced | GroundingKind::Inferred => Self::Owed,
        }
    }
}

/// One row, as every surface should print it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRow {
    pub path: &'static str,
    pub declared: Declared,
    /// `None` on a surface with no pulse in hand — the specimen page describes
    /// a contract and has no run to report.
    pub observed: Option<Observed>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DECLARED_ALL: &[Declared] = &[
        Declared::Resolved,
        Declared::Unresolvable,
        Declared::Pending,
        Declared::Derived,
        Declared::Inferred,
        Declared::Narrative,
    ];
    const OBSERVED_ALL: &[Observed] = &[
        Observed::Filled,
        Observed::AbsentByContract,
        Observed::ToolEmpty,
        Observed::Owed,
        Observed::Stripped,
    ];

    /// **The defect, made impossible.**
    ///
    /// `unsourced` meant a declared kind on the specimen page ("pending", a
    /// standing request, not a defect) and a violation on the artifact trace
    /// ("1 unsourced claim was removed"). Two opposite meanings, one word, two
    /// panels a reader sees together.
    ///
    /// A reader who learns a token must learn one thing. This is the assertion
    /// that keeps that true as either vocabulary grows.
    #[test]
    fn the_two_vocabularies_share_no_token() {
        for d in DECLARED_ALL {
            for o in OBSERVED_ALL {
                assert_ne!(
                    d.token(),
                    o.token(),
                    "`{}` is both a contract state and a run state. That is the \
                     `unsourced` collision returning: one word meaning a \
                     declared kind on one surface and a violation on another, \
                     which is what made the two panels read as contradicting \
                     each other.",
                    d.token()
                );
            }
        }
    }

    /// No surface may print `refused` as a field state.
    ///
    /// `Decision::Refused` is the ledger's word for *this gate acted*, and it
    /// is correct there. On the artifact page it read as the platform having
    /// declined, when the answer was delivered whole and one field was nulled.
    /// `Stripped` is that event.
    #[test]
    fn refused_is_not_a_field_state() {
        for t in DECLARED_ALL
            .iter()
            .map(|d| d.token())
            .chain(OBSERVED_ALL.iter().map(|o| o.token()))
        {
            assert_ne!(
                t, "refused",
                "`refused` is a field state again. It is the ledger's word for \
                 a gate acting, and on a surface it claims something that did \
                 not happen — the artifact was delivered."
            );
        }
    }

    /// Every token is a token, and every one carries its argument.
    #[test]
    fn every_token_is_lowercase_and_explained() {
        let all: Vec<(&str, &str)> = DECLARED_ALL
            .iter()
            .map(|d| (d.token(), d.why()))
            .chain(OBSERVED_ALL.iter().map(|o| (o.token(), o.why())))
            .collect();
        for (t, why) in &all {
            assert!(
                t.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "`{t}` is not a token; a cell holds a token, never a sentence"
            );
            assert!(
                why.len() > 30,
                "`{t}` has no argument behind it, and a state a reader cannot \
                 look up is one they will guess at"
            );
        }
        let mut seen: Vec<&str> = all.iter().map(|(t, _)| *t).collect();
        let n = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), n, "two states share a token: {seen:?}");
    }

    fn field(path: &'static str, kind: GroundingKind, value: serde_json::Value) -> GradedField {
        GradedField {
            path,
            block: "b",
            value,
            provenance: "tool_verified",
            settleable_by: None,
            kind,
        }
    }

    /// **A stripped field must not read as an ordinary absence.**
    ///
    /// This is the football_analyst pulse in miniature. The field is empty
    /// *because the platform emptied it*, and an absence and a removal are
    /// different findings with different remedies: one is the contract
    /// working, the other is the model having fabricated.
    #[test]
    fn a_removal_and_an_absence_are_different_findings() {
        let f = field("ratings.elo_current", GroundingKind::Unsourced, json!(null));
        let clean = Report::default();
        let dirty = Report {
            violations: vec![crate::grounding_trust::Violation {
                path: "ratings.elo_current".into(),
                removed: json!(1834),
                kind: crate::grounding_trust::ViolationKind::UngroundedField,
            }],
            provenance: vec![],
        };

        assert_eq!(
            Observed::of(&f, &clean, None),
            Observed::AbsentByContract,
            "an Unsourced field that is null is the contract being obeyed"
        );
        assert_eq!(
            Observed::of(&f, &dirty, None),
            Observed::Stripped,
            "the same field, nulled BY US after the model filled it, must not \
             read as compliance. That is the difference the trace could not \
             express, and the reason `1 unsourced` looked like it contradicted \
             `4 pending`."
        );
        assert_eq!(Observed::AbsentByContract.whose(), "nobody's");
        assert_eq!(Observed::Stripped.whose(), "the agent's");
    }

    /// The world's gap and the agent's are told apart by the run record.
    #[test]
    fn a_tool_asked_and_empty_is_not_the_agents_fault() {
        use crate::completeness::{Assessment, Gap, Owed};
        let f = field("match_statistics", GroundingKind::Sourced, json!(null));
        let clean = Report::default();

        let world = Assessment {
            asked_for: 1,
            filled: 0,
            owed: vec![],
            no_data: vec!["match_statistics"],
            excused: 0,
        };
        assert_eq!(Observed::of(&f, &clean, Some(&world)), Observed::ToolEmpty);
        assert_eq!(Observed::ToolEmpty.whose(), "the world's");

        let agent = Assessment {
            asked_for: 1,
            filled: 0,
            owed: vec![Gap {
                path: "match_statistics",
                why: Owed::ToolNeverCalled,
                tool: Some("call_football_api"),
            }],
            no_data: vec![],
            excused: 0,
        };
        assert_eq!(Observed::of(&f, &clean, Some(&agent)), Observed::Owed);
        assert_eq!(Observed::Owed.whose(), "the agent's");
    }

    /// A contract naming a tool the platform cannot run is a defect, and the
    /// only one in the declared vocabulary.
    #[test]
    fn a_tool_the_platform_cannot_dispatch_is_unresolvable() {
        let g = Grounding::Sourced {
            tool: "call_football_api",
            response_field: "x",
        };
        assert_eq!(Declared::of(&g, |_| true), Declared::Resolved);
        assert_eq!(
            Declared::of(&g, |_| false),
            Declared::Unresolvable,
            "a contract naming an undispatchable tool must not read as \
             resolved — nothing can ever settle that field, and the specimen \
             page's green count would be counting a permanent gap as healthy"
        );
    }
}
