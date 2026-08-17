//! What an agent quantified, and how much of it we can stand behind.
//!
//! # Why this is separate from a claim
//!
//! `forecast_agent_claims` has `workspace_id NOT NULL` and `driver NOT NULL`,
//! which is right for a claim — a claim is an adjustment applied to a driver,
//! neutralisable at 1.0, and that neutralisability is what lets the Shapley
//! engine compute exact per-agent credit from a single real forecast. None of it
//! means anything without a driver.
//!
//! But it was the only place an agent's quantified output could go, so
//! `execution.rs` gates the write on having a workspace and a standalone
//! evaluation loses everything. Measured: **14 quantified judgements, all 14
//! produced outside a workspace, 0 claims recorded.** Standalone evaluation is
//! how agents are mostly exercised, so no agent could build a track record.
//!
//! So: an **assertion** is what the agent quantified, recorded whenever it ran.
//! A **claim** is that assertion bound to a driver, `0..n` per assertion.
//!
//! # A multiplier can never be tool-verified
//!
//! The load-bearing rule in this module, and it follows from what a multiplier
//! is. No database anywhere contains "the multiplier for this driver" — the
//! agent is *asked* to produce it, and producing it is the entire job. That is
//! `Grounding::Inferred`, one layer out, and it means
//! [`AssertionKind::Multiplier`]'s ceiling is [`PROV_INFERRED`] permanently, not
//! pending better tooling.
//!
//! Which settles a question that would otherwise have no answer: **you cannot
//! verify a multiplier.** "Is 0.85 correct?" is not a checkable proposition. So
//! verification routes to the multiplier's *basis* — the Elo, the injury list,
//! the xG — and the multiplier's standing is the floor over those. Verify the
//! inputs, inherit the verdict.
//!
//! A [`AssertionKind::Quantity`] is the opposite: `elo_current = 1834` purports
//! to be a retrieval, so it is checkable, and if no tool call stands behind it
//! the honest verdict is `pending_*` rather than a value.
//!
//! # One ladder, not two
//!
//! Every verdict and all the arithmetic comes from [`crate::grounding_trust`].
//! A second copy of a trust calculation is a second answer to the same
//! question, and the one that disagrees is the one nearest the writer — this
//! module has already had that bug once, when cards said `gbif_verified` and the
//! runtime emitted `tool_verified`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::grounding_trust::{
    floor, strength, PROV_INFERRED, PROV_PENDING_HUMAN, PROV_PENDING_TOOL, PROV_UNAVAILABLE,
};

/// What kind of quantity an assertion carries.
///
/// The variant decides what verification even means, which is why this is an
/// enum rather than a units string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    /// A neutralisable adjustment to a driver's base rate. Neutral at 1.0.
    ///
    /// Unverifiable by construction, and permanently capped at
    /// `model_inference`: no tool returns a multiplier, because the agent is
    /// asked to produce one. Verification routes to `basis`.
    Multiplier,
    /// An absolute measurement that purports to come from somewhere —
    /// `elo_current = 1834`, a market value, a chromosome count.
    ///
    /// Checkable, and therefore the interesting case. If a tool could supply it
    /// the route is automated; if none can, a person must.
    Quantity,
    /// A probability the agent asserts directly rather than as an adjustment.
    ///
    /// Treated like `Multiplier` for verification: it is a judgement, so its
    /// standing is the floor over what it reasoned from.
    Probability,
}

