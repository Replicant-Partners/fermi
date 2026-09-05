//! **What a caller may rely on**, in one token.
//!
//! # The gap this closes
//!
//! `docs/plans/WHAT_THE_PLATFORM_CAN_REFUSE.md` §4.1 asks: *if the platform
//! refuses, the caller receives what?* It says the delegation hop has an
//! envelope and *"HTTP has none"*.
//!
//! Half of that has since stopped being true, and the half that remains is the
//! interesting one. `POST /api/agents/:id/execute` now returns the enforced
//! `document`, a `grounding` block naming what was stripped, a `completeness`
//! block naming what was owed, and a `validation` block with the declared type
//! and its violations. There is plenty there.
//!
//! What there is not is **one thing to branch on**. A caller asking the only
//! question a caller actually has — *can I use this answer?* — has to read four
//! sub-objects written in four vocabularies:
//!
//! ```text
//! status              Success | Failed         (did the RUN finish)
//! validation.status   valid | invalid | unverified_*   (does it match its type)
//! grounding.amended   bool + stripped[]        (did we remove fabrication)
//! completeness.owed   [{path, why}]            (did the agent skip work)
//! ```
//!
//! and then implement the platform's own judgement about how those combine.
//! That is the defect this repository keeps finding in its own surfaces: the
//! trace strip re-derived the gate it drew, and the page computed question three
//! from the values because no checkpoint computed it. Both were fixed by having
//! **one** producer of the verdict and everything else read it. A caller
//! re-deriving reliance from four fields is the same shape, one process further
//! out, where we cannot see them get it wrong.
//!
//! # Why this is the thing §4.1 was asking for
//!
//! Not a refusal envelope. A refusal envelope with no refusal behind it would
//! be a claim that the platform can refuse, which is false, and inventing the
//! shape now is how the three stale comments this subsystem has already
//! produced got written.
//!
//! The prerequisite for promoting a gate to `Control` is that **callers already
//! branch on this field**. Then the promotion adds a value to a vocabulary
//! consumers understand, rather than a shape they have never seen. A refusal
//! introduced alongside the contract that describes it is a refusal callers
//! route around — §4.1's own argument for why this has to come first.
//!
//! So [`RELIANCE`] deliberately has no `refused` variant. It gains one in the
//! commit that can emit it, and [`tests::the_vocabulary_has_no_refusal_it_cannot_emit`]
//! holds that line.
//!
//! # The worst available reading wins
//!
//! Several of these can be true at once: a document can be amended *and*
//! incomplete. One token means a precedence, and it runs worst-first — the same
//! rule `gate_api::the_worst_available_reading_wins` applies to gate readings,
//! for the same reason. A caller that acts on the best of several true readings
//! has been told the platform's most flattering opinion of its own output.

use crate::completeness::Assessment;
use crate::grounding_trust::Report;

/// What a caller may rely on. Closed, worst-first.
///
/// Worst-first is the declaration order **and** the precedence order, so the
/// two cannot drift: [`reliance`] returns the first that applies.
pub const RELIANCE: &[(&str, &str)] = &[
    (
        "unusable",
        "No structured document at all. Either the agent returned prose, or it \
         returned nothing. Not a failure on its own — a prose agent is \
         legitimate and explicitly non-composable — but there is no artifact to \
         rely on and a caller expecting one must not proceed.",
    ),
    (
        "malformed",
        "There is a document and it contradicts the type the agent declared. \
         Worse than a stripped field: a caller that trusted `produces_schema` \
         has already decided how to read this, and the shape it planned for is \
         not what arrived. `validation.violations` names the paths.",
    ),
    (
        "amended",
        "The platform removed values no tool of this agent's could have \
         supplied. What remains is trustworthy and some of it is gone. Ranked \
         above `incomplete` deliberately: an absent field is a gap, whereas a \
         field the model invented is evidence about the model, and a caller may \
         reasonably trust the rest of THIS document less. \
         `grounding.stripped` names the fields.",
    ),
    (
        "incomplete",
        "The agent did not fill fields it was asked for and no tool was asked \
         on its behalf. Nothing is wrong with what is here; there is less of it \
         than the contract commissioned. `completeness.owed` names the paths, \
         and it is a floor rather than a total.",
    ),
    (
        "unchecked",
        "A document, and no contract was applied to it. **Not a pass.** The \
         platform has no opinion about this artifact, which is a different state \
         from having looked and found nothing wrong, and 81 of 102 agents are in \
         it. Distinguished for the same reason `Decision::Undetermined` is not \
         `Decision::Approved`: counting silence as consent would report a gate \
         that almost never engages as a control that never needed to fire.",
    ),
    (
        "clean",
        "A contract was applied, nothing was stripped, nothing is owed, and the \
         document matches its declared type. The only value that means the \
         platform looked and found nothing wrong.",
    ),
];

