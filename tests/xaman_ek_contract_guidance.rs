//! # xaman_ek's contract guidance must not drift from the contract
//!
//! `xaman_ek` drafts agent cards for developers. Its own prompt says: "Do NOT
//! describe these rules from memory — call the tool." That instruction is
//! aimed at the model, and it is exactly as applicable to the 8,000 characters
//! of prose sitting above it, which *is* a description from memory — one
//! written once and then left to rot while `card_contract.rs` moved.
//!
//! A stale guide here is worse than no guide: the assistant confidently walks
//! a developer into a card the gate refuses, and the developer reasonably
//! concludes the gate is broken. So every closed vocabulary the prompt
//! enumerates is asserted against the code that owns it.
//!
//! These are cheap, unglamorous string assertions. They are also the only
//! thing standing between "the navigator knows how to build contracts" and
//! "the navigator used to know how to build contracts".

use fermi::card_contract::{GROUNDING_STATUSES, MIN_WHY};
use fermi::grounding_trust::{
    PROV_INFERRED, PROV_NO_MATCH, PROV_PENDING_TOOL, PROV_TOOL, PROV_UNAVAILABLE,
};
use serde_json::Value;

const CARD: &str = "agents/curated/xaman_ek/agent_card.json";

fn card() -> Value {
    let raw = std::fs::read_to_string(CARD).unwrap_or_else(|e| panic!("read {CARD}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {CARD}: {e}"))
}

fn prompt() -> String {
    card()
        .get("system_prompt")
        .and_then(|p| p.as_str())
        .expect("system_prompt")
        .to_string()
}

