//! # Grounding trust contract — the fields that have no source
//!
//! Third sibling to [`crate::schema_trust`] and [`crate::rollup_trust`]:
//!
//! | Contract | Question |
//! | --- | --- |
//! | `schema_trust` | Is the column **present**? |
//! | `rollup_trust` | Is the column **telling the truth**? |
//! | `grounding_trust` | **Could this value have come from anywhere?** |
//!
//! ## The failure class
//!
//! `genome_profiler`'s system prompt asks for four blocks — `taxonomy`,
//! `genome`, `phylogeny`, `conservation` — and the agent has exactly two
//! tools, both GBIF, both returning taxonomy. Three of the four blocks have
//! no possible source. The model filled them anyway, confidently, and 56
//! episodes and 13 cached profiles shipped with values like
//! `estimated_size_mb: "200-400"` for a scale insect nobody has sequenced.
//!
//! Every existing check passed:
//!
//! | Guard | Why it missed this |
//! | --- | --- |
//! | `agent_contract` | Requires `accepts`/`produces` to be non-empty. They were. |
//! | `agent_card::tests` | Structural conformance: description, tags, tools-as-objects. All present. |
//! | `parse_agent_json` | Parses. The document parsed perfectly. |
//! | `cache_is_valid` (`agent_modules.rs`) | Requires non-empty `taxonomy`. Taxonomy was real — it is the one block with a tool. |
//! | Type checking | `String` is `String` whether it means "Not Evaluated" or an invention. |
//!
//! The prompt even carried a guard — *"Do not invent taxonomy — if GBIF
//! returns no match, say so"* — which protects the **only block that has a
//! tool**. A correct guard, aimed at the one place it was not needed.
//!
//! ## Why the narrative is checked too
//!
//! Nulling `genome.estimated_size_mb` is not sufficient. The `summary` field
//! is prose written by the same model in the same turn, and it restates the
//! numbers: the fixture at `tool_executor.rs` reads *"occupies Holometabola
//! with a ~480 Mb genome typical for Lepidoptera"*. `parse_evidence_text`
//! then lifts that summary out as the episode's `evidence`, so the sentence a
//! user reads would still carry the fabrication after every structured field
//! was cleared.
//!
//! So [`Grounding::Narrative`] fields are scanned for claims that exceed
//! what the sourced blocks can support. A prose channel that is not checked
//! is the channel the fabrication moves to.
//!
//! ## What this module is not
//!
//! It does not fetch anything. Making these fields *available* is a separate
//! integration task (NCBI, TimeTree, IUCN). This module's whole job is to
//! ensure that until such a tool exists, the gap reads as a gap.

use serde_json::Value;

// ─── provenance vocabulary ─────────────────────────────────────────────

/// A tool was called and returned content for this block.
///
/// Deliberately not named for a specific tool. The first draft used
/// `gbif_verified`, which stopped being true the moment a second agent
/// (`enemy_sensor`, sourcing from `scan_nearby_creatures`) was brought under
/// the contract. Which tool did it is recorded once, in
/// [`FIELD_CONTRACTS`], rather than smuggled into a status string.
pub const PROV_TOOL: &str = "tool_verified";
/// The block's tool was consulted and had nothing for this subject.
/// Materially different from "no tool exists".
pub const PROV_NO_MATCH: &str = "tool_no_match";
/// No tool in this agent's kit can supply this block.
pub const PROV_UNAVAILABLE: &str = "unavailable_no_tool_source";
/// The block is a judgement the agent was asked to make by reasoning over
/// sourced inputs. Legitimate output, explicitly labelled as reasoning
/// rather than retrieval.
pub const PROV_INFERRED: &str = "model_inference";
/// Computed by platform code from a sourced value, deterministically.
///
/// Distinct from [`PROV_INFERRED`] in the way that matters: a derivation is
/// reproducible and auditable — the same input yields the same output, and
/// the transform can be read. A model inference is neither. Collapsing them
/// would lose exactly the property that makes a derived value trustworthy.
pub const PROV_DERIVED: &str = "platform_derived";

// ─── verification states (the pending tier) ────────────────────────────
//
// The four verdicts below exist because stripping an ungrounded value was
// destroying research. `enforce` nulls a field a tool could not have supplied,
// and `Violation.removed` retains it — but nothing ever looked at the
// quarantine, so the practical effect was deletion with extra steps.
//
// A claim nobody has checked yet is not the same as a claim nothing could
// check. The first is work waiting to be done; the second is an honest
// absence. Collapsing them loses the only actionable state in the system.
//
// The route falls out of the contract with no new declarations:
// `Grounding::Sourced { tool, response_field }` already names the tool and the
// field, so a Sourced value that arrived without a recorded tool call has an
// automated check available and knows which one. An Unsourced value has no tool
// at all, so it routes to a person — and increments the tool-integration demand
// signal, which is the same fact seen from the other side.

/// Declared `Sourced`, value present, no tool call recorded. An automated
/// check exists and [`FIELD_CONTRACTS`] already names it.
pub const PROV_PENDING_TOOL: &str = "pending_tool_check";
/// No tool can answer this. A person must source it, or a tool must be built.
pub const PROV_PENDING_HUMAN: &str = "pending_human_check";
/// A person checked it and recorded what they checked it against.
///
/// Strength 2, alongside [`PROV_TOOL`], and the citation is what earns that: a
/// verdict someone else can follow to the same source is reproducible, which is
/// the only property the ladder measures. Enforced at the database level — a
/// `human_sourced` row without a citation is rejected by CHECK, because a
/// one-click "verified" button is how a queue becomes a laundering UI.
pub const PROV_HUMAN_SOURCED: &str = "human_sourced";
/// A person vouched for it without citing anything.
///
/// Deliberately available, deliberately weaker. Requiring a citation for every
/// judgement would push reviewers to paste a plausible URL, which is worse than
/// an honest "I believe this". Strength 1, level with
/// [`PROV_INFERRED`]: an uncited human opinion and a model's judgement are the
/// same kind of claim, and pretending otherwise because a person typed it is
/// exactly the deference this module exists to remove.
pub const PROV_HUMAN_ENDORSED: &str = "human_endorsed";
/// Checked, and found wrong.
///
/// Strength 0. Reliance-wise that is identical to unknown, and the ladder only
/// measures reliance — what differs is what happens next, which is routing.
/// Retained rather than deleted: a rejection rate is the first per-agent
/// quality signal on this platform that is not self-reported.
pub const PROV_REJECTED: &str = "rejected";

/// Every value `<block>_provenance` is permitted to take.
///
/// A closed set, asserted by [`tests::provenance_values_are_closed`]. An
/// open one would let a future edit invent `"estimated"`, which is the
/// fabrication reappearing as a metadata value.
pub const PROVENANCE_VALUES: &[&str] = &[
    PROV_TOOL,
    PROV_NO_MATCH,
    PROV_UNAVAILABLE,
    PROV_INFERRED,
    PROV_DERIVED,
    PROV_PENDING_TOOL,
    PROV_PENDING_HUMAN,
    PROV_HUMAN_SOURCED,
    PROV_HUMAN_ENDORSED,
    PROV_REJECTED,
];

/// `Sourced` fields the platform holds no independent copy of, with the
/// reason and what it would take to change that.
///
/// This list is the honest half of the completeness claim. A `Sourced` field
/// that is neither cross-checked nor listed here is a claim nobody can
/// falsify, and `every_sourced_field_is_verifiable_or_admits_it_is_not`
/// refuses to compile such a contract. The point is not that everything is
/// verified — it is that nothing is *silently* unverified.
///
/// `(agent_id, path, why_not_and_what_would_fix_it)`
pub const CROSS_CHECK_EXEMPTIONS: &[(&str, &str, &str)] = &[
    (
        "genome_profiler",
        "phylogeny.sister_taxa",
        "GBIF returns sibling taxa but the platform stores none of them, so \
         there is no second copy to disagree with. Fixable by persisting the \
         sibling list on the creature row at creation, the same way \
         `taxonomy` already is — at which point this becomes a cross-check \
         identical to the taxonomy one.",
    ),
    (
        "genome_profiler",
        "genome.estimated_size_mb",
        "NCBI is the only holder; the platform keeps no assembly mirror. The \
         tool does return `assembly_accession`, so a replay check is possible: \
         re-query that accession and compare `total_length`. Deferred because \
         it costs an external call per row and the value is immutable once an \
         assembly is released.",
    ),
    (
        "genome_profiler",
        "genome.chromosome_count",
        "Same as genome size: sourced from the same assembly record, \
         verifiable only by replaying the accession against NCBI.",
    ),
    (
        "genome_profiler",
        "genome.assembly_name",
        "Provenance metadata naming its own source. Cross-checking it would \
         require the replay that would supersede it.",
    ),
    (
        "genome_profiler",
        "genome.assembly_accession",
        "The identifier a replay check would be keyed on, so cross-checking it \
         against itself is circular. It becomes verifiable the moment the \
         accession is re-queried, which is the deferred replay noted above.",
    ),
    (
        "enemy_sensor",
        "threats[].species",
        "The scan's `scientific_name` travels with the `creature_id` on the \
         same row, and that id IS cross-checked. Verifying the name \
         separately would re-derive it from the creature row, which is the \
         same assertion twice.",
    ),
    (
        "prey_locator",
        "prey_targets[].creature_id",
        "Identical in kind to the enemy_sensor check and worth adding; \
         deferred only because this agent's output shape has two modes (scan \
         and stalk) and the SQL must not treat a stalk response as a missing \
         prey list.",
    ),
    (
        "prey_locator",
        "prey_targets[].species",
        "Carried on the same scan row as the creature_id, so verifying it \
         separately re-derives the same assertion twice. Becomes covered the \
         moment the prey_targets[].creature_id check above is written, since \
         that check reaches the row this name came from.",
    ),
    (
        "prey_locator",
        "prey_targets[].order",
        "Resolved from stored taxonomy on the same scan row as the id, so it \
         is covered by whatever checks that id. Fixed alongside the \
         prey_targets[].creature_id check.",
    ),
    (
        "hud_field_scout",
        "taxonomy",
        "There is nothing to compare against, and the reason is structural \
         rather than a missing integration. `genome_profiler`'s taxonomy is \
         cross-checkable because the platform independently holds \
         `creatures.taxonomy` for a minted creature; here the subject is \
         whatever a wearer pointed a camera at, and no platform row claims to \
         know what that was. The check that WOULD be meaningful is a different \
         one: agreement between two independent determiners on the same frame. \
         That needs a second recogniser, which is a capability decision, not a \
         query someone forgot to write.",
    ),
    (
        "hud_field_scout",
        "taxonomy.fungal_nomenclature",
        "MycoBank is the only source the platform has for fungal name status, \
         so comparing it to itself is circular. It becomes checkable against \
         GBIF's accepted-name view for the same binomial, which is worth doing \
         precisely because the two databases disagree often enough for the \
         disagreement to be the interesting signal — deferred rather than \
         dismissed.",
    ),
    (
        "hud_field_scout",
        "observations",
        "iNaturalist occurrence counts are a live third-party aggregate the \
         platform stores no copy of, and a snapshot would go stale faster than \
         it could be verified against. Replaying the same bounded query and \
         comparing counts would test iNaturalist's stability, not this agent's \
         honesty.",
    ),
];

/// Is this `Sourced` field knowingly un-cross-checked?
pub fn cross_check_exempt(agent_id: &str, path: &str) -> bool {
    CROSS_CHECK_EXEMPTIONS
        .iter()
        .any(|(a, p, _)| *a == agent_id && *p == path)
}

/// Every declared cross-check, for the live tier to run.
pub fn cross_checks() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    FIELD_CONTRACTS
        .iter()
        .filter_map(|c| c.cross_check_sql.map(|sql| (c.agent_id, c.path, sql)))
}

