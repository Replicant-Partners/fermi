//! # HUD contract — the agent boundary for a glanceable card
//!
//! Fourth consumer of the provenance vocabulary in [`crate::grounding_trust`],
//! and the first one whose output a person reads in half a second while
//! walking. That changes what enforcement has to do.
//!
//! | Contract | Question |
//! | --- | --- |
//! | `schema_trust` | Is the column **present**? |
//! | `rollup_trust` | Is the column **telling the truth**? |
//! | `grounding_trust` | **Could this value have come from anywhere?** |
//! | `hud_contract` | **Can the wearer SEE which answer is which?** |
//!
//! `grounding_trust` already nulls a field nothing could have supplied and
//! stamps `<block>_provenance`. That is sufficient for an API consumer, which
//! can read the tag. It is not sufficient for a heads-up display, because a
//! tag in JSON that renders as identical text on glass is a tag nobody reads.
//! The `genome_profiler` incident was a fabricated value in a data field; the
//! HUD equivalent is a correctly-tagged value rendered indistinguishably from
//! a verified one, which reproduces the same harm through the presentation
//! layer instead of the data layer.
//!
//! So this module owns three jobs `grounding_trust` deliberately does not:
//!
//! 1. **Treatment.** Every rendered line carries a typographic marker derived
//!    from its provenance ([`Treatment`]). Not a colour — see below.
//! 2. **Subject conditioning.** A lookup keyed on a guess is not a retrieval.
//!    See [`conditioned`].
//! 3. **Computed confidence.** `card.confidence_display` is overwritten from
//!    the measured floor, never accepted from the model. See [`enforce`].
//!
//! ## Why the treatment is typographic and not chromatic
//!
//! The obvious way to distinguish a sourced field from a guess is colour. It
//! is not available. Rokid's own AIUI design system ships
//! `design/monochrome/design-system-green.md` describing the target hardware
//! as reproducing "one luminous green channel over pure black", with the
//! full-colour design tokens marked planned and unauthored. A field report on
//! the same optics notes that assets not pre-filtered to the green channel
//! render black.
//!
//! A confidence signal encoded as colour on a monochrome panel is a
//! confidence signal that does not exist. So the markers in [`Treatment`] are
//! leading ASCII glyphs, which survive a single-channel panel, a text-to-
//! speech fallback, and a log file.
//!
//! ## Relationship to the requested four-value vocabulary
//!
//! The specification for this work asked for `SOURCED | DERIVED | UNCLEAR |
//! UNSOURCED`. The platform already has a closed five-value set in
//! [`crate::grounding_trust::PROVENANCE_VALUES`], asserted by tests on both
//! the Rust constants and every card's declared enums. Minting a second
//! vocabulary would give the platform two provenance channels, and the one
//! that nothing checks is the one a fabrication moves to — which is the
//! documented reason `grounding_trust` scans narrative prose at all.
//!
//! So the wire vocabulary stays the platform's five values, and the requested
//! four are a display alias over them ([`spec_word`]):
//!
//! | Requested | Platform value | |
//! | --- | --- | --- |
//! | `SOURCED` | `tool_verified` | a declared tool returned it |
//! | `DERIVED` | `platform_derived` | reproducible computation over a sourced value |
//! | `UNCLEAR` | `tool_no_match` | the tool was asked and had nothing |
//! | `UNSOURCED` | `unavailable_no_tool_source` | no tool could supply it |
//! | — | `model_inference` | **has no slot in the requested set** |
//!
//! That last row is the one place this module declines to follow the spec as
//! written, and the reason is load-bearing rather than pedantic. The requested
//! set has four values and the platform emits five. Folding `model_inference`
//! into `DERIVED` would erase the distinction `grounding_trust` was extended
//! to preserve: a derivation is reproducible and auditable, a model judgement
//! is neither. Folding it into `UNSOURCED` would null the output of every
//! agent whose product *is* a judgement. Neither is available, so
//! `model_inference` keeps its own display word, `INFERRED`, and the set the
//! HUD renders has five members. This is stated rather than silently
//! reconciled because a vocabulary that quietly grew a member is exactly the
//! drift `no_card_declares_a_provenance_value_the_runtime_cannot_emit` exists
//! to catch.
//!
//! ## What this module does not do
//!
//! It does not talk to any glasses. Capture (layer 1) and the phone relay
//! (layer 2) are not implemented and not stubbed — see
//! `docs/specs/HUD_AGENT_LAYERS.md` for what is unresolved about the
//! transport and why writing it now would be guesswork.

use serde_json::{json, Value};

use crate::card_contract::Finding;
use crate::grounding_trust::{
    self, LeakRule, PROVENANCE_VALUES, PROV_DERIVED, PROV_HUMAN_ENDORSED, PROV_HUMAN_SOURCED,
    PROV_INFERRED, PROV_NO_MATCH, PROV_PENDING_HUMAN, PROV_PENDING_TOOL, PROV_REJECTED, PROV_TOOL,
    PROV_UNAVAILABLE,
};

// ─── glanceability budget ──────────────────────────────────────────────

/// Maximum title length. The wearer is reading this while their eyes are
/// mostly somewhere else.
pub const TITLE_MAX: usize = 40;

/// Maximum length of one rendered line, marker included. Chosen so a line
/// plus its marker still fits the narrow binocular text column without the
/// renderer choosing its own wrap point — a wrapped line silently becomes
/// two, which breaks [`MAX_LINES`] downstream of the check.
pub const LINE_MAX: usize = 60;

/// Maximum number of lines on one card.
///
/// A budget, not a preference. Past roughly this many rows the wearer stops
/// reading top-to-bottom and starts sampling, and a sampled card is one where
/// the flagged line is the one that gets skipped.
pub const MAX_LINES: usize = 5;

// ─── confidence bands ──────────────────────────────────────────────────

