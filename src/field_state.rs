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
    /// Every contract state, for a surface that has to render a legend.
    pub const ALL: &'static [Declared] = &[
        Declared::Resolved,
        Declared::Unresolvable,
        Declared::Pending,
        Declared::Derived,
        Declared::Inferred,
        Declared::Narrative,
    ];

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

    /// The same decision, from a [`GradedField`].
    ///
    /// A surface with a pulse in hand holds `GradedField`s rather than
    /// `Grounding`s: `graded_fields` has already resolved the contract and kept
    /// only [`GroundingKind`] and the tool name. Both carry everything this
    /// decision needs, and **the point of the second constructor is that
    /// neither caller has to write the mapping.**
    ///
    /// Without it, the artifact trace would need a five-arm match from
    /// `GroundingKind` to `Declared` — which is the fourth inline copy
    /// `no_surface_maps_grounding_to_a_state_itself` exists to forbid, one
    /// enum to the left.
    ///
    /// `the_two_constructors_agree_on_every_kind` pins the two together.
    pub fn of_graded(field: &GradedField, dispatchable: impl Fn(&str) -> bool) -> Self {
        match field.kind {
            GroundingKind::Sourced => match field.settleable_by {
                Some(tool) if dispatchable(tool) => Self::Resolved,
                // A sourced field whose tool cannot be dispatched, and one
                // whose contract somehow named none, are the same finding: no
                // retrieval can ever settle it.
                _ => Self::Unresolvable,
            },
            GroundingKind::Unsourced => Self::Pending,
            GroundingKind::Derived => Self::Derived,
            GroundingKind::Inferred => Self::Inferred,
            GroundingKind::Narrative => Self::Narrative,
        }
    }

    /// The same decision, from a **card's** compiled
    /// `output_contract.grounding` map.
    ///
    /// The third home a contract can have, and the reason the Team tab printed
    /// *"no contract — nothing about this member's output is checked"* over
    /// `supply_chain_oracle`, an agent the runtime strips and stamps at every
    /// hop. `FIELD_CONTRACTS` is Rust and per-field; a card's `grounding` map is
    /// JSON and per-block, with owned strings. [`Grounding`] cannot hold one —
    /// its `tool` is `&'static str` — so a surface reading a card cannot reach
    /// [`Declared::of`], and would have written the match a fourth time.
    ///
    /// **Two things this constructor cannot do, and a caller must not paper
    /// over.**
    ///
    /// A block is not a field. Counts from this constructor are not comparable
    /// with counts from [`Declared::of`]; a surface showing both must say which
    /// it counted.
    ///
    /// `unavailable` maps to [`Declared::Pending`]: both say nothing can supply
    /// the field, so it must be null and a value in it is the violation.
    ///
    /// ## Two card vocabularies, and this reads the wider one
    ///
    /// [`crate::card_contract::GROUNDING_STATUSES`] is what a BLOCK may
    /// declare; [`crate::card_contract::FIELD_GROUNDING_STATUSES`] adds
    /// `derived` for a field inside a refined block. This constructor accepts
    /// the wider set, deliberately: **reading is not gating.** Which token is
    /// legal where is `card_contract`'s judgement, made once at publish with
    /// the agent's identity in hand — `derived` is admitted only where
    /// `DERIVATIONS` computes or `CROSS_CHECKS` checks that exact
    /// `(agent_id, path)`, per
    /// [`crate::card_contract::validate_derived_declarations`]. Re-deciding it
    /// here would be a second gate with less evidence than the first, and the
    /// two would disagree.
    ///
    /// Returns `None` for a token in neither set. Bucketing an unrecognised
    /// status into a state is how a status nobody vetted acquires a trust
    /// verdict, so the caller should decline to count it rather than guess.
    pub fn of_card_status(
        status: &str,
        tool: Option<&str>,
        dispatchable: impl Fn(&str) -> bool,
    ) -> Option<Self> {
        match status {
            "sourced" => Some(match tool {
                Some(t) if dispatchable(t) => Self::Resolved,
                // Same finding as `of_graded`: a sourced block naming a tool
                // the platform cannot dispatch, and one naming none at all,
                // are both blocks no retrieval can ever settle.
                _ => Self::Unresolvable,
            }),
            "inferred" => Some(Self::Inferred),
            "narrative" => Some(Self::Narrative),
            "unavailable" => Some(Self::Pending),
            // Field-level only, and gated at publish rather than here.
            "derived" => Some(Self::Derived),
            _ => None,
        }
    }
}