/// Marker written by migration 200 onto profiles produced **before** any
/// grounding contract existed.
///
/// It exists because of a trap this codebase walked into: when
/// `ncbi_genome_search` was added, `genome.estimated_size_mb` moved from
/// `Unsourced` to `Sourced` — and `enforce` therefore stopped stripping it.
/// The 13 cached profiles written while the field was fabricated suddenly had
/// their invented values *un-stripped*, because the field had become
/// sourceable in general even though those particular values were never
/// sourced.
///
/// Wiring up a tool must not retroactively bless data that predates it. A
/// document carrying this marker has every non-narrative field treated as
/// unsourced regardless of the current contract, because provenance is a
/// claim about how a value was obtained, not about what is obtainable now.
pub const PRE_CONTRACT_MARKER: &str = "_grounding_review";

/// Minimum length of a `why`.

// ─── the provenance lattice ────────────────────────────────────────────
//
// Provenance is a **floor, not a stamp**. It can only decrease along a
// derivation chain, and this is the machinery that enforces that.
//
// ## The cycle this exists to break
//
// `football_analyst` has one tool: `execute_agent`. No football data source
// at all. It nonetheless declares `ELO`, `MATCH STATS` and `INJURY IMPACT`
// as finding labels, and it is the second most-run agent on the platform.
// So its numbers are model output. Then:
//
// ```text
//   football_analyst  →  "Liverpool ELO 1823"          [ungrounded]
//     → episode
//     → ontologist extraction  →  semantic_rules
//                                 verification_method = 'llm_extraction:<model>'
//     → football_analyst reads the KG fact back as knowledge
// ```
//
// `semantic_rules` records how a rule was made and never what it was made
// *from*. So a fabrication acquires institutional standing by passing through
// extraction, and the knowledge graph becomes a laundry. Every dream cycle
// adds more, and the source episodes eventually age out, at which point the
// groundedness is unrecoverable.
//
// ## Two rules
//
// 1. **A derived value is never stronger than its weakest source.**
// 2. **Extraction is itself an inference**, so an extracted rule is capped at
//    [`EXTRACTION_CEILING`] however well-sourced its inputs were. The
//    ontologist reading prose and writing "teams with higher ELO win 62% of
//    the time" has made a judgement, not performed a lookup — even when every
//    number it read was tool-verified.
//
// Consequence, and the point of the whole exercise: **a knowledge-graph fact
// can never be presented as `tool_verified`.** It is at best a judgement, and
// at worst a laundered invention.

/// How much a value can be trusted, as an ordinal for taking minima.
///
/// Deliberately coarse. A finer scale would invite arguments about whether
/// `platform_derived` outranks `tool_verified`, which is not a question worth
/// having: both are reproducible, and that is the only property a floor cares
/// about.
///
/// The two "absent" verdicts collapse to 0 because they describe the lack of
/// a value rather than the strength of one. A rule extracted from an empty
/// block was extracted from prose, and prose is ungrounded.
pub fn strength(verdict: &str) -> u8 {
    match verdict {
        // Reproducible: run the tool, apply the transform, or follow the
        // citation, and you land on the same value.
        PROV_TOOL | PROV_DERIVED | PROV_HUMAN_SOURCED => 2,
        // A judgement. Legitimate, and not a retrieval. An uncited human
        // opinion sits here too: a person saying so is the same kind of claim
        // as a model saying so.
        PROV_INFERRED | PROV_HUMAN_ENDORSED => 1,
        // Nothing to rely on yet. pending_* is weaker than model_inference on
        // purpose — a judgement the agent was ASKED to make is legitimate
        // output, while a retrieval claim with no retrieval behind it is not
        // yet anything. `rejected` is also 0: reliance-wise a disproven value
        // is worth exactly as much as an unknown one, and the difference is
        // routing rather than reliance.
        //
        // unavailable_no_tool_source, tool_no_match, pending_tool_check,
        // pending_human_check, rejected, and anything unrecognised.
        _ => 0,
    }
}

/// The strongest provenance an *extracted* value may claim.
///
/// Extraction reads text and writes an assertion. That is inference, so no
/// amount of well-sourced input makes the output a retrieval.
pub const EXTRACTION_CEILING: &str = PROV_INFERRED;

/// Weakest verdict among `sources`, or `unavailable_no_tool_source` when
/// there are none.
///
/// No sources means no evidence, which must not read as clean. An empty
/// iterator returning the strongest value is the single most common way a
/// floor calculation silently inverts.
///
/// Returns a verdict that **actually occurred** among the inputs, never a
/// stand-in for its strength tier. The first draft collapsed to one
/// representative per tier — `tool_verified` for anything scoring 2 — so a
/// value settled by a human citing a source came back claiming a tool had run,
/// and `tool_no_match` ("the tool answered, and had nothing") came back as
/// `unavailable_no_tool_source` ("no tool exists"). Both are misattributions of
/// mechanism, which is the specific error this module exists to prevent, and
/// both were invisible because the strength was right.
///
/// Ties at the minimum resolve to the first one seen. Any of them is a true
/// statement about a real source, which is the property that matters; where
/// there is exactly one source the answer is exactly right.
///
/// Asserted by [`tests::the_floor_never_invents_a_verdict_that_was_not_there`].
pub fn floor<'a>(sources: impl IntoIterator<Item = &'a str>) -> &'static str {
    let mut worst: Option<(u8, &'static str)> = None;
    for s in sources {
        // Canonicalise to the &'static str from PROVENANCE_VALUES so the
        // returned verdict is one the runtime can emit, and an unrecognised
        // input cannot be echoed back as though it were vocabulary.
        let canonical = PROVENANCE_VALUES.iter().copied().find(|v| *v == s);
        let v = strength(s);
        let candidate = (v, canonical.unwrap_or(PROV_UNAVAILABLE));
        worst = Some(match worst {
            Some((w, _)) if w <= v => worst.unwrap(),
            _ => candidate,
        });
    }
    worst
        .map(|(_, verdict)| verdict)
        .unwrap_or(PROV_UNAVAILABLE)
}

/// Provenance for a value derived by extraction from `sources`.
///
/// `min(floor(sources), EXTRACTION_CEILING)` — both rules applied at once.
/// This is what `semantic_rules.provenance_floor` should be written with.
pub fn extracted_floor<'a>(sources: impl IntoIterator<Item = &'a str>) -> &'static str {
    let base = floor(sources);
    if strength(base) > strength(EXTRACTION_CEILING) {
        EXTRACTION_CEILING
    } else {
        base
    }
}

/// The provenance floor of one agent response.
///
/// Runs the grounding contract over a raw response and returns the weakest
/// block verdict. This is how an episode gets a groundedness score at all:
/// before migration 199 the response was discarded, so nothing about a
/// historical episode's groundedness is recoverable — those rows get `None`,
/// which must be read as *unknown*, never as clean.
pub fn response_floor(agent_id: &str, response_text: &str) -> Option<&'static str> {
    let mut doc: Value = match serde_json::from_str::<Value>(response_text) {
        Ok(v) if v.is_object() => v,
        // Prose. An extraction from prose is ungrounded by construction: there
        // are no typed fields to have been sourced.
        _ => return Some(PROV_UNAVAILABLE),
    };
    if contracts_for(agent_id).next().is_none() {
        // No contract, so nothing is known about this agent's grounding.
        // Distinct from "ungrounded" — see the caller.
        return None;
    }
    let report = enforce(agent_id, &mut doc);
    Some(floor(report.provenance.iter().map(|(_, v)| *v)))
}

// ─── contract ──────────────────────────────────────────────────────────

/// Where a field's value can legitimately come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grounding {
    /// A named tool's response supplies this field.
    Sourced {
        tool: &'static str,
        response_field: &'static str,
    },
    /// No available tool returns data for this field. The model could only
    /// produce it from parametric knowledge, so it must be `null`.
    Unsourced,
    /// A judgement the agent is *asked* to make by reasoning over sourced
    /// inputs. Kept, and labelled `model_inference`.
    ///
    /// This variant exists because the second and third agents brought under
    /// the contract nearly broke it. `enemy_sensor` is asked to rate
    /// predation risk from taxonomy and proximity; `prey_locator` is asked
    /// for an intercept strategy. Those are inferences, and they are the
    /// entire product. Treating them like `genome.estimated_size_mb` would
    /// null the agents' only output and prove that the contract cannot tell
    /// an agent that fabricates from one that reasons — at which point it
    /// would rightly be switched off.
    ///
    /// The distinction is retrieval versus judgement. A genome size is a
    /// fact sitting in a database the agent did not query. A threat level is
    /// not in any database; producing it is the job.
    Inferred {
        /// What the judgement is reasoned from, for the report.
        from: &'static str,
    },
    /// Computed by platform code from a sourced field, deterministically.
    ///
    /// `phylogeny.superorder` is the motivating case: it is not a GBIF rank
    /// and no tool returns it, but it follows from `taxonomy.order` through a
    /// closed ~30-entry table (Lepidoptera -> Holometabola). While the model
    /// supplied it from memory it was `Unsourced`; once platform code applies
    /// the table it is reproducible, and that is a different kind of claim.
    Derived {
        /// The sourced field it is computed from.
        from: &'static str,
        /// The transform, named so a reader can check it.
        how: &'static str,
    },
    /// Free prose. Permitted, but must not assert anything the sourced
    /// blocks cannot support — see [`NARRATIVE_LEAKS`].
    Narrative,
}

/// One output field and what may legitimately fill it.
#[derive(Debug, Clone, Copy)]
pub struct FieldContract {
    pub agent_id: &'static str,
    /// Dotted path into the agent's output document, e.g.
    /// `genome.estimated_size_mb`. A leading segment names a top-level
    /// block, which is what gets a `_provenance` sibling.
    pub path: &'static str,
    pub grounding: Grounding,
    /// Why, in enough detail that the next person does not have to
    /// re-derive it from the tool list.
    pub why: &'static str,
    /// A read-only query returning **one row per disagreement** between what
    /// the agent produced and an independently-held source of truth.
    ///
    /// Modelled directly on [`crate::rollup_trust::RollupContract::mismatch_sql`],
    /// because it answers the same question one layer up. `rollup_trust`
    /// exists because `agents.total_executions` was present, correctly typed,
    /// declared in the schema contract, and permanently zero — a *content*
    /// failure invisible to every check that reasons about shape.
    ///
    /// `Grounding::Sourced` turned out to have the same hole. It asserts a
    /// tool COULD supply a field; it never compared the value to anything. So
    /// `Antaxius beieri`, a bush-cricket, was profiled as a cerambycid beetle
    /// and passed every check: present, non-null, correctly typed, declared
    /// sourced. The verified answer was one `JOIN` away the whole time.
    ///
    /// `None` means the platform holds no independent copy to compare
    /// against; it must then appear in [`CROSS_CHECK_EXEMPTIONS`] with a
    /// reason. A `Sourced` field that is neither cross-checked nor explicitly
    /// exempt is a claim nobody can falsify, which is what
    /// `every_sourced_field_is_verifiable_or_admits_it_is_not` refuses to
    /// allow.
    pub cross_check_sql: Option<&'static str>,
}