/// Everything [`reliance`] needs, and nothing it could compute differently.
///
/// A struct rather than five positional arguments because the two execute
/// routes must pass the same things, and a bare `bool` in fourth position is
/// how the streaming sibling came to run four of the six checks its twin ran.
#[derive(Debug, Clone, Copy)]
pub struct Answer<'a> {
    /// Did enforcement produce a document at all?
    pub document: bool,
    /// Was any grounding contract applied? `false` is `undetermined` — no
    /// contract in either home — and must not read as a pass.
    pub contract_applied: bool,
    /// What enforcement did. Read for the strip count only; the paths are
    /// reported separately and this must not restate them.
    pub report: &'a Report,
    /// What the agent owed. `None` when completeness was not assessed.
    pub completeness: Option<&'a Assessment>,
    /// The `validation.status` token already computed by the route.
    /// `"invalid"` is the only value that lowers reliance: `unverified_*` means
    /// nothing was checked, and absent must look different from bad.
    pub validation: &'a str,
}

/// Which single token describes this answer.
///
/// Derived, never passed in. Every input is something the route already
/// computed, so this cannot disagree with the `grounding`, `completeness` and
/// `validation` blocks it summarises — which is the whole point of it existing
/// rather than each caller combining them.
pub fn reliance(a: Answer<'_>) -> &'static str {
    if !a.document {
        return "unusable";
    }
    // Only a contradiction counts. `unverified_no_schema` is the common case
    // (most cards declare no schema) and reporting it as malformed would make
    // this token useless on the majority of the corpus — the same reasoning
    // `schema_validate` uses for its own three-way answer.
    if a.validation == "invalid" {
        return "malformed";
    }
    if !a.report.is_clean() {
        return "amended";
    }
    if a.completeness.is_some_and(|c| !c.owed.is_empty()) {
        return "incomplete";
    }
    if !a.contract_applied {
        return "unchecked";
    }
    "clean"
}

/// The sentence explaining a token, for the response's own legend.
///
/// Said once, keyed by the token the response carries, rather than repeated at
/// every consumer. The reason belongs to the state.
pub fn why(token: &str) -> Option<&'static str> {
    RELIANCE.iter().find(|(t, _)| *t == token).map(|(_, w)| *w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::completeness::{Assessment, Gap, Owed};
    use crate::grounding_trust::{Violation, ViolationKind};

    fn dirty() -> Report {
        Report {
            violations: vec![Violation {
                path: "genome.ploidy".into(),
                removed: serde_json::json!("diploid"),
                kind: ViolationKind::UngroundedField,
            }],
            provenance: vec![],
        }
    }

    fn answer<'a>(report: &'a Report, validation: &'a str) -> Answer<'a> {
        Answer {
            document: true,
            contract_applied: true,
            report,
            completeness: None,
            validation,
        }
    }

    /// The happy path is reachable, and it is the only one that reads as one.
    #[test]
    fn only_a_checked_and_untouched_document_is_clean() {
        let clean = Report::default();
        assert_eq!(reliance(answer(&clean, "valid")), "clean");

        // Same document, no contract behind it. Not a pass.
        let mut unchecked = answer(&clean, "valid");
        unchecked.contract_applied = false;
        assert_eq!(
            reliance(unchecked),
            "unchecked",
            "an unchecked document read as `clean`, which is the platform \
             reporting silence as consent"
        );
    }

    /// `unverified_*` must not lower reliance, and `invalid` must.
    ///
    /// The distinction absent-versus-bad, on the axis where it is easiest to
    /// lose: most cards declare no schema, so a token that treated
    /// `unverified_no_schema` as a fault would report almost the whole corpus
    /// as malformed and would be switched off within a day.
    #[test]
    fn nothing_checked_is_not_the_same_as_checked_and_wrong() {
        let clean = Report::default();
        for unverified in [
            "unverified_no_schema",
            "unverified_no_payload",
            "unverified_unsupported_schema",
        ] {
            assert_eq!(
                reliance(answer(&clean, unverified)),
                "clean",
                "`{unverified}` lowered reliance. Nothing was checked, which is \
                 not the same as something being wrong."
            );
        }
        assert_eq!(reliance(answer(&clean, "invalid")), "malformed");
    }

    /// When several readings are true, the worst wins.
    ///
    /// Each pair below is a document in two bad states at once, and the
    /// assertion is which one a caller is told about. Without this the
    /// precedence is whatever order the `if`s happen to be in, which is how a
    /// caller comes to act on the platform's most flattering reading of its own
    /// output.
    #[test]
    fn the_worst_available_reading_wins() {
        let d = dirty();
        let short = Assessment {
            asked_for: 2,
            filled: 0,
            owed: vec![Gap {
                path: "conservation.iucn_status",
                why: Owed::ToolNeverCalled,
                tool: Some("iucn_lookup"),
            }],
            no_data: vec![],
            excused: 0,
        };

        // Amended AND malformed -> malformed. A caller that planned its parse
        // around `produces_schema` is already wrong; that outranks a strip.
        let mut a = answer(&d, "invalid");
        a.completeness = Some(&short);
        assert_eq!(reliance(a), "malformed");

        // Amended AND incomplete -> amended.
        let mut b = answer(&d, "valid");
        b.completeness = Some(&short);
        assert_eq!(
            reliance(b),
            "amended",
            "a document that was both repaired and short must report the \
             repair: an invented field is evidence about the model and a gap is \
             not, so it is the one that should change what a caller does with \
             the REST of the document"
        );

        // Incomplete alone, with a clean report, is reachable — otherwise the
        // branch above could be hiding it permanently.
        let clean = Report::default();
        let mut c = answer(&clean, "valid");
        c.completeness = Some(&short);
        assert_eq!(
            reliance(c),
            "incomplete",
            "`incomplete` is unreachable, so the precedence above is masking it \
             rather than ordering it"
        );
    }

    /// No document outranks everything, including a schema contradiction.
    #[test]
    fn an_absent_document_is_not_described_by_what_it_would_have_failed() {
        let clean = Report::default();
        let mut none = answer(&clean, "invalid");
        none.document = false;
        assert_eq!(
            reliance(none),
            "unusable",
            "a response with no document reported `malformed`, which tells a \
             caller to go and read violations on a thing that does not exist"
        );
    }

    /// **The vocabulary may not promise a refusal the platform cannot make.**
    ///
    /// This is the guard that keeps this module from becoming the fourth stale
    /// claim in this subsystem. No artifact gate is a `Control`: grounding
    /// amends, completeness reports, schema validation reports. Nothing on the
    /// execute path declines to hand over the artifact.
    ///
    /// A `refused` token would therefore describe a state no code can produce —
    /// exactly the shape of `AMENDS_LATER`'s stale exemption, the trace's
    /// "Question 3 has no gate", and `grade`'s discarded `output_contract`. All
    /// three were prose that outlived its code; this would be prose that
    /// preceded it, which is the same defect wearing optimism.
    ///
    /// It comes off in the commit that promotes a gate to `Control`, and the
    /// enforcement ladder in `command_registry` is what says whether that has
    /// happened.
    #[test]
    fn the_vocabulary_has_no_refusal_it_cannot_emit() {
        assert!(
            !RELIANCE.iter().any(|(t, _)| *t == "refused"),
            "the vocabulary declares `refused` and no gate on the execute path \
             can refuse. Add it in the commit that promotes one to Control, not \
             before — a contract describing a capability the platform lacks is \
             what §4.1 exists to prevent, pointed the other way."
        );

        // And the ladder is what decides when that changes.
        //
        // Scoped to the three gates this module actually summarises — the ones
        // behind `Answer`'s fields. Not "every gate on the route": `credit`,
        // `rate_limit` and `attachment` are already `Control` and always will
        // be. They refuse before there is an answer, so the caller gets a
        // status code and no body to carry a token, and folding them in here
        // would make this assertion permanently red for the wrong reason.
        //
        // A list rather than a predicate because the coupling is real and
        // narrow: these are exactly the verdicts `reliance` reads, and if a
        // fourth starts feeding it, this list and `Answer` change together.
        const SUMMARISED: &[&str] = &["grounding", "completeness", "output_schema"];
        for cmd in ["agent.execute", "agent.execute_stream"] {
            let Some(command) = crate::command_registry::command(cmd) else {
                continue;
            };
            for g in command.gates {
                if !SUMMARISED.contains(&g.gate.id()) {
                    continue;
                }
                assert!(
                    !g.enforcement.refuses(),
                    "`{}` refuses on `{cmd}`, and it is one of the verdicts \
                     `reliance` summarises — so a caller can now be handed a \
                     response this vocabulary has no word for. Add `refused` to \
                     RELIANCE and delete this assertion in the same commit. \
                     That ordering is the whole of §4.1: the contract exists \
                     before the refusal, or callers route around the refusal.",
                    g.gate.id()
                );
            }
        }
    }

    /// Every token carries its own explanation, and no two share a name.
    #[test]
    fn every_token_is_unique_and_explained() {
        let mut seen: Vec<&str> = RELIANCE.iter().map(|(t, _)| *t).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), before, "two reliance tokens share a name");

        for (token, why) in RELIANCE {
            assert!(
                token.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "`{token}` is not a token; a cell holds a token, never a sentence"
            );
            assert!(
                why.len() > 80,
                "`{token}` has no argument behind it. Every value here changes \
                 what a caller does, and one that cannot say why will be \
                 guessed at."
            );
            assert_eq!(
                super::why(token),
                Some(*why),
                "`{token}` is not reachable through `why`, so the response \
                 cannot carry its own legend"
            );
        }
    }
}
