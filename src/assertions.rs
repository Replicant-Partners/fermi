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
    /// A non-numeric claim that purports to come from somewhere —
    /// `taxonomy.order = "Coleoptera"`, a station code, a species name.
    ///
    /// # Why this kind had to exist
    ///
    /// The other three all carry a number, so [`Assertion::value`] was a
    /// [`Spread`] and a claim that was not a number could not be recorded at
    /// all. That excluded the single most important case this platform has:
    /// **`Antaxius beieri`**, a bush-cricket (Orthoptera / Tettigoniidae) whose
    /// profile confidently reported Coleoptera / Cerambycidae and called it a
    /// longhorn beetle. Every check passed — the field was present, non-null,
    /// correctly typed, and declared sourced — and the GBIF-verified answer sat
    /// one table over.
    ///
    /// So the claim most worth verifying was the one the verification queue
    /// could not carry. `grounding_trust` has a whole [`ViolationKind`] for it
    /// (`ContradictsCanonical`) and the queue had nowhere to put the result.
    ///
    /// Checkable, like [`AssertionKind::Quantity`] and for the same reason: it
    /// purports to be a retrieval, so there is a fact of the matter.
    ///
    /// [`ViolationKind`]: crate::grounding_trust::ViolationKind
    Fact,
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
    /// `Quantity` and `Fact` have no ceiling here: a real tool call can make
    /// either `tool_verified`, and a cited human check can make it
    /// `human_sourced`. Both purport to be retrievals, which is precisely what
    /// makes them capable of reaching the top of the ladder — and what makes a
    /// multiplier permanently incapable of it.
    pub fn ceiling(self) -> Option<&'static str> {
        match self {
            AssertionKind::Multiplier | AssertionKind::Probability => Some(PROV_INFERRED),
            AssertionKind::Quantity | AssertionKind::Fact => None,
        }
    }

    /// Can this kind of assertion be verified at all?
    ///
    /// Routing a multiplier into a human verification queue would ask a person
    /// to confirm a number that is not a proposition about the world. They
    /// would either rubber-stamp it or reject it on taste, and both outcomes
    /// pollute the rejection rate that makes the queue worth having.
    pub fn is_verifiable(self) -> bool {
        matches!(self, AssertionKind::Quantity | AssertionKind::Fact)
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

/// What an assertion asserts.
///
/// # Why `untagged`, and what that costs
///
/// `episodes.assertions` holds 94 live rows written as bare `{"p5":…,"p50":…,
/// "p95":…}` objects, plus 124 empty arrays. An externally tagged enum would
/// change that shape and every stored assertion would stop deserialising, so the
/// representation has to stay compatible — `untagged` keeps the existing bytes
/// meaning exactly what they meant.
///
/// The cost is real and is why [`Assertion::shape_is_consistent`] exists:
/// `untagged` **falls through silently**. A malformed spread — `{"p5":1,"p50":2}`
/// with no `p95` — does not fail to parse, it becomes a [`Claim::Literal`]
/// carrying an object. Nothing in serde will tell you. So the kind and the claim
/// are checked against each other, and a `Quantity` whose value arrived as a
/// literal is reported rather than quietly accepted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Claim {
    /// A `(p5, p50, p95)` triple. The shape every stored assertion has today.
    Numeric(Spread),
    /// Anything else the agent asserted, verbatim.
    ///
    /// Retained rather than stringified: `"2.4"` and `2.4` are different claims
    /// and a reviewer needs to see which was made.
    Literal(serde_json::Value),
}

impl Claim {
    /// The spread, when this is a numeric claim.
    ///
    /// Returns `Option` rather than panicking, and callers must handle the
    /// `None`. Every prose-extracted assertion is numeric so the `None` arm looks
    /// unreachable at those two call sites — which is exactly why it must not be
    /// an `unwrap`: the arm becomes reachable the moment a `Fact` reaches a
    /// binder, and a panic in a request handler is a worse answer than a skip.
    pub fn as_spread(&self) -> Option<Spread> {
        match self {
            Claim::Numeric(s) => Some(*s),
            Claim::Literal(_) => None,
        }
    }