/// How a leak needle is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeakRule {
    /// A distinctive word. Plain substring match is safe.
    Word(&'static str),
    /// A unit that only implies a claim when a number precedes it.
    ///
    /// This variant exists because the first version of this table used a
    /// plain `" gb"` needle, which matches **"GBIF"** — so an honest
    /// taxonomy-only summary citing its own source was reported as leaking
    /// a genome size. A check that fires on correct output is worse than no
    /// check: it gets switched off, and the switching-off looks like
    /// cleanup.
    Quantity(&'static str),
}

/// Patterns in a [`Grounding::Narrative`] field that assert something only
/// an unsourced block could support, paired with the block that would have
/// to be sourced for the claim to be legitimate.
///
/// Deliberately narrow, and matched against a lowercased haystack.
pub const NARRATIVE_LEAKS: &[(&str, LeakRule)] = &[
    ("genome", LeakRule::Quantity("mb")),
    ("genome", LeakRule::Quantity("gb")),
    ("genome", LeakRule::Quantity("kb")),
    ("genome", LeakRule::Word("megabase")),
    ("genome", LeakRule::Word("gigabase")),
    ("genome", LeakRule::Word("chromosom")),
    ("genome", LeakRule::Word("karyotype")),
    ("genome", LeakRule::Word("diploid")),
    ("genome", LeakRule::Word("haploid")),
    ("genome", LeakRule::Word("2n=")),
    ("genome", LeakRule::Word("2n =")),
    ("phylogeny", LeakRule::Quantity("mya")),
    ("phylogeny", LeakRule::Word("million years")),
    ("phylogeny", LeakRule::Word("diverged")),
    ("phylogeny", LeakRule::Word("divergence")),
    ("conservation", LeakRule::Word("iucn")),
    ("conservation", LeakRule::Word("least concern")),
    ("conservation", LeakRule::Word("endangered")),
    ("conservation", LeakRule::Word("vulnerable")),
    ("conservation", LeakRule::Word("red list")),
];

impl LeakRule {
    /// Does this rule fire against an already-lowercased haystack?
    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            LeakRule::Word(w) => haystack.contains(w),
            LeakRule::Quantity(unit) => {
                let bytes = haystack.as_bytes();
                let mut from = 0usize;
                while let Some(rel) = haystack[from..].find(unit) {
                    let at = from + rel;
                    // Walk back over the separators a writer puts between a
                    // number and its unit: "480 Mb", "420-480Mb", "~90 mya".
                    let mut i = at;
                    while i > 0 && matches!(bytes[i - 1], b' ' | b'-' | b'~' | 0xE2) {
                        i -= 1;
                    }
                    if i > 0 && bytes[i - 1].is_ascii_digit() {
                        return true;
                    }
                    from = at + unit.len();
                }
                false
            }
        }
    }
}

/// Every field we have an opinion about.
///
/// Seeded with `genome_profiler` — the agent the class was found in. Extend
/// per agent as each is remediated; `port_census.py` reports which agents
/// have output fields and no entry here.
pub const FIELD_CONTRACTS: &[FieldContract] = &[
    // ── genome_profiler ────────────────────────────────────────────
    // Tools: gbif_species_search, gbif_taxonomy_tree. Both taxonomy.
    FieldContract {
        agent_id: "genome_profiler",
        path: "taxonomy",
        grounding: Grounding::Sourced {
            tool: "gbif_taxonomy_tree",
            response_field: "hierarchy (kingdom..species)",
        },
        why: "The one block with a real tool. GBIF returns the full rank \
              ladder with keys.",
        // The check that would have caught `Antaxius beieri` — a bush-cricket
        // profiled as a cerambycid beetle — without a human noticing.
        // Post-contract rows only: pre-contract documents are known-bad and
        // archived by migration 202, so including them would leave this
        // permanently red and therefore permanently ignored.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM creature_conditions cc \
               JOIN creatures c ON c.creature_id = cc.creature_id \
              WHERE cc.genome_profile IS NOT NULL \
                AND NOT cc.genome_profile ? '_grounding_review' \
                AND cc.genome_profile->'taxonomy'->>'order' IS NOT NULL \
                AND ( lower(c.taxonomy->>'order') \
                        <> lower(cc.genome_profile->'taxonomy'->>'order') \
                   OR lower(c.taxonomy->>'family') \
                        <> lower(cc.genome_profile->'taxonomy'->>'family') )",
        ),
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.sister_taxa",
        grounding: Grounding::Sourced {
            tool: "gbif_taxonomy_tree",
            response_field: "sibling taxa at each rank",
        },
        why: "The tool's own description promises sibling taxa for \
              phylogenetic context, so this is retrievable today at \
              genus/family rank. Notably the ONE phylogeny field that is \
              real — stripping it along with its neighbours would be a \
              check that overreaches, and an overreaching check gets \
              switched off.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.estimated_size_mb",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "estimated_size_mb (assembly total_length)",
        },
        why: "Was Unsourced; now answerable. NCBI Assembly reports \
              `total_length` and the tool names which assembly supplied it. \
              Coverage is ~2 of 6 for the species actually in \
              creature_conditions, so `tool_no_match` stays the common \
              outcome — a fact about the world, not a gap in the platform. \
              The value this replaces was not merely unsourced but wrong: the \
              prompt asserted Lepidoptera ~400-500Mb; the monarch is 245Mb.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.chromosome_count",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "assembled_chromosome_count",
        },
        why: "Was Unsourced on the belief that karyotype data has no API. \
              Partly wrong: NCBI reports `chromosome_count` in assembly \
              metadata. It counts assembled chromosome-level replicons, not a \
              cytological karyotype — for Danaus plexippus it returns 30, \
              matching published n=30, and that agreement is not a licence to \
              relabel it. Same coverage as genome size.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.assembly_name",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "assembly_name",
        },
        why: "Names WHICH assembly supplied the size and chromosome count, so \
              a number is traceable to a specific release rather than to \
              \"NCBI\". Without it the figures are unfalsifiable even when \
              correct, because nobody can tell which of several assemblies \
              they came from.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.assembly_accession",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "assembly_accession",
        },
        why: "The stable identifier the replay cross-check would use: given an \
              accession, NCBI can be re-queried and the size compared. It is \
              therefore the field that makes the other two verifiable later.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.notable_genes",
        grounding: Grounding::Unsourced,
        why: "Species-level gene-family claims have no tool behind them.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.ploidy",
        grounding: Grounding::Unsourced,
        why: "Stays unsourced even though NCBI is now wired up, and this is \
              the instructive one. NCBI returns `assemblytype: haploid` for \
              the monarch, which it is very tempting to map here — and it \
              would be FALSE: that field describes how the ASSEMBLY \
              represents the genome, not the organism. A monarch is diploid. \
              A plausible, convenient, wrong mapping is exactly the class \
              this contract exists to stop, so the tool returns ploidy as \
              null with that reason attached.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.superorder",
        grounding: Grounding::Derived {
            from: "taxonomy.order",
            how: "ncbi_tools::superorder_of — a closed table over the ~30 insect orders",
        },
        why: "The table got written, so this moved from Unsourced to Derived. \
              No tool returns a superorder and none needs to: it is a \
              deterministic function of an order GBIF does return. The \
              distinction from Inferred matters — a derivation is reproducible \
              and its table can be checked row by row, which is not true of \
              the recall it replaces. Unknown orders return None rather than \
              something plausible.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.divergence_mya",
        grounding: Grounding::Unsourced,
        why: "Needs a dated phylogeny (TimeTree). Coverage is decent at \
              order/family and sparse at species, so null stays the common \
              answer even once wired.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.defining_traits",
        grounding: Grounding::Unsourced,
        why: "Order-level trait narration from parametric knowledge.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.iucn_status",
        grounding: Grounding::Unsourced,
        why: "The most dangerous of the set. \"Not Evaluated\" is a REAL IUCN \
              value, so a fabricated \"Not Evaluated (presumed Least \
              Concern)\" is indistinguishable from a successful Red List \
              lookup. Once the IUCN tool exists this needs its own \
              provenance value — a queried NE is data; an invented one is \
              not.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.population_trend",
        grounding: Grounding::Unsourced,
        why: "An IUCN Red List field with no IUCN tool wired up. Reported \
              as \"stable\" for species that have never been assessed, which \
              reads as a measurement of a population nobody has counted.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.genetic_diversity_notes",
        grounding: Grounding::Unsourced,
        why: "No structured source at species level. Deprioritised \
              indefinitely absent a literature-mining step.",
        cross_check_sql: None,
    },
    // ── enemy_sensor ────────────────────────────────────────
    // Tools: scan_nearby_creatures (returns the actual nearby rows),
    // gbif_species_search. **Nothing here is Unsourced**, and that result
    // matters: a contract under which every agent looks guilty is a
    // contract nobody will keep. `enemy_sensor` reports creatures its tool
    // returned and rates a risk it was asked to judge. It is well-formed.
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].creature_id",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].creature_id",
        },
        why: "The scan returns the creatures; the agent may only report ones \
              it was handed. An id not in the scan would be an invented \
              creature.",
        // Generality check: this agent keeps no cache table, so its output is
        // read back out of `episodes.response_text` (mig-199). Every creature
        // id it reports must exist. `jsonb` cast is guarded by a regex so a
        // prose response cannot raise instead of returning zero rows.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM ( \
                 SELECT jsonb_array_elements(e.response_text::jsonb->'threats') AS t \
                   FROM episodes e JOIN agents a ON a.agent_id = e.agent_id \
                  WHERE a.agent_name = 'enemy_sensor' \
                    AND e.response_text ~ '^\\s*\\{' \
               ) x \
              WHERE x.t->>'creature_id' IS NOT NULL \
                AND NOT EXISTS ( \
                      SELECT 1 FROM creatures c \
                       WHERE c.creature_id::text = x.t->>'creature_id')",
        ),
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].species",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].scientific_name",
        },
        why: "Same row as the creature_id it accompanies — the scan carries \
              scientific_name, family and order for every hit.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].relationship",
        grounding: Grounding::Inferred {
            from: "order/family from the scan, plus predator-prey reasoning",
        },
        why: "Judgement, not retrieval: no database holds 'Odonata prey on \
              Lepidoptera' keyed by these two creature ids. The card already \
              guards it — 'Do not invent predation relationships that do not \
              exist' — and that guard is aimed correctly, unlike the one on \
              genome_profiler.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].risk",
        grounding: Grounding::Inferred {
            from: "taxonomy, size differential, habitat overlap, proximity",
        },
        why: "An enumerated judgement over sourced inputs. Producing it is \
              the entire point of the agent.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threat_level",
        grounding: Grounding::Inferred {
            from: "the aggregate of threats[].risk",
        },
        why: "Roll-up of the per-threat judgements; same status as its parts.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose over the assessment. Checked for the same reason \
              genome_profiler's is: parse_evidence_text lifts it out as the \
              episode's evidence, so it is the sentence a reader sees.",
        cross_check_sql: None,
    },
    // ── prey_locator ────────────────────────────────────────
    // Same scan tool as enemy_sensor, but this card asks for GEOMETRY, and
    // that is where it parts company. `scan_nearby_creatures` returns
    // `lat`/`lng` for the TARGET creature only; every nearby row carries
    // `h3_cell` and no coordinates and no distance
    // (`tools_legacy.rs:2800-2809`). So a waypoint latitude toward a prey
    // creature is a coordinate the agent was never given — in a flight plan.
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].creature_id",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].creature_id",
        },
        why: "As enemy_sensor: prey must be a creature the scan returned.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].species",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].scientific_name",
        },
        why: "Carried on the same scan row as the creature_id, so it is \
              retrieved rather than recalled — the agent cannot name a \
              species the scan did not hand it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].order",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].order",
        },
        why: "The scan resolves order and family from stored taxonomy.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].vulnerability",
        grounding: Grounding::Inferred {
            from: "size ratio, life stage, defences, habitat overlap",
        },
        why: "The tactical judgement the agent exists to make.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].reasoning",
        grounding: Grounding::Inferred {
            from: "the factors behind the vulnerability rating",
        },
        why: "Explanation of a judgement, and therefore judgement.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].distance_cells",
        grounding: Grounding::Unsourced,
        why: "The scan returns `h3_cell` per neighbour and no distance of any \
              kind. A ring count between two H3 cells is exactly computable \
              and `h3o` is already a dependency — so this is the cheapest \
              Unsourced field in the corpus to retire: compute it in \
              `execute_scan_nearby_creatures` and it becomes Sourced. Until \
              someone does, the number is the model's guess at a quantity \
              the platform could state exactly.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].lat",
        grounding: Grounding::Unsourced,
        why: "Nearby creatures come back with `h3_cell` and no coordinates; \
              only the target creature has lat/lng. A waypoint latitude is \
              therefore invented, and it is invented into a FLIGHT PLAN, \
              where a plausible-looking wrong number is acted on rather than \
              read. H3 cell centres are exactly resolvable, so this is a \
              missing derivation rather than an impossible one.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].lng",
        grounding: Grounding::Unsourced,
        why: "Same as the latitude it is paired with — no coordinate for any \
              nearby creature reaches this agent.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].altitude_m",
        grounding: Grounding::Unsourced,
        why: "No altitude appears in any tool response, and unlike the \
              coordinates it is not derivable from an H3 cell either.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.estimated_distance_m",
        grounding: Grounding::Unsourced,
        why: "A metre distance to a creature whose position the agent was \
              never told. Derivable once cell centres are resolved; guessed \
              today.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.approach",
        grounding: Grounding::Inferred {
            from: "predator capability and prey escape behaviour",
        },
        why: "Tactical judgement, which is the requested product.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.intercept_strategy",
        grounding: Grounding::Inferred {
            from: "approach vector and prey behaviour",
        },
        why: "Tactical judgement over the approach vector — no tool \
              returns an intercept plan, and none could; it is the \
              reasoning the caller is paying for.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.difficulty",
        grounding: Grounding::Inferred {
            from: "the intercept problem as assessed",
        },
        why: "An enumerated judgement, not a measured quantity.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "hunting_summary",
        grounding: Grounding::Narrative,
        why: "Prose over the scan result; same leak channel as every other \
              summary field in this contract.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "tactical_notes",
        grounding: Grounding::Narrative,
        why: "Free prose accompanying the flight plan, and therefore the \
              place a stripped coordinate would reappear as text.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose over whatever was retrieved. Checked because \
              `parse_evidence_text` lifts it out as the episode's evidence, \
              making it the sentence a user actually reads — and therefore \
              the channel a stripped number moves into.",
        cross_check_sql: None,
    },
    // ── hud_field_scout ────────────────────────────────────────────
    // Tools: gbif_species_search, inat_observations, mycobank_lookup.
    // Read alongside `crate::hud_contract`, which conditions every block
    // below on `subject` before choosing how to render it.
    //
    // The block layout is deliberate: `capture` (reproducible) and `subject`
    // (a guess) are separate blocks even though a wearer would describe them
    // as one thing. Merging them would put a `Derived` field and an
    // `Inferred` field in the same block, and `enforce` ranks Derived above
    // Inferred when both appear — so the species guess would inherit
    // `platform_derived` and render on glass with no marker at all. The
    // split exists to stop that.
    FieldContract {
        agent_id: "hud_field_scout",
        path: "capture.modality",
        grounding: Grounding::Derived {
            from: "the request envelope",
            how: "presence of an image part in the MCP request: text-only \
                  yields `voice`, an image part yields `voice+image`",
        },
        why: "Which sensors fed this answer is a fact about the request the \
              platform can read off the envelope, so it is reproducible and \
              belongs in its own block. It also has to be on the card: a \
              wearer judging an identification needs to know whether the \
              agent looked at anything or only heard a description.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "subject.scientific_name",
        grounding: Grounding::Inferred {
            from: "the voice transcript and, when an image is present, the \
                   model's reading of the frame",
        },
        why: "This is the load-bearing entry. A model's guess about what is in \
              frame is a judgement, never a retrieval, and it is not a \
              `Derived` value either: two runs over the same photograph can \
              disagree, so it fails the reproducibility test that makes a \
              derivation auditable. There is no ground-truth database to match \
              a live frame against, so `Sourced` is unavailable by \
              construction rather than by omission. Everything downstream is \
              keyed on this name, which is why `hud_contract::conditioned` \
              floors every other block against it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "subject.common_name",
        grounding: Grounding::Inferred {
            from: "the same transcript and frame as the scientific name",
        },
        why: "Same status as the binomial and stated separately because a \
              vernacular name is the part a wearer actually reads, and it is \
              the part most likely to be right about a genus while wrong about \
              the species that matters.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "taxonomy",
        grounding: Grounding::Sourced {
            tool: "gbif_species_search",
            response_field: "results[0] rank ladder (kingdom..species) and usageKey",
        },
        why: "GBIF really does return this ladder for a name, so the block is a \
              genuine retrieval. What it is NOT is confirmation that the \
              wearer is looking at that species — the lookup is keyed on an \
              inferred name. The distinction is invisible in the data layer, \
              which is exactly why the display layer conditions on \
              `subject` before choosing a marker.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "taxonomy.fungal_nomenclature",
        grounding: Grounding::Sourced {
            tool: "mycobank_lookup",
            response_field: "records[].name_status / current_name",
        },
        why: "MycoBank is the nomenclatural authority for fungi and returns \
              whether a name is current or a synonym. Worth surfacing because \
              a superseded name is the common way a field guide and a database \
              appear to disagree about a mushroom when they do not.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "observations",
        grounding: Grounding::Sourced {
            tool: "inat_observations",
            response_field: "total_results and results[].observed_on / place_guess",
        },
        why: "Corroboration that is real, cheap, and honest about what it is: \
              how often this taxon has been recorded near here recently. It \
              does not identify anything, and the card must not imply that it \
              does — a dense observation cluster raises a prior, it does not \
              confirm a determination.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "edibility.verdict",
        grounding: Grounding::Unsourced,
        why: "The most important null in this contract. No tool this agent has \
              returns edibility: GBIF returns taxonomy, MycoBank returns \
              nomenclature, iNaturalist returns occurrences. A model asked \
              whether something is edible will answer, fluently, from \
              parametric memory — the `genome_profiler` failure mode with a \
              consequence worse than a wrong megabase count. The refusal is \
              the product: a wearer who sees `not available` consults a human, \
              whereas one who sees a confident answer does not.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "edibility.lookalikes",
        grounding: Grounding::Unsourced,
        why: "A toxic-lookalike list is a claim about the world held in \
              references this agent cannot query, and it is the field where a \
              plausible-but-invented answer does the most harm: naming three \
              real lookalikes while omitting the one that matters reads as \
              thorough. Null until a curated lookalike source is wired — \
              `adaptogen_curator` holds the nearest existing schema for this.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "edibility.hazard_check_performed",
        grounding: Grounding::Unsourced,
        why: "Kept as an explicit field, and explicitly null, so the absence \
              of a safety check is something the card can SAY rather than \
              something a wearer has to notice by the silence. An unmentioned \
              check reads as a passed check.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "card",
        grounding: Grounding::Derived {
            from: "the provenance verdicts of every other block",
            how: "hud_contract::enforce — subject conditioning, then floor() \
                  for confidence_display and treatment() per line",
        },
        why: "The card is computed by platform code from the blocks above, so \
              it is reproducible: the same document yields the same markers \
              and the same confidence band. Declared `Derived` rather than \
              left uncontracted because `confidence_display` is the single \
              field a wearer reads as a verdict, and an uncontracted field is \
              one nothing stops the model from writing itself.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "hud_field_scout",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose for the audio channel, where there are no markers at all \
              and therefore no typographic cue to carry provenance. It is \
              scanned for the same reason every other summary here is: a \
              stripped edibility verdict reappearing as a spoken sentence is \
              the worst outcome this contract can produce.",
        cross_check_sql: None,
    },
];