/// What a contract's entries are counted in.
///
/// The platform stores declarations in two places at two granularities, and a
/// count is meaningless without saying which. `23` beside `3` reads as one
/// agent being eight times better specified; `23 fields` beside `3 blocks`
/// reads as what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    /// `FIELD_CONTRACTS`, the registered table. One entry per dotted path.
    Field,
    /// The agent card's compiled `output_contract.grounding`. One entry per
    /// top-level response block, which is several fields.
    Block,
}

impl Grain {
    pub fn token(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::Block => "block",
        }
    }
}

/// One declaration: where it lands in the document, what it can be trusted
/// about, and the tool that would settle it.
#[derive(Debug, Clone)]
pub struct DeclaredEntry {
    /// A dotted path (`Grain::Field`) or a block name (`Grain::Block`).
    pub path: String,
    pub state: Declared,
    /// The tool named by a `sourced` declaration, whether or not the platform
    /// can dispatch it. `None` for every other state.
    pub tool: Option<String>,
}

/// Everything the platform declares about one agent's output, from whichever
/// home holds it.
#[derive(Debug, Clone)]
pub struct ContractReading {
    pub entries: Vec<DeclaredEntry>,
    /// `None` when there is no contract in either home. Absent, not zero.
    pub grain: Option<Grain>,
}

impl ContractReading {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries per state token, for a surface that renders chips.
    pub fn counts(&self) -> std::collections::BTreeMap<&'static str, usize> {
        let mut counts = std::collections::BTreeMap::new();
        for e in &self.entries {
            *counts.entry(e.state.token()).or_default() += 1;
        }
        counts
    }

    /// Entries that name a tool, dispatchable or not.
    ///
    /// Both `Resolved` and `Unresolvable` count: the question this answers is
    /// "does this contract expect a retrieval", and a contract naming a tool
    /// the platform lost still expects one. Used to detect a prompt that
    /// removes the tool loop out from under a contract that needs it.
    pub fn expects_a_retrieval(&self) -> usize {
        self.entries
            .iter()
            .filter(|e| matches!(e.state, Declared::Resolved | Declared::Unresolvable))
            .count()
    }
}