/// Everything on the card came from a tool or a reproducible computation.
pub const CONF_HIGH: &str = "high";
/// The weakest thing on the card is a model judgement.
pub const CONF_MEDIUM: &str = "medium";
/// A tool was consulted and returned nothing for this subject.
pub const CONF_LOW: &str = "low";
/// Something on the card has no possible source. The wearer must see this.
pub const CONF_FLAGGED: &str = "flagged";

/// Every value `card.confidence_display` may take.
pub const CONFIDENCE_VALUES: &[&str] = &[CONF_HIGH, CONF_MEDIUM, CONF_LOW, CONF_FLAGGED];

/// Key [`enforce`] writes onto a document it had to correct, listing the paths
/// it cleared.
///
/// The counterpart of [`crate::grounding_trust::PRE_CONTRACT_MARKER`], and it
/// exists for the same reason: a guarantee that lives only in the return value
/// lasts exactly one pass. A card is cached, re-read, and re-enforced, and by
/// then the fabricated field it once contained is null — indistinguishable from
/// a field that was honestly empty all along. Without this marker the second
/// pass rates the corrected card *higher* than the first did, which is the
/// un-stripping trap `grounding_trust` documents, arriving as a confidence band
/// instead of as a value.
pub const REVIEW_MARKER: &str = "_hud_review";

/// Confidence band for a provenance verdict.
///
/// `tool_no_match` bands to `low` rather than `flagged` deliberately: the tool
/// was asked and honestly had nothing, which is a weaker claim than a
/// retrieval but a stronger epistemic position than a field nothing can ever
/// fill. Collapsing the two would make an unsequenced species look exactly
/// like an unanswerable question.
pub fn confidence_for(verdict: &str) -> &'static str {
    match verdict {
        // Reproducible: a tool, a deterministic computation, or a human verdict
        // carrying a citation someone else can follow to the same source.
        PROV_TOOL | PROV_DERIVED | PROV_HUMAN_SOURCED => CONF_HIGH,
        // A judgement. `human_endorsed` sits here rather than above because
        // `grounding_trust` puts it at the same strength as a model inference:
        // an uncited human opinion and a model's judgement are the same kind of
        // claim, and ranking one higher because a person typed it is the
        // deference that module exists to remove.
        PROV_INFERRED | PROV_HUMAN_ENDORSED => CONF_MEDIUM,
        // Asked and empty, or queued and not yet asked. Both are recoverable —
        // a better frame, or a check that has not run yet.
        PROV_NO_MATCH | PROV_PENDING_TOOL | PROV_PENDING_HUMAN => CONF_LOW,
        // `rejected` means checked and found wrong, `unavailable` means nothing
        // can check it. Different facts, same band: neither may be relied on.
        // Anything unrecognised lands here too — an unrecognised verdict is
        // worthless rather than trusted.
        _ => CONF_FLAGGED,
    }
}

// ─── treatment ─────────────────────────────────────────────────────────

/// How one line is rendered so its provenance is visible without reading a
/// tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    /// A tool returned it, platform code computed it reproducibly, or a person
    /// verified it against a citation.
    Verified,
    /// A judgement — the agent's, or an uncited human endorsement.
    Inferred,
    /// The tool was consulted and had nothing for this subject.
    NoMatch,
    /// A check exists and has not run yet.
    ///
    /// Distinct from [`Treatment::NoMatch`] and [`Treatment::Unavailable`]
    /// because the wearer's next move differs: a pending value may simply be
    /// waiting, where a no-match wants a better frame and an unavailable wants a
    /// person.
    Pending,
    /// Checked, and found wrong.
    ///
    /// Has its own marker rather than sharing `Unavailable`'s. "Someone looked
    /// and this is incorrect" and "nothing can tell us" are opposite epistemic
    /// positions that happen to share a reliance score, and collapsing them on
    /// the one surface a person actually reads would throw away the more
    /// actionable of the two.
    Rejected,
    /// Nothing could supply it.
    Unavailable,
}

impl Treatment {
    /// The leading glyph. ASCII, because the panel is single-channel green and
    /// the same string has to survive TTS and a log file.
    ///
    /// `Verified` gets no marker: the unmarked case must be the trustworthy
    /// one, so that a marker always means "read this more carefully" and a
    /// renderer that drops markers degrades to *less* confident rather than
    /// more.
    pub fn marker(self) -> &'static str {
        match self {
            Treatment::Verified => "",
            Treatment::Inferred => "~ ",
            Treatment::NoMatch => "? ",
            Treatment::Pending => "* ",
            Treatment::Rejected => "x ",
            Treatment::Unavailable => "! ",
        }
    }

    /// A word for the legend, for TTS, and for the case where a renderer
    /// cannot show glyphs at all.
    pub fn word(self) -> &'static str {
        match self {
            Treatment::Verified => "sourced",
            Treatment::Inferred => "inferred",
            Treatment::NoMatch => "no match",
            Treatment::Pending => "not checked yet",
            Treatment::Rejected => "checked, wrong",
            Treatment::Unavailable => "not available",
        }
    }
}

// ─── the prose channel ─────────────────────────────────────────────────

/// Block whose absence of evidence the [`SAFETY_LEAKS`] needles would
/// contradict.
pub const SAFETY_BLOCK: &str = "edibility";