impl AssertionKind {
    /// The strongest provenance this kind of assertion may ever hold.
    ///
    /// `Multiplier` and `Probability` are judgements the agent was asked to
    /// make, so `model_inference` is not a limitation to be engineered away —
    /// it is the correct and permanent ceiling. Claiming better would report a
    /// fact about the inputs as a fact about the conclusion, which is the same
    /// error `EXTRACTION_CEILING` prevents for semantic rules.
    ///
    /// `Quantity` has no ceiling here: a real tool call can make it
    /// `tool_verified`, and a cited human check can make it `human_sourced`.
    pub fn ceiling(self) -> Option<&'static str> {
        match self {
            AssertionKind::Multiplier | AssertionKind::Probability => Some(PROV_INFERRED),
            AssertionKind::Quantity => None,
        }
    }

    /// Can this kind of assertion be verified at all?
    ///
    /// Routing a multiplier into a human verification queue would ask a person
    /// to confirm a number that is not a proposition about the world. They
    /// would either rubber-stamp it or reject it on taste, and both outcomes
    /// pollute the rejection rate that makes the queue worth having.
    pub fn is_verifiable(self) -> bool {
        matches!(self, AssertionKind::Quantity)
    }
}

/// How the number was recovered from the agent's output.
///
/// Provenance is a property of the extraction path, not a label someone
/// attaches. Reading a typed field out of a validated payload is a different
/// act from pattern-matching a sentence, and the difference has to survive into
/// the verdict — otherwise the retrofit has no gradient and there is no reason
/// for an agent to ever emit structured output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "path", rename_all = "snake_case")]
pub enum ExtractionPath {
    /// Read from a declared field in a schema-validated payload.
    ///
    /// No ceiling of its own: the value's provenance is whatever its payload
    /// block earned, so a `tool_verified` block yields a `tool_verified`
    /// quantity. This is the only path that can reach the top of the ladder.
    TypedField { schema: String, field_path: String },
    /// Recovered from prose by pattern match.
    ///
    /// Capped at `model_inference`, and not out of squeamishness. The prose has
    /// no typed provenance to inherit, so nothing about the number's origin was
    /// ever recorded — and the recovery itself is lossy in a way we have
    /// measured: **8 of 14** multiplier lines were unrecoverable because the
    /// model wrote `**1.15**` where the pattern wanted `1.15`. A path that
    /// silently drops 57% of its input must not be able to produce the same
    /// verdict as one that reads a field.
    Prose { pattern: String },
}

impl ExtractionPath {
    pub fn ceiling(&self) -> Option<&'static str> {
        match self {
            ExtractionPath::TypedField { .. } => None,
            ExtractionPath::Prose { .. } => Some(PROV_INFERRED),
        }
    }
}

/// A `(p5, p50, p95)` triple, as agents actually emit them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spread {
    pub p5: f64,
    pub p50: f64,
    pub p95: f64,
}

/// The declared range for a multiplier, from the card contract.
pub const MULTIPLIER_MIN: f64 = 0.1;
pub const MULTIPLIER_MAX: f64 = 3.0;

impl Spread {
    /// Is this a coherent spread, and in range for its kind?
    ///
    /// Ordering is checked because a reversed spread is not a wide estimate,
    /// it is a broken one, and `p5 > p95` silently inverts every downstream
    /// interval. Rejecting is right: unlike a fabricated value there is no
    /// honest reading to preserve.
    pub fn validate(&self, kind: AssertionKind) -> Result<(), String> {
        if !(self.p5.is_finite() && self.p50.is_finite() && self.p95.is_finite()) {
            return Err("spread contains a non-finite value".into());
        }
        if self.p5 > self.p50 || self.p50 > self.p95 {
            return Err(format!(
                "spread is not ordered: p5={} p50={} p95={}",
                self.p5, self.p50, self.p95
            ));
        }
        match kind {
            AssertionKind::Multiplier => {
                if self.p5 < MULTIPLIER_MIN || self.p95 > MULTIPLIER_MAX {
                    return Err(format!(
                        "multiplier outside declared range [{MULTIPLIER_MIN}, {MULTIPLIER_MAX}]: \
                         p5={} p95={}",
                        self.p5, self.p95
                    ));
                }
            }
            AssertionKind::Probability => {
                if self.p5 < 0.0 || self.p95 > 1.0 {
                    return Err(format!(
                        "probability outside [0, 1]: p5={} p95={}",
                        self.p5, self.p95
                    ));
                }
            }
            AssertionKind::Quantity => {}
        }
        Ok(())
    }
}