/// **The one reader.** Every declaration the platform holds about one agent's
/// output, from whichever of the two homes holds it.
///
/// ## Why there are two homes
///
/// `grounding_trust::FIELD_CONTRACTS` is a Rust table, per dotted field path.
/// A card's compiled `output_contract.grounding` is JSON, per top-level block.
/// `docs/DESIGN_a2a_contracting.md` 7.6 calls the table *legacy for tiers 1
/// and 2* and *permanent for tier 3*: most of it should migrate onto cards.
/// Two things hold the rest back, and they are different in kind.
///
/// The SQL cross-checks are no longer one of them. They were welded to the
/// declaration as a `FieldContract` field, so migrating a declaration deleted
/// the platform's only falsifiable check of it; they now live in
/// `grounding_trust::CROSS_CHECKS`, keyed by `(agent_id, path)`, and the two
/// move independently.
///
/// What remains: a `Grounding::Derived` field cannot migrate, because it
/// asserts that PLATFORM code computes the value and
/// `card_contract::PLATFORM_ASSIGNED_ONLY` withholds the token so an author
/// cannot claim that about their own agent. And the card vocabulary is
/// per-block, so the ~105 dotted paths have nowhere to land until it grows a
/// per-field form.
///
/// Until that split is finished both homes are real, and **every consumer that
/// reads one of them is wrong.** Two already were: the workspace Team tab
/// printed "no contract - nothing about this member's output is checked" over
/// ten agents the runtime enforces at every hop, and the specimen page
/// reported those same agents as compiling cleanly over an empty field table,
/// because zero rows yield zero errors. This function exists so a third
/// consumer cannot repeat it.
///
/// ## Precedence
///
/// The table wins where it exists. This is the opposite of what the migration
/// direction suggests, and it is deliberate: it matches
/// `grounding_trust::enforce_from_output_contract`, where reversing it was
/// measured and *lost* enforcement. Seven agents declare in both homes, and
/// several have mixed blocks - `genome_profiler.genome` is `sourced` from
/// `ncbi_genome_search` while `genome.ploidy` beneath it is `Unsourced` - a
/// distinction the per-block card vocabulary cannot express. A surface that
/// preferred the card would show those agents as better grounded than they
/// are.
pub fn read_contract(
    agent_id: &str,
    output_contract: Option<&serde_json::Value>,
    dispatchable: impl Fn(&str) -> bool,
) -> ContractReading {
    let entries: Vec<DeclaredEntry> = crate::grounding_trust::contracts_for(agent_id)
        .map(|c| DeclaredEntry {
            path: c.path.to_string(),
            state: Declared::of(&c.grounding, &dispatchable),
            tool: match c.grounding {
                Grounding::Sourced { tool, .. } => Some(tool.to_string()),
                _ => None,
            },
        })
        .collect();
    if !entries.is_empty() {
        return ContractReading {
            entries,
            grain: Some(Grain::Field),
        };
    }

    let mut entries = Vec::new();
    let mut refined = false;
    if let Some(blocks) = output_contract
        .and_then(|oc| oc.get("grounding"))
        .and_then(|g| g.as_object())
    {
        for (block, spec) in blocks {
            // `<block>_provenance` keys are the platform's own stamps, written
            // by `enforce_from_grounding_map` rather than authored. Every card
            // that has them declares them `inferred`, so counting them would
            // report each contract at twice its real size with all of the
            // surplus in one state. Skipped on the same rule, and for the same
            // reason, as the enforcement path.
            if block.ends_with("_provenance") {
                continue;
            }
            let status = spec
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            // A block may REFINE itself into per-field declarations. When it
            // does, the refinements are the declared paths and the block's own
            // status is not one of them: it exists to stamp
            // `<block>_provenance` and is derived from the fields beneath it.
            //
            // This is the vocabulary extension the migration needs. A block is
            // one stamp over what can be several different kinds of claim, and
            // a single status over a mixed block is an OVERCLAIM, not a
            // rounding. `football_analyst.advanced_metrics` is the worked case:
            // declared `sourced` from `call_football_api` as a block, while
            // `xg` is retrieved, `xgd` is computed from it, and `ppda` is Opta
            // event data the tool will never carry. One stamp said all three
            // came back from the API.
            if let Some(fields) = spec.get("fields").and_then(|f| f.as_object()) {
                for (field, fspec) in fields {
                    let status = fspec
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default();
                    let tool = fspec.get("tool").and_then(|s| s.as_str());
                    if let Some(state) = Declared::of_card_status(status, tool, &dispatchable) {
                        entries.push(DeclaredEntry {
                            path: format!("{block}.{field}"),
                            state,
                            tool: tool.map(str::to_string),
                        });
                        refined = true;
                    }
                }
                continue;
            }

            let status = spec
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or_default();
            let tool = spec.get("tool").and_then(|s| s.as_str());
            // `None` for a token no author may declare. Skipped rather than
            // bucketed: a status nobody vetted must not acquire a trust
            // verdict on its way to a screen.
            if let Some(state) = Declared::of_card_status(status, tool, &dispatchable) {
                entries.push(DeclaredEntry {
                    path: block.clone(),
                    state,
                    tool: tool.map(str::to_string),
                });
            }
        }
    }

    // A card that refines any block is declaring per PATH, which is the same
    // unit `FIELD_CONTRACTS` uses -- and `FIELD_CONTRACTS` is itself mixed:
    // `football_analyst` declares the bare block `league_context` beside the
    // dotted `advanced_metrics.xg`. So the grain is a property of the finest
    // declaration present, not of every row.
    let grain = if entries.is_empty() {
        None
    } else if refined {
        Some(Grain::Field)
    } else {
        Some(Grain::Block)
    };
    ContractReading { entries, grain }
}