/// Prose claims that only a sourced [`SAFETY_BLOCK`] could support.
///
/// The narrative field matters more here than anywhere else in the platform,
/// for a reason specific to this hardware: prose is what goes to the speakers,
/// and the audio channel has no markers. Every distinction [`Treatment`] draws
/// is typographic, so a claim that survives into spoken output arrives with no
/// provenance signal at all. The one channel that cannot show a caveat is the
/// one the wearer is most likely to be using while their hands are full.
///
/// Scoped to this module rather than appended to
/// [`crate::grounding_trust::NARRATIVE_LEAKS`] deliberately. That table is
/// keyed by block name alone, and `block_is_sourced` returns `None` for an
/// agent that has no such block — so a needle like `"edible"` filed under
/// `edibility` would fire on every agent already under contract, including
/// `prey_locator`, whose tactical prose about predation could legitimately use
/// that exact word. Their own comment on the `" gb"`/`"GBIF"` collision is the
/// governing precedent: a check that fires on correct output gets switched
/// off, and the switching-off looks like cleanup. Agent-scoping the shared
/// table is the better fix; it is a change to a table four other agents
/// depend on, so it is noted as follow-up rather than smuggled in here.
pub const SAFETY_LEAKS: &[LeakRule] = &[
    LeakRule::Word("edible"),
    LeakRule::Word("poisonous"),
    LeakRule::Word("toxic"),
    LeakRule::Word("safe to eat"),
    LeakRule::Word("safe for consumption"),
    LeakRule::Word("deadly"),
    LeakRule::Word("choice edible"),
];

/// Treatment for a provenance verdict.
///
/// Every member of [`PROVENANCE_VALUES`] is named explicitly. The `_` arm exists
/// only for a value this module has not been taught, and
/// `every_provenance_value_is_explicitly_handled` fails when one appears — the
/// vocabulary grew from five members to ten during this module's first week, and
/// a silent fallback meant `human_sourced` rendered as "not available", which
/// makes a reviewer's cited verification invisible on the only surface that
/// matters.
pub fn treatment(verdict: &str) -> Treatment {
    match verdict {
        PROV_TOOL | PROV_DERIVED | PROV_HUMAN_SOURCED => Treatment::Verified,
        PROV_INFERRED | PROV_HUMAN_ENDORSED => Treatment::Inferred,
        PROV_NO_MATCH => Treatment::NoMatch,
        PROV_PENDING_TOOL | PROV_PENDING_HUMAN => Treatment::Pending,
        PROV_REJECTED => Treatment::Rejected,
        PROV_UNAVAILABLE => Treatment::Unavailable,
        _ => Treatment::Unavailable,
    }
}

/// The requested spec vocabulary as a display alias over the platform's five
/// values. See the module docs for why there are five words and not four.
pub fn spec_word(verdict: &str) -> &'static str {
    match verdict {
        PROV_TOOL => "SOURCED",
        PROV_DERIVED => "DERIVED",
        PROV_HUMAN_SOURCED => "HUMAN_SOURCED",
        PROV_INFERRED => "INFERRED",
        PROV_HUMAN_ENDORSED => "HUMAN_ENDORSED",
        PROV_NO_MATCH => "UNCLEAR",
        PROV_PENDING_TOOL | PROV_PENDING_HUMAN => "PENDING",
        PROV_REJECTED => "REJECTED",
        _ => "UNSOURCED",
    }
}

// ─── the display ordinal ───────────────────────────────────────────────

/// How much a verdict is worth *for display*, as a total order.
///
/// This refines [`crate::grounding_trust::strength`] rather than reusing it, and
/// the difference is narrow but load-bearing.
///
/// `strength` has three levels and puts `tool_no_match`,
/// `unavailable_no_tool_source`, `pending_*` and `rejected` all at 0, on the
/// stated grounds that they "describe the lack of a value rather than the
/// strength of one". For deciding **how much to rely on** a value that is
/// exactly right, and this module does not disagree with it.
///
/// A display has a second question to answer that reliance does not cover:
/// **what should the wearer do next.** "GBIF was asked and had nothing for this
/// name" means try a better frame. "No tool can ever answer this" means ask a
/// person. Equal reliance, opposite next actions — and the brief fixes four
/// confidence bands, so the display needs four ranks where reliance needs
/// three.
///
/// Hence `low` (asked-and-empty, or queued) sitting above `flagged`
/// (unanswerable, or disproven). Note that
/// [`crate::grounding_trust::floor`] *does* now preserve which verdict was
/// weakest — it canonicalises rather than collapsing to
/// `unavailable_no_tool_source`, which it did when this module was first
/// written. So the two are no longer in tension; this one simply grades on a
/// finer scale, and only for choosing a glyph and a band.
pub fn band_rank(verdict: &str) -> u8 {
    match verdict {
        PROV_TOOL | PROV_DERIVED | PROV_HUMAN_SOURCED => 3,
        PROV_INFERRED | PROV_HUMAN_ENDORSED => 2,
        PROV_NO_MATCH | PROV_PENDING_TOOL | PROV_PENDING_HUMAN => 1,
        // unavailable, rejected, and anything unrecognised.
        _ => 0,
    }
}

/// Weakest verdict in `verdicts` by [`band_rank`], or
/// `unavailable_no_tool_source` when there are none.
///
/// An empty iterator returning the strongest value is the single most common
/// way a floor calculation silently inverts, so it returns the weakest.
pub fn weakest<'a>(verdicts: impl IntoIterator<Item = &'a str>) -> &'static str {
    let mut worst: Option<(u8, &'static str)> = None;
    for v in verdicts {
        let canon = leak_static(v);
        let r = band_rank(canon);
        if worst.is_none_or(|(wr, _)| r < wr) {
            worst = Some((r, canon));
        }
    }
    worst.map_or(PROV_UNAVAILABLE, |(_, v)| v)
}

// ─── subject conditioning ──────────────────────────────────────────────