/// One quantified judgement, as stored in `episodes.assertions`.
///
/// Immutable. Written once with the episode and never updated: it is what the
/// agent said at that moment. Verification lives in `assertion_verifications`
/// because it transitions, and a mutable status field here would destroy the
/// previous verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assertion {
    /// Minted here, not derived from array position.
    ///
    /// The one real cost of storing assertions flat in a JSONB array: position
    /// is not an identity, so reordering the array would silently repoint every
    /// verification. No foreign key can protect this, so `liveness_trust`
    /// checks it instead — an unresolvable verification is a finding.
    pub assertion_id: Uuid,
    pub kind: AssertionKind,
    pub value: Spread,
    /// What the agent reasoned from, as provenance verdicts.
    ///
    /// For a `Multiplier` this is the whole verification story: the multiplier
    /// itself is not a checkable proposition, so its standing is the floor over
    /// these. Empty means it named nothing, which floors at
    /// `unavailable_no_tool_source` rather than passing — the same empty-set
    /// inversion `grounding_trust::floor` guards against.
    pub basis: Vec<String>,
    pub extraction: ExtractionPath,
    /// The driver the agent seems to be talking about, if it said.
    ///
    /// A hint and not a binding: binding is the claim's job and requires a
    /// workspace. Recorded so a later bind does not have to re-guess.
    pub target_hint: Option<String>,
    /// Verbatim text the number came from, retained so a verdict can be
    /// re-derived rather than trusted. The same reasoning as
    /// `Violation.removed` and migration 202's `superseded_profile`.
    pub raw: Option<String>,
}

impl Assertion {
    /// The provenance this assertion is entitled to, before any verification.
    ///
    /// `min(basis floor, kind ceiling, path ceiling)` — three independent caps,
    /// and each one has been the difference between an honest verdict and a
    /// flattering one somewhere in this codebase.
    ///
    /// A `Quantity` with an empty basis is the interesting case: it purports to
    /// be a retrieval and names nothing, so it is exactly what the pending tier
    /// is for. It returns `pending_human_check` here — the honest default,
    /// upgraded to `pending_tool_check` by the caller when a field contract says
    /// a tool could have answered.
    pub fn entitled_provenance(&self) -> &'static str {
        let mut best = if self.basis.is_empty() {
            match self.kind {
                // Judgements from nothing: the agent was asked to reason and
                // cited no inputs. Not a retrieval claim, so not pending —
                // just ungrounded.
                AssertionKind::Multiplier | AssertionKind::Probability => PROV_UNAVAILABLE,
                // A measurement with no stated source is work to be done, not
                // an absence. This is the whole point of the pending tier: the
                // research is real and must not be discarded.
                AssertionKind::Quantity => PROV_PENDING_HUMAN,
            }
        } else {
            floor(self.basis.iter().map(|s| s.as_str()))
        };

        for cap in [self.kind.ceiling(), self.extraction.ceiling()] {
            if let Some(c) = cap {
                if strength(best) > strength(c) {
                    best = c;
                }
            }
        }
        best
    }

    /// Where an unverified assertion should be routed.
    ///
    /// `tool_available` comes from the field contract: `Grounding::Sourced`
    /// already names the tool and the response field, so the automated route is
    /// derivable rather than declared. That is the whole reason this costs
    /// nothing to wire.
    pub fn route(&self, tool_available: bool) -> Route {
        if !self.kind.is_verifiable() {
            return Route::InheritFromBasis;
        }
        if strength(self.entitled_provenance()) >= 2 {
            return Route::None;
        }
        if tool_available {
            Route::Automated
        } else {
            Route::Human
        }
    }
}

/// What to do about an assertion that is not yet standing on anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Already reproducible. Nothing to do.
    None,
    /// A tool can settle it, and the field contract names which.
    Automated,
    /// No tool can. A person must source it — and the same gap is a
    /// tool-integration request, which is the demand signal seen from the
    /// other side.
    Human,
    /// Not a checkable proposition. Verify what it reasoned from instead.
    InheritFromBasis,
}