// ─── enforcement ───────────────────────────────────────────────────────

/// A field that was populated when it had no business being.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// Dotted path of the offending field.
    pub path: String,
    /// What the model put there. Retained rather than discarded so the
    /// caller can quarantine it for later comparison against a real source
    /// — the difference between "tag for reprocessing" and "delete".
    pub removed: Value,
    /// Machine-stable reason, for anomaly payloads.
    pub kind: ViolationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// An `Unsourced` field was non-null.
    UngroundedField,
    /// A `Narrative` field asserted something an unsourced block would have
    /// had to supply.
    NarrativeLeak,
    /// A `Sourced` field disagreed with the canonical record it claims to
    /// come from.
    ///
    /// This kind exists because `Sourced` turned out to be a weaker claim
    /// than it reads as. It asserts *a tool could supply this field* — not
    /// *this value came from that tool*. `Antaxius beieri` is the canonical
    /// case: a bush-cricket (Orthoptera / Tettigoniidae) whose profile
    /// confidently reported Coleoptera / Cerambycidae and called it a
    /// longhorn beetle, while the GBIF-verified answer sat one table over on
    /// the creature record. Every check passed: the field was present,
    /// non-null, correctly typed, and declared sourced.
    ContradictsCanonical,
}

/// What [`enforce`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub violations: Vec<Violation>,
    /// `(block, provenance)` pairs written onto the document.
    pub provenance: Vec<(String, &'static str)>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Is this value an actual claim, as opposed to absent or a placeholder?
///
/// `"..."` counts as absent: it is the card's own schema-example filler, and
/// a model echoing it has declined to answer rather than fabricated one.
fn is_claim(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => {
            let t = s.trim();
            !t.is_empty() && t != "..." && t != "null" && t != "N/A" && t != "-"
        }
        Value::Array(a) => a.iter().any(is_claim),
        Value::Object(o) => o.values().any(is_claim),
        _ => true,
    }
}

/// Collect every value a path selects.
///
/// A `[]` segment means "each element of this array", so
/// `threats[].species` selects the species of every reported threat. Needed
/// because the second agent under the contract keeps its interesting fields
/// inside arrays, and a contract that can only address top-level scalars
/// would silently decline to check them — passing by being unable to look.
fn select<'a>(doc: &'a Value, segs: &[&str]) -> Vec<&'a Value> {
    let Some((head, rest)) = segs.split_first() else {
        return vec![doc];
    };
    if *head == "[]" {
        return match doc.as_array() {
            Some(items) => items.iter().flat_map(|it| select(it, rest)).collect(),
            None => vec![],
        };
    }
    match doc.get(head) {
        Some(v) => select(v, rest),
        None => vec![],
    }
}

/// Split a dotted path, turning `foo[]` into `foo` + `[]`.
fn segments(path: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for seg in path.split('.') {
        if let Some(name) = seg.strip_suffix("[]") {
            out.push(name);
            out.push("[]");
        } else {
            out.push(seg);
        }
    }
    out
}

fn get_path<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    select(doc, &segments(path)).into_iter().next()
}

/// Does any value this path selects constitute a claim?
fn path_has_claim(doc: &Value, path: &str) -> bool {
    select(doc, &segments(path)).into_iter().any(is_claim)
}

/// Replace the value at `path` with `null`, returning what was there.
///
/// Null rather than removed: an absent key is indistinguishable from a
/// serialisation bug or a truncated response, and the UI needs to be able to
/// say "we do not have this" rather than render a blank that reads as a
/// loading error.
fn null_path(doc: &mut Value, path: &str) -> Option<Value> {
    let removed = null_all(doc, &segments(path));
    match removed.len() {
        0 => None,
        1 => removed.into_iter().next(),
        // Several array elements were cleared. Report them together so the
        // caller can quarantine the lot; a violation per element would spam
        // the anomaly log with one row per threat in a list.
        _ => Some(Value::Array(removed)),
    }
}

