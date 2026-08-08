//! Parity guard: `fermi::taxonomy` (Rust) must agree with
//! `scripts/taxonomy.py` (Python) on every derived rank, for every card in
//! the corpus.
//!
//! # Why two implementations exist at all
//!
//! The Python tool is the editorial instrument: it audits the on-disk
//! `agent_card.json` corpus and helps a human fill the ranks that need a
//! human. The Rust module classifies agents *created through the API*,
//! which have no card on disk — the gap that left all 13 efra agents
//! permanently undescribed.
//!
//! # Why this test exists
//!
//! Two implementations of one rule will diverge. That is not speculative in
//! this codebase: `test_agent_` filtering was duplicated inline across
//! handlers, drifted, and the Observatory ended up opening on a wall of
//! test rows. A duplicated *classification* rule would be quieter and
//! worse — agents silently filed under different orders depending on which
//! path created them, which is exactly the kind of incoherence the SPEC_30
//! reform was undoing.
//!
//! The fixture is authored by the Python side:
//!
//! ```sh
//! scripts/taxonomy.py audit --emit-expected agents/taxonomy_derived_expected.json
//! ```
//!
//! If either implementation changes a rule, this fails until the fixture is
//! regenerated and both agree.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fermi::taxonomy::{self, DeriveInput};
use serde_json::Value;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Walk the agent corpus, excluding `agents/templates/`.
///
/// That exclusion is load-bearing, not tidiness. The worked examples under
/// `agents/templates/examples/` reuse real agent_ids (`sentiment_analyzer`,
/// `market_research`), so keying by agent_id across the whole tree collides.
/// This test originally reported two "rule mismatches" that were really the
/// two tools walking the tree in different orders and picking different
/// winners for the same id — a false alarm that would have sent someone
/// hunting a nonexistent divergence in the derivation rules.
///
/// Mirrors `EXCLUDE_DIRS` in scripts/taxonomy.py. The production registry
/// loads from `agents/curated` only, so nothing shadows a real card at
/// runtime either.
fn load_cards() -> Vec<(String, Value)> {
    let mut out: Vec<(String, Value)> = Vec::new();
    let mut seen: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut stack = vec![repo_root().join("agents")];
    while let Some(dir) = stack.pop() {
        if dir.ends_with("templates") {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()) == Some("agent_card.json") {
                let Ok(text) = std::fs::read_to_string(&p) else {
                    continue;
                };
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(id) = v.get("agent_id").and_then(|x| x.as_str()) {
                        if let Some(first) = seen.get(id) {
                            panic!(
                                "duplicate agent_id {id:?} in the corpus: {} and {} — \
                                 two cards claiming one identity makes every \
                                 id-keyed comparison order-dependent",
                                first.display(),
                                p.display()
                            );
                        }
                        seen.insert(id.to_string(), p.clone());
                        out.push((id.to_string(), v));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn rust_and_python_derive_identical_ranks_for_every_card() {
    let fixture_path = repo_root().join("agents/taxonomy_derived_expected.json");
    let text = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
        panic!(
            "missing derived-rank fixture at {}: {e}\n\
             Regenerate with:\n  \
             scripts/taxonomy.py audit --emit-expected agents/taxonomy_derived_expected.json",
            fixture_path.display()
        )
    });
    let expected: BTreeMap<String, Value> =
        serde_json::from_str(&text).expect("fixture is not valid JSON");

    let cards = load_cards();
    assert!(
        cards.len() >= 90,
        "only found {} cards — the corpus walk is probably broken, which would \
         make this test vacuously pass",
        cards.len()
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut compared = 0usize;

    for (agent_id, card) in &cards {
        let Some(want) = expected.get(agent_id) else {
            mismatches.push(format!(
                "{agent_id}: present on disk but absent from the fixture — regenerate it"
            ));
            continue;
        };
        let got = taxonomy::derive(&taxonomy::input_from_card(card));
        compared += 1;

        for rank in taxonomy::DERIVED {
            let w = want.get(rank);
            let g = got.get(rank);
            if w != g {
                mismatches.push(format!(
                    "{agent_id}.{rank}: python={} rust={}",
                    w.map(|v| v.to_string())
                        .unwrap_or_else(|| "(absent)".into()),
                    g.map(|v| v.to_string())
                        .unwrap_or_else(|| "(absent)".into()),
                ));
            }
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} derivation mismatch(es) between Rust and Python across {} card(s):\n  {}",
        mismatches.len(),
        compared,
        mismatches.join("\n  ")
    );
    assert!(compared >= 90, "compared only {compared} cards");
}

/// The fixture must not contain editorial ranks. If it ever does, some
/// generator started inventing kinship claims, which SPEC_30 §6 rules out.
#[test]
fn fixture_contains_no_editorial_ranks() {
    let path = repo_root().join("agents/taxonomy_derived_expected.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let expected: BTreeMap<String, Value> = serde_json::from_str(&text).unwrap();

    for (agent_id, ranks) in &expected {
        for rank in taxonomy::EDITORIAL {
            assert!(
                ranks.get(rank).is_none(),
                "{agent_id}: fixture contains editorial rank `{rank}` — kingdom, family \
                 and genus are claims about kinship and must never be auto-derived"
            );
        }
    }
}

/// Every card's `species` must be its own `agent_id`. Five cards violated
/// this before the retrofit (`ar_avatar_renderer` claimed `ar_renderer`),
/// which is the kind of quiet inconsistency that makes a taxonomy
/// untrustworthy.
#[test]
fn species_always_equals_agent_id() {
    for (agent_id, card) in load_cards() {
        let derived = taxonomy::derive(&taxonomy::input_from_card(&card));
        assert_eq!(
            derived.get("species").and_then(|v| v.as_str()),
            Some(agent_id.as_str()),
            "{agent_id}: derived species must equal agent_id"
        );
    }
}

/// Guards the corpus walk itself. If `agents/` moves or the walk breaks,
/// the parity test would pass while comparing nothing.
#[test]
fn corpus_walk_finds_the_known_cards() {
    let ids: Vec<String> = load_cards().into_iter().map(|(id, _)| id).collect();
    for known in ["macro_forecaster", "market_research", "sentiment_analyzer"] {
        assert!(
            ids.iter().any(|i| i == known),
            "corpus walk missed {known}; found {} card(s)",
            ids.len()
        );
    }
    assert!(Path::new(&repo_root().join("agents")).is_dir());
}