/// The prompt with whitespace runs collapsed.
///
/// Prose in a system prompt is hard-wrapped, so a sentence that reads as one
/// line contains newlines at unpredictable places. Asserting exact substrings
/// against it makes these tests fail on reflow — a false alarm that teaches
/// whoever hits it to delete the assertion. Normalise instead, so the tests
/// track what the prompt *says* rather than how it is laid out.
fn said() -> String {
    prompt().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tool_names() -> Vec<String> {
    card()
        .pointer("/capabilities/mcp_tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ─── the tools it is told to call ──────────────────────────────────────

/// The prompt instructs the model to call these by name. A name that does not
/// dispatch is answered `Unknown tool: X` at runtime, after the developer has
/// been promised a compiled contract.
#[test]
fn the_tools_it_is_told_to_call_actually_dispatch() {
    let p = prompt();
    let dispatchable = fermi::agent_backend::tools::platform_tool_names();
    for tool in ["build_output_contract", "validate_agent_card"] {
        assert!(
            p.contains(tool),
            "the prompt no longer mentions `{tool}` — if the workflow changed, \
             this test should change with it rather than after it"
        );
        assert!(
            dispatchable.contains(&tool),
            "`{tool}` is named in the prompt but has no dispatch arm"
        );
    }
}

/// And the card declares them, so what the agent advertises matches what its
/// instructions assume. Builtins reach every agent at runtime, which makes
/// this easy to forget and therefore worth pinning.
#[test]
fn the_card_declares_the_contract_tools() {
    let declared = tool_names();
    for tool in ["build_output_contract", "validate_agent_card"] {
        assert!(
            declared.iter().any(|t| t == tool),
            "`{tool}` is not in xaman_ek's `capabilities.mcp_tools`. Declared: {declared:?}"
        );
    }
}

// ─── the closed vocabularies it enumerates ─────────────────────────────

/// The prompt lists the four grounding statuses. `card_contract` owns that
/// set, and it is closed on purpose — an open one would admit `estimated`.
#[test]
fn it_enumerates_exactly_the_authorable_statuses() {
    let p = prompt();
    for s in GROUNDING_STATUSES {
        assert!(
            p.contains(s),
            "the prompt does not mention the `{s}` status, so the assistant will \
             never suggest it"
        );
    }
    // The value that must NOT appear as authorable. `platform_derived` is
    // assigned by the runtime and has no authoring token; a prompt offering it
    // would have the assistant propose cards the gate refuses on
    // `grounding_status_valid`.
    assert!(
        !p.contains("\"platform_derived\""),
        "the prompt offers `platform_derived` as if an author could declare it. \
         See card_contract::PLATFORM_ASSIGNED_ONLY for why there is no token."
    );
}

/// The provenance verdicts the prompt names must be the ones the runtime
/// stamps. This is the `gbif_verified` / `tool_verified` class of bug: a card
/// naming a value the runtime never emits.
#[test]
fn it_names_the_verdicts_the_runtime_actually_stamps() {
    let p = prompt();
    for v in [
        PROV_TOOL,
        PROV_NO_MATCH,
        PROV_UNAVAILABLE,
        PROV_INFERRED,
        PROV_PENDING_TOOL,
    ] {
        assert!(
            p.contains(v),
            "the prompt never mentions the `{v}` verdict, so it cannot explain \
             which coverage setting produces it"
        );
    }
}

/// Every `coverage` value the compiler accepts is explained, because this is
/// the single question that decides how wide a stamp's enum gets and the one
/// an author cannot guess.
#[test]
fn it_explains_every_coverage_setting() {
    let p = prompt();
    // Read off the enum, not a literal list. The literal version described
    // three settings and would have gone on describing three after
    // `partial_deferred` was added — an assistant confidently enumerating a
    // vocabulary it no longer knows, which is worse than one that says it is
    // unsure.
    for c in fermi::contract_sketch::Coverage::TOKENS {
        assert!(
            p.contains(c),
            "the prompt does not explain `coverage: {c}`. It is in \
             `Coverage::TOKENS`, so it is authorable and the assistant guiding \
             authors has to know it exists."
        );
    }
}

/// The `why` minimum is a number in the code and a number in the prose. Two
/// copies of one fact is the drift this file exists to catch.
#[test]
fn the_why_minimum_matches_the_gate() {
    let p = prompt();
    assert!(
        p.contains(&format!("{MIN_WHY}+ chars")) || p.contains(&format!("{MIN_WHY}+ characters")),
        "the prompt does not state the {MIN_WHY}-character `why` minimum in a \
         form that tracks card_contract::MIN_WHY"
    );
}

/// The type mini-language, and — more importantly — the keywords that are
/// deliberately absent. An assistant that suggests `minimum` produces a schema
/// which reports `unverified_unsupported_schema` for every document, i.e. it
/// declares more and verifies nothing.
#[test]
fn it_warns_about_the_keywords_that_are_not_available() {
    let p = prompt();
    assert!(
        p.contains("unverified_unsupported_schema"),
        "the prompt must explain WHY `minimum` is unavailable, not just that it \
         is — otherwise the assistant will apologise and work around it"
    );
    for kw in ["minimum", "pattern", "format"] {
        assert!(p.contains(kw), "the prompt does not mention `{kw}`");
    }
}

// ─── the anti-fabrication boundary ────────────────────────────────────

/// The load-bearing instruction. `why` is the one field the compiler refuses
/// to write, because its subject is where the developer's data comes from. An
/// assistant that composes one and lets it pass as the developer's has moved
/// the fabrication from the model into the helper.
#[test]
fn it_is_forbidden_from_inventing_a_why() {
    let p = said();
    assert!(
        p.contains("Never invent a `why`"),
        "the prompt no longer forbids inventing a `why`. This is the boundary \
         that keeps the assistant a drafting aid rather than a fabrication \
         engine with good manners."
    );
    assert!(
        p.contains("say you drafted it"),
        "drafting a `why` is allowed and useful; passing it off as the \
         developer's is not. The attribution instruction is what separates them."
    );
}

/// The prompt must still refuse to derive schemas from port labels, with the
/// measurement that justifies the refusal. Without the number it reads as
/// fussiness and gets ignored.
#[test]
fn it_still_refuses_to_invent_a_type_from_port_labels() {
    let p = said();
    assert!(p.contains("5% of labels"), "the measurement is missing");
    assert!(
        p.contains("plausible schema is exactly as well-formed as a true one"),
        "the reason is missing, and a rule without its reason is a rule that \
         gets argued with"
    );
}

// ─── the compositional framing ────────────────────────────────────────

/// The feedback that prompted this rewrite: a contract authored with no view
/// of the ecosystem is a form filled in a vacuum. `produces_schema` exists so
/// another agent can match on it, so "it compiles" is not the finish line.
#[test]
fn it_frames_the_contract_as_a_composition_artefact() {
    let p = said();
    assert!(
        p.contains(
            "A contract with no consumer is a contract that has not finished \
             being designed"
        ),
        "the compositional framing is gone — this is the difference between a \
         schema builder and a contract builder"
    );
    for synthesis in [
        "aggregation",
        "pipeline",
        "selection",
        "max_risk",
        "cep_weighted",
    ] {
        assert!(
            p.contains(synthesis),
            "the prompt cannot name how a coordinator would combine this \
             document: `{synthesis}` missing"
        );
    }
}

/// It must tell developers the two steps that turn a compiled contract into a
/// live one. Skipping either leaves a schema that is checked against prose
/// forever and reads as fine from any distance.
#[test]
fn it_says_what_to_do_after_it_compiles() {
    let p = said();
    assert!(
        p.contains("system prompt must ask for the document"),
        "an agent never asked to produce its declared type reports \
         `unverified_no_payload` and looks healthy"
    );
    assert!(
        p.contains("output_contract.sketch.json"),
        "without keeping the sketch, the next recompile reverts hand edits"
    );
}

// ─── the count that must be maintained ────────────────────────────────

/// The prompt states how many agents are grandfathered. That number lives in
/// `agent_contract::TYPED_TIER_EXEMPT` and shrinks as the burndown proceeds,
/// so the prose has to move with it. Pinned because "we will remember to
/// update the prompt" is precisely the intention that does not survive.
#[test]
fn the_grandfathered_count_is_current() {
    let p = said();
    let n = fermi::workflows::agent_contract::TYPED_TIER_EXEMPT.len();
    assert!(
        p.contains(&format!("{n} agents predate this contract")),
        "the prompt's grandfathered count has drifted; TYPED_TIER_EXEMPT is now \
         {n}. Update the sentence in the same commit that shrinks the list."
    );
}