/// The weaker of two verdicts, keeping the block's own kind on a tie.
///
/// This is the rule that stops a HUD card lying by composition. Every lookup
/// this agent performs is keyed on a subject it *guessed* — from a voice
/// transcript, or from a model's reading of a captured frame. GBIF's answer
/// for *Amanita phalloides* is tool-verified in the sense that GBIF really
/// said it; it is not verified that the thing in front of the wearer is an
/// *Amanita phalloides*. Rendering the taxonomy as `sourced` would present a
/// retrieval about a possibly-wrong subject with the treatment reserved for
/// things the platform can stand behind.
///
/// This is the same defect as `Antaxius beieri` — a bush-cricket profiled as a
/// cerambycid beetle with every check green, because `Sourced` asserted that a
/// tool *could* supply the field and nothing compared the subject. There the
/// fix was a cross-check against an independently-held record. Here there is
/// no independently-held record of what the wearer is pointing at, so the
/// honest move is not to claim more than the weakest link supports.
///
/// Uses [`band_rank`] rather than [`crate::grounding_trust::floor`] for two
/// reasons, neither of which is disagreement about reliance:
///
/// 1. **Finer scale.** `floor` grades on `strength`, which cannot separate
///    `tool_no_match` from `unavailable_no_tool_source`. Those need different
///    glyphs, so the cap has to be taken on the display ordinal or the
///    distinction is lost before rendering.
/// 2. **Tie direction.** On equal rank this keeps the *block's* verdict; `floor`
///    keeps the first it encounters. When a block and the subject are equally
///    weak but differently shaped, the block's own verdict is the one that
///    describes what is actually on the card.
///
/// Worth stating plainly because `floor` changed underneath this module once
/// already: it used to collapse every strength-0 verdict to
/// `unavailable_no_tool_source` and now canonicalises instead. If it later grows
/// a display-grade ordinal of its own, this function should be deleted rather
/// than kept in parallel — two implementations of one trust rule is two answers
/// to one question, and the one that disagrees is whichever is nearest the
/// writer.
pub fn conditioned(subject: &str, block: &str) -> &'static str {
    let (s, b) = (leak_static(subject), leak_static(block));
    // `<=` so a tie keeps the block's own verdict: the cap only ever replaces
    // a claim that outranks it.
    if band_rank(b) <= band_rank(s) {
        b
    } else {
        s
    }
}

/// Is every contract for this block `Unsourced` — a gap nothing can ever fill?
///
/// Such a block reporting itself empty is a *correct* answer, not a defect, and
/// that distinction has to reach the confidence band. `edibility` is
/// permanently unsourced for `hud_field_scout` by design, so counting it toward
/// the floor would make every card this agent ever produced read `flagged`. A
/// band that never varies carries no information, and worse, it would make
/// `flagged` the normal case — at which point a genuinely alarming card, one
/// with an invented verdict or an untagged line, becomes indistinguishable from
/// a routine one.
///
/// That is the overreach their own contract warns about in
/// `does_not_touch_the_one_phylogeny_field_that_is_real`: a check that fires on
/// correct output gets switched off, and the switching-off looks like cleanup.
/// The gap still shows on the card, every time, with the `!` marker — what it
/// does not do is claim the card's assertions are less trustworthy than they
/// are.
fn is_declared_gap(agent_id: &str, block: &str) -> bool {
    let mut any = false;
    for c in grounding_trust::contracts_for(agent_id).filter(|c| block_of(c.path) == block) {
        any = true;
        if c.grounding != grounding_trust::Grounding::Unsourced {
            return false;
        }
    }
    any
}

// ─── report ────────────────────────────────────────────────────────────

/// Outcome of enforcing the HUD contract on one response.
#[derive(Debug, Default)]
pub struct HudReport {
    /// What the grounding contract stripped and stamped underneath.
    pub grounding: grounding_trust::Report,
    /// Everything wrong with the card, phrased for whoever has to fix it.
    pub findings: Vec<Finding>,
    /// Weakest verdict across every block the contract covers, after subject
    /// conditioning. `None` when the agent declares no grounding contract, in
    /// which case nothing is known — which must never read as clean.
    pub floor: Option<&'static str>,
    /// The band actually written to `card.confidence_display`.
    pub confidence_display: &'static str,
    /// Did this document have to be corrected, now or on an earlier pass?
    ///
    /// Sticky, because it is read back from [`REVIEW_MARKER`] rather than from
    /// this pass's violations.
    pub corrected: bool,
}

impl HudReport {
    /// Did the card pass without a single finding?
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.grounding.is_clean()
    }
}

fn finding(check: &'static str, message: impl Into<String>) -> Finding {
    Finding {
        check,
        message: message.into(),
    }
}