/// Null every value the path selects, returning the non-null ones removed.
fn null_all(doc: &mut Value, segs: &[&str]) -> Vec<Value> {
    let Some((head, rest)) = segs.split_first() else {
        if doc.is_null() {
            return vec![];
        }
        return vec![std::mem::replace(doc, Value::Null)];
    };
    if *head == "[]" {
        return match doc.as_array_mut() {
            Some(items) => items.iter_mut().flat_map(|it| null_all(it, rest)).collect(),
            None => vec![],
        };
    }
    match doc.get_mut(head) {
        Some(v) => null_all(v, rest),
        None => vec![],
    }
}

/// Top-level block a dotted path belongs to.
fn block_of(path: &str) -> &str {
    let head = path.split('.').next().unwrap_or(path);
    // Strip the array marker: `threats[].species` belongs to block
    // `threats`, which is where its `_provenance` sibling goes. Without
    // this the block was named `threats[]`, nothing matched, and every
    // array-bearing agent got no provenance at all -- silently, because an
    // absent key is indistinguishable from an agent nobody has contracted.
    head.strip_suffix("[]").unwrap_or(head)
}

/// Contracts for one agent.
pub fn contracts_for(agent_id: &str) -> impl Iterator<Item = &'static FieldContract> + '_ {
    FIELD_CONTRACTS
        .iter()
        .filter(move |c| c.agent_id == agent_id)
}

/// Check `Sourced` fields against the canonical record they claim to derive
/// from, overwriting disagreements and reporting them.
///
/// ## Why this is separate from `enforce`
///
/// `enforce` answers *could this value have come from anywhere*. It cannot
/// answer *did it*, because it never sees the tool response. So a model that
/// calls GBIF and then writes taxonomy from memory produces a document
/// `enforce` passes completely.
///
/// The cheap fix is not to verify the tool response but to **stop
/// regenerating what is already known**. A Rabble creature row already
/// carries GBIF-verified `taxonomy` and a `gbif_key`; the profile has no
/// business re-deriving it. Passing that record in here makes the canonical
/// copy authoritative: a mismatch is overwritten, not merely flagged, because
/// a reader who sees a corrected value is better served than one who sees a
/// wrong value with a warning attached.
///
/// `canonical` is a document whose shape mirrors the output. Only paths that
/// are present in BOTH the canonical record and the contract as `Sourced` are
/// compared — a canonical record that happens to be missing a field must not
/// erase a legitimately retrieved one.
pub fn reconcile(agent_id: &str, doc: &mut Value, canonical: &Value) -> Vec<Violation> {
    let mut out = Vec::new();
    let sourced: Vec<&'static str> = contracts_for(agent_id)
        .filter(|c| matches!(c.grounding, Grounding::Sourced { .. }))
        .map(|c| c.path)
        .collect();

    // Compare leaf-by-leaf beneath each Sourced path, so a contract naming a
    // whole block (`taxonomy`) still checks the fields inside it.
    for path in sourced {
        let Some(expected_node) = get_path(canonical, path) else {
            continue;
        };
        match expected_node {
            Value::Object(fields) => {
                for (leaf, expected) in fields {
                    if !is_claim(expected) {
                        continue;
                    }
                    let leaf_path = format!("{path}.{leaf}");
                    let Some(actual) = get_path(doc, &leaf_path) else {
                        continue;
                    };
                    if !is_claim(actual) || values_agree(actual, expected) {
                        continue;
                    }
                    let removed = actual.clone();
                    if set_path(doc, &leaf_path, expected.clone()) {
                        out.push(Violation {
                            path: leaf_path,
                            removed,
                            kind: ViolationKind::ContradictsCanonical,
                        });
                    }
                }
            }
            expected => {
                let Some(actual) = get_path(doc, path) else {
                    continue;
                };
                if is_claim(actual) && !values_agree(actual, expected) {
                    let removed = actual.clone();
                    if set_path(doc, path, expected.clone()) {
                        out.push(Violation {
                            path: path.to_string(),
                            removed,
                            kind: ViolationKind::ContradictsCanonical,
                        });
                    }
                }
            }
        }
    }
    out
}

/// Compare two values as an author would: case- and whitespace-insensitively
/// for strings, exactly otherwise.
///
/// Deliberately not fuzzy beyond that. "Coleoptera" and "Orthoptera" are not
/// near-misses, and a comparison loose enough to forgive a real contradiction
/// would defeat the check.
fn values_agree(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => x.trim().eq_ignore_ascii_case(y.trim()),
        _ => a == b,
    }
}