impl Observed {
    /// Every run state, for a surface that has to render a legend.
    ///
    /// Exposed rather than left to the tests so a legend cannot be written by
    /// hand and then quietly fall behind the enum — which is how a page came to
    /// explain a state no row could print and omit one that several did.
    pub const ALL: &'static [Observed] = &[
        Observed::Filled,
        Observed::AbsentByContract,
        Observed::ToolEmpty,
        Observed::Owed,
        Observed::Stripped,
    ];

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
                "a value came back. What it is worth depends on the contract \
                 state beside it — a filled `resolved` field came from a tool, \
                 a filled `inferred` one is the agent's judgement"
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

/// **How bad is it** — the axis [`Observed::whose`] cannot carry.
///
/// `whose()` answers *attribution* and buckets `Owed | Stripped` together as
/// `"the agent's"`. Both are the agent's and **only one is damaging**, so
/// attribution cannot decide the colour. Conflating the two is what produced
/// the defect this enum exists to fix: a user read one trace and reported it as
/// *"a bunch of rejection"* when the artifact was deliverable and the only
/// fault-shaped event on the page was the smallest number on it.
///
/// Four findings were rendered in one red and they are four different kinds of
/// thing:
///
/// | what the page said | what it was |
/// |---|---|
/// | header badge `VIOLATIONS` | a **fault** — the model asserted what it could not know |
/// | `1 of 8 owed`, tone hardcoded | a **shortfall** — commissioned work not delivered |
/// | `11 ◌` in the fault colour | mostly **capability gaps** and **compliance** |
/// | `records only` | a **ceiling**, not a finding at all |
///
/// # The standing rule this encodes
///
/// > **Red is reserved for faults.** Shortfall is amber. Capability gap,
/// > compliance and delivery are neutral.
///
/// [`Finding::tone`] is the **one producer** of that colour (house rule 8).
/// A surface that hardcodes a tone from a count — `empty > 0 ? "bad"` — has
/// made a second, weaker copy of this decision, and the weaker copy is the one
/// that paints a shortfall as a defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finding {
    /// A value came back. Nothing to report.
    Delivered,
    /// Empty because the contract requires it. The contract **working**.
    Compliance,
    /// The named tool was asked and the world had nothing. Nobody's fault, and
    /// a real gap: it is the prioritised request for an integration.
    CapabilityGap,
    /// Commissioned work that did not come back. The agent's, and a matter of
    /// **ambition rather than legality** — the artifact is still deliverable.
    Shortfall,
    /// The model wrote a value the contract forbids. The one fault-shaped
    /// event: an assertion the agent could not have known.
    Fault,
}

impl Observed {
    /// How bad this state is, separately from whose it is.
    ///
    /// Total and injective over the two damaging states: `Owed` and `Stripped`
    /// are both `"the agent's"` under [`Observed::whose`] and **must not** share
    /// a `Finding`. `a_shortfall_and_a_fault_are_not_the_same_finding` is that
    /// invariant.
    pub fn finding(self) -> Finding {
        match self {
            Self::Filled => Finding::Delivered,
            Self::AbsentByContract => Finding::Compliance,
            Self::ToolEmpty => Finding::CapabilityGap,
            Self::Owed => Finding::Shortfall,
            Self::Stripped => Finding::Fault,
        }
    }
}