    /// The claim as JSON, whichever shape it is.
    pub fn as_json(&self) -> serde_json::Value {
        match self {
            Claim::Numeric(s) => serde_json::json!({"p5": s.p5, "p50": s.p50, "p95": s.p95}),
            Claim::Literal(v) => v.clone(),
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
    /// Does the stated interval cover `value`?
    ///
    /// The test for "has the model landed somewhere this agent would recognise".
    /// Lives here rather than in the console so there is one reader of the rule and
    /// it can be tested: the same comparison is wanted by the analyst's panel, by a
    /// cross-check over stored forecasts, and eventually by the composer.
    ///
    /// The interval is the agent's OWN, which makes the tolerance self-calibrating.
    /// An agent that is candidly uncertain earns a wide one; a confident agent earns
    /// a narrow one. A flat "disagree above 10pp" would nag where 10pp is noise and
    /// stay silent where 3pp is a scandal.
    ///
    /// Inclusive at both ends: a model exactly on p5 is the weakest reading the
    /// agent explicitly allowed for, so it is agreement.
    pub fn contains(&self, value: f64) -> bool {
        value >= self.p5 && value <= self.p95
    }

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
            // A `Fact` never reaches here: its claim is a `Claim::Literal` and
            // this validates a `Spread`. Matched explicitly rather than with a
            // catch-all so that a fifth kind cannot acquire "no range check"
            // by default — which is how `Quantity` would have got one if the
            // arm above had been `_ => {}`.
            AssertionKind::Fact => {}
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
    pub value: Claim,
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
                AssertionKind::Quantity | AssertionKind::Fact => PROV_PENDING_HUMAN,
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

    /// Do the kind and the claim agree about what shape this is?
    ///
    /// # Why this is needed at all
    ///
    /// [`Claim`] is `#[serde(untagged)]`, because 94 stored assertions are bare
    /// `{"p5":…,"p50":…,"p95":…}` objects and an externally tagged enum would stop
    /// deserialising every one of them. The cost of `untagged` is that it **falls
    /// through silently**: a malformed spread — `{"p5":1,"p50":2}` with no `p95` —
    /// does not fail to parse, it quietly becomes a [`Claim::Literal`] carrying an
    /// object, and serde says nothing.
    ///
    /// That is the same defect class as everything else here: a true fact about a
    /// parse (`this did not match Spread`) becoming a false fact about the world
    /// (`the agent asserted an object`). The kind is the independent witness —
    /// `extract_from_prose` sets it from which pattern matched, not from the
    /// value — so comparing the two catches the fallthrough.
    ///
    /// Returns the problem rather than a bool, because *which* way they disagree
    /// decides the remedy: a numeric kind holding a literal is a broken spread, a
    /// `Fact` holding a spread is a mislabelled extraction.
    pub fn shape_is_consistent(&self) -> Result<(), String> {
        match (self.kind, &self.value) {
            (AssertionKind::Fact, Claim::Numeric(_)) => Err(format!(
                "kind is `Fact` and the claim is a spread. A `Fact` is the \
                 non-numeric kind, so either the extraction mislabelled it or a \
                 numeric claim was recorded under the wrong kind. \
                 assertion_id={}",
                self.assertion_id
            )),
            (kind, Claim::Literal(v)) if kind != AssertionKind::Fact => Err(format!(
                "kind is `{kind:?}` and the claim is not a spread: {v}. Because \
                 `Claim` is untagged this is what a MALFORMED SPREAD looks like \
                 after deserialisation — serde fell through rather than failing — \
                 so the likely cause is a missing or non-finite percentile rather \
                 than a deliberately non-numeric claim. assertion_id={}",
                self.assertion_id
            )),
            _ => Ok(()),
        }
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

// ─── contracted fields ─────────────────────────────────────────────────

/// Why a contracted field did not become an assertion.
///
/// Returned rather than logged, and counted by the caller. A field the queue
/// cannot represent is a gap in the queue's coverage, and the one thing that must
/// not happen is for it to be invisible: an empty queue that is empty because
/// nothing could be enqueued reads exactly like one that is empty because nothing
/// is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotEnqueued {
    pub path: &'static str,
    pub why: &'static str,
}

/// Turn one graded contracted field into an assertion the queue can key on.
///
/// # Why this is the half that fills the queue
///
/// All 94 assertions in production are 75 [`AssertionKind::Multiplier`] and 19
/// [`AssertionKind::Probability`] — **zero** [`AssertionKind::Quantity`]. Neither
/// of those is verifiable: [`Assertion::route`] sends them to
/// [`Route::InheritFromBasis`], correctly, because *you cannot verify a
/// multiplier.* So the prose extractor cannot produce a queue item however well
/// it works, and the queue's first real content has to come from contracted
/// **fields**, which purport to be retrievals and are therefore checkable.
///
/// # `TypedField`, constructed for the first time
///
/// [`ExtractionPath::TypedField`] has existed since this module was written, is
/// documented as *"the only path that can reach the top of the ladder"*, and
/// **nothing has ever built one.** Every assertion in production is
/// [`ExtractionPath::Prose`], capped at `model_inference`. That cap is what gives
/// the retrofit a gradient, and this is the path that rewards it.
///
/// `schema` records **which declaration the field was read against**, and for a
/// contracted field that is the field contract rather than a JSON Schema. Spelled
/// `contract:<agent>` so a reader can tell the two apart: a field read from a
/// published `output_contract.schema` and one read against a Rust const are
/// different acts of extraction, and preserving that difference is what
/// `ExtractionPath` is for.
///
/// # The numeric limit, stated rather than worked around
///
/// [`Assertion::value`] is a [`Spread`], so only a numeric field can become an
/// assertion. `taxonomy.order = "Coleoptera"` cannot — and it is a real claim
/// that was really wrong in the canonical `Antaxius beieri` case. Widening
/// `value` to carry a non-numeric claim would change a stored JSONB shape that 94
/// live rows and 124 empty arrays already use, so it is a declared follow-up
/// rather than something done quietly here. Until then those fields come back as
/// [`NotEnqueued`] and the caller counts them, so the queue's coverage is visible
/// instead of assumed.
///
/// A degenerate spread (`p5 == p50 == p95`) is the honest record of a retrieval:
/// the agent stated one number and claimed no interval, and inventing one would
/// put a range into the record that no agent asserted.
pub fn from_graded_field(
    agent_id: &str,
    f: &crate::grounding_trust::GradedField,
) -> Result<Assertion, NotEnqueued> {
    // Absent is not a claim. A contracted field the document never carried has
    // nothing to verify, and enqueuing it would fill the queue with the agent's
    // silence rather than its assertions.
    if f.value.is_null() {
        return Err(NotEnqueued {
            path: f.path,
            why: "the document did not carry this field, so there is no claim to \
                  verify. Absence is the contract's business, not the queue's.",
        });
    }
    // Non-numeric claims are `Fact`, and they are the reason `Claim` exists.
    // `taxonomy.order = "Coleoptera"` is checkable, was wrong in the
    // `Antaxius beieri` case, and used to be refused here because
    // `Assertion::value` could only hold a `Spread`.
    let Some(n) = f.value.as_f64() else {
        return Ok(Assertion {
            assertion_id: Uuid::new_v4(),
            kind: AssertionKind::Fact,
            value: Claim::Literal(f.value.clone()),
            basis: vec![f.provenance.to_string()],
            extraction: ExtractionPath::TypedField {
                schema: format!("contract:{agent_id}"),
                field_path: f.path.to_string(),
            },
            target_hint: None,
            raw: Some(f.value.to_string()),
        });
    };
    if !n.is_finite() {
        return Err(NotEnqueued {
            path: f.path,
            why: "the claim is a non-finite number, which `Spread::validate` \
                  refuses. Dropped rather than repaired, for the same reason a \
                  malformed spread is.",
        });
    }

    Ok(Assertion {
        assertion_id: Uuid::new_v4(),
        // A contracted field purports to be a retrieval — that is what makes it
        // checkable, and what makes it the only kind that can produce a queue
        // item. `Grounding::Inferred` fields are also `Quantity` here and are
        // filtered by `route`, not by mislabelling their kind.
        kind: AssertionKind::Quantity,
        value: Claim::Numeric(Spread {
            p5: n,
            p50: n,
            p95: n,
        }),
        // The block's grade, and the whole reason this is worth writing: a
        // `Quantity` with an EMPTY basis floors at `pending_human_check`
        // regardless of how well sourced its block was, so the basis is what
        // lets a tool-verified field be recognised as one.
        basis: vec![f.provenance.to_string()],
        extraction: ExtractionPath::TypedField {
            schema: format!("contract:{agent_id}"),
            field_path: f.path.to_string(),
        },
        // A contracted field names a field, not a driver. Binding to a driver is
        // the claim's job and needs a workspace.
        target_hint: None,
        raw: Some(f.value.to_string()),
    })
}

/// Every contracted field of one document that the queue can carry, and why the
/// rest could not.
///
/// The pair is the point. A caller that took only the assertions would report an
/// empty queue identically whether nothing was wrong or nothing was
/// representable, which is the distinction this whole surface exists to keep.
pub fn from_graded_fields(
    agent_id: &str,
    fields: &[crate::grounding_trust::GradedField],
) -> (Vec<Assertion>, Vec<NotEnqueued>) {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for f in fields {
        match from_graded_field(agent_id, f) {
            Ok(a) => out.push(a),
            Err(e) => skipped.push(e),
        }
    }
    (out, skipped)
}

/// Name of the prose pattern, recorded in [`ExtractionPath::Prose`].
///
/// Versioned because the pattern is part of the provenance: a v2 that recovers
/// more is a different act of extraction, and assertions recorded under v1
/// should stay attributable to what v1 could actually see.
pub const MULTIPLIER_PATTERN: &str = "multiplier_v2";

/// The `[MULTIPLIER]` line, as agents actually write it.
///
/// v1 was `p50:\s+([\d.]+)\s*\(p5:` — correct against the format the card
/// specifies and unable to read the format the model emits. Measured against
/// production: **12 of 22 lines unrecoverable**, every one of them because the
/// model wrapped the number in markdown emphasis (`**1.15**`, or the
/// asymmetric `1.15**`) since the surrounding response is markdown and this is
/// the sentence it most wants to stress.
///
/// The card calls the format MANDATORY and machine-parsed. It is neither, and
/// the instruction cannot make it so: asking a model to suppress emphasis on
/// the one line it considers the conclusion is asking it to be less like
/// itself. So the reader tolerates the emphasis instead.
///
/// Replayed against every `Suggested p50` line in production: v1 recovered 10 of
/// 22, v2 recovers **22 of 22 with nothing rejected**. Tolerating markdown is not
/// the fix, though — it moves the loss from 55% to whatever the next
/// unanticipated flourish costs. The fix is a typed field,
/// and this pattern's ceiling is [`PROV_INFERRED`] precisely so that emitting
/// one is worth more than writing a good sentence.
static MULTIPLIER_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)Suggested\s+p50:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*\(\s*p5:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*,\s*p95:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*\)",
    )
    .expect("MULTIPLIER_RE")
});

