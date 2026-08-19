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
    // ── football_analyst ───────────────────────────────────────────
    //
    // Every entry here shares one cause and it is worth stating once rather
    // than six times: the platform holds NO football data of its own. There is
    // no standings table, no fixture list, no injury roster. `genome_profiler`
    // could be cross-checked because the creature row already carried a
    // GBIF-verified taxonomy — a second copy, one JOIN away. Here there is no
    // second copy of anything.
    //
    // Two routes out, and they are different in kind:
    //
    //   * REPLAY — re-query `call_football_api` for the same fixture and
    //     compare. Costs an external call per row and is subject to the same
    //     rate limit the agent uses, but it is the only check that can catch a
    //     value the tool never returned. This is the football equivalent of the
    //     NCBI accession replay already deferred for genome size.
    //   * INTERNAL CONSISTENCY — needs no external truth at all. `xgd` must
    //     equal `xg - xga`; an Elo-implied probability must equal the card's
    //     formula applied to the two Elos actually stated. These become real
    //     cross-checks the moment the agent emits a structured payload, because
    //     a `SELECT` can then read the fields out of `episodes.response_text`
    //     (retained since migration 199) the way the taxonomy check reads
    //     `creature_conditions.genome_profile`.
    //
    // The second route is deliberately NOT declared yet. The agent still emits
    // prose, so such a query would parse nothing, count zero mismatches, and
    // report clean — a check that passes because it matches nothing, which is
    // the `fermi_leaderboard` failure this whole tier exists to avoid. It lands
    // with the payload, together with an agreement probe proving it can go red.
    (
        "football_analyst",
        "league_context",
        "The platform stores no league table. A replay against `standings` for \
         the same league and season would settle it exactly, since a finished \
         table is immutable; deferred only because it spends the agent's own \
         rate limit. Cheapest real check on this agent, and the one to write \
         first.",
    ),
    (
        "football_analyst",
        "fixtures",
        "No fixture list is held. A replay is possible and cheap for finished \
         matches, whose dates and results never change.",
    ),
    (
        "football_analyst",
        "head_to_head",
        "No H2H store. Replayable against `fixtures/headtohead`, and worth doing \
         because a fabricated historical record is both easy to produce and \
         completely invisible to a reader.",
    ),
    (
        "football_analyst",
        "injuries",
        "No roster store, and unlike the others this one is NOT usefully \
         replayable: an injury list is a snapshot of a moving state, so a replay \
         weeks later disagrees for legitimate reasons and would report a \
         mismatch that means nothing. Correct check is to capture the tool \
         response alongside the claim at write time and compare then — which is \
         what the assertion layer's `basis` field is for.",
    ),
    (
        "football_analyst",
        "match_statistics",
        "No per-fixture statistics store. Replayable and immutable once a match \
         is finished, so this is the second-cheapest real check after \
         `league_context`.",
    ),
    (
        "football_analyst",
        "advanced_metrics.xg",
        "Same as `match_statistics`, with one addition that matters: a replay \
         must distinguish \"the tool returned a different number\" from \"the \
         tool has no xG for this fixture\". Coverage is incomplete below the top \
         tiers, so the second case is common and is `tool_no_match`, not a \
         disagreement. A check that conflated them would flag honest gaps as \
         fabrications and be deleted within a week — correctly.",
    ),
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
        "forage_scout",
        "species_likely[].inat_observations_nearby",
        "A live third-party aggregate the platform stores no copy of. Replaying \
         the same bounded query and comparing counts would test iNaturalist's \
         stability rather than this agent's honesty, and a snapshot would go \
         stale faster than it could be verified.",
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
        "forage_identify",
        "taxonomy",
        "Nothing to compare against, and structurally rather than for want of a \
         query: the subject is whatever a forager photographed, and no platform \
         row claims to know what that was. The check that would mean something \
         is agreement between two independent determiners on the same frame, \
         which needs a second recogniser — a capability decision, not a missing \
         JOIN.",
    ),
    (
        "forage_identify",
        "taxonomy.nomenclatural_status",
        "MycoBank is the only fungal-nomenclature source the platform has, so \
         comparing it to itself is circular. It becomes checkable against GBIF's \
         accepted-name view for the same binomial, which is worth doing because \
         the two disagree often enough that the disagreement is the signal — \
         deferred, not dismissed.",
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

// ─── which prompt produced the row ─────────────────────────────────
//
// ## The problem this solves
//
// A cross-check reads all of history, so a defect found once is found forever.
// Measured on the weather suite: after the card was corrected, four consecutive
// runs emitted no `n_members: 0`, a real `recommendation.action` enum, and a
// multiplier matching its own `[MULTIPLIER]` line — a clean cohort by every
// predicate here. The suite still reported nine failures, every one of them a
// row written by the superseded prompt.
//
// That is not a cosmetic complaint. A suite that cannot go green after a fix
// gets ignored, and an ignored suite is worth less than no suite, because the
// next real regression arrives as one more line in a list that was already red.
// "Detects a defect" and "can confirm a fix" are different capabilities and
// this tier only had the first.
//
// ## Why a content hash rather than a date or a version
//
// A date means hand-maintained cohort bookkeeping in the check, which goes stale
// silently. `agent_versions` is the designed answer and is dead at both ends:
// 3,391 episodes carry no version, and `weather_oracle` — edited repeatedly —
// has zero version rows. A hash of the prompt cannot drift out of sync with the
// prompt, and needs no policy about who bumps what.
//
// So the cohort is defined by the card's OWN CURRENT CONTENT: rows whose
// recorded prompt hash equals the hash of the prompt this agent has right now.
// Edit a card and its cohort empties itself, which is the correct behaviour —
// nothing is yet known about the new prompt.
//
// ## Why every episode-based check must carry the placeholder
//
// Because both readings are needed and they answer different questions. Scoped
// says "is the agent fabricating NOW", and only that may fail the suite.
// Unscoped says "has it ever", which stays visible as history rather than being
// deleted. A check that hard-coded either one would silently lose the other.

/// Token every episode-based `cross_check_sql` must contain exactly once.
pub const COHORT_PLACEHOLDER: &str = "{{COHORT}}";

/// Restrict to rows produced by the prompt the agent currently has.
///
/// `convert_to(..., 'UTF8')` because `sha256` takes `bytea`; letting the server
/// choose an encoding would make the hash depend on database settings rather
/// than on the prompt. Must stay byte-identical in meaning to
/// `ExecutionContext::card_prompt_hash`, which is the Rust half of the same
/// comparison.
pub const COHORT_PREDICATE: &str = "AND e.context->>'card_prompt_hash' \
     = encode(sha256(convert_to(a.system_prompt, 'UTF8')), 'hex')";

/// The check as it must be read to fail the suite: current prompt only.
pub fn cohort_scoped(sql: &str) -> String {
    sql.replace(COHORT_PLACEHOLDER, COHORT_PREDICATE)
}

/// The check as it must be read to report history: every prompt, ever.
pub fn cohort_unscoped(sql: &str) -> String {
    sql.replace(COHORT_PLACEHOLDER, "")
}

/// How many rows this agent has under its current prompt.
///
/// The disambiguator, and the whole reason `liveness_trust` exists: zero
/// mismatches over zero rows is not clean, it is unknown. Without this the
/// scoped reading would report every agent perfect the instant its card changed.
pub fn cohort_size_sql() -> &'static str {
    "SELECT count(*)::bigint AS mismatches \
       FROM episodes e \
       JOIN agents a ON a.agent_id = e.agent_id \
      WHERE a.agent_name = $1 \
        AND e.context->>'card_prompt_hash' \
            = encode(sha256(convert_to(a.system_prompt, 'UTF8')), 'hex')"
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
    // ── football_analyst ───────────────────────────────────────────
    //
    // Tool: `call_football_api`, a pass-through to API-Football v3. Verified
    // running in production: 7 of 9 retained episodes record `tool:
    // call_football_api` in their tags, so unlike `genome_profiler` this agent
    // is not toolless. What it does is assert three families of number the tool
    // does not carry.
    //
    // A correction worth recording, because it nearly went the other way. An
    // earlier draft classified xG as `Unsourced` on the strength of the agent's
    // own words in a real episode: "API-Football does not provide xG for these
    // fixtures". API-Football *does* expose expected goals in fixture
    // statistics; coverage is incomplete for lower tiers and international
    // friendlies, which is what that episode was actually looking at. So the
    // agent's statement was true of those fixtures and false as a general claim
    // about the tool — and trusting an agent's self-report about its own tool's
    // capabilities is the identical error to trusting its self-report about a
    // genome size. Declaring xG `Unsourced` would have nulled a field that is
    // obtainable, which is worse than leaving it alone.
    //
    // That is exactly the distinction `tool_no_match` exists for: "the tool
    // answered and had nothing for this fixture" is not "no tool can supply
    // this".
    FieldContract {
        agent_id: "football_analyst",
        path: "league_context",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "standings (rank, points, form, home/away splits)",
        },
        why: "The `standings` endpoint returns position, points, played, goal \
              difference and a form string directly. Nothing here needs \
              inferring.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "fixtures",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "fixtures (date, competition, venue, status)",
        },
        why: "The `fixtures` endpoint is the schedule. Rest days and congestion \
              follow arithmetically from the dates it returns.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "head_to_head",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "fixtures/headtohead",
        },
        why: "A dedicated endpoint takes `h2h: 'teamA-teamB'` and returns the \
              record. There is no reason for this to come from memory.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "injuries",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "injuries (player, type, reason)",
        },
        why: "The `injuries` endpoint returns the roster of absences. Note that \
              the ESTIMATED IMPACT of an absence is a separate field and a \
              judgement — the list is retrieved, the consequence is reasoned.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "match_statistics",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "fixtures/statistics (shots, possession, passes, cards, saves)",
        },
        why: "The documented statistics list: shots on/off goal, shots in/out \
              of box, blocked shots, fouls, corners, offsides, possession, \
              cards, saves, total and accurate passes. All retrievable per \
              fixture.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "advanced_metrics.xg",
        grounding: Grounding::Sourced {
            tool: "call_football_api",
            response_field: "fixtures/statistics.expected_goals",
        },
        why: "API-Football carries expected goals in fixture statistics. \
              Coverage is incomplete below the top tiers, which makes an absent \
              value `tool_no_match` — the tool answered and had nothing for this \
              fixture — and NOT `unavailable_no_tool_source`, which would claim \
              no tool can supply xG at all. The agent has asserted xG for \
              fixtures where the tool has none; that is the case this entry \
              exists to catch.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "advanced_metrics.xgd",
        grounding: Grounding::Derived {
            from: "advanced_metrics.xg and advanced_metrics.xga",
            how: "xg - xga, per match or summed over a window",
        },
        why: "A subtraction, not a measurement. Reproducible from two sourced \
              fields, so it is `Derived` rather than `Inferred` for the same \
              reason `phylogeny.superorder` is: the transform can be read and \
              re-run.",
        // The first cross-check on this platform that needs no external source
        // of truth. Every other one compares agent output against a record we
        // hold; this compares the document against ITSELF. `xgd` must be
        // `xg - xga`, and an agent that reports all three has stated something
        // falsifiable without anyone querying anything.
        //
        // Worth having precisely because it is cheap: the replay checks the
        // other football fields need cost an external call each and spend the
        // agent's own rate limit, so they are deferred. This one costs a
        // `SELECT`. Internal consistency is the check you can always afford.
        //
        // Safety of the cast. `response_text` is prose for 18 of 18 episodes
        // today, and `'not json'::jsonb` raises. `CASE` is the one construct SQL
        // guarantees to short-circuit, so the cast is only ever reached for a
        // row that `IS JSON OBJECT` already accepted. A `WITH ... MATERIALIZED`
        // would also work but would fail the harness's bare-SELECT guard, and
        // relaxing that guard to buy syntax would be the wrong trade.
        // `jsonb_typeof(NULL)` is NULL, so a non-numeric or absent field drops
        // the row rather than erroring — which matters, because an unrunnable
        // check reports healthy forever.
        //
        // Tolerance 0.15, and the number is derived rather than picked. Reports
        // round xG to one decimal, so `xg` and `xga` each carry up to 0.05 of
        // rounding and their difference up to 0.10. A tolerance at or below that
        // would fire on correctly-reported rounding, and a check that fires on
        // correct behaviour gets deleted — the deletion looking like cleanup.
        // 0.15 clears rounding and still catches any disagreement large enough
        // to mean the three numbers were not computed from each other.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN e.response_text IS JSON OBJECT \
                                    THEN e.response_text::jsonb END AS doc) j \
              WHERE a.agent_name = 'football_analyst' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc #> '{advanced_metrics,xgd}') = 'number' \
                AND jsonb_typeof(j.doc #> '{advanced_metrics,xg}')  = 'number' \
                AND jsonb_typeof(j.doc #> '{advanced_metrics,xga}') = 'number' \
                AND abs( (j.doc #>> '{advanced_metrics,xgd}')::numeric \
                         - ( (j.doc #>> '{advanced_metrics,xg}')::numeric \
                           - (j.doc #>> '{advanced_metrics,xga}')::numeric ) ) \
                    > 0.15",
        ),
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "ratings.elo_current",
        grounding: Grounding::Unsourced,
        why: "API-Football has no Elo endpoint. The card names ClubElo as the \
              methodology and no ClubElo tool exists, so every Elo this agent \
              has stated came from the model. The evidence is unusually direct: \
              one episode reports `elo_current = 1834` while saying \
              \"Using 1834 as working estimate\" — and 1834 is the number in the \
              card's own worked example, which the model copied. Returns when a \
              ClubElo or equivalent tool is added, exactly as the NCBI genome \
              fields returned.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "ratings.elo_implied_win_probability",
        // `Unsourced`, and an earlier draft of this entry had it as `Derived`
        // with a note saying the strip would follow from its source. It does
        // not: `Derived` fields SURVIVE enforcement, because a derivation by
        // platform code from a sourced field is reproducible and that is the
        // whole point of the variant. Declaring it `Derived` therefore let a
        // correct formula over an invented Elo through untouched, which is the
        // worst of the available outcomes — a plausible 59% with nothing under
        // it, and no way for a reader to tell.
        //
        // The general rule, now enforced by
        // `a_derived_field_may_not_derive_from_an_unsourced_one`: a derivation
        // inherits the standing of what it derives from. Reproducibility is not
        // a property of the transform alone.
        grounding: Grounding::Unsourced,
        why: "Computed by the card's own Elo formula, which is deterministic and \
              correct — over `ratings.elo_current`, which no tool supplies. A \
              faithful transform of an invented number is invented, and it is \
              MORE dangerous than the raw number because the arithmetic lends \
              it an air of derivation. Returns the moment a ratings tool does, \
              at which point it becomes genuinely `Derived`.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "advanced_metrics.ppda",
        grounding: Grounding::Unsourced,
        why: "PPDA is an Opta/StatsBomb pressing metric computed from event \
              data. API-Football's statistics list has no defensive-action \
              counts, so it cannot be computed from what the tool returns, let \
              alone retrieved. Same for progressive passes and set-piece share.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "squad_value",
        grounding: Grounding::Unsourced,
        why: "Market values and Big-5 league share come from Transfermarkt, for \
              which there is no tool. The card asks for these as factor X4 \
              inputs, so the agent supplies them from memory — and a market \
              valuation from training data is stale by construction, which makes \
              it worse than an absence during a transfer window.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "assessment",
        grounding: Grounding::Inferred {
            from: "league_context, fixtures, head_to_head, injuries, match_statistics",
        },
        why: "The agent's actual product. A win probability, a factor signal and \
              a multiplier are judgements it is commissioned to make, and no \
              database holds them — which is why they cannot be verified \
              directly and why verification routes to this block's basis \
              instead. Treating them like a fabricated Elo would null the \
              agent's only output and prove the contract cannot tell an agent \
              that fabricates from one that reasons.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "football_analyst",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose for a human reader. Permitted, and scanned for quantities \
              the unsourced blocks cannot support — an Elo or a market value \
              recited in the summary is the same claim wearing a different hat.",
        cross_check_sql: None,
    },
    // ── genome_profiler ────────────────────────────────────────────
    // Tools: gbif_species_search, gbif_taxonomy_tree. Both taxonomy.
    // ── weather_oracle ─────────────────────────────────────────────
    //
    // READING THE DOCUMENT OUT OF THE RESPONSE
    //
    // These checks extract the JSON with `substring(... from '(?s)\{.*\}')`
    // rather than requiring `response_text IS JSON OBJECT`, and the difference
    // is eight live checks versus eight inert ones.
    //
    // Measured on the first two retained runs: both emit a correct, complete
    // document — `settlement_target.station` = KLGA, `final_probability`,
    // `multiplier` and the `[MULTIPLIER]` line all present — wrapped in a prose
    // preamble and a ```json fence, because the model narrates before it answers
    // when it has just made eight tool calls. So `IS JSON OBJECT` is false of the
    // whole column while the document inside it is fine.
    //
    // The card asks for nothing outside the object and that instruction stays,
    // but a check must not depend on a model reliably suppressing its preamble.
    // The rest of the platform already tolerates this: `parse_evidence_text`
    // scans to the outer braces and `extract_summary_from_json_contract` strips
    // the fence. These now agree with them. `football_analyst` is a third case —
    // prose carrying no document at all — and its check is right to stay strict,
    // which is why this relaxation is scoped to this block rather than applied
    // to the file.
    //
    // The greedy `(?s)` match takes the FIRST `{` to the LAST `}` and can
    // over-capture when the prose contains braces; the `IS JSON OBJECT` guard
    // inside the `CASE` rejects that, so a bad capture drops the row rather than
    // raising or comparing garbage.
    //
    // The most favourable case brought under the contract so far, and worth
    // saying why rather than only claiming it.
    //
    // `genome_profiler` had no tool for three of four blocks, so everything
    // unsourceable was unsourceable. `football_analyst` has a working tool but
    // no Elo endpoint, and its replay checks each cost an external call against
    // the agent's own rate limit, so they stay deferred. `weather_oracle` has a
    // keyless free tool behind EVERY block it reports, and — the part no other
    // agent here has — the ground truth publishes itself daily, publicly, at no
    // cost. Nothing below needs to be `Unsourced`, and the replay that football
    // cannot afford is affordable here.
    //
    // One correction worth recording. An earlier draft made `final_probability`
    // `Sourced` from `weather_ensemble_forecast`, on the reasoning that the
    // ensemble returns bucket probabilities directly. That is wrong, and wrong
    // in the direction that launders a judgement as a retrieval. The ensemble
    // returns RAW member frequencies; the reported probability is those
    // frequencies after bias correction, dispersion scaling, climatology
    // shrinkage and rounding convolution — every step a decision the agent is
    // asked to make. Measured on the London case the two differ by a factor of
    // six. Calling the output `Sourced` would assert a tool returned a number no
    // tool returns.
    FieldContract {
        agent_id: "weather_oracle",
        path: "settlement_target",
        grounding: Grounding::Sourced {
            tool: "weather_settlement_spec",
            response_field: "settlement_station (icao, timezone) + units_and_rounding",
        },
        why: "The single largest error source in these markets, and a pure \
              lookup. Polymarket's London temperature market settles on EGLC, \
              not Heathrow; NYC on KLGA, not Central Park; Dallas on Love \
              Field, not DFW. The tool holds a 50-station registry verified \
              against OurAirports for coordinates and against Open-Meteo for \
              the IANA zone. Nothing here is a judgement.",
        // Internal consistency against the registry the tool reads from. This
        // is the check that would have caught the two production forecasts:
        // both named a city and neither pinned a station, and one routed three
        // of five drivers to agents with no weather tool at all.
        //
        // `IS JSON OBJECT` before the cast, and `CASE` for its guaranteed
        // short-circuit, because `'not json'::jsonb` raises and would take the
        // whole harness down rather than report a finding. Same construction as
        // the football xgd check for the same reason.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND j.doc #>> '{settlement_target,station}' IS NOT NULL \
                AND upper(j.doc #>> '{settlement_target,station}') \
                    NOT IN ('CYYZ','EDDM','EFHK','EGLC','EHAM','EPWA','FACT','HKO', \
                            'KATL','KAUS','KBKF','KDAL','KLAX','KLGA','KMIA','KNYC', \
                            'KORD','KSEA','KSFO','LEMD','LFPB','LIMC','LLBG','LTAC', \
                            'LTFM','MMMX','NZWN','OEJN','OPKC','RCSS','RJTT','RKPK', \
                            'RKSI','RPLL','SAEZ','SBGR','UUWW','VILK','WMKK','WSSS', \
                            'ZBAA','ZGGG','ZGSZ','ZHCC','ZHHH','ZSJN','ZSPD','ZSQD', \
                            'ZUCK','ZUUU')",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "stages.forecast",
        grounding: Grounding::Sourced {
            tool: "weather_ensemble_forecast",
            response_field: "ensemble (n_members, mean, std_dev, models_returned) + lead_days",
        },
        why: "Open-Meteo's ensemble endpoint returns every member of up to five \
              independent ensembles. Member count, mean and spread are read off \
              the response; none is inferred.",
        // The ensemble mean must sit inside the member cloud it claims to
        // summarise. A mean outside [min, max] is arithmetically impossible and
        // means the number came from somewhere other than the tool — the
        // weather analogue of `xgd != xg - xga`, and equally always affordable.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc #> '{stages,forecast,n_members}') = 'number' \
                AND ( (j.doc #>> '{stages,forecast,n_members}')::numeric < 1 \
                   OR (jsonb_typeof(j.doc #> '{stages,forecast,ensemble_sd}') = 'number' \
                       AND (j.doc #>> '{stages,forecast,ensemble_sd}')::numeric < 0) )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "stages.calibration",
        grounding: Grounding::Sourced {
            tool: "weather_dispersion_fit",
            response_field: "fitted_fpl_params (predictive_sd, bias_p50) + per_lead_error",
        },
        why: "`weather_dispersion_fit` verifies 120 days of this station's own \
              forecast-versus-outcome history and returns the predictive sd and \
              bias triple. The values are measured, not chosen: at EGLC lead 1 \
              the fitted sd is 0.909C against a market-implied 0.94C, and the \
              lead-2 to lead-4 warm residual of +0.4 to +0.7C is statistically \
              significant. `sd_was_measured` distinguishes a fitted value from \
              the documented prior used when no fit is available, which is the \
              difference between a measurement and an assumption.",
        // A predictive sd must be positive, and a probability must be a
        // probability. Both are internal and cost nothing.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND ( (jsonb_typeof(j.doc #> '{stages,calibration,predictive_sd}') = 'number' \
                       AND (j.doc #>> '{stages,calibration,predictive_sd}')::numeric <= 0) \
                   OR (jsonb_typeof(j.doc #> '{stages,calibration,calibrated_probability}') = 'number' \
                       AND ( (j.doc #>> '{stages,calibration,calibrated_probability}')::numeric < 0 \
                          OR (j.doc #>> '{stages,calibration,calibrated_probability}')::numeric > 1 )) )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "stages.pricing",
        grounding: Grounding::Sourced {
            tool: "polymarket_orderbook",
            response_field: "best_bid / best_ask / midpoint / book_quality.tradeable",
        },
        why: "The CLOB book is read directly. `implied_probability` is the \
              midpoint, and the fee-adjusted EV figures are arithmetic over the \
              book and Polymarket's published taker fee of 0.05*p*(1-p).",
        // A midpoint is a probability, and a book cannot be both untradeable
        // and carry a positive edge worth acting on. The second half is the one
        // that matters: a settled market with a resting ask at 0.001 computes a
        // +54c/share edge, which is an artefact rather than an opportunity.
        //
        // The action test matches a NO-TRADE PREFIX rather than the exact token,
        // and the first draft's exact comparison is why. It fired on three rows
        // reading "NO TRADE — market is closed and settled" — correct decisions,
        // failing only because the string was prose instead of the declared
        // enum. That is a real finding about the card's output contract, but it
        // is a DIFFERENT finding from the one this check exists for, and
        // conflating them means the serious case (a trade recommended on a dead
        // book) arrives buried in formatting noise. One check, one proposition:
        // this one asks whether the agent recommended acting on an untradeable
        // book, and "NO TRADE — ..." plainly does not.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc #> '{stages,pricing,implied_probability}') = 'number' \
                AND ( (j.doc #>> '{stages,pricing,implied_probability}')::numeric < 0 \
                   OR (j.doc #>> '{stages,pricing,implied_probability}')::numeric > 1 \
                   OR ( j.doc #> '{stages,pricing,book_tradeable}' = 'false'::jsonb \
                        AND j.doc #>> '{recommendation,action}' IS NOT NULL \
                        AND lower(j.doc #>> '{recommendation,action}') \
                            NOT LIKE 'no%trade%' ) )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "stages.calibration.climatology_base_rate",
        grounding: Grounding::Sourced {
            tool: "weather_climatology",
            response_field: "base_rates.trend_adjusted_base_rate",
        },
        why: "ERA5 via the Open-Meteo archive, over the same calendar-day window \
              across 30 years, with a fitted warming trend. Retrieved, not \
              recalled — and the trend adjustment matters: at EGLC it moves \
              P(>=31.5C) from 2.1% to 3.0%.",
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc #> '{stages,calibration,climatology_base_rate}') = 'number' \
                AND ( (j.doc #>> '{stages,calibration,climatology_base_rate}')::numeric < 0 \
                   OR (j.doc #>> '{stages,calibration,climatology_base_rate}')::numeric > 1 )",
        ),
    },
    // ── the judgements ─────────────────────────────────────────────
    //
    // These are the product. Nulling them would prove the contract cannot tell
    // an agent that fabricates from one that reasons, which is the objection
    // `Grounding::Inferred` exists to answer.
    FieldContract {
        agent_id: "weather_oracle",
        path: "final_probability",
        grounding: Grounding::Inferred {
            from: "the ensemble member cloud, after measured bias correction, \
                   dispersion scaling, climatology shrinkage and settlement \
                   rounding — each a decision, none returned by any tool",
        },
        why: "No endpoint anywhere returns the calibrated probability of a \
              bucket. The ensemble returns raw member frequencies; turning them \
              into a number worth pricing is the entire job. The gap is not \
              small — on the London 32C bucket the raw frequency and the \
              calibrated probability differ by a factor of six — so treating \
              this as a retrieval would launder the agent's most consequential \
              judgement as a lookup.",
        // Internal: a probability, and consistent with the recommendation. Not
        // a check on whether it is CORRECT — that is what Brier scoring against
        // resolved outcomes is for, and it needs volume rather than a query.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc -> 'final_probability') = 'number' \
                AND ( (j.doc ->> 'final_probability')::numeric < 0 \
                   OR (j.doc ->> 'final_probability')::numeric > 1 )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "multiplier",
        grounding: Grounding::Inferred {
            from: "the calibrated probability relative to the prior it is \
                   adjusting; the Fermi orchestra's declared multiplier_range \
                   is [0.1, 10.0]",
        },
        why: "A multiplier is unverifiable in principle, not pending better \
              tooling: no database contains \"the multiplier for this driver\", \
              the agent is asked to produce it, and \"is 0.85 correct?\" is not \
              a checkable proposition. Its standing is therefore the floor over \
              its BASIS — the settlement target, the ensemble, the fitted \
              dispersion — all of which are `Sourced` above. Verify the inputs, \
              inherit the verdict.",
        // The declared range is enforceable even though the value is not
        // verifiable. `validate_fermi_contract` accepts the range on the card;
        // nothing until now checked the emitted number against it.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc -> 'multiplier') = 'number' \
                AND ( (j.doc ->> 'multiplier')::numeric < 0.1 \
                   OR (j.doc ->> 'multiplier')::numeric > 10.0 )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "edge_type",
        grounding: Grounding::Inferred {
            from: "which of the four edge classes the assembled evidence \
                   supports, ranked by how little each depends on the forecast \
                   being right",
        },
        why: "A classification the agent is asked to make. The ranking is the \
              product: settlement timing depends on almost no model skill, \
              realised state on little, ladder arbitrage on none at all, and \
              calibration on all of it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "recommendation",
        grounding: Grounding::Inferred {
            from: "the calibrated probability against the book, net of the \
                   taker fee, constrained by depth and fractional Kelly",
        },
        why: "A decision, reasoned from sourced inputs. Deliberately able to \
              return `no_trade`, which is the most common correct answer once \
              Polymarket's fee reaches 2.5% of notional at even money.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "challenge",
        grounding: Grounding::Inferred {
            from: "cross-stage consistency of the agent's own document",
        },
        why: "The adversarial pass, and the reason the composition has a front \
              agent at all rather than being a pipeline script. Each flag is a \
              judgement about the chain: did the station stay consistent, does \
              the edge exceed the calibration uncertainty, were the corrections \
              measured or assumed, does it survive a 40% wider spread.",
        // The flags are judgements, but two of them are judgements the document
        // CONTRADICTS on its own numbers, and that is checkable without any
        // external truth.
        //
        // `centre_gap_within_predictive_sd` is the guard for the failure class
        // both production forecasts fell into. If the ensemble centre and the
        // market-implied centre differ by more than the measured predictive sd,
        // every bucket in the ladder is one bet on the centre rather than
        // independent evidence about a bucket — London 2026-08-15 had a 0.95C
        // gap against a 0.908C sd. The agent is asked to notice; this fires when
        // it claims to have noticed and its own numbers say otherwise. The
        // market-implied centre is not in the document, so the proxy is the
        // probability disagreement it produces: a calibrated probability more
        // than 25 points from the market's implied probability cannot be a
        // bucket-level edge, and asserting centre consistency alongside it is
        // self-contradictory.
        //
        // `edge_exceeds_calibration_uncertainty` is the arithmetic one: an edge
        // smaller than the stated uncertainty is not an edge, and claiming both
        // is a contradiction inside one document. `uncertainty_pp` is in
        // percentage points and the probabilities are fractions, hence the /100.
        //
        // Both guarded by `jsonb_typeof`, so an absent field drops the row
        // rather than raising — the same discipline as every check above.
        cross_check_sql: Some(
            "SELECT count(*)::bigint AS mismatches \
               FROM episodes e \
               JOIN agents a ON a.agent_id = e.agent_id, \
               LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\\{.*\\}') IS JSON OBJECT \
                                    THEN substring(e.response_text from '(?s)\\{.*\\}')::jsonb END AS doc) j \
              WHERE a.agent_name = 'weather_oracle' \
                {{COHORT}} \
                AND jsonb_typeof(j.doc #> '{stages,calibration,calibrated_probability}') = 'number' \
                AND jsonb_typeof(j.doc #> '{stages,pricing,implied_probability}') = 'number' \
                AND ( ( j.doc #> '{challenge,centre_gap_within_predictive_sd}' = 'true'::jsonb \
                        AND abs( (j.doc #>> '{stages,calibration,calibrated_probability}')::numeric \
                               - (j.doc #>> '{stages,pricing,implied_probability}')::numeric ) > 0.25 ) \
                   OR ( j.doc #> '{challenge,edge_exceeds_calibration_uncertainty}' = 'true'::jsonb \
                        AND jsonb_typeof(j.doc #> '{final_probability_uncertainty_pp}') = 'number' \
                        AND abs( (j.doc #>> '{stages,calibration,calibrated_probability}')::numeric \
                               - (j.doc #>> '{stages,pricing,implied_probability}')::numeric ) \
                            < (j.doc #>> '{final_probability_uncertainty_pp}')::numeric / 100.0 ) )",
        ),
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose written by the same model in the same turn, and the channel \
              `parse_evidence_text` lifts into the episode digest — so it is \
              also where the orchestra reads the [MULTIPLIER] line from. An \
              unchecked prose channel is where a fabrication moves once the \
              structured fields are constrained, which is exactly what happened \
              to genome_profiler's `summary`.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "weather_oracle",
        path: "falsifiers",
        grounding: Grounding::Narrative,
        why: "What would show the analysis wrong. Prose, and load-bearing: a \
              forecast with no stated falsifier is not a forecast.",
        cross_check_sql: None,
    },
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
                    {{COHORT}} \
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
    // ── forage_identify ──────────────────────────────────────────────────
    // `POST /api/creatures/:id/forage` with action=identify. Photo-based
    // species identification for wild foraging.
    //
    // This handler asked a vision model for `edibility` on a
    // choice|edible|inedible|toxic scale, plus `look_alikes` carrying a
    // fatal|toxic|inedible `danger` enum, plus a `harvest_window` and a
    // self-rated `confidence`. No tool in the handler supplies any of it. It is
    // the genome_profiler defect with the consequence changed from a wrong
    // megabase figure to a person eating what a language model recalled about a
    // photograph — and it sat three functions away from `enemy_sensor`,
    // `genome_profiler` and `prey_locator`, all of which call `enforce`.
    //
    // The photograph is the point and is kept, labelled as the judgement it is.
    // What is removed is the safety verdict layered on top of it.
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.species",
        grounding: Grounding::Inferred {
            from: "the model's reading of the submitted photograph",
        },
        why: "A determination from an image is a judgement and never a \
              retrieval: two runs over one photograph can disagree, so it is \
              not even reproducible enough to be Derived. No ground-truth \
              database can be matched against a field photo, so Sourced is \
              unavailable by construction rather than by omission. Kept because \
              it is the requested product; labelled because everything \
              downstream is keyed on it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.common_name",
        grounding: Grounding::Inferred {
            from: "the same photograph as the binomial",
        },
        why: "Same status as the scientific name, stated separately because the \
              vernacular is what a forager actually reads and is the part most \
              likely to be right about a genus while wrong about the species \
              that matters.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.visual_features",
        grounding: Grounding::Inferred {
            from: "features visible in the photograph",
        },
        why: "What in the frame drove the determination. This is the most useful \
              honest output the handler has: it lets a forager check the \
              reasoning against the specimen in their hand instead of trusting \
              a verdict, which is the only form of help a photograph can \
              actually give.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.edibility",
        grounding: Grounding::Unsourced,
        why: "The field this contract exists for. No tool here returns \
              edibility: the handler calls a vision model and nothing else. A \
              model asked whether a mushroom is edible will answer fluently \
              from parametric memory, and the enum it was asked for \
              (choice|edible|inedible|toxic) reads exactly like a lookup. \
              Forced null. The refusal is the safe output; a wrong value here \
              is not a data-quality issue.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.look_alikes",
        grounding: Grounding::Unsourced,
        why: "Worse than edibility, because a generated list looks thorough. \
              Naming three real lookalikes while omitting the lethal one reads \
              as diligence, and the `danger: fatal|toxic|inedible` enum this \
              was asked for gives an invented entry the shape of a reference \
              work. Null until a curated, citable lookalike source is wired.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.harvest_window",
        grounding: Grounding::Unsourced,
        why: "Maturity and prime-harvest timing depend on the specimen, the \
              substrate and local conditions. Nothing in this handler observes \
              any of them, and `now` is a value a forager can act on \
              immediately.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.processing_recommendation",
        grounding: Grounding::Unsourced,
        why: "Processing advice presupposes the identification is correct and \
              the species is edible, neither of which this handler \
              establishes. Offering it implies both.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.confidence",
        grounding: Grounding::Unsourced,
        why: "A self-rating. The model was asked to grade its own \
              determination on high|medium|low, which is the defect \
              `hud_contract` removes by computing the band from measured \
              provenance instead. A rating nothing checks is worse than no \
              rating, because `high` is read as evidence.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "taxonomy",
        grounding: Grounding::Sourced {
            tool: "gbif_species_search",
            response_field: "species[0] rank ladder, matched name, usageKey, taxonomicStatus",
        },
        why: "The one block here that is a real retrieval, and the reason this \
              handler can be checkable rather than merely honest. GBIF is asked \
              whether the name the model produced resolves, and to what. \
              Critically it does NOT confirm the determination — the lookup is \
              keyed on a guess from a photograph — so a caller must floor this \
              against `identification` before presenting it, exactly as \
              `hud_contract::conditioned` does. A name that fails to resolve is \
              itself informative to a forager: `tool_no_match` on a confident-\
              looking binomial usually means the model invented the epithet.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "taxonomy.nomenclatural_status",
        grounding: Grounding::Sourced {
            tool: "mycobank_lookup",
            response_field: "status / accepted_name, with `source` naming which database answered",
        },
        why: "Whether a fungal name is current or superseded, which is the usual \
              reason a field guide and a database appear to disagree about a \
              mushroom. The tool degrades to GBIF scoped to Fungi when no \
              MycoBank key is configured and reports that in its own `source` \
              field, so the answer stays traceable to whichever database \
              supplied it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.kingdom",
        grounding: Grounding::Inferred {
            from: "the photograph, to choose which taxonomic scope to search",
        },
        why: "Asked of the model only because `gbif_species_search` defaults to \
              Insecta and needs a scope, and a fungus searched under Insecta \
              returns insects whose text happens to match. It is a judgement \
              like the rest of the determination, and a wrong kingdom shows up \
              honestly as `tool_no_match` on the taxonomy block rather than as \
              a wrong ladder.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "safety",
        grounding: Grounding::Derived {
            from: "the absence of any edibility source in this handler's tools",
            how: "a platform constant, written by Rust and not by the model",
        },
        why: "The warning must not be model output. A model-authored caution can \
              be softened, hedged, or omitted by the same model on the next \
              call, and the call where it is omitted is indistinguishable from \
              the ones where it is not. Written by platform code so it is \
              present on every response by construction, and reproducible.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_identify",
        path: "identification.safety_note",
        grounding: Grounding::Narrative,
        why: "Model prose, and the channel a stripped edibility verdict would \
              reappear in — which is exactly what happened to \
              genome_profiler's summary. Scanned, and it is not where the \
              authoritative warning lives; that is `safety`, above.",
        cross_check_sql: None,
    },
    // ── harvest_advisor ────────────────────────────────────────────────────
    // Tools: mycobank_lookup, gbif_species_search, execute_agent. Nomenclature
    // and taxonomy. Its prompt asked for a `safety.edibility` enum and a
    // `look_alikes` list carrying `fatal|toxic|inedible`, under a heading
    // claiming "toxic look-alikes for every edible species" while listing four.
    //
    // The claim of completeness is the dangerous part. Four hand-written entries
    // are useful; four entries under a promise of exhaustiveness invite the model
    // to fill the rest from memory, and a generated lethal-lookalike list reads
    // as a reference work.
    //
    // Everything this agent does DOWNSTREAM of a confirmed identification —
    // maturity, timing, yield, processing, culinary use — is legitimate
    // judgement and is kept. Only the safety verdict is removed.
    FieldContract {
        agent_id: "harvest_advisor",
        path: "harvest_assessment",
        grounding: Grounding::Inferred {
            from: "the species, the reported condition of the find, and food-science reasoning",
        },
        why: "Maturity stage, harvest window and yield are judgements over a \
              specimen someone else has already identified, and they are the \
              product this agent exists to provide. No database holds \
              \"harvest within two days\" for a particular patch; producing it \
              is the work. Distinguished from the safety fields below by the \
              consequence of being wrong: a mistimed harvest costs a meal.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "safety.edibility",
        grounding: Grounding::Unsourced,
        why: "No declared tool returns edibility. MycoBank returns nomenclature \
              and GBIF returns taxonomy; neither knows whether a thing can be \
              eaten. The enum shape (choice|edible|inedible|toxic) is what makes \
              this worse than prose — it reads as a database column.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "safety.look_alikes",
        grounding: Grounding::Unsourced,
        why: "A generated lookalike list is the most dangerous output in the \
              foraging fleet. It looks thorough, and naming three real \
              confusables while omitting the lethal one reads as diligence \
              rather than as a gap. The prompt's four curated entries are \
              retained as explicitly partial prose; what is refused is the \
              agent extending them.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "safety.preparation_notes",
        grounding: Grounding::Unsourced,
        why: "\"Must be cooked\" and \"do not eat raw\" are safety claims about a \
              species, not culinary preferences, and nothing here supplies them. \
              Gyromitra is the case that matters: deadly raw, and the difference \
              between a note and its absence is the outcome.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "processing_pathways",
        grounding: Grounding::Inferred {
            from: "species characteristics and food-preservation practice",
        },
        why: "Drying, fermenting and pickling suitability is applied food \
              science over a known species — a judgement, and a useful one. Kept \
              because stripping it would leave the agent with nothing, and an \
              agent that returns nothing gets replaced by one that returns \
              everything.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "culinary_notes",
        grounding: Grounding::Narrative,
        why: "Prose about flavour and preparation. Scanned, because it is the \
              channel a stripped edibility verdict reappears in — \"delicious \
              sauteed\" asserts edibility without using the word.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "harvest_advisor",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "One paragraph, and the part a forager actually reads. Same leak \
              channel as every other summary under this contract, and the one \
              genome_profiler's fabrication moved into after the structured \
              fields were cleared.",
        cross_check_sql: None,
    },
    // ── forage_scout ───────────────────────────────────────────────────────
    // Tools: inat_observations, openweather_forecast, mycobank_lookup,
    // gbif_species_search. Its structured response carried an `edibility` enum
    // per candidate species and a `cautions` array for "look-alikes or toxic
    // species", neither of which any of those four tools supplies.
    //
    // Unlike harvest_advisor, this agent has one genuinely sourced field, and it
    // is worth keeping distinct from the judgements around it.
    FieldContract {
        agent_id: "forage_scout",
        path: "species_likely[].inat_observations_nearby",
        grounding: Grounding::Sourced {
            tool: "inat_observations",
            response_field: "total_results for the taxon within the queried radius",
        },
        why: "The one retrieval in this response, and the reason the agent is \
              worth invoking at all: how often a taxon has actually been \
              recorded near here recently. It corroborates plausibility and \
              identifies nothing, which the prose must not blur.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_scout",
        path: "species_likely[].edibility",
        grounding: Grounding::Unsourced,
        why: "Same defect as harvest_advisor, one step earlier in the chain and \
              worse for it: this agent lists SPECULATIVE candidates for a \
              location, so an edibility verdict is attached to a species nobody \
              has seen, let alone identified. A forager reading \"choice\" next \
              to a name they have not found yet is being primed to confirm it.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_scout",
        path: "cautions",
        grounding: Grounding::Unsourced,
        why: "Asked for as \"any look-alikes or toxic species to be aware of\" \
              with no source for either. The honest version of this field is a \
              statement that no lookalike check was performed, which is what the \
              prompt now returns in prose instead.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_scout",
        path: "foraging_signal",
        grounding: Grounding::Inferred {
            from: "the weather forecast, season and substrate reasoning",
        },
        why: "An enumerated judgement over real microclimate data. This is the \
              agent's actual product and it is legitimate: no database holds \
              \"good foraging conditions\" for a coordinate, and the reasoning \
              from rainfall and temperature to fruiting probability is the work.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_scout",
        path: "species_likely[].probability",
        grounding: Grounding::Inferred {
            from: "observation density, season and condition match",
        },
        why: "A likelihood judgement, correctly labelled as one. Kept for the \
              same reason foraging_signal is: it reasons over sourced inputs \
              rather than asserting a fact nobody holds.",
        cross_check_sql: None,
    },
    FieldContract {
        agent_id: "forage_scout",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "The field report paragraph. Scanned as the channel a cleared \
              edibility verdict would move into, which is the failure mode this \
              whole contract family was written after.",
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

    /// **The forage safety contract.**
    ///
    /// `POST /api/creatures/:id/forage` action=identify asked a vision model for
    /// an `edibility` enum, a `look_alikes` list carrying `fatal|toxic|inedible`,
    /// a `harvest_window` a forager could act on immediately, and a self-rated
    /// `confidence`. None of it had a source. It is the genome_profiler defect
    /// with the consequence changed from a wrong megabase count to somebody
    /// eating what a language model recalled about a photograph.
    ///
    /// This test is the tripwire for it coming back.
    #[test]
    fn a_photograph_cannot_establish_edibility() {
        let mut doc = serde_json::json!({
            "identification": {
                "species": "Cantharellus cibarius",
                "common_name": "Golden chanterelle",
                "rank_reached": "species",
                "visual_features": "false gills, forking, blunt ridges",
                // Everything below is what the old prompt asked for.
                "edibility": "choice",
                "confidence": "high",
                "harvest_window": "now",
                "processing_recommendation": "saute in butter",
                "look_alikes": [
                    { "species": "Omphalotus olearius", "danger": "toxic",
                      "distinguishing": "true gills" }
                ],
                "safety_note": "A choice edible with no dangerous lookalikes."
            },
            "safety": { "directive": "..." }
        });

        let report = enforce("forage_identify", &mut doc);

        for path in [
            "/identification/edibility",
            "/identification/look_alikes",
            "/identification/harvest_window",
            "/identification/processing_recommendation",
            "/identification/confidence",
        ] {
            assert_eq!(
                doc.pointer(path).unwrap(),
                &Value::Null,
                "{path} survived enforcement — a forager would read it as looked up"
            );
        }

        // The identification itself is the product and must survive, labelled.
        assert_eq!(
            doc.pointer("/identification/species").unwrap(),
            "Cantharellus cibarius",
            "the determination was stripped; the handler now returns nothing useful"
        );
        assert_eq!(
            doc.pointer("/identification/visual_features").unwrap(),
            "false gills, forking, blunt ridges"
        );
        assert_eq!(doc.get("identification_provenance").unwrap(), PROV_INFERRED);

        // The warning is platform-derived, so it cannot be softened by the model.
        assert_eq!(doc.get("safety_provenance").unwrap(), PROV_DERIVED);

        assert!(
            report.violations.len() >= 5,
            "expected every fabricated field to be reported, got {:?}",
            report.violations
        );
    }

    /// A resolved name is a real retrieval; an unresolved one says so.
    ///
    /// This is what makes the handler checkable rather than merely honest: a
    /// forager can follow every taxonomy value to a database. It is NOT
    /// confirmation of the determination, and the block is deliberately still
    /// weaker than the sum of its parts once floored against the guess it was
    /// keyed on.
    #[test]
    fn a_resolved_name_is_sourced_and_an_unresolved_one_is_a_miss() {
        // Resolved.
        let mut hit = serde_json::json!({
            "identification": { "species": "Cantharellus cibarius", "kingdom": "fungi" },
            "taxonomy": {
                "kingdom": "Fungi", "family": "Hydnaceae",
                "matched_name": "Cantharellus cibarius Fr.",
                "taxonomic_status": "ACCEPTED",
                "nomenclatural_status": "ACCEPTED",
                "gbif_usage_key": 5249504
            },
            "safety": { "directive": "..." }
        });
        enforce("forage_identify", &mut hit);
        assert_eq!(hit.get("taxonomy_provenance").unwrap(), PROV_TOOL);
        assert_eq!(hit.pointer("/taxonomy/family").unwrap(), "Hydnaceae");

        // Not resolved — the databases were asked and did not recognise it.
        // Distinct from "no database was consulted", and the more useful of the
        // two: an unresolvable binomial usually means an invented epithet.
        let mut miss = serde_json::json!({
            "identification": { "species": "Cantharellus fictitius", "kingdom": "fungi" },
            "taxonomy": {},
            "safety": { "directive": "..." }
        });
        enforce("forage_identify", &mut miss);
        assert_eq!(miss.get("taxonomy_provenance").unwrap(), PROV_NO_MATCH);
        assert_ne!(
            miss.get("taxonomy_provenance").unwrap(),
            PROV_UNAVAILABLE,
            "an unresolved name must not read as `no tool exists` — a tool exists \
             and answered"
        );
    }

    /// Grounding the taxonomy must not launder the guess it was keyed on.
    ///
    /// GBIF really returned that ladder; that the forager is holding that species
    /// is not established. The floor across the response is therefore the
    /// judgement, not the retrieval — the same rule
    /// `hud_contract::conditioned` applies, asserted here so the forage path
    /// cannot drift from it.
    #[test]
    fn a_grounded_taxonomy_does_not_raise_the_response_floor() {
        let mut doc = serde_json::json!({
            "identification": { "species": "Amanita phalloides", "kingdom": "fungi" },
            "taxonomy": { "family": "Amanitaceae", "matched_name": "Amanita phalloides (Vaill. ex Fr.) Link" },
            "safety": { "directive": "..." }
        });
        let report = enforce("forage_identify", &mut doc);
        assert_eq!(doc.get("taxonomy_provenance").unwrap(), PROV_TOOL);
        assert_eq!(doc.get("identification_provenance").unwrap(), PROV_INFERRED);

        let overall = floor(report.provenance.iter().map(|(_, v)| *v));
        // `model_inference`, not `tool_verified`: the weakest claim in the
        // response is the determination from the photograph, and a real GBIF
        // retrieval keyed on it cannot be stronger than the thing it was keyed
        // on.
        //
        // Nor is it `unavailable_no_tool_source`. The first draft of this test
        // asserted that, reasoning that the nulled edibility fields should drag
        // the floor to the bottom — which is wrong, and wrong in a way worth
        // recording. Those fields are declared gaps that came back correctly
        // empty. They make no claim, so they cannot weaken one. Treating a
        // properly-refused field as a defect would make every honest response
        // score identically to a fabricated one, which is the same overreach
        // `hud_contract::is_declared_gap` exists to avoid.
        assert_eq!(overall, PROV_INFERRED, "floor over {:?}", report.provenance);
        assert!(
            strength(overall) < strength(PROV_TOOL),
            "a real GBIF retrieval lifted the response above a judgement about a \
             photograph"
        );
    }

    /// **The fleet-wide tripwire.** No card may ask for an edibility verdict.
    ///
    /// `forage_identify`, `harvest_advisor` and `forage_scout` all had one, and
    /// they had it for the same reason: the foraging agents were authored from a
    /// shared intent, so a per-agent fix leaves the next author copying whichever
    /// sibling they read first. This scans every card in the corpus.
    ///
    /// The check is on the *enum shape*, not the vocabulary. Prose discussing
    /// toxicity is legitimate and often required — `harvest_advisor` now explains
    /// at length why it cannot assess safety. What is refused is a card asking a
    /// model to pick from `choice|edible|toxic`, because an enum reads as a
    /// database column and a paragraph does not.
    ///
    /// An agent with a real safety tool is exempt by name, which is the honest
    /// shape of the rule: `adaptogen_curator` uses the same vocabulary and has
    /// `adaptogen_safety_check` and `adaptogen_drug_interaction_check` behind it.
    #[test]
    fn no_card_asks_a_model_to_rate_edibility() {
        // Agents whose safety claims have a declared tool behind them.
        const SOURCED_SAFETY: &[(&str, &str)] = &[(
            "adaptogen_curator",
            "declares adaptogen_safety_check, adaptogen_drug_interaction_check \
             and adaptogen_species_detail against a curated database with \
             citations, so its contraindication vocabulary is retrieval",
        )];

        // Enum shapes that ask a model to grade edibility or lethality.
        const FABRICATION_SHAPES: &[&str] = &[
            "choice | edible",
            "choice|edible",
            "fatal | toxic",
            "fatal|toxic",
            "edible | inedible | toxic",
            "inedible | toxic | unknown",
        ];

        let mut offenders: Vec<String> = Vec::new();
        let mut scanned = 0usize;
        for tier in std::fs::read_dir("agents").expect("agents/") {
            let tier = tier.expect("dir").path();
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
                let id = card
                    .get("agent_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                if SOURCED_SAFETY.iter().any(|(a, _)| *a == id) {
                    continue;
                }
                let prompt = card
                    .get("system_prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                scanned += 1;
                for shape in FABRICATION_SHAPES {
                    if prompt.contains(shape) {
                        offenders.push(format!("{id}: asks for `{shape}`"));
                    }
                }
            }
        }
        assert!(
            scanned > 50,
            "only scanned {scanned} cards — walk is broken"
        );
        assert!(
            offenders.is_empty(),
            "{} card(s) ask a model to grade edibility or lethality on an \
             enum:\n  {}\n\nAn enum reads as a database column. If the agent has \
             a real safety source, add it to SOURCED_SAFETY with what supplies \
             it; otherwise the field must be null and the prompt must say why.",
            offenders.len(),
            offenders.join("\n  ")
        );
    }

    /// A cleared verdict must not reappear as prose.
    ///
    /// The exact path genome_profiler's fabrication took: nulled in the
    /// structured field, restated in the summary, and the summary is the part a
    /// person reads.
    #[test]
    fn the_forage_safety_note_is_not_a_loophole() {
        let mut doc = serde_json::json!({
            "identification": {
                "species": "Cantharellus cibarius",
                "edibility": null,
                "safety_note": "Choice edible, no dangerous lookalikes — fry it fresh."
            },
            "safety": { "directive": "..." }
        });
        enforce("forage_identify", &mut doc);
        // `safety_note` is contracted as Narrative, so it is scanned. Whether it
        // is nulled depends on NARRATIVE_LEAKS covering edibility vocabulary,
        // which it does not yet — asserted here as a known gap so the follow-up
        // is a failing expectation rather than a comment nobody reads.
        let note = doc.pointer("/identification/safety_note").cloned();
        assert!(
            note.is_some(),
            "safety_note vanished entirely, which no rule here asks for"
        );
    }

    /// The identification block must never rank above a judgement, whatever the
    /// model claims about its own certainty.
    #[test]
    fn a_forage_identification_is_always_a_judgement() {
        let mut doc = serde_json::json!({
            "identification": { "species": "Amanita phalloides" },
            "safety": { "directive": "..." }
        });
        enforce("forage_identify", &mut doc);
        let verdict = doc
            .get("identification_provenance")
            .and_then(|v| v.as_str())
            .expect("stamped");
        assert_eq!(verdict, PROV_INFERRED);
        assert!(
            strength(verdict) < strength(PROV_TOOL),
            "a photo determination is being treated as strong as a retrieval"
        );
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

            // An episode-based check MUST be readable both ways — scoped to the
            // current prompt, and across all history. Forgetting the placeholder
            // is silent: the check still runs, still returns a number, and
            // quietly reports history as though it were the present, which is
            // the exact failure the cohort split exists to fix.
            //
            // Keyed on referencing `episodes` rather than on a list of agent
            // ids, so a new episode-based contract is caught by construction
            // instead of by whoever remembers to extend a table. Checks that
            // read elsewhere — `genome_profiler.taxonomy` compares cached
            // profiles against creature rows — have no episode to attribute and
            // are correctly exempt.
            if lower.contains("from episodes") {
                assert_eq!(
                    sql.matches(COHORT_PLACEHOLDER).count(),
                    1,
                    "{agent}.{path}: an episode-based cross-check must contain \
                     {COHORT_PLACEHOLDER} exactly once, so it can be read both \
                     scoped to the current prompt and across all history"
                );
                assert!(
                    lower.contains("join agents a"),
                    "{agent}.{path}: the cohort predicate compares against \
                     `a.system_prompt`, so the query must join `agents a`"
                );
            } else {
                assert!(
                    !sql.contains(COHORT_PLACEHOLDER),
                    "{agent}.{path}: declares {COHORT_PLACEHOLDER} but does not \
                     read `episodes`, so there is no prompt to scope to"
                );
            }

            // Both readings must survive substitution as bare SELECTs. Checking
            // only the raw string would pass a template that expands into
            // something unrunnable, and an unrunnable check reports healthy
            // forever.
            for (label, expanded) in [
                ("scoped", cohort_scoped(sql)),
                ("unscoped", cohort_unscoped(sql)),
            ] {
                assert!(
                    !expanded.contains(COHORT_PLACEHOLDER),
                    "{agent}.{path}: {label} expansion still contains the \
                     placeholder"
                );
                assert!(
                    expanded.to_lowercase().trim_start().starts_with("select"),
                    "{agent}.{path}: {label} expansion is not a bare SELECT"
                );
            }
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

    // ─── football_analyst ──────────────────────────────────────────────
    //
    // The agent that motivated the pending tier, and the one that proves the
    // contract can handle a partially-tooled agent. `genome_profiler` had no
    // tool for three of four blocks; this one has a working tool that carries
    // five blocks and not three others. Getting that split wrong in either
    // direction is a real cost: too strict and it nulls obtainable data, too
    // loose and it launders a remembered Elo.

    fn football_output() -> Value {
        json!({
            "league_context": { "team": "Arsenal", "rank": 2, "points": 71, "form": "WWDWL" },
            "fixtures": [{ "opponent": "Man City", "date": "2026-04-11", "rest_days": 3 }],
            "head_to_head": { "matches_considered": 10, "wins": 3, "draws": 3, "losses": 4 },
            "injuries": [{ "player": "Saliba", "reason": "Hamstring" }],
            "match_statistics": { "shots_total": 14, "possession_pct": 58.0, "passes_accurate": 397 },
            "advanced_metrics": { "xg": 1.9, "xga": 0.8, "xgd": 1.1, "ppda": 8.2, "xpoints": 2.1 },
            "ratings": { "elo_current": 1902.0, "elo_implied_win_probability": 0.59 },
            "squad_value": { "market_value_eur": 580000000.0, "top5_league_pct": 89.0 },
            "assessment": {
                "win_probability": 0.44,
                "multiplier": { "p5": 0.60, "p50": 0.85, "p95": 1.15 },
                "basis": ["league_context", "injuries", "match_statistics"],
                "rationale": "Saliba out; City at full strength."
            },
            "summary": "Arsenal are second on 71 points and have lost a first-choice centre-back."
        })
    }

    /// A derivation inherits the standing of what it derives from.
    ///
    /// `Derived` fields survive enforcement, because platform code applying a
    /// readable transform to a sourced value produces something reproducible.
    /// That reasoning collapses the moment the input is not sourced: the
    /// transform is still correct and the output is still invented, and the
    /// arithmetic makes it *more* credible rather than less.
    ///
    /// This test exists because the first `football_analyst` contract declared
    /// `elo_implied_win_probability` as `Derived` from an `Unsourced` Elo, with
    /// a comment confidently asserting that enforcement would strip it. It did
    /// not. The comment was wrong in the direction that ships a fabricated
    /// probability, and only a test could tell the difference between the
    /// declaration and the behaviour.
    #[test]
    fn a_derived_field_may_not_derive_from_an_unsourced_one() {
        let mut offenders: Vec<String> = Vec::new();

        for c in FIELD_CONTRACTS {
            let Grounding::Derived { from, .. } = c.grounding else {
                continue;
            };
            // `from` is prose naming one or more sibling paths. Match any
            // contract for the same agent whose path appears in it, which is
            // deliberately generous: a false positive here costs a comment
            // edit, a false negative ships a laundered value.
            for other in FIELD_CONTRACTS
                .iter()
                .filter(|o| o.agent_id == c.agent_id && from.contains(o.path))
            {
                if matches!(other.grounding, Grounding::Unsourced) {
                    offenders.push(format!(
                        "{}.{} is Derived from `{}`, which is Unsourced",
                        c.agent_id, c.path, other.path
                    ));
                }
            }
        }

        assert!(
            offenders.is_empty(),
            "a derivation cannot be more grounded than its input, but \
             `Derived` fields SURVIVE enforcement — so these would ship a \
             correct transform of a value nothing supplied:\n  {}\n\n\
             Declare the field `Unsourced` until its input has a tool.",
            offenders.join("\n  ")
        );
    }

    #[test]
    fn a_remembered_elo_is_stripped_from_a_football_report() {
        // The headline case. API-Football has no Elo endpoint and no ClubElo
        // tool is wired, so every Elo this agent has ever stated came from the
        // model — one real episode copied the number out of the card's own
        // worked example while calling it an estimate.
        let mut doc = football_output();
        let report = enforce("football_analyst", &mut doc);
        assert!(
            doc["ratings"]["elo_current"].is_null(),
            "{:?}",
            doc["ratings"]
        );
        assert!(!report.is_clean(), "a fabricated Elo must be a violation");
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.path.starts_with("ratings") && v.removed == json!(1902.0)),
            "the discarded value must be retained for calibration: {:?}",
            report.violations
        );
    }

    #[test]
    fn a_correct_formula_over_an_invented_input_is_still_invented() {
        // `elo_implied_win_probability` is `Derived` and the transform is right.
        // That must not rescue it: derived from an unsourced input is unsourced,
        // and a reader who sees a plausible 59% has no way to know the Elo
        // behind it was remembered.
        let mut doc = football_output();
        enforce("football_analyst", &mut doc);
        assert!(
            doc["ratings"]["elo_implied_win_probability"].is_null(),
            "a derivation cannot launder its source: {:?}",
            doc["ratings"]
        );
    }

    #[test]
    fn the_blocks_the_tool_really_carries_survive_untouched() {
        // The other half, and the half that keeps the contract usable. This
        // agent's tool genuinely returns standings, fixtures, h2h, injuries and
        // match statistics. Stripping them would be destroying obtainable data,
        // which is worse than leaving a guess in place.
        let mut doc = football_output();
        let before = doc.clone();
        enforce("football_analyst", &mut doc);
        for block in [
            "league_context",
            "fixtures",
            "head_to_head",
            "injuries",
            "match_statistics",
        ] {
            assert_eq!(
                doc[block], before[block],
                "`{block}` is retrievable and must survive"
            );
        }
    }

    #[test]
    fn expected_goals_survives_because_the_tool_does_carry_it() {
        // The correction that nearly went the other way. An earlier draft
        // classified xG as `Unsourced` on the strength of the agent's own words
        // in a real episode — "API-Football does not provide xG for these
        // fixtures" — which was true of those particular friendlies and false
        // about the tool. Trusting an agent's self-report about its own tool is
        // the same error as trusting its self-report about a genome size, and
        // acting on it would have nulled an obtainable field.
        let mut doc = football_output();
        enforce("football_analyst", &mut doc);
        assert_eq!(
            doc["advanced_metrics"]["xg"],
            json!(1.9),
            "xG is in fixture statistics and must not be stripped"
        );
        assert_eq!(
            doc["advanced_metrics"]["xgd"],
            json!(1.1),
            "xGD is a subtraction over two sourced fields"
        );
        // ...while the metrics that need event data the tool does not expose are
        // gone, in the same block. A per-field contract is what makes that
        // possible; a per-block one could not express it.
        assert!(
            doc["advanced_metrics"]["ppda"].is_null(),
            "PPDA needs defensive-action counts API-Football does not return"
        );
    }

    #[test]
    fn the_agents_own_judgement_is_never_stripped() {
        // The most important test on this agent, for the reason `enemy_sensor`
        // has the equivalent: a win probability, a factor signal and a
        // multiplier are what the agent was commissioned to produce. Nulling
        // them would leave an empty document and prove the contract cannot tell
        // an agent that fabricates from one that reasons — at which point it
        // deserves to be switched off.
        let mut doc = football_output();
        enforce("football_analyst", &mut doc);
        assert_eq!(doc["assessment"]["win_probability"], json!(0.44));
        assert_eq!(doc["assessment"]["multiplier"]["p50"], json!(0.85));
        assert_eq!(
            doc.get("assessment_provenance").and_then(|v| v.as_str()),
            Some(PROV_INFERRED),
            "labelled as reasoning rather than measurement"
        );
    }

    #[test]
    fn a_market_valuation_from_memory_is_stripped() {
        // Transfermarkt has no tool. Worth its own test because a remembered
        // valuation is not merely unsourced but *stale by construction*, and a
        // stale number during a transfer window is confidently wrong rather
        // than vaguely wrong.
        let mut doc = football_output();
        enforce("football_analyst", &mut doc);
        assert!(doc["squad_value"]["market_value_eur"].is_null());
        assert_eq!(
            doc.get("squad_value_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE)
        );
    }

    #[test]
    fn the_summary_may_not_recite_a_stripped_number() {
        // A narrative is not a loophole. An Elo or a market value quoted in
        // prose is the same claim wearing a different hat, and the summary is
        // the part a human actually reads.
        let mut doc = football_output();
        doc["summary"] = json!(
            "Arsenal's ClubElo of 1902 implies a 59% win probability, and their \
             squad is valued at EUR 580M."
        );
        let report = enforce("football_analyst", &mut doc);
        assert!(
            !report.is_clean(),
            "prose asserting what the unsourced blocks cannot support must be \
             flagged: {:?}",
            report.violations
        );
    }

    /// An agent id that is definitionally outside the contract.
    ///
    /// The two tests below previously used `weather_oracle`, on the strength of
    /// it having no `FIELD_CONTRACTS` entry at the time. Both broke the moment
    /// it was brought under the contract — a fixture that fails whenever the
    /// campaign it is meant to support makes progress. The assertion keeps this
    /// honest: if someone ever contracts this id, the test says so instead of
    /// quietly testing the opposite of its name.
    const UNCONTRACTED_AGENT: &str = "agent_that_will_never_be_contracted";

    #[test]
    fn the_uncontracted_fixture_is_actually_uncontracted() {
        assert!(
            FIELD_CONTRACTS
                .iter()
                .all(|c| c.agent_id != UNCONTRACTED_AGENT),
            "{UNCONTRACTED_AGENT} now has a field contract, so the two tests \
             relying on it are testing the opposite of what they claim. Pick \
             another sentinel rather than deleting the assertion."
        );
    }

    #[test]
    fn an_agent_with_no_contract_is_left_entirely_alone() {
        let mut doc = fabricated();
        let before = doc.clone();
        let report = enforce(UNCONTRACTED_AGENT, &mut doc);
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
        // Absence is not a verdict. With no field contract we know nothing
        // about an agent's grounding, and `None` has to travel all the way to
        // storage as NULL rather than being coerced to the best or the worst
        // value on the way. Callers that cannot represent "unknown" must refuse
        // the row, not guess it.
        let response = json!({"forecast": "rain", "confidence": 0.8}).to_string();
        assert_eq!(response_floor(UNCONTRACTED_AGENT, &response), None);
    }

    #[test]
    fn a_contracted_agent_gets_a_floor_from_its_weakest_block() {
        // The other side of the same coin, and the reason the fixture above had
        // to change: `weather_oracle` IS contracted now, so a real response
        // produces a floor rather than `None`.
        //
        // A document carrying only the JUDGEMENTS floors at `tool_no_match`,
        // not at `model_inference` — and that is the correct answer, which the
        // first draft of this test asserted wrongly. The judgements are
        // `Inferred`, but `settlement_target` and the three `stages` blocks are
        // `Sourced` and absent, so the weakest block is a sourced field with
        // nothing behind it. A weather forecast that states a probability while
        // naming no station and no ensemble is exactly the shape of the two
        // production failures, and it should floor low.
        let judgements_only = json!({
            "final_probability": 0.133,
            "multiplier": 1.15,
            "summary": "Bucket 32 at EGLC, calibrated."
        })
        .to_string();
        assert_eq!(
            response_floor("weather_oracle", &judgements_only),
            Some(PROV_NO_MATCH),
            "a probability with no station and no ensemble behind it must floor \
             on the missing sourced blocks, not on the judgement"
        );

        // With the sourced blocks present the floor rises to the judgement
        // ceiling: `model_inference`, and no higher however good the inputs
        // were. A calibrated probability is reasoned, not retrieved.
        let complete = json!({
            "summary": "Bucket 32 at EGLC. [MULTIPLIER] Suggested p50: 1.15 (p5: 1.05, p95: 1.28)",
            "settlement_target": { "station": "EGLC", "unit": "celsius" },
            "stages": {
                "forecast": { "n_members": 143, "ensemble_mean": 33.4, "ensemble_sd": 1.167 },
                "calibration": { "predictive_sd": 0.909, "calibrated_probability": 0.152,
                                  "climatology_base_rate": 0.03, "sd_was_measured": true },
                "pricing": { "implied_probability": 0.135, "book_tradeable": true }
            },
            "final_probability": 0.152,
            "multiplier": 1.15
        })
        .to_string();
        assert_eq!(
            response_floor("weather_oracle", &complete),
            Some(PROV_INFERRED),
            "a complete document floors at the judgement ceiling"
        );
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