/// Write `value` at a dotted path, returning whether the slot existed.
fn set_path(doc: &mut Value, path: &str, value: Value) -> bool {
    let segs: Vec<&str> = path.split('.').collect();
    let Some((last, parents)) = segs.split_last() else {
        return false;
    };
    let mut cur = doc;
    for seg in parents {
        match cur.get_mut(seg) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    match cur.get_mut(last) {
        Some(slot) => {
            *slot = value;
            true
        }
        None => false,
    }
}

/// Strip ungrounded values from `doc`, stamp provenance, and report what
/// was wrong.
///
/// A no-op for agents with no declared contract: silence must not read as a
/// verdict. `port_census.py` is what reports the absence of a contract;
/// this function must not invent one.
pub fn enforce(agent_id: &str, doc: &mut Value) -> Report {
    let contracts: Vec<&FieldContract> = contracts_for(agent_id).collect();
    if contracts.is_empty() || !doc.is_object() {
        return Report::default();
    }

    let mut report = Report::default();

    // 1. Ungrounded structured fields.
    // A document written before any contract existed cannot have
    // tool-verified fields, whatever the contract says today.
    let pre_contract = doc.get(PRE_CONTRACT_MARKER).is_some();

    for c in &contracts {
        let treat_as_unsourced = c.grounding == Grounding::Unsourced
            || (pre_contract && c.grounding != Grounding::Narrative);
        if !treat_as_unsourced {
            continue;
        }
        let populated = path_has_claim(doc, c.path);
        if populated {
            if let Some(removed) = null_path(doc, c.path) {
                report.violations.push(Violation {
                    path: c.path.to_string(),
                    removed,
                    kind: ViolationKind::UngroundedField,
                });
            }
        }
    }

    // 2. Which blocks ended up with any real, sourced content?
    let mut sourced_block_has_content: Vec<(&str, bool)> = Vec::new();
    for c in &contracts {
        if let Grounding::Sourced { .. } = c.grounding {
            let b = block_of(c.path);
            let has = !pre_contract && path_has_claim(doc, c.path);
            match sourced_block_has_content.iter_mut().find(|(n, _)| *n == b) {
                Some(entry) => entry.1 |= has,
                None => sourced_block_has_content.push((b, has)),
            }
        }
    }

    // Blocks whose content is judgement rather than retrieval.
    let inferred_blocks: Vec<&str> = contracts
        .iter()
        .filter(|c| matches!(c.grounding, Grounding::Inferred { .. }))
        .map(|c| block_of(c.path))
        .collect();

    // Blocks computed by platform code from a sourced value. Ranked above
    // `inferred` when both are present in a block, because a reproducible
    // derivation is the stronger claim and the label should say so.
    let derived_blocks: Vec<&str> = contracts
        .iter()
        .filter(|c| matches!(c.grounding, Grounding::Derived { .. }))
        .map(|c| block_of(c.path))
        .collect();
    let block_is_sourced = |b: &str| {
        sourced_block_has_content
            .iter()
            .find(|(n, _)| *n == b)
            .map(|(_, has)| *has)
    };

    // 3. Narrative leaks — claims the sourced blocks cannot support.
    for c in &contracts {
        if c.grounding != Grounding::Narrative {
            continue;
        }
        let Some(text) = get_path(doc, c.path).and_then(|v| v.as_str()) else {
            continue;
        };
        let haystack = text.to_ascii_lowercase();
        let mut leaked = false;
        for (block, rule) in NARRATIVE_LEAKS {
            // Only a leak if that block is not actually sourced here.
            if block_is_sourced(block) == Some(true) {
                continue;
            }
            if rule.matches(&haystack) {
                leaked = true;
                break;
            }
        }
        if leaked {
            // Nulled, not merely flagged. A validator cannot rewrite prose
            // into honesty, and leaving the sentence in place would move the
            // fabrication into the one string a user actually reads. The
            // text is retained on the violation so the claim can be checked
            // against a real source later rather than lost.
            //
            // Nulling is also what makes enforcement idempotent: a cached
            // profile re-read through this function must not raise a fresh
            // anomaly every time.
            let removed = null_path(doc, c.path).unwrap_or(Value::Null);
            report.violations.push(Violation {
                path: c.path.to_string(),
                removed,
                kind: ViolationKind::NarrativeLeak,
            });
        }
    }

    // 4. Stamp `<block>_provenance`. Every block a contract mentions gets
    //    one, so a consumer never has to infer availability from emptiness.
    let mut blocks: Vec<&str> = Vec::new();
    for c in &contracts {
        let b = block_of(c.path);
        if c.grounding != Grounding::Narrative && !blocks.contains(&b) {
            blocks.push(b);
        }
    }
    // A block that is only ever a narrative gets no provenance key: the
    // summary is not a data block and labelling it as one would imply a
    // retrieval claim it never makes.
    for b in blocks {
        // A pre-contract document consulted no tool, so no block of it can
        // claim one. Deliberately reusing `unavailable_no_tool_source` rather
        // than minting a fifth value: the card schemas declare the closed set
        // in their provenance enums, and a verdict they cannot express would
        // make every legacy document fail validation for the wrong reason.
        let verdict = if pre_contract {
            PROV_UNAVAILABLE
        } else {
            match block_is_sourced(b) {
                // Has sourced fields, and at least one came back populated.
                Some(true) => PROV_TOOL,
                // Has sourced fields and none came back: the tool was asked and
                // had nothing. Distinct from "no tool exists".
                Some(false) => PROV_NO_MATCH,
                // No sourced field. A reproducible derivation outranks a model
                // judgement; anything else genuinely has no source.
                None if derived_blocks.contains(&b) => PROV_DERIVED,
                None if inferred_blocks.contains(&b) => PROV_INFERRED,
                None => PROV_UNAVAILABLE,
            }
        };
        if let Some(obj) = doc.as_object_mut() {
            obj.insert(format!("{b}_provenance"), Value::String(verdict.into()));
        }
        report.provenance.push((b.to_string(), verdict));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A document shaped exactly like the ones in production today.
    fn fabricated() -> Value {
        json!({
            "taxonomy": {
                "kingdom": "Animalia", "phylum": "Arthropoda", "class": "Insecta",
                "order": "Lepidoptera", "family": "Nymphalidae",
                "genus": "Apatura", "species": "Apatura iris"
            },
            "genome": {
                "estimated_size_mb": "420-480",
                "chromosome_count": "n=31 (diploid 2n=62)",
                "notable_genes": ["opsin duplications"],
                "ploidy": "diploid"
            },
            "phylogeny": {
                "superorder": "Holometabola",
                "sister_taxa": ["Apatura ilia"],
                "divergence_mya": "~90-100 million years ago",
                "defining_traits": ["iridescent scales"]
            },
            "conservation": {
                "iucn_status": "Not Evaluated (presumed Least Concern)",
                "population_trend": "stable",
                "genetic_diversity_notes": "no known concerns"
            },
            "summary": "Apatura iris sits in Nymphalidae with a ~450 Mb genome, \
                        having diverged ~90 MYA."
        })
    }

    #[test]
    fn provenance_values_are_closed() {
        for (_, rule) in NARRATIVE_LEAKS {
            let needle = match rule {
                LeakRule::Word(w) => *w,
                LeakRule::Quantity(u) => *u,
            };
            assert!(!needle.is_empty(), "an empty needle matches everything");
            assert_eq!(
                needle,
                needle.to_ascii_lowercase(),
                "needles are matched against a lowercased haystack, so an \
                 uppercase needle can never fire: {needle}"
            );
        }
        for (block, _) in NARRATIVE_LEAKS {
            assert!(
                FIELD_CONTRACTS.iter().any(|c| block_of(c.path) == *block),
                "leak rule names block `{block}`, which no field contract \
                 mentions — the rule can never be adjudicated"
            );
        }
    }

    /// No card may declare a provenance value the runtime cannot emit.
    ///
    /// This exists because the drift actually happened, during the work that
    /// introduced it. `PROV_GBIF = "gbif_verified"` was renamed to
    /// `PROV_TOOL = "tool_verified"` when a second agent joined the contract
    /// and a status string naming one specific tool stopped being true. The
    /// Rust constant changed; `genome_profiler`'s card kept declaring the old
    /// enum. Nothing noticed until a schema-validation test compared the two,
    /// by which point the card was asserting a vocabulary its own platform
    /// had abandoned.
    ///
    /// A closed vocabulary split across two files is only closed if something
    /// checks both.
    #[test]
    fn no_card_declares_a_provenance_value_the_runtime_cannot_emit() {
        let mut checked = 0usize;
        for entry in std::fs::read_dir("agents").expect("agents/") {
            let tier = entry.expect("dir").path();
            if !tier.is_dir() {
                continue;
            }
            for agent in std::fs::read_dir(&tier).expect("tier") {
                let path = agent.expect("dir").path().join("agent_card.json");
                if !path.exists() {
                    continue;
                }
                let raw = std::fs::read_to_string(&path).expect("read card");
                let card: Value = serde_json::from_str(&raw)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                let Some(props) = card
                    .pointer("/capabilities/output_contract/schema/properties")
                    .and_then(|p| p.as_object())
                else {
                    continue;
                };
                for (field, spec) in props {
                    if !field.ends_with("_provenance") {
                        continue;
                    }
                    checked += 1;
                    let declared: Vec<String> = match (spec.get("enum"), spec.get("const")) {
                        (Some(Value::Array(a)), _) => a
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect(),
                        (_, Some(Value::String(c))) => vec![c.clone()],
                        _ => continue,
                    };
                    for v in declared {
                        assert!(
                            PROVENANCE_VALUES.contains(&v.as_str()),
                            "{}: `{field}` declares provenance value `{v}`, which is \
                             not in PROVENANCE_VALUES {PROVENANCE_VALUES:?}. The card \
                             and the runtime have drifted — a closed vocabulary split \
                             across two files is only closed if something checks both.",
                            path.display()
                        );
                    }
                }
            }
        }
        assert!(
            checked > 0,
            "no provenance enums found in any card — this guard is inert"
        );
    }

    /// **The completeness tripwire.**
    ///
    /// Every `Sourced` field must either declare a cross-check or appear in
    /// [`CROSS_CHECK_EXEMPTIONS`] with a reason. Neither is optional.
    ///
    /// This test is the answer to "why did the verification system miss a
    /// bush-cricket reported as a beetle". It missed it because `Sourced`
    /// asserted *a tool could supply this* and nothing ever compared the
    /// value to anything — and nothing in the contract required anyone to
    /// notice that gap. `rollup_trust` had already solved the identical
    /// problem for database columns via `mismatch_sql`; the grounding
    /// contract simply stopped one rung short and said nothing about it.
    ///
    /// The claim this test enforces is deliberately weak and therefore
    /// keepable: **not that every sourced field is verified, but that no
    /// sourced field is *silently* unverified.**
    #[test]
    fn every_sourced_field_is_verifiable_or_admits_it_is_not() {
        let mut unaccounted = Vec::new();
        for c in FIELD_CONTRACTS {
            if !matches!(c.grounding, Grounding::Sourced { .. }) {
                continue;
            }
            if c.cross_check_sql.is_none() && !cross_check_exempt(c.agent_id, c.path) {
                unaccounted.push(format!("{}.{}", c.agent_id, c.path));
            }
        }
        assert!(
            unaccounted.is_empty(),
            "{} `Sourced` field(s) can neither be cross-checked nor say why \
             not:\n  {}\n\nDeclaring a field `Sourced` asserts a tool COULD \
             supply it. It does not assert the value CAME from that tool — \
             which is how `Antaxius beieri` was profiled as a cerambycid \
             beetle with every check green. Either add a `cross_check_sql` \
             comparing it against an independently-held copy, or add it to \
             CROSS_CHECK_EXEMPTIONS with what it would take to fix.",
            unaccounted.len(),
            unaccounted.join("\n  ")
        );
    }

    #[test]
    fn cross_check_queries_are_read_only_and_shaped_for_the_harness() {
        // Same discipline as rollup_trust: these run against production.
        for (agent, path, sql) in cross_checks() {
            let lower = sql.to_lowercase();
            assert!(
                lower.trim_start().starts_with("select"),
                "{agent}.{path}: cross_check_sql must be a bare SELECT"
            );
            assert!(
                lower.contains("as mismatches"),
                "{agent}.{path}: must alias its count as `mismatches`"
            );
            for forbidden in [
                "insert ",
                "update ",
                "delete ",
                "drop ",
                "alter ",
                "truncate ",
                "grant ",
            ] {
                assert!(
                    !lower.contains(forbidden),
                    "{agent}.{path}: must not contain `{forbidden}` — this runs \
                     against a live database"
                );
            }
        }
    }

    #[test]
    fn cross_check_exemptions_are_real_and_reference_real_fields() {
        for (agent, path, why) in CROSS_CHECK_EXEMPTIONS {
            assert!(
                FIELD_CONTRACTS
                    .iter()
                    .any(|c| c.agent_id == *agent && c.path == *path),
                "exemption for {agent}.{path} names no declared field"
            );
            assert!(
                FIELD_CONTRACTS.iter().any(|c| c.agent_id == *agent
                    && c.path == *path
                    && matches!(c.grounding, Grounding::Sourced { .. })),
                "{agent}.{path} is exempted from cross-checking but is not \
                 Sourced — only Sourced fields make a retrieval claim"
            );
            assert!(
                why.len() > 60,
                "exemption for {agent}.{path} needs to say why not AND what \
                 would fix it, not `{why}`"
            );
        }
    }

    #[test]
    fn at_least_one_cross_check_exists_or_this_is_all_theatre() {
        // A contract where every field is exempt satisfies the tripwire above
        // while verifying nothing. Guard against arriving there by attrition.
        assert!(
            cross_checks().count() >= 2,
            "the empirical tier has fewer than two real cross-checks; the \
             completeness claim would be satisfied entirely by exemptions"
        );
    }

    #[test]
    fn every_contract_explains_itself() {
        for c in FIELD_CONTRACTS {
            assert!(
                c.why.len() > 40,
                "{}.{} needs a real justification, not `{}` — an \
                 unexplained entry is how the contract rots",
                c.agent_id,
                c.path,
                c.why
            );
        }
    }

    #[test]
    fn strips_every_ungrounded_field_and_keeps_what_it_removed() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        // Still unsourceable: no tool supplies these, so they go.
        for path in [
            "genome.ploidy",
            "genome.notable_genes",
            "phylogeny.divergence_mya",
            "phylogeny.defining_traits",
            "conservation.iucn_status",
            "conservation.population_trend",
            "conservation.genetic_diversity_notes",
        ] {
            assert!(
                get_path(&doc, path).unwrap().is_null(),
                "{path} still has no source and must be stripped"
            );
            assert!(
                report.violations.iter().any(|v| v.path == path),
                "{path} was stripped without being reported"
            );
        }

        let iucn = report
            .violations
            .iter()
            .find(|v| v.path == "conservation.iucn_status")
            .unwrap();
        assert_eq!(
            iucn.removed,
            json!("Not Evaluated (presumed Least Concern)"),
            "the fabricated value must be retained for reprocessing"
        );
    }

    /// The fields the NCBI integration gave back.
    ///
    /// Before `ncbi_genome_search` existed, `enforce` nulled genome size and
    /// chromosome count and stamped the block `unavailable_no_tool_source`.
    /// The contract entries changed and nothing else did: same enforcement
    /// code, same card, opposite outcome. That is the property worth having —
    /// grounding is a statement about the tools available, so wiring up a tool
    /// is the whole fix.
    #[test]
    fn newly_sourced_fields_come_back() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        for path in ["genome.estimated_size_mb", "genome.chromosome_count"] {
            assert!(
                !get_path(&doc, path).unwrap().is_null(),
                "{path} is sourced from ncbi_genome_search now and must survive"
            );
            assert!(
                !report.violations.iter().any(|v| v.path == path),
                "{path} must no longer be reported as a violation"
            );
        }

        assert_eq!(
            doc.get("genome_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL),
            "the genome block has a real tool behind it now"
        );
    }

    /// A derivation is not a retrieval and not a guess.
    #[test]
    fn a_derived_field_survives_and_says_it_was_computed() {
        let mut doc = json!({
            "taxonomy": { "order": "Lepidoptera" },
            "phylogeny": { "superorder": "Holometabola", "sister_taxa": [] },
            "summary": "A nymphalid."
        });
        let report = enforce("genome_profiler", &mut doc);
        assert_eq!(
            get_path(&doc, "phylogeny.superorder").unwrap(),
            &json!("Holometabola"),
            "superorder is derivable from taxonomy.order by a closed table"
        );
        assert!(!report
            .violations
            .iter()
            .any(|v| v.path == "phylogeny.superorder"));
    }

    #[test]
    fn does_not_touch_the_one_phylogeny_field_that_is_real() {
        let mut doc = fabricated();
        enforce("genome_profiler", &mut doc);
        assert_eq!(
            get_path(&doc, "phylogeny.sister_taxa").unwrap(),
            &json!(["Apatura ilia"]),
            "gbif_taxonomy_tree returns sibling taxa; stripping it would be \
             a check that overreaches, and those get switched off"
        );
    }

    #[test]
    fn taxonomy_survives_untouched() {
        let mut doc = fabricated();
        let before = doc.get("taxonomy").cloned().unwrap();
        enforce("genome_profiler", &mut doc);
        assert_eq!(doc.get("taxonomy"), Some(&before));
        assert_eq!(
            doc.get("taxonomy_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL)
        );
    }

    #[test]
    fn the_narrative_is_not_a_loophole() {
        // Partial sourcing means partial leak detection, which is the correct
        // behaviour and worth pinning: now that genome size HAS a tool, prose
        // may cite it. Conservation still does not, so prose may not.
        let mut doc = fabricated();
        doc["summary"] = json!("Apatura iris is Not Evaluated by the IUCN and of least concern.");
        let report = enforce("genome_profiler", &mut doc);
        let leak = report
            .violations
            .iter()
            .find(|v| v.path == "summary" && v.kind == ViolationKind::NarrativeLeak);
        assert!(
            leak.is_some(),
            "the summary asserts a conservation status no tool can supply"
        );
        assert!(doc.get("summary").unwrap().is_null());
    }

    #[test]
    fn prose_may_cite_a_field_once_it_has_a_tool() {
        // The same sentence that was a leak before the NCBI integration is
        // legitimate after it. A leak rule keyed to a word rather than to
        // whether the block is sourced would have got this wrong forever.
        let mut doc = fabricated();
        doc["summary"] = json!("Apatura iris has a genome of roughly 450 Mb.");
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            !report.violations.iter().any(|v| v.path == "summary"),
            "genome is sourced now, so citing a size is not a fabrication: {:?}",
            report.violations
        );
    }

    #[test]
    fn an_honest_narrative_is_left_alone() {
        let mut doc = fabricated();
        doc["summary"] = json!(
            "Apatura iris is a nymphalid butterfly; GBIF places it in \
             Apatura alongside Apatura ilia."
        );
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            !report.violations.iter().any(|v| v.path == "summary"),
            "a summary that claims only what taxonomy supports must pass, or \
             the check cries wolf and gets ignored: {:?}",
            report.violations
        );
    }

    #[test]
    fn unsourced_blocks_are_labelled_as_unavailable_not_as_empty() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        // `conservation` still has no tool — IUCN needs a token nobody has
        // supplied yet — so it must say why it is empty rather than just be
        // empty.
        assert_eq!(
            doc.get("conservation_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE),
            "a null field with no provenance is indistinguishable from a \
             loading error"
        );
        assert!(report
            .provenance
            .iter()
            .all(|(_, v)| PROVENANCE_VALUES.contains(v)));
    }

    #[test]
    fn a_gbif_miss_is_distinguishable_from_a_missing_tool() {
        let mut doc = fabricated();
        doc["taxonomy"] = json!({});
        doc["phylogeny"]["sister_taxa"] = Value::Null;
        let _ = enforce("genome_profiler", &mut doc);
        assert_eq!(
            doc.get("taxonomy_provenance").and_then(|v| v.as_str()),
            Some(PROV_NO_MATCH),
            "GBIF was asked and had nothing — materially different from \
             'no tool exists', and prompt 2's IUCN 'Not Evaluated' case is \
             the same distinction"
        );
        assert_eq!(
            doc.get("conservation_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE),
            "conservation has no tool at all, which is not the same as GBIF \
             having no match"
        );
    }

    #[test]
    fn enforcement_is_idempotent() {
        let mut doc = fabricated();
        let first = enforce("genome_profiler", &mut doc);
        let second = enforce("genome_profiler", &mut doc);
        assert!(!first.is_clean());
        assert!(
            second.is_clean(),
            "a second pass must find nothing; otherwise the validator would \
             log an anomaly every time a cached profile is re-read: {:?}",
            second.violations
        );
    }

    #[test]
    fn placeholders_are_an_absence_not_a_fabrication() {
        let mut doc = json!({
            "taxonomy": {"species": "Apatura iris"},
            "genome": {"estimated_size_mb": "...", "ploidy": ""},
            "summary": "A nymphalid."
        });
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            report.violations.is_empty(),
            "the card's own example used `...` as filler; a model echoing it \
             has declined to answer, not invented one: {:?}",
            report.violations
        );
    }

    // ── the second and third agents ─────────────────────────────

    #[test]
    fn a_well_grounded_agent_passes_completely() {
        // `enemy_sensor` reports creatures its scan returned and rates a
        // risk it was asked to judge. Nothing it produces is a retrieval
        // claim it cannot back.
        //
        // This is the single most important test in the module. A contract
        // under which every agent looks guilty is a contract nobody keeps,
        // and "the checker found a problem everywhere" is indistinguishable
        // from "the checker is broken".
        let mut doc = json!({
            "threat_level": "medium",
            "threats": [{
                "creature_id": "6f1a...",
                "species": "Aeshna cyanea",
                "relationship": "Odonata are aerial predators of Lepidoptera",
                "risk": "medium"
            }],
            "summary": "One dragonfly within the scan radius poses a moderate risk."
        });
        let report = enforce("enemy_sensor", &mut doc);
        assert!(
            report.is_clean(),
            "enemy_sensor is well-formed; flagging it would prove the \
             contract cannot tell an agent that fabricates from one that \
             reasons: {:?}",
            report.violations
        );
        assert_eq!(
            doc.get("threats_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL),
            "the threats block is retrieved-plus-judged, and its creature \
             rows came from the scan"
        );
    }

    #[test]
    fn a_judgement_is_labelled_as_judgement_not_stripped() {
        let mut doc = json!({
            "threat_level": "high",
            "threats": [],
            "summary": "No neighbours in range."
        });
        enforce("enemy_sensor", &mut doc);
        assert_eq!(
            doc.get("threat_level").and_then(|v| v.as_str()),
            Some("high"),
            "an Inferred field is the agent's product and must survive"
        );
        assert_eq!(
            doc.get("threat_level_provenance").and_then(|v| v.as_str()),
            Some(PROV_INFERRED),
            "but it must be labelled as reasoning rather than measurement"
        );
    }

    #[test]
    fn invented_coordinates_are_stripped_from_a_flight_plan() {
        // `scan_nearby_creatures` returns lat/lng for the TARGET only; every
        // nearby row carries h3_cell and no coordinates. So these waypoints
        // are numbers the agent was never given, in a document meant to be
        // acted on.
        let mut doc = json!({
            "prey_targets": [{
                "creature_id": "abc",
                "species": "Pieris rapae",
                "order": "Lepidoptera",
                "vulnerability": "high",
                "reasoning": "slow flier, no defences",
                "distance_cells": 3
            }],
            "flight_plan": {
                "approach": "downwind from the treeline",
                "waypoints": [
                    {"lat": 51.5072, "lng": -0.1276, "altitude_m": 12, "instruction": "climb"},
                    {"lat": 51.5081, "lng": -0.1290, "altitude_m": 8, "instruction": "intercept"}
                ],
                "intercept_strategy": "stoop from above",
                "estimated_distance_m": 140,
                "difficulty": "moderate"
            },
            "hunting_summary": "One viable target to the north-west.",
            "tactical_notes": "Approach low."
        });
        let report = enforce("prey_locator", &mut doc);

        for path in [
            "flight_plan.waypoints[].lat",
            "flight_plan.waypoints[].lng",
            "flight_plan.waypoints[].altitude_m",
            "flight_plan.estimated_distance_m",
            "prey_targets[].distance_cells",
        ] {
            assert!(
                report.violations.iter().any(|v| v.path == path),
                "{path} was invented and not reported"
            );
        }

        // Both array elements cleared, not just the first.
        let wps = doc["flight_plan"]["waypoints"].as_array().unwrap();
        assert_eq!(wps.len(), 2);
        for wp in wps {
            assert!(wp["lat"].is_null(), "every waypoint must be cleared");
            assert!(wp["lng"].is_null());
            assert_eq!(
                wp["instruction"].as_str(),
                Some("climb")
                    .filter(|_| wp["instruction"] == "climb")
                    .or(Some("intercept")),
                "the instruction is prose the agent may write; only the \
                 fabricated geometry goes"
            );
        }

        // And the judgement survives.
        assert_eq!(
            doc["flight_plan"]["approach"].as_str(),
            Some("downwind from the treeline")
        );
        assert_eq!(
            doc["prey_targets"][0]["species"].as_str(),
            Some("Pieris rapae"),
            "the scan supplied this; stripping it would be over-reach"
        );
    }

    #[test]
    fn array_paths_report_every_element_they_cleared() {
        let mut doc = json!({
            "prey_targets": [
                {"creature_id": "a", "distance_cells": 1},
                {"creature_id": "b", "distance_cells": 4}
            ]
        });
        let report = enforce("prey_locator", &mut doc);
        let v = report
            .violations
            .iter()
            .find(|v| v.path == "prey_targets[].distance_cells")
            .expect("array path must be checked, not skipped");
        assert_eq!(
            v.removed,
            json!([1, 4]),
            "both guesses retained for reprocessing, as one violation rather \
             than one row per array element"
        );
    }

    // ── Case 1 / Case 2 regression fixtures (2026-08-16 field report) ──

    /// `Sphingonotus personatus`, verbatim from `creature_conditions`.
    ///
    /// The UI rendered "8001200 Mb" and "21624 chromosomes (10812 pairs)".
    /// Neither number is in the data: the client coerced these free-text
    /// strings to numbers by stripping non-digits, so "800–1200" became
    /// 8001200 and "2n = 16–24 (typical for Acrididae)" became 21624. The
    /// chart marker sat near 8000 Mb because it plotted the coerced value
    /// while the label printed the string — one bug, not two sources.
    #[test]
    fn case_1_a_pre_contract_profile_is_stripped_even_now_the_field_is_sourced() {
        let mut doc = json!({
            "_grounding_review": { "reason": "written before migration 200" },
            "taxonomy": { "order": "Orthoptera", "family": "Acrididae",
                          "species": "Sphingonotus personatus" },
            "genome": { "estimated_size_mb": "800–1200",
                        "chromosome_count": "2n = 16–24 (typical for Acrididae)" },
            "phylogeny": { "sister_taxa": [] },
            "conservation": { "iucn_status": "Not Evaluated (NE)" },
            "summary": "An Italian sand grasshopper."
        });
        let report = enforce("genome_profiler", &mut doc);

        // The trap: `estimated_size_mb` became Sourced when NCBI was wired
        // up. Without the pre-contract marker these fabricated strings would
        // now survive, because the FIELD is sourceable even though THIS
        // VALUE never was.
        assert!(
            get_path(&doc, "genome.estimated_size_mb")
                .unwrap()
                .is_null(),
            "a value predating the tool must not be blessed by the tool's arrival"
        );
        assert!(get_path(&doc, "genome.chromosome_count").unwrap().is_null());
        assert!(get_path(&doc, "conservation.iucn_status")
            .unwrap()
            .is_null());
        assert!(report
            .violations
            .iter()
            .any(|v| v.path == "genome.estimated_size_mb"));
        assert_eq!(
            doc.get("genome_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE),
            "a pre-contract document cannot claim tool_verified"
        );
    }

    /// `Antaxius beieri` — a bush-cricket reported as a longhorn beetle.
    ///
    /// Canonical GBIF taxonomy (already on the creature row, gbif_key
    /// 1683920): Orthoptera / Tettigoniidae. The profile said Coleoptera /
    /// Cerambycidae. `enforce` passed it completely: the field is declared
    /// `Sourced`, and `Sourced` only ever asserted that a tool COULD supply
    /// it.
    #[test]
    fn case_2_enforce_alone_cannot_catch_a_contradicted_sourced_field() {
        let mut doc = json!({
            "taxonomy": { "order": "Coleoptera", "family": "Cerambycidae",
                          "genus": "Antaxius", "species": "Antaxius beieri" },
            "phylogeny": { "sister_taxa": ["Antaxius wroughtoni"] },
            "summary": "Antaxius beieri is a cerambycid beetle."
        });
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            !report
                .violations
                .iter()
                .any(|v| v.path.starts_with("taxonomy")),
            "this is the gap: grounding cannot see that a sourced value is wrong"
        );
        assert_eq!(
            get_path(&doc, "taxonomy.order").unwrap(),
            &json!("Coleoptera"),
            "and so the fabrication survives enforcement untouched"
        );
    }

    #[test]
    fn case_2_reconcile_corrects_it_against_the_creature_record() {
        let mut doc = json!({
            "taxonomy": { "order": "Coleoptera", "family": "Cerambycidae",
                          "genus": "Antaxius", "species": "Antaxius beieri" },
            "phylogeny": { "sister_taxa": ["Antaxius wroughtoni"] },
            "summary": "Antaxius beieri is a cerambycid beetle."
        });
        // Shape mirrors the output; this is `creatures.taxonomy` for gbif_key
        // 1683920.
        let canonical = json!({
            "taxonomy": { "order": "Orthoptera", "family": "Tettigoniidae" }
        });
        let violations = reconcile("genome_profiler", &mut doc, &canonical);

        assert_eq!(
            get_path(&doc, "taxonomy.order").unwrap(),
            &json!("Orthoptera")
        );
        assert_eq!(
            get_path(&doc, "taxonomy.family").unwrap(),
            &json!("Tettigoniidae")
        );
        assert_eq!(violations.len(), 2, "{violations:?}");
        assert!(violations
            .iter()
            .all(|v| v.kind == ViolationKind::ContradictsCanonical));
        let order = violations
            .iter()
            .find(|v| v.path == "taxonomy.order")
            .unwrap();
        assert_eq!(
            order.removed,
            json!("Coleoptera"),
            "the contradicted value is retained so the fabrication is auditable"
        );
    }

    #[test]
    fn reconcile_leaves_agreeing_and_absent_fields_alone() {
        let mut doc = json!({
            "taxonomy": { "order": "orthoptera", "family": "Tettigoniidae",
                          "genus": "Antaxius" }
        });
        // Case-insensitive agreement, and a canonical record that says
        // nothing about `genus` must not erase it.
        let canonical = json!({ "taxonomy": { "order": "Orthoptera" } });
        let v = reconcile("genome_profiler", &mut doc, &canonical);
        assert!(v.is_empty(), "{v:?}");
        assert_eq!(
            get_path(&doc, "taxonomy.genus").unwrap(),
            &json!("Antaxius")
        );
    }

    #[test]
    fn reconcile_does_not_touch_fields_that_are_not_sourced() {
        // `conservation.iucn_status` is Unsourced. A canonical record must not
        // become a back door for populating it.
        let mut doc = json!({ "conservation": { "iucn_status": null } });
        let canonical = json!({ "conservation": { "iucn_status": "LC" } });
        let v = reconcile("genome_profiler", &mut doc, &canonical);
        assert!(v.is_empty());
        assert!(get_path(&doc, "conservation.iucn_status")
            .unwrap()
            .is_null());
    }

    #[test]
    fn an_agent_with_no_contract_is_left_entirely_alone() {
        let mut doc = fabricated();
        let before = doc.clone();
        let report = enforce("weather_oracle", &mut doc);
        assert_eq!(doc, before, "silence is not a verdict");
        assert!(report.is_clean());
        assert!(report.provenance.is_empty());
    }

    // ─── the provenance floor ──────────────────────────────────────────
    //
    // The floor exists because provenance is not transitive upward. A rule
    // extracted from a well-sourced episode is not itself sourced, and a
    // rollup over a mixed set of episodes is only as good as its worst
    // member. Both of those are one-line arithmetic; both invert silently
    // if you get the empty case or the direction wrong, and an inverted
    // floor reads as *more* trustworthy than no floor at all. So each way
    // of getting it wrong gets a named test.

    #[test]
    fn an_empty_source_set_floors_to_unavailable_not_to_verified() {
        // `min` over an empty iterator has no answer, and the two ways of
        // supplying one are not symmetric: the identity element for `min`
        // is the *maximum* value, which here means "tool_verified". A rule
        // whose source cluster came back empty would then claim to be
        // measured. This is the single most likely way the floor breaks,
        // and it breaks in the direction that manufactures trust.
        let none: Vec<&str> = vec![];
        assert_eq!(floor(none), PROV_UNAVAILABLE);
        assert_eq!(extracted_floor(Vec::<&str>::new()), PROV_UNAVAILABLE);
    }

    #[test]
    fn the_floor_is_the_weakest_link_not_the_most_common_one() {
        // Nine sourced episodes and one guess is a guess. Averaging, or
        // taking a majority, would let volume launder a fabrication.
        let mut many = vec![PROV_TOOL; 9];
        many.push(PROV_UNAVAILABLE);
        assert_eq!(floor(many), PROV_UNAVAILABLE);
        assert_eq!(floor(vec![PROV_TOOL, PROV_INFERRED]), PROV_INFERRED);
        assert_eq!(floor(vec![PROV_TOOL, PROV_DERIVED]), PROV_TOOL);
        // `tool_no_match` survives rather than collapsing to
        // `unavailable_no_tool_source`. Both score 0, but they say different
        // things — "the tool answered and had nothing for this subject" versus
        // "no tool exists" — and the second is a claim about our tooling that
        // would be false here.
        assert_eq!(floor(vec![PROV_INFERRED, PROV_NO_MATCH]), PROV_NO_MATCH);
    }

    #[test]
    fn an_extraction_from_verified_sources_is_still_only_an_inference() {
        // The ceiling, which is the half of the rule the floor cannot
        // express. Reading ten tool-verified episodes and writing down a
        // semantic rule is an act of judgement about them; it does not
        // inherit their retrieval. Without this, the ontologist would
        // manufacture `tool_verified` facts out of nothing but its own
        // reading, and the KG would be full of claims no tool ever made.
        assert_eq!(
            extracted_floor(vec![PROV_TOOL, PROV_TOOL, PROV_DERIVED]),
            PROV_INFERRED
        );
    }

    #[test]
    fn the_ceiling_never_raises_a_weak_floor() {
        // The ceiling caps; it must not lift. An extraction from prose is
        // ungrounded, and `min(unavailable, inferred)` has to stay
        // unavailable rather than settling at the ceiling.
        assert_eq!(
            extracted_floor(vec![PROV_TOOL, PROV_UNAVAILABLE]),
            PROV_UNAVAILABLE
        );
        assert_eq!(extracted_floor(vec![PROV_UNAVAILABLE]), PROV_UNAVAILABLE);
    }

    #[test]
    fn the_floor_never_invents_a_verdict_that_was_not_there() {
        // The invariant the tier-representative version violated. A floor that
        // reports a mechanism nobody used is a misattribution, and it is the
        // hardest kind to notice because the strength is correct.
        let cases: &[&[&str]] = &[
            &[PROV_HUMAN_SOURCED],
            &[PROV_DERIVED],
            &[PROV_NO_MATCH],
            &[PROV_HUMAN_ENDORSED],
            &[PROV_REJECTED],
            &[PROV_TOOL, PROV_HUMAN_SOURCED],
            &[PROV_TOOL, PROV_PENDING_TOOL],
        ];
        for inputs in cases {
            let got = floor(inputs.iter().copied());
            assert!(
                inputs.contains(&got),
                "floor({inputs:?}) returned `{got}`, which no source claimed"
            );
        }
        // The single documented exception: with nothing to report, the floor
        // must still not read as clean.
        assert_eq!(floor(Vec::<&str>::new()), PROV_UNAVAILABLE);
    }

    #[test]
    fn an_unrecognised_verdict_is_worthless_rather_than_trusted() {
        // Vocabulary drift is the failure this module has already had once
        // (`gbif_verified` for `tool_verified`). If a stale or misspelled
        // verdict scored as strong, a renamed constant would quietly raise
        // every floor computed from it. Unknown means unknown.
        assert_eq!(strength("gbif_verified"), 0);
        assert_eq!(strength(""), 0);
        assert_eq!(
            floor(vec![PROV_TOOL, "verified_probably"]),
            PROV_UNAVAILABLE
        );
    }

    #[test]
    fn every_provenance_value_has_a_deliberate_strength() {
        // Adding a verdict to PROVENANCE_VALUES without deciding where it
        // sits would default it to 0 — safe, but silently, and a genuinely
        // strong new verdict reading as ungrounded would push authors to
        // stop trusting the floor. Force the decision at compile-time-ish.
        for v in PROVENANCE_VALUES {
            let expected = match *v {
                PROV_TOOL | PROV_DERIVED | PROV_HUMAN_SOURCED => 2,
                PROV_INFERRED | PROV_HUMAN_ENDORSED => 1,
                PROV_UNAVAILABLE | PROV_NO_MATCH | PROV_PENDING_TOOL | PROV_PENDING_HUMAN
                | PROV_REJECTED => 0,
                other => panic!(
                    "provenance value `{other}` has no declared strength; \
                     decide whether it is reproducible (2), a judgement (1), \
                     or an absence (0) and say so in `strength`"
                ),
            };
            assert_eq!(strength(v), expected, "strength of `{v}`");
        }
    }

    #[test]
    fn a_pending_claim_is_weaker_than_a_judgement_the_agent_was_asked_to_make() {
        // The ordering that makes the pending tier honest. `enemy_sensor` is
        // ASKED to rate predation risk: that judgement is its product and is
        // legitimate output. A retrieval claim with no retrieval behind it is
        // not yet anything at all. If pending outranked inference, an agent
        // could improve its floor by asserting an unsourced fact instead of
        // reasoning — rewarding exactly the behaviour the contract exists to
        // discourage.
        assert!(strength(PROV_PENDING_TOOL) < strength(PROV_INFERRED));
        assert!(strength(PROV_PENDING_HUMAN) < strength(PROV_INFERRED));
        assert_eq!(
            floor(vec![PROV_INFERRED, PROV_PENDING_TOOL]),
            // Reported as `pending_tool_check`, not flattened to
            // `unavailable_no_tool_source`: the first says a check is available
            // and owed, the second says no tool exists. Only one is a work item,
            // and losing that distinction loses the whole pending tier.
            PROV_PENDING_TOOL
        );
    }

    #[test]
    fn a_cited_human_check_is_worth_a_tool_call_and_an_uncited_one_is_not() {
        // The citation is the whole difference, and it is the only thing that
        // makes the human route safe: a verdict someone else can follow to the
        // same source is reproducible, which is all the ladder measures. Drop
        // the citation and it becomes an opinion — the same kind of claim a
        // model makes, and deferring to it because a person typed it is the
        // deference this module exists to remove.
        assert_eq!(strength(PROV_HUMAN_SOURCED), strength(PROV_TOOL));
        assert_eq!(strength(PROV_HUMAN_ENDORSED), strength(PROV_INFERRED));
        assert!(strength(PROV_HUMAN_ENDORSED) < strength(PROV_HUMAN_SOURCED));
    }

    #[test]
    fn a_rejected_claim_cannot_be_relied_on_and_cannot_lift_a_floor() {
        // Disproven and unknown are worth the same when you are deciding what
        // to trust; they differ in what happens next, which is routing rather
        // than reliance.
        assert_eq!(strength(PROV_REJECTED), 0);
        // `rejected` survives the floor. "Checked and found wrong" is the most
        // actionable thing the platform can say, and collapsing it would report
        // a disproven value as a merely missing one.
        assert_eq!(floor(vec![PROV_TOOL, PROV_REJECTED]), PROV_REJECTED);
    }

    #[test]
    fn a_response_floor_is_the_weakest_block_of_a_real_response() {
        // `enemy_sensor` retrieves its neighbour rows and judges the risk.
        // The response as a whole is therefore a judgement: one inferred
        // block drags the floor down even though the scan data is real.
        let response = json!({
            "threat_level": "medium",
            "threats": [{
                "creature_id": "6f1a...",
                "species": "Aeshna cyanea",
                "relationship": "Odonata are aerial predators of Lepidoptera",
                "risk": "medium"
            }],
            "summary": "One dragonfly within the scan radius poses a moderate risk."
        })
        .to_string();
        assert_eq!(
            response_floor("enemy_sensor", &response),
            Some(PROV_INFERRED)
        );
    }

    #[test]
    fn prose_has_no_floor_above_ungrounded() {
        // 74 of 100 curated agents return prose only. There are no typed
        // fields, so nothing was sourced, so an extraction from the text is
        // ungrounded by construction. This is the common case, and it is
        // supposed to look poor: that is the demand signal for tools.
        assert_eq!(
            response_floor("enemy_sensor", "Two dragonflies are nearby. Be careful."),
            Some(PROV_UNAVAILABLE)
        );
    }

    #[test]
    fn an_uncontracted_agent_has_an_unknown_floor_not_a_clean_one() {
        // Absence is not a verdict. `weather_oracle` has no field contract,
        // so we know nothing about its grounding, and `None` has to travel
        // all the way to storage as NULL rather than being coerced to the
        // best or the worst value on the way. Callers that cannot represent
        // "unknown" must refuse the row, not guess it.
        let response = json!({"forecast": "rain", "confidence": 0.8}).to_string();
        assert_eq!(response_floor("weather_oracle", &response), None);
    }

    #[test]
    fn a_historical_episode_with_no_retained_response_has_no_floor() {
        // Before migration 199 the response text was discarded. Nothing
        // about those episodes' groundedness is recoverable, and a floor
        // computed from an empty string must not read as a finding.
        assert_eq!(response_floor("enemy_sensor", ""), Some(PROV_UNAVAILABLE));
        assert_eq!(response_floor("weather_oracle", ""), Some(PROV_UNAVAILABLE));
    }
}