/// Has this exact judgement already been recovered from the same response?
///
/// # Why this is needed
///
/// `extract_from_prose` scans the whole `raw_response`, and an agent states its
/// conclusion in more than one place: once in the `key_findings` array, again in
/// the prose body, and again in a JSON restatement. Each appearance matched, and
/// each was recorded as a separate `Assertion` with its own `assertion_id`.
///
/// Measured over the production episode table: **64 multiplier rows across 31
/// episodes, and 31 distinct triples** — every episode holds exactly one
/// judgement, and 33 of the 64 rows (52%) are that judgement counted again.
/// Not one episode contains two different multipliers.
///
/// That mattered beyond storage. The count is what a caller uses to decide
/// whether an agent bound to N drivers said N things: a broker agent covering
/// five drivers and stating one adjustment looked, at the binding site, like an
/// agent that had stated three. Anything pairing assertions with `driver_refs`
/// would have stamped one number onto several drivers and compounded it.
///
/// # Why keyed on the value and not the text
///
/// The restatements are not byte-identical — `raw` differs by surrounding
/// markdown — but the triple is what gets bound, and two identical triples are
/// indistinguishable downstream. `target_hint` is part of the key so that when
/// an agent does start naming its target, the same number offered for two
/// different drivers survives as two claims. No agent populates it today, so
/// that arm is unreachable and deliberately so: it is the shape this becomes
/// correct in, not a guess about the present.
fn is_restatement(
    seen: &[Assertion],
    kind: AssertionKind,
    value: &Spread,
    target_hint: Option<&str>,
) -> bool {
    seen.iter().any(|a| {
        a.kind == kind
            && a.value.as_spread().as_ref() == Some(value)
            && a.target_hint.as_deref() == target_hint
    })
}

/// Every multiplier an agent stated in prose.
///
/// Returns **all** matches rather than the first. `agent_params_hook` took the
/// first and `break`, then applied that one triple to every driver the agent
/// covered — so `football_analyst`, asked for three separate factors, had one
/// number stamped onto `dynamic`, `squad` and `tactical` alike. Recording each
/// match separately is what lets three bindings of one judgement be told apart
/// from three judgements.
///
/// A malformed spread is **dropped, and the reason returned**, because an
/// unordered or out-of-range multiplier is not a wide estimate — it is a broken
/// one, and silently repairing it would put a number into a forecast that no
/// agent asserted.
pub fn extract_from_prose(text: &str) -> (Vec<Assertion>, Vec<String>) {
    let mut out: Vec<Assertion> = Vec::new();
    let mut rejected = Vec::new();

    for caps in MULTIPLIER_RE.captures_iter(text) {
        let parse = |i: usize| caps.get(i).and_then(|m| m.as_str().parse::<f64>().ok());
        let (Some(p50), Some(p5), Some(p95)) = (parse(1), parse(2), parse(3)) else {
            continue;
        };
        let value = Spread { p5, p50, p95 };
        if let Err(why) = value.validate(AssertionKind::Multiplier) {
            rejected.push(format!(
                "{why} (from {:?})",
                caps.get(0).map(|m| m.as_str())
            ));
            continue;
        }
        if is_restatement(&out, AssertionKind::Multiplier, &value, None) {
            continue;
        }
        out.push(Assertion {
            assertion_id: Uuid::new_v4(),
            kind: AssertionKind::Multiplier,
            value: Claim::Numeric(value),
            // Prose names no typed source, so there is nothing to inherit. An
            // uncited judgement is worth less than one reasoned from verified
            // inputs, and that gap is the gradient: cite structurally, or score
            // as ungrounded.
            basis: Vec::new(),
            extraction: ExtractionPath::Prose {
                pattern: MULTIPLIER_PATTERN.to_string(),
            },
            target_hint: None,
            raw: caps.get(0).map(|m| m.as_str().to_string()),
        });
    }

    (out, rejected)
}

/// Wire format for a stated PROBABILITY, as opposed to a ratio.
pub const PROBABILITY_PATTERN: &str = "probability_v1";