impl Route {
    /// The verdict an unrouted assertion should carry while it waits.
    pub fn pending_verdict(self) -> Option<&'static str> {
        match self {
            Route::Automated => Some(PROV_PENDING_TOOL),
            Route::Human => Some(PROV_PENDING_HUMAN),
            Route::None | Route::InheritFromBasis => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding_trust::{
        PROV_HUMAN_ENDORSED, PROV_HUMAN_SOURCED, PROV_REJECTED, PROV_TOOL,
    };

    fn spread(p5: f64, p50: f64, p95: f64) -> Spread {
        Spread { p5, p50, p95 }
    }

    fn assertion(kind: AssertionKind, basis: &[&str], path: ExtractionPath) -> Assertion {
        Assertion {
            assertion_id: Uuid::new_v4(),
            kind,
            value: spread(0.6, 0.85, 1.15),
            basis: basis.iter().map(|s| s.to_string()).collect(),
            extraction: path,
            target_hint: None,
            raw: None,
        }
    }

    fn typed() -> ExtractionPath {
        ExtractionPath::TypedField {
            schema: "fermi/football_evidence".into(),
            field_path: "ratings.elo_current".into(),
        }
    }

    fn prose() -> ExtractionPath {
        ExtractionPath::Prose {
            pattern: "multiplier_v1".into(),
        }
    }

    #[test]
    fn a_multiplier_can_never_be_tool_verified_however_good_its_inputs() {
        // The load-bearing rule. No database contains "the multiplier for this
        // driver"; the agent is asked to produce one. A multiplier claiming
        // tool_verified would report a fact about its inputs as a fact about
        // its conclusion.
        let a = assertion(
            AssertionKind::Multiplier,
            &[PROV_TOOL, PROV_TOOL, PROV_TOOL],
            typed(),
        );
        assert_eq!(a.entitled_provenance(), PROV_INFERRED);
    }

    #[test]
    fn a_quantity_from_a_typed_tool_backed_field_reaches_the_top() {
        // And the converse, which is what makes the ceiling meaningful rather
        // than a blanket cap. If nothing could ever be tool_verified there
        // would be no gradient and no reason to emit structured output.
        let a = assertion(AssertionKind::Quantity, &[PROV_TOOL], typed());
        assert_eq!(a.entitled_provenance(), PROV_TOOL);
    }

    #[test]
    fn the_same_quantity_recovered_from_prose_is_only_an_inference() {
        // Identical basis, different path, different verdict. This is the
        // gradient: structured output earns a better standing than the same
        // number in a sentence, because the sentence records nothing about
        // where the number came from and the recovery drops 8 of 14.
        let a = assertion(AssertionKind::Quantity, &[PROV_TOOL], prose());
        assert_eq!(a.entitled_provenance(), PROV_INFERRED);
    }

    #[test]
    fn a_measurement_with_no_stated_source_is_pending_not_absent() {
        // The correction that motivated the pending tier. Stripping this would
        // destroy real research; calling it `unavailable` would say no tool
        // could ever answer, which is a different and usually false claim.
        let a = assertion(AssertionKind::Quantity, &[], typed());
        assert_eq!(a.entitled_provenance(), PROV_PENDING_HUMAN);
        assert_eq!(a.route(false), Route::Human);
        assert_eq!(a.route(true), Route::Automated);
        assert_eq!(a.route(true).pending_verdict(), Some(PROV_PENDING_TOOL));
    }

    #[test]
    fn a_judgement_that_cites_nothing_is_ungrounded_rather_than_pending() {
        // A multiplier reasoned from no stated inputs is not work waiting to be
        // done — there is nothing to check. Routing it to a person would ask
        // them to confirm a number that is not a proposition about the world,
        // and both possible answers pollute the rejection rate.
        let a = assertion(AssertionKind::Multiplier, &[], prose());
        assert_eq!(a.entitled_provenance(), PROV_UNAVAILABLE);
        assert_eq!(a.route(true), Route::InheritFromBasis);
    }

    #[test]
    fn a_multipliers_standing_is_the_floor_over_what_it_reasoned_from() {
        // Verification routes to the basis, so one unsourced input drags the
        // conclusion down however good the rest are. Nine measurements and one
        // guess is a guess.
        let a = assertion(
            AssertionKind::Multiplier,
            &[PROV_TOOL, PROV_TOOL, PROV_PENDING_HUMAN],
            typed(),
        );
        // And it names the actual weakest link, so the queue learns a human
        // check is owed rather than merely that the number is ungrounded.
        assert_eq!(a.entitled_provenance(), PROV_PENDING_HUMAN);
    }

    #[test]
    fn a_cited_human_check_lifts_a_quantity_and_an_uncited_one_does_not() {
        let cited = assertion(AssertionKind::Quantity, &[PROV_HUMAN_SOURCED], typed());
        assert_eq!(cited.entitled_provenance(), PROV_HUMAN_SOURCED);
        assert_eq!(cited.route(true), Route::None, "nothing left to check");

        let uncited = assertion(AssertionKind::Quantity, &[PROV_HUMAN_ENDORSED], typed());
        assert_eq!(uncited.entitled_provenance(), PROV_HUMAN_ENDORSED);
        assert_eq!(
            uncited.route(true),
            Route::Automated,
            "an uncited opinion has not settled a measurement; the tool still should run"
        );
    }

    #[test]
    fn a_rejected_basis_cannot_support_anything() {
        let a = assertion(
            AssertionKind::Multiplier,
            &[PROV_TOOL, PROV_REJECTED],
            typed(),
        );
        // `rejected`, not `unavailable`: a multiplier reasoning over an input
        // that was checked and disproven should be retracted rather than merely
        // distrusted, and only one of those verdicts says so.
        assert_eq!(a.entitled_provenance(), PROV_REJECTED);
    }

    #[test]
    fn a_reversed_spread_is_broken_rather_than_merely_wide() {
        // Unlike a fabricated value there is no honest reading to preserve:
        // p5 > p95 silently inverts every interval downstream.
        assert!(spread(1.5, 1.0, 0.5)
            .validate(AssertionKind::Multiplier)
            .is_err());
        assert!(spread(0.6, 0.85, 1.15)
            .validate(AssertionKind::Multiplier)
            .is_ok());
    }

    #[test]
    fn the_declared_multiplier_range_is_enforced() {
        // The card declares [0.1, 3.0]. A 12x multiplier is not a strong
        // signal, it is a misread of the format.
        assert!(spread(0.6, 4.0, 12.0)
            .validate(AssertionKind::Multiplier)
            .is_err());
        // ...and the same numbers are fine for a Quantity, which has no range.
        assert!(spread(0.6, 4.0, 12.0)
            .validate(AssertionKind::Quantity)
            .is_ok());
    }

    #[test]
    fn a_probability_outside_zero_to_one_is_rejected() {
        assert!(spread(0.1, 0.5, 1.4)
            .validate(AssertionKind::Probability)
            .is_err());
    }

    #[test]
    fn only_a_quantity_is_verifiable() {
        assert!(AssertionKind::Quantity.is_verifiable());
        assert!(!AssertionKind::Multiplier.is_verifiable());
        assert!(!AssertionKind::Probability.is_verifiable());
    }

    #[test]
    fn an_assertion_round_trips_through_json() {
        // It is stored as JSONB in episodes.assertions, so a serde change that
        // renamed a variant would silently orphan every historical row.
        let a = assertion(AssertionKind::Quantity, &[PROV_TOOL], typed());
        let s = serde_json::to_string(&a).unwrap();
        assert!(s.contains("\"kind\":\"quantity\""), "{s}");
        assert!(s.contains("\"path\":\"typed_field\""), "{s}");
        let back: Assertion = serde_json::from_str(&s).unwrap();
        assert_eq!(a, back);
    }
}
