//! Parity guard: `fermi::port_trust` (Rust) must agree with
//! `scripts/port_census.py` (Python) on the input binding of every card in
//! the corpus.
//!
//! # Why two implementations exist at all
//!
//! They answer the same question for different consumers. The Python census
//! is the **scoreboard**: it reports how many agents declare a port the
//! execute path can actually bind to, and that number is what the
//! port-typing campaign burns down. The Rust module is the **gate**: it runs
//! at both execute boundaries (`handlers/execution.rs`,
//! `handlers/execution_stream.rs`) and stamps a verdict onto every episode.
//!
//! # Why this test exists
//!
//! If the two rules drift, the scoreboard stops describing the gate. That is
//! a worse failure than either being wrong on its own, because the number
//! would still look healthy while the thing it claims to measure did
//! something else — which is precisely the defect class this whole workstream
//! was opened to find.
//!
//! It is also not hypothetical. There were **three** implementations of this
//! rule within a week:
//!
//!   1. `crates/fermi-console/src/negotiate.rs` (v0.16.0) — narrow, and the
//!      only one wired anywhere, into the desktop client.
//!   2. `scripts/port_census.py` — deliberately mirroring the console's rule
//!      *including its misses*, to report what the shipped detector would say.
//!   3. `src/port_trust.rs` — the server-side gate, which widened the rule
//!      after the census found eight false positives.
//!
//! (2) and (3) are pinned to each other here. (1) remains a pre-flight hint
//! in the desktop app and is **not** covered by this test — a known gap,
//! recorded in `docs/ABW_VERIFICATION_RECONCILIATION.md` §7.9 rather than
//! quietly tolerated. It cannot be pinned without the console depending on
//! this crate, which would drag sqlx and axum into a GPUI desktop binary.
//!
//! # Regenerating the fixture
//!
//! ```sh
//! scripts/port_census.py --emit-expected agents/port_binding_expected.json
//! ```
//!
//! If either side changes the rule, this fails until the fixture is
//! regenerated and both agree.

use fermi::port_trust::{bind_input, InputBinding};
use std::collections::BTreeMap;

#[derive(serde::Deserialize)]
struct Expected {
    binding: String,
    labels: Vec<String>,
}

fn load_expected() -> BTreeMap<String, Expected> {
    let raw = std::fs::read_to_string("agents/port_binding_expected.json").expect(
        "agents/port_binding_expected.json missing — regenerate with \
         `scripts/port_census.py --emit-expected agents/port_binding_expected.json`",
    );
    serde_json::from_str(&raw).expect("fixture is not valid JSON")
}

fn load_cards() -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for tier in std::fs::read_dir("agents").expect("agents/ unreadable") {
        let tier = tier.expect("dir entry");
        if !tier.path().is_dir() {
            continue;
        }
        for agent in std::fs::read_dir(tier.path()).expect("tier unreadable") {
            let path = agent.expect("dir entry").path().join("agent_card.json");
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&path).expect("card unreadable");
            let card: serde_json::Value = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", path.display()));
            let id = card["agent_id"].as_str().expect("card has no agent_id");
            let accepts = card["accepts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            out.insert(id.to_string(), accepts);
        }
    }
    out
}

/// The Python verdict vocabulary, collapsed onto the Rust enum.
///
/// `declared_by_convention` is a *reporting* distinction the census keeps so
/// the `free_text*` / `*_task` widening stays visible and reversible. Both
/// map to `Declared` — the gate treats them identically, and if it ever
/// stops doing so this mapping is where that has to be stated.
fn rust_verdict(binding: &InputBinding) -> &'static str {
    match binding {
        InputBinding::Declared { .. } => "declared",
        InputBinding::NoTextInput { .. } => "no_text_input",
        InputBinding::Undeclared => "undeclared",
    }
}

#[test]
fn rust_and_python_agree_on_every_cards_input_binding() {
    let expected = load_expected();
    let cards = load_cards();

    assert!(!cards.is_empty(), "no cards loaded — glob is wrong");
    assert_eq!(
        cards.len(),
        expected.len(),
        "fixture covers {} agents, corpus has {} — regenerate with \
         `scripts/port_census.py --emit-expected agents/port_binding_expected.json`",
        expected.len(),
        cards.len()
    );

    let mut disagreements = Vec::new();
    for (agent_id, accepts) in &cards {
        let Some(exp) = expected.get(agent_id) else {
            disagreements.push(format!("{agent_id}: absent from the fixture"));
            continue;
        };
        let got = bind_input(accepts);
        let got_verdict = rust_verdict(&got);
        let exp_verdict = if exp.binding == "declared_by_convention" {
            "declared"
        } else {
            exp.binding.as_str()
        };

        if got_verdict != exp_verdict {
            disagreements.push(format!(
                "{agent_id}: python says `{}`, rust says `{}` (accepts: {:?})",
                exp.binding, got_verdict, accepts
            ));
            continue;
        }

        // The label matters as much as the verdict: a report that names the
        // wrong port sends the reader to the wrong line of the card.
        let got_labels: Vec<String> = match &got {
            InputBinding::Declared { label } => vec![label.clone()],
            InputBinding::NoTextInput { declared } => declared.clone(),
            InputBinding::Undeclared => vec![],
        };
        if got_labels != exp.labels {
            disagreements.push(format!(
                "{agent_id}: label mismatch — python {:?}, rust {:?}",
                exp.labels, got_labels
            ));
        }
    }

    assert!(
        disagreements.is_empty(),
        "the scoreboard and the gate disagree about {} agent(s). The census \
         number would no longer describe what the execute boundary does:\n  {}",
        disagreements.len(),
        disagreements.join("\n  ")
    );
}

#[test]
fn the_pilot_agents_port_fix_is_reflected_in_the_fixture() {
    // genome_profiler v1.2.0 replaced [species_data, taxonomy, gbif_key]
    // with [query], because the only caller sends one prose string
    // (`agent_modules.rs`). Before the fix this was a live mismatch on a
    // path that charges 2 credits a call.
    let expected = load_expected();
    let gp = expected
        .get("genome_profiler")
        .expect("genome_profiler missing from fixture");
    assert_eq!(gp.binding, "declared");
    assert_eq!(gp.labels, vec!["query".to_string()]);
}

#[test]
fn a_declared_absence_is_never_reported_as_a_mismatch() {
    // Guard against a future widening that turns silence into a defect.
    // Nothing in the corpus declares nothing today, so this is a property
    // test rather than a corpus assertion.
    let b = bind_input(&[]);
    assert!(!b.is_mismatch());
    assert_eq!(rust_verdict(&b), "undeclared");
}