/// The `[PROBABILITY]` line.
///
/// ## Why a second channel was needed at all
///
/// `[MULTIPLIER]` carries a RATIO, and a ratio cannot carry a LEVEL. That is not a
/// stylistic complaint; it destroyed a real answer. On a live Chicago weather
/// market `weather_oracle` computed a bucket probability of 35% from a 103-member
/// ensemble. To cross this boundary it had to become `35% / 11.1% = 3.15x`, which
/// exceeded the driver's declared `[0.55, 1.75]`, so the agent emitted `1.00` — and
/// `1.00` is not "no opinion", it asserts that climatology is right. The model
/// stayed at its base rate of 6.7% and the panel reported a 40-point edge.
///
/// The level was destroyed AT THE INTERFACE. Nothing downstream could recover it,
/// because nothing downstream ever had it — which is also why no check caught it:
/// there was no surviving number to disagree with.
///
/// ## Why the platform declares the format
///
/// `AssertionKind::Probability` has existed since the assertion layer was written,
/// with a `[0,1]` range rule and its own provenance ceiling, and **nothing has ever
/// constructed one**. The kind was waiting for a producer. Declaring the wire
/// format here rather than describing it in an agent card is the point: the
/// platform states what a legal stated probability looks like, and a card conforms
/// to it. The reverse — inferring the contract from whatever prose an agent
/// happened to write — is how two drivers came to hold quantities from different
/// spaces.
///
/// ## Shape
///
/// ```text
/// [PROBABILITY] Calibrated p50: 0.35 (p5: 0.25, p95: 0.45) — one-sentence basis
/// ```
///
/// Anchored on the literal label, and using `Calibrated` where the multiplier line
/// uses `Suggested`, so the two cannot match each other's text. Both appear in the
/// same response, so "cannot collide" has to be true by construction rather than by
/// luck — `a_probability_line_is_not_read_as_a_multiplier` holds it.
///
/// Markdown emphasis is tolerated for the same reason the multiplier pattern
/// tolerates it: v1 of that regex lost 12 of 22 production lines to `**1.15**`,
/// because the surrounding response is markdown and this is the sentence a model
/// most wants to stress.
static PROBABILITY_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)\[PROBABILITY\][^\n]*?p50:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*\(\s*p5:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*,\s*p95:\s*\**\s*([0-9]*\.?[0-9]+)\s*\**\s*\)",
    )
    .expect("PROBABILITY_RE")
});