/// Top-level block a dotted contract path belongs to.
fn block_of(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

// ─── enforcement ───────────────────────────────────────────────────────

/// Enforce the HUD contract on an agent response, in place.
///
/// Runs [`grounding_trust::enforce`] first — so ungrounded fields are already
/// nulled and `<block>_provenance` already stamped — then does the three
/// things a display boundary has to do that a data boundary does not.
///
/// **Fails closed everywhere.** A line with no provenance tag is treated as
/// [`PROV_UNAVAILABLE`] and reported, never defaulted to something clean. An
/// agent with no grounding contract gets `flagged`, because silence about
/// grounding is not evidence of grounding.
pub fn enforce(agent_id: &str, doc: &mut Value) -> HudReport {
    let mut report = HudReport::default();

    if !doc.is_object() {
        report.confidence_display = CONF_FLAGGED;
        report.findings.push(finding(
            "hud_response_is_an_object",
            "The response is not a JSON object, so it has no fields to carry \
             provenance and no card to render. A HUD card is a typed document; \
             prose cannot be labelled field-by-field and must not be rendered \
             as though it had been.",
        ));
        return report;
    }

    // 1. The data boundary. Ungrounded fields nulled, blocks stamped.
    report.grounding = grounding_trust::enforce(agent_id, doc);

    let contracted: Vec<String> = grounding_trust::contracts_for(agent_id)
        .map(|c| block_of(c.path).to_string())
        .collect();

    if contracted.is_empty() {
        report.confidence_display = CONF_FLAGGED;
        report.floor = None;
        report.findings.push(finding(
            "hud_contract_declared",
            format!(
                "`{agent_id}` declares no grounding contract, so nothing is known \
                 about where any of its fields came from and the HUD cannot label \
                 them. Rendered as `flagged`, not as clean: an absent contract is \
                 an absence of evidence, not evidence of grounding. Add \
                 FIELD_CONTRACTS entries in src/grounding_trust.rs."
            ),
        ));
        // Still worth checking the card's shape, so an author fixes both in one
        // pass rather than discovering the length limits after wiring grounding.
        check_card_shape(doc, &mut report);
        write_confidence(doc, CONF_FLAGGED);
        return report;
    }

    // 2. Every contracted block must have come back with a provenance tag.
    //    `grounding_trust::enforce` stamps these itself, so a gap here means
    //    the two modules have drifted rather than that an author forgot.
    let mut verdicts: Vec<(String, &'static str)> = Vec::new();
    for block in &contracted {
        if verdicts.iter().any(|(b, _)| b == block) {
            continue;
        }
        let key = format!("{block}_provenance");
        match doc.get(&key).and_then(|v| v.as_str()) {
            Some(v) if PROVENANCE_VALUES.contains(&v) => {
                verdicts.push((block.clone(), leak_static(v)));
            }
            Some(v) => {
                report.findings.push(finding(
                    "hud_provenance_complete",
                    format!(
                        "`{key}` is `{v}`, which is not in PROVENANCE_VALUES \
                         {PROVENANCE_VALUES:?}. Treated as unavailable. A verdict \
                         the runtime cannot emit is worthless rather than trusted."
                    ),
                ));
                verdicts.push((block.clone(), PROV_UNAVAILABLE));
            }
            None => {
                // Only reachable for a narrative-only block, which correctly
                // gets no provenance key, or if the two modules have drifted.
                if grounding_trust::contracts_for(agent_id)
                    .filter(|c| block_of(c.path) == block.as_str())
                    .all(|c| c.grounding == grounding_trust::Grounding::Narrative)
                {
                    continue;
                }
                report.findings.push(finding(
                    "hud_provenance_complete",
                    format!(
                        "Block `{block}` is under contract but came back with no \
                         `{key}`. Treated as unavailable. A field with no tag is \
                         the one a reader cannot tell from a measurement, which is \
                         the entire failure this contract exists to prevent."
                    ),
                ));
                verdicts.push((block.clone(), PROV_UNAVAILABLE));
            }
        }
    }

    // 3. Subject conditioning. Every block except the subject itself describes
    //    a lookup keyed on the subject, so none may outrank it.
    let subject = verdicts
        .iter()
        .find(|(b, _)| b == "subject")
        .map(|(_, v)| *v);

    let effective: Vec<(String, &'static str)> = verdicts
        .iter()
        .map(|(b, v)| {
            let eff = match subject {
                Some(s) if b != "subject" && b != "capture" => conditioned(s, v),
                _ => *v,
            };
            (b.clone(), eff)
        })
        .collect();

    // The band answers "how much should the wearer trust what this card
    // ASSERTS", so three kinds of block are excluded from it. Each exclusion
    // is a claim that could be wrong, so each is stated:
    //
    //  · `card` — it is computed FROM the floor. Including it would put the
    //    output inside its own input.
    //  · a declared gap that stayed empty — see `is_declared_gap`. It is a
    //    correct answer, and it is still marked on the card.
    //  · nothing else. A block that was supposed to have content and came back
    //    empty counts, and so does a declared gap the model tried to fill —
    //    that is a grounding violation, handled just below.
    let mut counted: Vec<&'static str> = effective
        .iter()
        .filter(|(b, _)| b != "card")
        .filter(|(b, _)| {
            !is_declared_gap(agent_id, b)
                || report
                    .grounding
                    .violations
                    .iter()
                    .any(|v| block_of(&v.path) == b.as_str())
        })
        .map(|(_, v)| *v)
        .collect();

    // 4. The prose channel, which has no markers.
    scan_prose(doc, &effective, &mut report);

    // 5. The card itself. Line treatments are resolved BEFORE the band is
    //    computed, because a rendered line is part of what the card asserts:
    //    an untagged line is a claim with no evidence sitting on the display,
    //    and a band that ignored it would rate the card on the strength of
    //    blocks the wearer is not looking at.
    check_card_shape(doc, &mut report);
    counted.extend(apply_line_treatments(
        doc,
        &effective,
        subject,
        agent_id,
        &mut report,
    ));

    // 6. A response that had to be corrected is not a response to trust,
    //    whatever survived the correction — otherwise stripping a fabricated
    //    field would launder the card into confidence by the act of stripping.
    //
    //    The correction has to be recorded IN the document, not just in this
    //    report, or the guarantee lasts exactly one pass. `grounding_trust`
    //    walked into the same trap from the other direction: 13 cached profiles
    //    had their invented values un-stripped when a field later became
    //    sourceable, which is why `PRE_CONTRACT_MARKER` exists. A card re-read
    //    from cache must not come back more confident than when it was written.
    let corrected = !report.grounding.is_clean();
    if corrected {
        let paths: Vec<String> = report
            .grounding
            .violations
            .iter()
            .map(|v| v.path.clone())
            .collect();
        if let Some(obj) = doc.as_object_mut() {
            obj.insert(REVIEW_MARKER.into(), json!(paths));
        }
    }
    let previously_corrected = doc.get(REVIEW_MARKER).is_some();

    let card_floor = if previously_corrected {
        PROV_UNAVAILABLE
    } else {
        weakest(counted.iter().copied())
    };
    report.floor = Some(card_floor);
    report.corrected = previously_corrected;

    let band = confidence_for(card_floor);
    // Recorded, not silently corrected. A model that asserts `high` over a
    // flagged card is a behaviour someone should see in the eval signal, and a
    // correction nobody logs is a correction nobody learns from.
    if let Some(claimed) = doc
        .pointer("/card/confidence_display")
        .and_then(|v| v.as_str())
    {
        if claimed != band {
            report.findings.push(finding(
                "hud_confidence_is_computed",
                format!(
                    "The response claimed `confidence_display: {claimed}`; the \
                     measured floor across {} block(s) is `{card_floor}`, which \
                     bands to `{band}`. Overwritten. This field is computed from \
                     provenance and is never accepted from the model — a card that \
                     can rate its own confidence can rate a guess as high.",
                    counted.len()
                ),
            ));
        }
    }
    write_confidence(doc, band);
    report.confidence_display = band;

    report
}

/// Borrow one of the closed vocabulary's `'static` strs matching `v`.
///
/// `PROVENANCE_VALUES` membership was already checked by the caller, so the
/// fallback is unreachable in practice; it returns the weakest value rather
/// than panicking, on the principle that a parsing surprise should degrade
/// confidence rather than take down a response.
fn leak_static(v: &str) -> &'static str {
    PROVENANCE_VALUES
        .iter()
        .copied()
        .find(|c| *c == v)
        .unwrap_or(PROV_UNAVAILABLE)
}

/// Null a summary that asserts a safety claim the evidence cannot support.
///
/// Nulled rather than flagged, matching `grounding_trust`'s handling of a
/// narrative leak: a validator cannot rewrite prose into honesty, and leaving
/// the sentence in place would move the claim into the one channel a wearer
/// hears rather than reads. The text is retained on the finding so it can be
/// checked against a real source later instead of being lost.
fn scan_prose(doc: &mut Value, effective: &[(String, &'static str)], report: &mut HudReport) {
    let safety_sourced = effective
        .iter()
        .any(|(b, v)| b == SAFETY_BLOCK && *v == PROV_TOOL);
    if safety_sourced {
        return;
    }
    let Some(text) = doc.get("summary").and_then(|v| v.as_str()) else {
        return;
    };
    let haystack = text.to_ascii_lowercase();
    if !SAFETY_LEAKS.iter().any(|rule| rule.matches(&haystack)) {
        return;
    }
    let removed = text.to_string();
    report.findings.push(finding(
        "hud_prose_carries_no_unsourced_safety_claim",
        format!(
            "`summary` asserts an edibility or toxicity claim, and no tool this \
             agent has can supply one. Nulled. The removed text was: {removed:?}. \
             This scan exists because the summary is the audio channel, and the \
             audio channel has no markers — a caveat that survives only as a \
             glyph does not survive being spoken."
        ),
    ));
    if let Some(obj) = doc.as_object_mut() {
        obj.insert("summary".into(), Value::Null);
    }
}

fn write_confidence(doc: &mut Value, band: &'static str) {
    if let Some(card) = doc.get_mut("card").and_then(|c| c.as_object_mut()) {
        card.insert("confidence_display".into(), json!(band));
    }
}

/// Title present and short enough; line count within budget.
fn check_card_shape(doc: &mut Value, report: &mut HudReport) {
    let Some(card) = doc.get("card") else {
        report.findings.push(finding(
            "hud_card_present",
            "The response carries no `card`, so there is nothing to render on the \
             display. The typed blocks are the evidence; the card is the only part \
             the wearer sees.",
        ));
        return;
    };

    match card.get("title").and_then(|v| v.as_str()) {
        None | Some("") => report.findings.push(finding(
            "hud_card_present",
            "`card.title` is missing or empty.",
        )),
        Some(t) if t.chars().count() > TITLE_MAX => report.findings.push(finding(
            "hud_glanceable",
            format!(
                "`card.title` is {} characters; the budget is {TITLE_MAX}. \
                 Titles over the limit are truncated by the renderer at a point \
                 it chooses, which is how a qualifier falls off the end of a \
                 hedged claim and leaves a confident one.",
                t.chars().count()
            ),
        )),
        Some(_) => {}
    }

    let lines = card.get("lines").and_then(|v| v.as_array());
    match lines {
        None => report.findings.push(finding(
            "hud_card_present",
            "`card.lines` is missing or not an array.",
        )),
        Some(l) if l.len() > MAX_LINES => report.findings.push(finding(
            "hud_glanceable",
            format!(
                "`card.lines` has {} entries; the budget is {MAX_LINES}. Past this \
                 the wearer samples rather than reads, and the line most likely to \
                 be skipped is the flagged one.",
                l.len()
            ),
        )),
        Some(_) => {}
    }
}

/// Stamp each line with its effective provenance and treatment.
///
/// A line declares which block it speaks for. That indirection is the point:
/// a line cannot assert its own confidence, only which evidence it is
/// reporting, and the treatment follows from that block's measured verdict.
///
/// Returns the verdicts the rendered lines contribute to the confidence band:
/// every line except one correctly reporting a declared gap, which is a right
/// answer rather than a weak one.
fn apply_line_treatments(
    doc: &mut Value,
    effective: &[(String, &'static str)],
    subject: Option<&'static str>,
    agent_id: &str,
    report: &mut HudReport,
) -> Vec<&'static str> {
    let mut contributed: Vec<&'static str> = Vec::new();
    // Is any block genuinely a retrieval? If not, no line may render unmarked.
    let anything_sourced = effective.iter().any(|(_, v)| *v == PROV_TOOL);

    let Some(lines) = doc
        .pointer_mut("/card/lines")
        .and_then(|v| v.as_array_mut())
    else {
        return contributed;
    };

    for (i, line) in lines.iter_mut().enumerate() {
        let Some(obj) = line.as_object_mut() else {
            report.findings.push(finding(
                "hud_line_declares_block",
                format!(
                    "`card.lines[{i}]` is not an object. Each line must be \
                     `{{ \"text\": ..., \"block\": ... }}` so its treatment can be \
                     derived from the evidence it reports. A bare string cannot \
                     carry provenance, and a line that cannot carry provenance \
                     renders identically to one that can."
                ),
            ));
            continue;
        };

        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            if text.chars().count() > LINE_MAX {
                report.findings.push(finding(
                    "hud_glanceable",
                    format!(
                        "`card.lines[{i}].text` is {} characters; the budget is \
                         {LINE_MAX} including the marker.",
                        text.chars().count()
                    ),
                ));
            }
        } else {
            report.findings.push(finding(
                "hud_card_present",
                format!("`card.lines[{i}].text` is missing or not a string."),
            ));
        }

        // Which block does this line report?
        let declared = obj
            .get("block")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let verdict: &'static str = match declared.as_deref() {
            Some(b) => match effective.iter().find(|(name, _)| name == b) {
                Some((_, v)) => v,
                None => {
                    report.findings.push(finding(
                        "hud_line_declares_block",
                        format!(
                            "`card.lines[{i}].block` is `{b}`, which is not a block \
                             under this agent's grounding contract. Treated as \
                             unavailable — a line pointing at evidence that does not \
                             exist is not evidence."
                        ),
                    ));
                    PROV_UNAVAILABLE
                }
            },
            None => {
                report.findings.push(finding(
                    "hud_line_declares_block",
                    format!(
                        "`card.lines[{i}]` names no `block`, so there is nothing to \
                         derive its treatment from. Treated as unavailable and \
                         marked accordingly. This is the fail-closed direction on \
                         purpose: an untagged line defaulting to unmarked is exactly \
                         the schema-conformant, evidentially-silent output this \
                         contract exists to refuse."
                    ),
                ));
                PROV_UNAVAILABLE
            }
        };

        // A line may not render as a retrieval when nothing was retrieved. The
        // display-layer counterpart of `grounding_trust`'s narrative leak scan.
        let mut verdict = verdict;
        if verdict == PROV_TOOL && !anything_sourced {
            report.findings.push(finding(
                "hud_line_cannot_outrank_evidence",
                format!(
                    "`card.lines[{i}]` would render as sourced, but no block in this \
                     response came back tool-verified. Downgraded to unavailable."
                ),
            ));
            verdict = PROV_UNAVAILABLE;
        }

        let t = treatment(verdict);
        obj.insert("provenance".into(), json!(verdict));
        obj.insert("treatment".into(), json!(t.word()));
        obj.insert("marker".into(), json!(t.marker().trim_end()));
        obj.insert("spec_provenance".into(), json!(spec_word(verdict)));
        if let Some(s) = subject {
            // Recorded per line so a reader can see WHY a GBIF hit renders as
            // inferred, rather than concluding the tag is wrong.
            obj.insert("subject_provenance".into(), json!(s));
        }

        // A line honestly reporting a permanent gap does not lower the band.
        // Any other line does, including one that named no block at all: that
        // is an assertion on the display with nothing behind it.
        let honest_gap = declared
            .as_deref()
            .is_some_and(|b| is_declared_gap(agent_id, b));
        if !honest_gap {
            contributed.push(verdict);
        }
    }

    contributed
}

// ─── rendering ─────────────────────────────────────────────────────────

/// Render an enforced card to the lines a display would show.
///
/// Deterministic, no I/O, and the same function a relay would call once the
/// transport question is settled — so the treatment a wearer sees is decided
/// here, at the agent boundary, rather than by whatever renders it.
///
/// Call [`enforce`] first. Rendering an unenforced card would show the model's
/// own confidence claim, which is the failure this module exists to prevent.
pub fn render(doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let Some(card) = doc.get("card") else {
        return out;
    };
    if let Some(title) = card.get("title").and_then(|v| v.as_str()) {
        out.push(title.to_string());
    }
    for line in card
        .get("lines")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
    {
        let text = line.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let verdict = line
            .get("provenance")
            .and_then(|v| v.as_str())
            .unwrap_or(PROV_UNAVAILABLE);
        out.push(format!("{}{}", treatment(verdict).marker(), text));
    }
    if let Some(band) = card.get("confidence_display").and_then(|v| v.as_str()) {
        out.push(format!("[{band}]"));
    }
    out
}

/// The legend a wearer needs once, to read every card afterwards.
///
/// Exposed as data rather than prose so the relay renders the same words the
/// enforcement uses. A legend that drifts from the markers is worse than none.
pub fn legend() -> Vec<(&'static str, &'static str)> {
    ALL_TREATMENTS
        .iter()
        .map(|t| (t.marker().trim_end(), t.word()))
        .collect()
}

/// Every treatment, so the legend and the tests cannot drift from the enum.
pub const ALL_TREATMENTS: &[Treatment] = &[
    Treatment::Verified,
    Treatment::Inferred,
    Treatment::NoMatch,
    Treatment::Pending,
    Treatment::Rejected,
    Treatment::Unavailable,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_bands_cover_every_provenance_value() {
        for v in PROVENANCE_VALUES {
            let band = confidence_for(v);
            assert!(
                CONFIDENCE_VALUES.contains(&band),
                "{v} bands to `{band}`, which is not a confidence value"
            );
        }
    }

    /// **The drift tripwire, and it has already caught something.**
    ///
    /// `PROVENANCE_VALUES` had five members when this module was written and ten
    /// a week later, after the assertion layer added the pending tier. Every
    /// mapping here has a `_` arm, so all five new values were absorbed in
    /// silence — and two of them wrongly: `human_sourced` is strength 2 in
    /// `grounding_trust` and rendered `!` "not available", so a reviewer who
    /// cited a source saw their verification vanish on the only surface a wearer
    /// reads. Fail-closed, so nothing unsafe shipped, but the human queue was
    /// pointless on glass.
    ///
    /// Nothing caught it, because `every_provenance_value_has_a_spec_word`
    /// passed: the fallback returns a valid answer for anything. So this test
    /// checks the mapping is *deliberate* rather than merely total, by requiring
    /// each value to appear in a list a human maintains. Add a value to
    /// `grounding_trust` and this fails until someone decides what it looks like.
    #[test]
    fn every_provenance_value_is_explicitly_handled() {
        // Maintained by hand on purpose. Deriving it from the mappings would
        // make it agree with them by construction and test nothing.
        let taught: &[&str] = &[
            PROV_TOOL,
            PROV_DERIVED,
            PROV_HUMAN_SOURCED,
            PROV_INFERRED,
            PROV_HUMAN_ENDORSED,
            PROV_NO_MATCH,
            PROV_PENDING_TOOL,
            PROV_PENDING_HUMAN,
            PROV_REJECTED,
            PROV_UNAVAILABLE,
        ];
        let untaught: Vec<&str> = PROVENANCE_VALUES
            .iter()
            .copied()
            .filter(|v| !taught.contains(v))
            .collect();
        assert!(
            untaught.is_empty(),
            "grounding_trust gained {} provenance value(s) this module has never \
             been shown: {untaught:?}. They are currently absorbed by the `_` arm \
             of `treatment`, `confidence_for`, `band_rank` and `spec_word`, which \
             renders them `!` \"not available\" — safe, and wrong if any of them \
             describes a value a wearer could rely on. Decide what each looks \
             like on glass, then add it here.",
            untaught.len()
        );

        // Distinct treatments must stay distinct, or the marker vocabulary has
        // silently collapsed two different next-actions into one glyph.
        assert_eq!(treatment(PROV_HUMAN_SOURCED), Treatment::Verified);
        assert_eq!(treatment(PROV_HUMAN_ENDORSED), Treatment::Inferred);
        assert_eq!(treatment(PROV_PENDING_TOOL), Treatment::Pending);
        assert_eq!(treatment(PROV_REJECTED), Treatment::Rejected);
        assert_ne!(
            treatment(PROV_REJECTED),
            treatment(PROV_UNAVAILABLE),
            "`checked and wrong` renders identically to `nothing can tell us`"
        );
    }

    #[test]
    fn every_provenance_value_has_a_spec_word() {
        for v in PROVENANCE_VALUES {
            assert!(
                !spec_word(v).is_empty(),
                "{v} has no spec word, so the HUD alias table is incomplete"
            );
        }
        // The requested four-value set plus the fifth the platform emits.
        let words: Vec<&str> = PROVENANCE_VALUES.iter().map(|v| spec_word(v)).collect();
        for expected in ["SOURCED", "DERIVED", "UNCLEAR", "UNSOURCED", "INFERRED"] {
            assert!(
                words.contains(&expected),
                "no provenance value maps to `{expected}`"
            );
        }
    }

    /// The unmarked case must be the trustworthy one, so that a renderer which
    /// drops markers degrades toward caution instead of toward confidence.
    #[test]
    fn only_verified_renders_without_a_marker() {
        assert_eq!(Treatment::Verified.marker(), "");
        for t in ALL_TREATMENTS.iter().filter(|t| **t != Treatment::Verified) {
            assert!(!t.marker().is_empty(), "{t:?} renders unmarked");
        }
    }

    /// Distinct markers, or the distinction is decorative.
    #[test]
    fn markers_are_distinguishable() {
        let l = legend();
        let mut seen: Vec<&str> = Vec::new();
        for (marker, _) in &l {
            assert!(!seen.contains(marker), "duplicate marker `{marker}`");
            seen.push(marker);
        }
        assert_eq!(
            l.len(),
            ALL_TREATMENTS.len(),
            "the legend has drifted from the Treatment enum — a marker a wearer \
             sees with no legend entry is an unexplained glyph"
        );
    }

    /// Markers must survive a single luminous green channel and a TTS
    /// fallback. Non-ASCII is the failure mode: the panel renders what it
    /// cannot reproduce as black, i.e. as nothing.
    #[test]
    fn markers_are_ascii() {
        for (marker, word) in legend() {
            assert!(
                marker.is_ascii() && word.is_ascii(),
                "`{marker}`/`{word}` is not ASCII"
            );
        }
    }

    #[test]
    fn a_lookup_keyed_on_a_guess_cannot_outrank_the_guess() {
        assert_eq!(conditioned(PROV_INFERRED, PROV_TOOL), PROV_INFERRED);
        assert_eq!(conditioned(PROV_INFERRED, PROV_DERIVED), PROV_INFERRED);
        // And conditioning never raises anything.
        assert_eq!(conditioned(PROV_TOOL, PROV_UNAVAILABLE), PROV_UNAVAILABLE);
        assert_eq!(conditioned(PROV_TOOL, PROV_TOOL), PROV_TOOL);
    }

    #[test]
    fn prose_is_flagged_rather_than_rendered() {
        let mut doc = json!("I think that is a chanterelle.");
        let r = enforce("hud_field_scout", &mut doc);
        assert_eq!(r.confidence_display, CONF_FLAGGED);
        assert!(!r.is_clean());
    }

    #[test]
    fn an_agent_with_no_contract_cannot_render_a_confident_card() {
        let mut doc = json!({
            "card": { "title": "Whatever", "lines": [], "confidence_display": "high" }
        });
        let r = enforce("no_such_agent_anywhere", &mut doc);
        assert_eq!(r.confidence_display, CONF_FLAGGED);
        assert_eq!(r.floor, None, "an absent contract is unknown, not clean");
        assert_eq!(doc.pointer("/card/confidence_display").unwrap(), "flagged");
        assert!(r
            .findings
            .iter()
            .any(|f| f.check == "hud_contract_declared"));
    }
}