impl Finding {
    pub fn token(self) -> &'static str {
        match self {
            Self::Delivered => "delivered",
            Self::Compliance => "compliance",
            Self::CapabilityGap => "capability_gap",
            Self::Shortfall => "shortfall",
            Self::Fault => "fault",
        }
    }

    /// The colour, and the **only** place it is decided.
    ///
    /// Three tones, and exactly one of them is red. Deliberately not five: a
    /// tone per finding would let a surface distinguish compliance from a
    /// capability gap by colour, and the two call for the same reaction from a
    /// reader looking at a strip — neither is anybody's defect.
    pub fn tone(self) -> &'static str {
        match self {
            // Delivered is neutral, not green. Green means zero *errors*, and
            // painting every filled field green makes a document of filled
            // fields look verified when nothing has verified it.
            Self::Delivered | Self::Compliance | Self::CapabilityGap => "neutral",
            Self::Shortfall => "amber",
            Self::Fault => "red",
        }
    }

    /// How much worse than neutral, for a surface taking a worst-of over a set.
    ///
    /// Served so that "the tone of this row is the worst tone among its fields"
    /// is not a client-side ordering of the platform's vocabulary. A surface
    /// that ranks these itself has made a verdict out of a presentation detail,
    /// which is how `weakSourced > 0 ? "bad"` came to smuggle a verdict into a
    /// description of where numbers came from.
    pub fn rank(self) -> u8 {
        match self {
            Self::Delivered | Self::Compliance | Self::CapabilityGap => 0,
            Self::Shortfall => 1,
            Self::Fault => 2,
        }
    }

    /// Said once, keyed by the token the rows print.
    pub fn why(self) -> &'static str {
        match self {
            Self::Delivered => {
                "a value came back. Neutral rather than green on purpose: that a \
                 field is filled says nothing about whether the value is right, \
                 and green here would make a document of filled fields look \
                 verified when nothing has verified it"
            }
            Self::Compliance => {
                "empty because the contract requires it. Not a gap — the \
                 contract working"
            }
            Self::CapabilityGap => {
                "the tool was asked and the world had nothing. Nobody's fault, \
                 and a prioritised request for the integration that would close \
                 it"
            }
            Self::Shortfall => {
                "commissioned work that did not come back. The agent's, and a \
                 question of ambition rather than legality — the artifact is \
                 still deliverable, and pruning the contract to make this \
                 disappear would delete the ambition the contract exists to \
                 record"
            }
            Self::Fault => {
                "the model asserted what it could not have known. The one \
                 fault-shaped event a field can carry, and the only one that \
                 earns red"
            }
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
    const FINDING_ALL: &[Finding] = &[
        Finding::Delivered,
        Finding::Compliance,
        Finding::CapabilityGap,
        Finding::Shortfall,
        Finding::Fault,
    ];

    /// **The amendment, made impossible to undo.**
    ///
    /// `whose()` buckets `Owed` and `Stripped` together as `"the agent's"`.
    /// Both are the agent's; only one is damaging. If they ever share a
    /// `Finding`, the colour is back on the attribution axis and a shortfall is
    /// painted as a fault again — which is the whole of the reported defect:
    /// *"omissions not hallucinations … the error is agent completeness vs
    /// aspiration, not failure in the sense of producing untrusted data."*
    #[test]
    fn a_shortfall_and_a_fault_are_not_the_same_finding() {
        assert_eq!(
            Observed::Owed.whose(),
            Observed::Stripped.whose(),
            "the premise of this test has changed: these two shared an \
             attribution, which is why a second axis was needed at all"
        );
        assert_ne!(
            Observed::Owed.finding(),
            Observed::Stripped.finding(),
            "`owed` and `stripped` map to one finding. They are both the \
             agent's and only one is damaging: a value the agent owed and did \
             not deliver is a shortfall against its own ambition, and a value \
             the model asserted without a source is a fault. Painting them the \
             same is the defect this enum exists to fix."
        );
        assert_ne!(
            Observed::Owed.finding().tone(),
            Observed::Stripped.finding().tone(),
            "the two findings differ and their tones do not, so the page still \
             cannot tell them apart"
        );
    }

    /// **Red is reserved for faults.** The standing rule, as an assertion.
    ///
    /// Exactly one finding is red and it is the fabrication-shaped one. This is
    /// what stops the smallest number on the page — the single event that should
    /// alarm a reader — being drowned by two larger numbers that both mean
    /// "the agent did not do everything it hoped to".
    #[test]
    fn exactly_one_finding_is_red_and_it_is_the_fault() {
        let red: Vec<&str> = FINDING_ALL
            .iter()
            .filter(|f| f.tone() == "red")
            .map(|f| f.token())
            .collect();
        assert_eq!(
            red,
            vec!["fault"],
            "red is reserved for faults and these carry it: {red:?}. A \
             shortfall is amber; a capability gap and compliance are neutral. \
             Widening red is how a deliverable artifact came to read as \
             a bunch of rejection."
        );
        // And the ordering a surface takes a worst-of over agrees with the
        // tone, or a row can be amber while containing a red field.
        assert!(
            Finding::Fault.rank() > Finding::Shortfall.rank(),
            "a fault does not outrank a shortfall, so a worst-of over a set of \
             fields can report the shortfall and hide the fault"
        );
        assert_eq!(
            Finding::Delivered.rank(),
            Finding::Compliance.rank(),
            "delivery and compliance rank differently, which lets a row of \
             compliant absences read as worse than a row of values — and the \
             contract requiring an absence is the contract working"
        );
    }

    /// Every `Observed` maps to exactly one `Finding`, and every `Finding` is
    /// reachable.
    ///
    /// Totality is what the compiler gives us. **Surjectivity is not**: a
    /// `Finding` no `Observed` produces is a colour the legend explains and no
    /// row can ever print, which is how a legend comes to describe a state the
    /// platform retired.
    #[test]
    fn every_finding_is_reachable_from_some_observed_state() {
        for f in FINDING_ALL {
            assert!(
                OBSERVED_ALL.iter().any(|o| o.finding() == *f),
                "no run state produces `{}`, so the legend explains a colour no \
                 row can print",
                f.token()
            );
        }
        // Five states, five findings, and the map is injective — so the token a
        // row prints and the colour it wears cannot come apart.
        let mut seen: Vec<&str> = OBSERVED_ALL.iter().map(|o| o.finding().token()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(
            seen.len(),
            OBSERVED_ALL.len(),
            "two run states share a finding. That is allowed in principle and \
             is not what this vocabulary does today: if it becomes true, decide \
             deliberately which pair collapses and say so here, because the \
             pair that must never collapse is `owed` and `stripped`."
        );
    }

    /// The two `Declared` constructors agree, on every kind.
    ///
    /// `of` reads a `Grounding` and `of_graded` a `GradedField`, and the second
    /// exists so the artifact trace does not have to write a five-arm match
    /// from `GroundingKind` — which would be the fourth inline copy, one enum
    /// to the left of the one the scan already forbids.
    ///
    /// Two constructors for one decision is only safe while they cannot
    /// disagree, so this walks every kind through both.
    #[test]
    fn the_two_constructors_agree_on_every_kind() {
        let cases: &[(Grounding, GroundingKind, Option<&'static str>)] = &[
            (
                Grounding::Sourced {
                    tool: "call_football_api",
                    response_field: "x",
                },
                GroundingKind::Sourced,
                Some("call_football_api"),
            ),
            (Grounding::Unsourced, GroundingKind::Unsourced, None),
            (
                Grounding::Derived {
                    from: "taxonomy.order",
                    how: "a closed table",
                },
                GroundingKind::Derived,
                None,
            ),
            (
                Grounding::Inferred {
                    from: "taxonomy and proximity",
                },
                GroundingKind::Inferred,
                None,
            ),
            (Grounding::Narrative, GroundingKind::Narrative, None),
        ];

        // Both worlds: the tool dispatches, and it does not. `Resolved` versus
        // `Unresolvable` is the one arm where the closure changes the answer,
        // so testing only one world would leave it uncovered.
        for dispatchable in [true, false] {
            for (grounding, kind, tool) in cases {
                let mut f = field("a.b", *kind, serde_json::Value::Null);
                f.settleable_by = *tool;
                assert_eq!(
                    Declared::of(grounding, |_| dispatchable),
                    Declared::of_graded(&f, |_| dispatchable),
                    "the two constructors disagree about {kind:?} when \
                     dispatchable={dispatchable}. Two producers of one decision \
                     is the defect this module exists to end; the second \
                     constructor is only safe while it cannot say something \
                     different."
                );
            }
        }
    }

    /// Every token a card author may write resolves to a state, and every
    /// token they may not write resolves to nothing.
    ///
    /// Read off `card_contract` rather than retyped, because the whole failure
    /// mode here is a third vocabulary: if `GROUNDING_STATUSES` grows a token
    /// and `of_card_status` does not learn it, every block declaring it goes
    /// silently uncounted and the agent reads *less* contracted than it is —
    /// which is exactly how the Team tab came to call `supply_chain_oracle`
    /// untyped.
    #[test]
    fn every_card_status_maps_to_a_state_and_nothing_else_does() {
        use crate::card_contract::{
            FIELD_GROUNDING_STATUSES, GROUNDING_STATUSES, PLATFORM_ASSIGNED_ONLY,
        };

        for status in GROUNDING_STATUSES {
            assert!(
                Declared::of_card_status(status, Some("web_search"), |_| true).is_some(),
                "`{status}` is a publishable grounding status with no \
                 `Declared` state. A block declaring it would be dropped from \
                 every trust count, and an agent that declared nothing but \
                 `{status}` would render as having no contract at all."
            );
        }

        // The field vocabulary is wider, and every token in it must also map,
        // for the same reason: a refined block whose field declares a
        // publishable token that reaches no state is a declaration silently
        // dropped from every count.
        for status in FIELD_GROUNDING_STATUSES {
            assert!(
                Declared::of_card_status(status, Some("web_search"), |_| true).is_some(),
                "`{status}` is publishable on a refined field and reaches no \
                 `Declared` state."
            );
        }

        // A platform-assigned disposition is not *unconditionally* authorable.
        // It is admitted at field level under a gate, so the rule this asserts
        // is narrower than it used to be: the token must be refused at BLOCK
        // level, and gated rather than free at field level.
        //
        // This constructor deliberately does not enforce that split — see its
        // doc comment. Reading is not gating, and a second gate here would
        // have less evidence than `validate_derived_declarations`, which knows
        // the agent_id.
        for (token, _why) in PLATFORM_ASSIGNED_ONLY {
            assert!(
                !GROUNDING_STATUSES.contains(token),
                "`{token}` is in PLATFORM_ASSIGNED_ONLY and also blocklevel- \
                 declarable. A block-level `derived` stamps a whole block \
                 platform-derived on the author's word alone."
            );
            assert!(
                FIELD_GROUNDING_STATUSES.contains(token),
                "`{token}` is excused as platform-assigned and is not in the \
                 field vocabulary either, so nothing can ever declare it and \
                 the exemption describes a token with no home."
            );
        }

        assert!(
            Declared::of_card_status("estimated", None, |_| true).is_none(),
            "an unrecognised status must not be bucketed into a state. \
             `estimated` is the specific one card_contract refuses at publish, \
             and giving it a trust verdict here is the fabrication arriving \
             through the renderer instead of the model."
        );
    }

    /// The card constructor agrees with `of` wherever both can speak.
    ///
    /// Third producer of one decision; same rule as
    /// `the_two_constructors_agree_on_every_kind`. The card map cannot express
    /// `derived`, so that arm is absent by construction rather than untested —
    /// `every_card_status_maps_to_a_state_and_nothing_else_does` pins it.
    #[test]
    fn the_card_constructor_agrees_with_the_runtime_one() {
        let cases: &[(&str, Option<&'static str>, Grounding)] = &[
            (
                "sourced",
                Some("call_football_api"),
                Grounding::Sourced {
                    tool: "call_football_api",
                    response_field: "x",
                },
            ),
            (
                "inferred",
                None,
                Grounding::Inferred {
                    from: "taxonomy and proximity",
                },
            ),
            ("narrative", None, Grounding::Narrative),
            // The one mapping that is a judgement rather than a rename:
            // `unavailable` and `Unsourced` are the same claim — nothing can
            // supply this, so it must be null — said in two vocabularies.
            ("unavailable", None, Grounding::Unsourced),
        ];

        for dispatchable in [true, false] {
            for (status, tool, grounding) in cases {
                assert_eq!(
                    Declared::of_card_status(status, *tool, |_| dispatchable),
                    Some(Declared::of(grounding, |_| dispatchable)),
                    "`{status}` and {grounding:?} are the same declaration in \
                     two vocabularies and must produce the same state \
                     (dispatchable={dispatchable})."
                );
            }
        }

        // A sourced block naming no tool at all is unsettleable, matching
        // `of_graded`'s treatment of `settleable_by: None`.
        assert_eq!(
            Declared::of_card_status("sourced", None, |_| true),
            Some(Declared::Unresolvable)
        );
    }

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
            .chain(FINDING_ALL.iter().map(|f| (f.token(), f.why())))
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

    /// **Every surface that prints a contract state reads it from here.**
    ///
    /// Three surfaces show a field's contract state: the specimen page, the
    /// workspace Team tab, and (via the trace) the artifact page. Before this
    /// module the specimen had the five-way match inline and the trace had its
    /// own words, and they drifted — `unsourced` meaning a declared kind on one
    /// and a violation on the other.
    ///
    /// A fourth copy is the natural next step for anyone adding a panel: the
    /// match is six lines and inlining it is faster than finding this module. So
    /// the call sites are scanned, and a hand-rolled mapping is what the scan
    /// looks for.
    ///
    /// Deliberately not a check that they mention `field_state` — a file can
    /// import it and still spell its own tokens beside it, which is exactly how
    /// two vocabularies coexisted in one codebase for as long as they did.
    #[test]
    fn no_surface_maps_grounding_to_a_state_itself() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // The files that render contract states today. Named rather than
        // globbed: the population is small and a glob would either miss a
        // template or flag every file that mentions a token in prose.
        //
        // Paired with the constructor each one is obliged to call, because they
        // do not all print the same clock. The specimen page and the Team tab
        // describe a contract and have no pulse in hand, so they read
        // `Declared`. The artifact trace has the retained bytes and reads
        // `Observed` — and it is the surface the whole module was built for, so
        // leaving it unscanned would have exempted the one that drifted.
        //
        // The two `Declared` surfaces are required to call `read_contract`
        // rather than `Declared::of` directly. That is a STRENGTHENING, not a
        // relaxation: `read_contract` still produces the state through
        // `Declared::of`, and additionally reads BOTH places a contract can be
        // stored. Calling `Declared::of` straight was not wrong about the
        // vocabulary — it was wrong about the population, and both surfaces
        // reported an enforced agent as uncontracted because of it. One
        // producer of the verdict was never the whole requirement; one reader
        // of the evidence is the other half.
        const SURFACES: &[(&str, &str)] = &[
            ("src/handlers/specimen.rs", "field_state::read_contract"),
            (
                "src/handlers/workspace/core.rs",
                "field_state::read_contract",
            ),
            ("src/artifact_trace.rs", "field_state::Observed::of"),
        ];

        for (rel, required) in SURFACES {
            let body =
                std::fs::read_to_string(repo.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
            let code: String = body
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");

            assert!(
                code.contains(required),
                "{rel} renders field states and does not call `{required}`. \
                 One producer of the verdict, or the surfaces drift — which is \
                 what put `unsourced` on two pages meaning two things."
            );

            // A hand-rolled mapping is a `Grounding::` match beside a state
            // token. Either alone is innocent; together they are a second copy.
            let matches_grounding = code.contains("Grounding::Unsourced")
                || code.contains("Grounding::Narrative")
                || code.contains("Grounding::Inferred");
            let spells_tokens = code.contains("\"pending\"") || code.contains("\"narrative\"");
            assert!(
                !(matches_grounding && spells_tokens),
                "{rel} matches on `Grounding::` AND spells a state token. That \
                 is the five-way match inlined again beside the shared one, and \
                 the two will disagree the first time a state is added."
            );
        }
    }
}