/// Every probability an agent stated in prose, as `AssertionKind::Probability`.
///
/// Deliberately a SEPARATE function from [`extract_from_prose`] rather than more
/// results from it. `agent_params_hook::apply_agent_multipliers` takes
/// `assertions.first()` and binds it to a driver as a multiplier; folding
/// probabilities into the same vector would let a `0.35` probability be bound as a
/// 0.35x multiplier, silently, on whichever line happened to appear first. Two
/// kinds in one channel is what caused the original defect and it would be a poor
/// way to fix it.
///
/// Out-of-range and unordered values are dropped with a reason, exactly as for
/// multipliers: a probability outside `[0,1]` is broken rather than merely
/// surprising, and repairing it would put a number into a forecast no agent stated.
pub fn extract_probabilities_from_prose(text: &str) -> (Vec<Assertion>, Vec<String>) {
    let mut out: Vec<Assertion> = Vec::new();
    let mut rejected = Vec::new();

    for caps in PROBABILITY_RE.captures_iter(text) {
        let parse = |i: usize| caps.get(i).and_then(|m| m.as_str().parse::<f64>().ok());
        let (Some(p50), Some(p5), Some(p95)) = (parse(1), parse(2), parse(3)) else {
            continue;
        };
        let value = Spread { p5, p50, p95 };
        if let Err(why) = value.validate(AssertionKind::Probability) {
            rejected.push(format!(
                "{why} (from {:?})",
                caps.get(0).map(|m| m.as_str())
            ));
            continue;
        }
        if is_restatement(&out, AssertionKind::Probability, &value, None) {
            continue;
        }
        out.push(Assertion {
            assertion_id: Uuid::new_v4(),
            kind: AssertionKind::Probability,
            value: Claim::Numeric(value),
            basis: Vec::new(),
            extraction: ExtractionPath::Prose {
                pattern: PROBABILITY_PATTERN.to_string(),
            },
            target_hint: None,
            raw: caps.get(0).map(|m| m.as_str().to_string()),
        });
    }

    (out, rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grounding_trust::{
        PROV_HUMAN_ENDORSED, PROV_HUMAN_SOURCED, PROV_NO_MATCH, PROV_REJECTED, PROV_TOOL,
    };
    use serde_json::json;

    fn spread(p5: f64, p50: f64, p95: f64) -> Spread {
        Spread { p5, p50, p95 }
    }

    fn assertion(kind: AssertionKind, basis: &[&str], path: ExtractionPath) -> Assertion {
        Assertion {
            assertion_id: Uuid::new_v4(),
            kind,
            value: Claim::Numeric(spread(0.6, 0.85, 1.15)),
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

    /// The two channels must not read each other's lines.
    ///
    /// Both appear in the same response, so this has to be true by construction
    /// rather than by luck. If a probability of `0.35` were read as a multiplier it
    /// would be bound to a driver as a 0.35x downward adjustment — a wrong number
    /// arriving silently, which is worse than the missing number this channel was
    /// added to fix.
    #[test]
    fn a_probability_line_is_not_read_as_a_multiplier() {
        let both = "[MULTIPLIER] Suggested p50: 1.00 (p5: 0.70, p95: 1.40) — no directional view\n\
                    [PROBABILITY] Calibrated p50: 0.35 (p5: 0.25, p95: 0.45) — ensemble bucket";

        let (mults, _) = extract_from_prose(both);
        assert_eq!(mults.len(), 1, "got {mults:?}");
        assert_eq!(mults[0].kind, AssertionKind::Multiplier);
        assert_eq!(mults[0].value.as_spread().unwrap().p50, 1.00);

        let (probs, _) = extract_probabilities_from_prose(both);
        assert_eq!(probs.len(), 1, "got {probs:?}");
        assert_eq!(probs[0].kind, AssertionKind::Probability);
        assert_eq!(probs[0].value.as_spread().unwrap().p50, 0.35);
    }

    /// The Chicago numbers, which is why this channel exists.
    ///
    /// `0.35` is a legal probability and an ILLEGAL multiplier under the platform
    /// floor of 0.1 — no, it is legal as a multiplier too, which is precisely the
    /// trap: routed through the ratio channel it would be accepted and mean
    /// something entirely different. The kinds have to be distinguished at
    /// extraction, not by range.
    #[test]
    fn the_level_that_the_ratio_channel_destroyed_survives_this_one() {
        let line = "[PROBABILITY] Calibrated p50: 0.35 (p5: 0.25, p95: 0.45) — \
                    103-member pooled ensemble, bias-corrected centre 79.3F";
        let (probs, rejected) = extract_probabilities_from_prose(line);
        assert!(rejected.is_empty(), "{rejected:?}");
        assert_eq!(probs.len(), 1);
        assert_eq!(probs[0].value.as_spread().unwrap().p50, 0.35);
        // Floors at `unavailable_no_tool_source`, NOT at `model_inference`.
        //
        // `PROV_INFERRED` is the CEILING this kind can reach; the floor is set by
        // what it reasoned from, and a prose line cites nothing structurally. So
        // recovering the level does not launder it into a grounded value — it is a
        // stated judgement, recorded as one, exactly as a prose multiplier is.
        // Capturing the number and inflating its standing would be a different bug
        // wearing this fix's clothes.
        assert_eq!(probs[0].entitled_provenance(), PROV_UNAVAILABLE);
    }

    /// The Chicago disagreement, as the panel now computes it.
    ///
    /// Model 5.91% against an agent stating 35% with a 25–45% interval. The point of
    /// the fixture is that the gap is not marginal — the model is nowhere near the
    /// weakest reading the agent allowed for — and a rule that needed tuning to
    /// catch it would be the wrong rule.
    #[test]
    fn a_model_outside_the_agents_own_interval_is_a_disagreement() {
        let stated = Spread {
            p5: 0.25,
            p50: 0.35,
            p95: 0.45,
        };
        assert!(!stated.contains(0.0591), "Chicago must be flagged");

        // Inside, and on both boundaries, is agreement.
        assert!(stated.contains(0.35));
        assert!(stated.contains(0.25));
        assert!(stated.contains(0.45));

        // A candidly uncertain agent earns a wide tolerance, and the same model
        // value is then NOT a disagreement. This is the property that keeps the
        // rule from nagging — and it is why the interval is the agent's own rather
        // than a constant.
        let humble = Spread {
            p5: 0.02,
            p50: 0.20,
            p95: 0.60,
        };
        assert!(humble.contains(0.0591));
    }

    /// Markdown emphasis, because that is what cost the multiplier pattern 12 of 22
    /// production lines and the same model writes both.
    #[test]
    fn emphasis_does_not_hide_a_stated_probability() {
        let (probs, _) = extract_probabilities_from_prose(
            "[PROBABILITY] Calibrated p50: **0.35** (p5: **0.25**, p95: 0.45)",
        );
        assert_eq!(probs.len(), 1, "got {probs:?}");
        assert_eq!(probs[0].value.as_spread().unwrap().p50, 0.35);
    }

    /// The format the CARD declares is the format the EXTRACTOR reads.
    ///
    /// This is the port contract between a prompt and a parser, and it is the exact
    /// place this platform has already been burned: v1 of `MULTIPLIER_RE` matched
    /// the shape the card specified and not the shape the model emitted, losing 12
    /// of 22 production lines. A card and a regex agreeing today is worth nothing
    /// unless something fails when they stop.
    ///
    /// Takes the template out of the card ON DISK and fills it, rather than
    /// restating the format here — restating it would create a third copy that can
    /// drift from both.
    #[test]
    fn the_probability_line_the_card_declares_is_the_one_this_module_parses() {
        let path = "agents/curated/weather_oracle/agent_card.json";
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            // Test runs from the workspace root; if the card is not there the
            // packaging changed and that is a separate problem.
            Err(e) => panic!("read {path}: {e}"),
        };
        let card: serde_json::Value = serde_json::from_str(&raw).expect("card is json");
        let prompt = card["system_prompt"].as_str().expect("system_prompt");

        let template = prompt
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("[PROBABILITY]"))
            .unwrap_or_else(|| panic!("the card no longer declares a [PROBABILITY] line at all"));

        // Fill the placeholders in the order the card writes them.
        let mut filled = template.to_string();
        for v in ["0.35", "0.25", "0.45"] {
            filled = filled.replacen("0.XX", v, 1);
        }
        assert!(
            !filled.contains("0.XX"),
            "template has more placeholders than expected: {template}"
        );

        let (probs, rejected) = extract_probabilities_from_prose(&filled);
        assert!(
            rejected.is_empty(),
            "the card's own declared line was rejected: {rejected:?} from {filled}"
        );
        assert_eq!(
            probs.len(),
            1,
            "the card declares a line this module cannot read: {filled}"
        );
        assert_eq!(probs[0].value.as_spread().unwrap().p50, 0.35);
        assert_eq!(probs[0].value.as_spread().unwrap().p5, 0.25);
        assert_eq!(probs[0].value.as_spread().unwrap().p95, 0.45);

        // ...and the multiplier extractor must not claim it, which is the property
        // that lets both lines live in one response.
        let (mults, _) = extract_from_prose(&filled);
        assert!(
            mults.is_empty(),
            "the card's probability line was also read as a multiplier: {mults:?}"
        );
    }

    /// A probability outside `[0,1]` is dropped with a reason, not clamped.
    #[test]
    fn an_impossible_probability_is_dropped_and_says_why() {
        let (probs, rejected) = extract_probabilities_from_prose(
            "[PROBABILITY] Calibrated p50: 1.20 (p5: 0.90, p95: 1.40)",
        );
        assert!(probs.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].contains("probability outside"), "{rejected:?}");
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

    /// The exact `[MULTIPLIER]` lines this platform has actually produced.
    ///
    /// Copied verbatim out of `episodes.response_text`, not invented. The first
    /// five are the ones v1 could not read, and they are the majority.
    const OBSERVED: &[&str] = &[
        "[MULTIPLIER] Suggested p50: **1.40** (p5: **1.20**, p95: **1.65**) — Argentina's institutional depth",
        "[MULTIPLIER] Suggested p50: **0.85** (p5: 0.60, p95: 1.15) — Saliba's absence and City's full-strength squad",
        "[MULTIPLIER] Suggested p50: 1.15** (p5: 0.75, p95: 1.65) — Man City playoff victory + Ancelotti",
        "[MULTIPLIER] Suggested p50: **1.00** (p5: 1.00, p95: 1.00) — **Season complete: Liverpool won",
        "[MULTIPLIER] Suggested p50: **0.70** (p5: 0.60, p95: 0.82) — Haaland's absence reduces",
        "[MULTIPLIER] Suggested p50: 1.85 (p5: 1.40, p95: 2.30) — Bayern's home fortress",
        "[MULTIPLIER] Suggested p50: 1.15 (p5: 1.05, p95: 1.28) — the format the card specifies",
    ];

    #[test]
    fn every_multiplier_line_this_platform_has_emitted_is_recoverable() {
        // The regression that matters. v1 read 10 of 22 in production; the
        // twelve it missed were all markdown emphasis. Fixtures are verbatim
        // rather than constructed, because a pattern tested only against the
        // format the card specifies is exactly how the 55% loss went unnoticed.
        for line in OBSERVED {
            let (found, rejected) = extract_from_prose(line);
            assert_eq!(
                found.len(),
                1,
                "could not recover a multiplier from an line this platform \
                 really produced: {line}\nrejected: {rejected:?}"
            );
        }
    }

    #[test]
    fn the_recovered_numbers_are_the_ones_that_were_written() {
        // Tolerating markdown must not mean absorbing a digit from it. `**1.15`
        // has to read as 1.15, never as 115 or 1.
        let (a, _) = extract_from_prose(OBSERVED[0]);
        assert_eq!(a[0].value.as_spread().unwrap().p50, 1.40);
        assert_eq!(a[0].value.as_spread().unwrap().p5, 1.20);
        assert_eq!(a[0].value.as_spread().unwrap().p95, 1.65);
    }

    #[test]
    fn three_factor_findings_yield_three_assertions_not_one() {
        // `agent_params_hook` took the FIRST match and `break`, then applied it
        // to every driver the agent covered — so football_analyst's three
        // factors became one number stamped on three drivers. Every match is
        // returned so three bindings of one judgement stay distinguishable from
        // three judgements.
        let text = format!("{}\n\n{}\n\n{}", OBSERVED[1], OBSERVED[5], OBSERVED[6]);
        let (found, _) = extract_from_prose(&text);
        assert_eq!(found.len(), 3);
        assert_ne!(found[0].assertion_id, found[1].assertion_id);
    }

    /// One judgement stated three times is one judgement.
    ///
    /// An agent repeats its conclusion: once in `key_findings`, again in the prose
    /// body, again in a JSON restatement. `extract_from_prose` reads the whole
    /// `raw_response`, so every appearance matched and every appearance was stored
    /// as its own `Assertion` with its own id.
    ///
    /// Measured over the production episode table before this landed: 64 multiplier
    /// rows across 31 episodes, 31 distinct triples. 33 rows — 52% — were the same
    /// number counted again, and not one episode held two different multipliers.
    ///
    /// The count is load-bearing, which is why this is a correctness bug and not
    /// housekeeping: a binder deciding whether an agent bound to five drivers said
    /// five things would have read three restatements as three judgements.
    #[test]
    fn one_judgement_restated_is_recovered_once() {
        let line = "[MULTIPLIER] Suggested p50: 0.99 (p5: 0.97, p95: 1.00)";
        let text =
            format!("key_findings: {line}\n\nIn summary, {line}\n\n{{\"assessment\": \"{line}\"}}");

        let (found, rejected) = extract_from_prose(&text);
        assert_eq!(
            found.len(),
            1,
            "the same triple appearing three times is one judgement, not three"
        );
        assert!(
            rejected.is_empty(),
            "a restatement is not a malformed claim"
        );
        assert_eq!(found[0].value.as_spread().unwrap().p50, 0.99);
    }

    /// Deduplication must not swallow a genuinely different second judgement.
    ///
    /// The guard against over-correcting: `football_analyst` stating three distinct
    /// factor adjustments must still yield three, which is what
    /// `three_factor_findings_yield_three_assertions_not_one` asserts from the
    /// other side. This pins the near-miss — same p50, different interval.
    #[test]
    fn two_different_judgements_both_survive() {
        let text = "[MULTIPLIER] Suggested p50: 1.20 (p5: 1.00, p95: 1.40)\n\
                    [MULTIPLIER] Suggested p50: 1.20 (p5: 0.80, p95: 1.60)";
        let (found, _) = extract_from_prose(text);
        assert_eq!(
            found.len(),
            2,
            "same centre, different interval — that is a different claim about \
             confidence and must not be folded away"
        );
    }

    /// The probability channel had the same duplication and gets the same rule.
    #[test]
    fn a_restated_probability_is_also_recovered_once() {
        let line = "[PROBABILITY] Calibrated p50: 0.35 (p5: 0.20, p95: 0.50)";
        let (found, _) = extract_probabilities_from_prose(&format!("{line}\n\n{line}"));
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_prose_multiplier_is_ungrounded_and_that_is_the_gradient() {
        // Uncited judgement scores below a judgement reasoned from verified
        // inputs. Without that gap there is no reason for any agent to ever
        // emit a typed field, and the retrofit has no incentive behind it.
        let (found, _) = extract_from_prose(OBSERVED[0]);
        assert_eq!(found[0].entitled_provenance(), PROV_UNAVAILABLE);
        assert_eq!(found[0].route(true), Route::InheritFromBasis);
    }

    #[test]
    fn a_broken_spread_is_dropped_and_says_why() {
        // Not repaired. Reordering p5 and p95 to make them fit would put a
        // number into a forecast that no agent asserted.
        let (found, rejected) =
            extract_from_prose("[MULTIPLIER] Suggested p50: 1.00 (p5: 2.00, p95: 0.50)");
        assert!(found.is_empty());
        assert_eq!(rejected.len(), 1);
        assert!(rejected[0].contains("not ordered"), "{:?}", rejected);
    }

    #[test]
    fn an_out_of_range_multiplier_is_dropped_rather_than_clamped() {
        let (found, rejected) =
            extract_from_prose("[MULTIPLIER] Suggested p50: 8.00 (p5: 5.00, p95: 12.00)");
        assert!(found.is_empty());
        assert!(rejected[0].contains("declared range"), "{:?}", rejected);
    }

    #[test]
    fn prose_with_no_multiplier_yields_nothing_rather_than_a_default() {
        let (found, rejected) = extract_from_prose(
            "Arsenal are 4W-1D-0L in their last 5 with an xGD of +2.1 over that run.",
        );
        assert!(found.is_empty());
        assert!(
            rejected.is_empty(),
            "a sentence with no claim is not a rejection"
        );
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

    // ── contracted fields ───────────────────────────────────────────────

    fn graded(
        path: &'static str,
        value: serde_json::Value,
        provenance: &'static str,
        tool: Option<&'static str>,
    ) -> crate::grounding_trust::GradedField {
        crate::grounding_trust::GradedField {
            path,
            block: path.split('.').next().unwrap_or(path),
            value,
            provenance,
            settleable_by: tool,
        }
    }

    /// The basis is what the write buys, and this is the measurement of it.
    ///
    /// A `Quantity` with an **empty** basis floors at `pending_human_check`
    /// however well sourced its block was — that is `entitled_provenance`'s
    /// documented behaviour and it is correct, because a measurement with no
    /// stated source is work to be done. So an assertion minted from a
    /// tool-verified field without carrying the block's grade would enqueue a
    /// person to go and re-check something a tool already answered.
    ///
    /// This pair is the whole argument for `from_graded_field` existing rather
    /// than the caller building an `Assertion` inline.
    #[test]
    fn carrying_the_blocks_grade_is_the_difference_between_verified_and_pending() {
        let f = graded(
            "form.xg_last_5",
            json!(1.83),
            PROV_TOOL,
            Some("call_football_api"),
        );
        let a = from_graded_field("football_analyst", &f).expect("a numeric claim enqueues");

        assert_eq!(a.basis, vec![PROV_TOOL.to_string()]);
        assert_eq!(
            a.entitled_provenance(),
            PROV_TOOL,
            "a tool-verified block must reach the top of the ladder; \
             `ExtractionPath::TypedField` is the only path that allows it"
        );
        assert_eq!(
            a.route(true),
            Route::None,
            "an already-reproducible claim must not be queued: a queue that \
             contains everything is not a queue"
        );

        // The counterfactual, stated so the value of the basis is measured and
        // not asserted. Same field, same value, basis dropped.
        let bare = Assertion {
            basis: vec![],
            ..a.clone()
        };
        assert_eq!(
            bare.entitled_provenance(),
            PROV_PENDING_HUMAN,
            "with the grade dropped the same claim becomes a work item for a \
             person, which is exactly the cost of not writing the basis"
        );
        assert_ne!(bare.entitled_provenance(), a.entitled_provenance());
    }

    /// A sourced field with nothing behind it routes to the tool that could
    /// settle it; an unsourced one routes to a person.
    ///
    /// The routing is **derived from the contract**, not declared a second time:
    /// `Grounding::Sourced { tool }` already names the tool, which is why this
    /// costs nothing to wire and why `settleable_by` is an `Option` rather than a
    /// separate flag. `null` is not merely "a person must do it" — per the paper
    /// it is also a prioritised request for the data integration that would close
    /// it, which is the same gap seen from the other side.
    #[test]
    fn the_route_comes_from_the_contract_and_not_from_a_second_declaration() {
        let sourced_but_empty = graded(
            "genome.estimated_size_mb",
            json!(2.4),
            PROV_NO_MATCH,
            Some("gbif_lookup"),
        );
        let a = from_graded_field("genome_profiler", &sourced_but_empty).expect("enqueues");
        assert_eq!(
            a.route(
                a.basis
                    .iter()
                    .any(|_| sourced_but_empty.settleable_by.is_some())
            ),
            Route::Automated,
            "a tool is declared for this field, so a person must not be asked"
        );

        let unsourced = graded(
            "genome.estimated_size_mb",
            json!(2.4),
            PROV_UNAVAILABLE,
            None,
        );
        let b = from_graded_field("genome_profiler", &unsourced).expect("enqueues");
        assert_eq!(
            b.route(unsourced.settleable_by.is_some()),
            Route::Human,
            "no tool can settle this, so it is a person's work and a tool \
             integration request at the same time"
        );
        assert_eq!(b.route(false).pending_verdict(), Some(PROV_PENDING_HUMAN));
        assert_eq!(a.route(true).pending_verdict(), Some(PROV_PENDING_TOOL));
    }

    /// Absence is not a claim.
    ///
    /// A contracted field the document never carried has nothing to verify.
    /// Enqueuing it would fill the queue with the agent's silence — and every
    /// contracted agent has more absent fields than present ones, so the queue
    /// would be mostly noise on arrival and abandoned.
    #[test]
    fn an_absent_field_is_not_a_queue_item() {
        let f = graded("genome.estimated_size_mb", json!(null), PROV_NO_MATCH, None);
        let e = from_graded_field("genome_profiler", &f).expect_err("absence must not enqueue");
        assert_eq!(e.path, "genome.estimated_size_mb");
        assert!(e.why.contains("did not carry"), "{}", e.why);
    }

    /// What the queue still cannot carry is refused **and counted**.
    ///
    /// This test used to assert that a non-numeric claim was refused, and that
    /// was the coverage gap: `taxonomy.order = "Coleoptera"` — the bush-cricket
    /// `Antaxius beieri` reported as a longhorn beetle — was the claim most worth
    /// verifying and the one shape the queue could not hold. `Claim::Literal`
    /// closed it, and `a_non_numeric_claim_becomes_a_verifiable_fact` now asserts
    /// the opposite.
    ///
    /// [`NotEnqueued`] is still live, and this pins what remains: a **non-finite**
    /// number. `Spread::validate` refuses those, and a claim that cannot survive
    /// its own validator must not be written — dropped rather than repaired, the
    /// same rule a malformed spread from prose gets, because there is no honest
    /// reading of `NaN` to preserve.
    ///
    /// It stays a returned value rather than a log line for the original reason:
    /// an empty queue that is empty because nothing could be enqueued reads
    /// identically to one that is empty because nothing is wrong.
    #[test]
    fn a_claim_the_queue_still_cannot_carry_is_refused_with_its_reason() {
        let f = graded(
            "genome.estimated_size_mb",
            json!(f64::INFINITY),
            PROV_TOOL,
            Some("gbif_lookup"),
        );
        // `serde_json` cannot even represent a non-finite float, so it arrives as
        // `null` — which is the absent case, and is refused for that reason.
        // Stated because it is worth knowing the two collapse into one here.
        let e = from_graded_field("genome_profiler", &f)
            .expect_err("a non-finite claim must not enqueue");
        assert!(
            e.why.contains("did not carry") || e.why.contains("non-finite"),
            "the refusal must say why, so the coverage gap stays visible: {}",
            e.why
        );

        // And the pair form keeps both halves: one enqueued, one refused.
        let (enqueued, skipped) = from_graded_fields(
            "genome_profiler",
            &[
                graded("genome.estimated_size_mb", json!(2.4), PROV_NO_MATCH, None),
                f,
            ],
        );
        assert_eq!(enqueued.len(), 1);
        assert_eq!(skipped.len(), 1);

        // The claim that used to be here is now carried, not skipped.
        let (enqueued, skipped) = from_graded_fields(
            "genome_profiler",
            &[graded(
                "taxonomy.order",
                json!("Coleoptera"),
                PROV_NO_MATCH,
                Some("gbif_lookup"),
            )],
        );
        assert_eq!(enqueued.len(), 1, "the Antaxius case must now enqueue");
        assert!(skipped.is_empty());
    }

    /// A contracted field is read as a `TypedField`, and that is load-bearing.
    ///
    /// `ExtractionPath::Prose` is capped at `model_inference`, so a contracted
    /// field mislabelled as prose could never be tool-verified however good its
    /// source — the cap would silently defeat the whole point of the contract.
    /// Every assertion in production today is `Prose`; this is the first
    /// `TypedField` the platform mints.
    #[test]
    fn a_contracted_field_is_a_typed_field_and_not_prose() {
        let f = graded("form.xg_last_5", json!(1.83), PROV_TOOL, Some("t"));
        let a = from_graded_field("football_analyst", &f).unwrap();
        match &a.extraction {
            ExtractionPath::TypedField { schema, field_path } => {
                assert_eq!(field_path, "form.xg_last_5");
                assert!(
                    schema.starts_with("contract:"),
                    "the schema must say which declaration the field was read \
                     against; a field contract is not a JSON Schema and the two \
                     must be distinguishable: {schema}"
                );
            }
            other => panic!(
                "a contracted field was recorded as {other:?}, which caps it at model_inference"
            ),
        }
        assert_eq!(a.extraction.ceiling(), None, "a typed field has no ceiling");
        assert_eq!(
            a.raw.as_deref(),
            Some("1.83"),
            "the claimed value must be retained verbatim: it is the only evidence \
             that could answer which model fabricates what, and a null cannot be \
             labelled"
        );
    }

    /// A malformed spread must not pass as a literal claim.
    ///
    /// The cost of `#[serde(untagged)]`, made visible. `{"p5":1,"p50":2}` has no
    /// `p95`, so it does not match `Spread` — and untagged serde does not fail,
    /// it falls through to `Claim::Literal` carrying the object. Nothing in serde
    /// reports it. Without `shape_is_consistent` a broken spread would be stored,
    /// read back, and rendered as though the agent had asserted a JSON object.
    #[test]
    fn a_malformed_spread_is_caught_rather_than_becoming_a_literal() {
        let json = r#"{
            "assertion_id": "00000000-0000-0000-0000-000000000001",
            "kind": "quantity",
            "value": {"p5": 1.0, "p50": 2.0},
            "basis": [],
            "extraction": {"path": "prose", "pattern": "multiplier_v2"},
            "target_hint": null,
            "raw": null
        }"#;
        let a: Assertion = serde_json::from_str(json).expect("untagged serde accepts this");
        assert!(
            matches!(a.value, Claim::Literal(_)),
            "serde did not fall through, so this test is no longer about the \
             hazard it was written for"
        );
        let problem = a
            .shape_is_consistent()
            .expect_err("a quantity holding a literal was accepted");
        assert!(
            problem.contains("MALFORMED SPREAD"),
            "the message must name the likely cause, or a reader chases the \
             wrong thing: {problem}"
        );
    }

    /// Every stored assertion shape still deserialises.
    ///
    /// The compatibility guarantee. 94 live rows and 124 empty arrays were written
    /// before `Claim` existed, as bare percentile objects, and an enum
    /// representation that changed those bytes would silently orphan all of them —
    /// `episodes.assertions` would read as an array of things that no longer parse.
    #[test]
    fn the_shape_already_in_the_database_still_reads() {
        let stored = r#"{
            "assertion_id": "00000000-0000-0000-0000-000000000002",
            "kind": "multiplier",
            "value": {"p5": 1.05, "p50": 1.15, "p95": 1.28},
            "basis": [],
            "extraction": {"path": "prose", "pattern": "multiplier_v2"},
            "target_hint": "home_advantage",
            "raw": "[MULTIPLIER] Suggested p50: 1.15"
        }"#;
        let a: Assertion = serde_json::from_str(stored).expect("a stored row must still parse");
        assert_eq!(a.kind, AssertionKind::Multiplier);
        assert_eq!(a.value.as_spread().map(|s| s.p50), Some(1.15));
        assert!(a.shape_is_consistent().is_ok());

        // And it round-trips to the same bytes, so nothing rewrites the column
        // into a shape an older reader would refuse.
        let back = serde_json::to_value(&a).unwrap();
        assert_eq!(
            back["value"],
            serde_json::json!({"p5": 1.05, "p50": 1.15, "p95": 1.28}),
            "the stored representation changed, so every row written from now on \
             is in a shape the previous release cannot read"
        );
    }

    /// The claim most worth verifying can now be recorded, and it routes.
    ///
    /// `Antaxius beieri`: a bush-cricket whose profile reported Coleoptera and
    /// called it a longhorn beetle. Present, non-null, correctly typed, declared
    /// sourced — every check passed. It could not previously become an assertion
    /// at all, so the verification queue's coverage gap was exactly the case the
    /// queue existed for.
    #[test]
    fn a_non_numeric_claim_becomes_a_verifiable_fact() {
        let f = graded(
            "taxonomy.order",
            json!("Coleoptera"),
            PROV_NO_MATCH,
            Some("gbif_lookup"),
        );
        let a =
            from_graded_field("genome_profiler", &f).expect("a non-numeric claim must now enqueue");

        assert_eq!(a.kind, AssertionKind::Fact);
        assert_eq!(a.value, Claim::Literal(json!("Coleoptera")));
        assert!(a.shape_is_consistent().is_ok());
        assert!(
            a.kind.is_verifiable(),
            "a Fact purports to be a retrieval, so there is a fact of the matter"
        );
        assert_eq!(
            a.kind.ceiling(),
            None,
            "a tool call can settle it, so it must be able to reach the top of \
             the ladder"
        );
        assert_eq!(
            a.route(true),
            Route::Automated,
            "`gbif_lookup` is named by the contract, so a person must not be asked"
        );
        assert_eq!(a.route(true).pending_verdict(), Some(PROV_PENDING_TOOL));
        assert_eq!(
            a.raw.as_deref(),
            Some("\"Coleoptera\""),
            "the claim is retained verbatim, quotes and all: `\"2.4\"` and `2.4` \
             are different claims and a reviewer must see which was made"
        );
    }
}
