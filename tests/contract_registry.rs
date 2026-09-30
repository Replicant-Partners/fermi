//! The contract registry, read the way every surface has to read it.
//!
//! A declaration about an agent's output lives in one of two places:
//!
//!   `grounding_trust::FIELD_CONTRACTS`   a Rust table, one row per dotted field path
//!   `output_contract.grounding`          the agent's card, one row per response block
//!
//! `docs/DESIGN_a2a_contracting.md` 7.6 calls the table legacy for tiers 1
//! and 2 and permanent for tier 3, so the split is intended to close but has
//! not. Until it does, **a consumer that reads one home is wrong**, and two
//! shipped that way:
//!
//!   * the workspace Team tab printed "no contract - nothing about this
//!     member's output is checked" over every card-only agent;
//!   * the specimen page reported those same agents as compiling cleanly,
//!     because `compiles` is `error_count == 0` and zero rows has zero errors.
//!
//! The second is the worse one: a tick over an empty table is a claim, and
//! nothing downstream noticed that a contract the page had just affirmed had
//! produced nothing to show.
//!
//! These tests are fleet-wide on purpose. Both bugs were found on one agent,
//! and a test naming that agent would have passed while the other nine stayed
//! invisible.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn dispatchable() -> HashSet<&'static str> {
    fermi::agent_backend::tools::dispatchable_tool_names()
        .into_iter()
        .collect()
}

struct Card {
    id: String,
    path: PathBuf,
    output_contract: Option<Value>,
}

/// Every agent card in the repo.
fn fleet() -> Vec<Card> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("agents");
    let mut out = Vec::new();
    for tier in std::fs::read_dir(&root).expect("agents/").flatten() {
        if !tier.path().is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(tier.path()).expect("tier dir").flatten() {
            let path = entry.path().join("agent_card.json");
            if !path.exists() {
                continue;
            }
            let v: Value = serde_json::from_str(
                &std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            )
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let id = v["agent_id"].as_str().unwrap_or_default().to_string();
            let oc = v["capabilities"]["output_contract"].clone();
            let oc = if oc.is_object() { Some(oc) } else { None };
            out.push(Card {
                id,
                path,
                output_contract: oc,
            });
        }
    }
    assert!(
        out.len() > 50,
        "found only {} cards; the walk is wrong",
        out.len()
    );
    out
}

/// Blocks the author declared, excluding the platform's own stamps.
fn authored_blocks(oc: &Value) -> Vec<String> {
    oc["grounding"]
        .as_object()
        .map(|g| {
            g.keys()
                .filter(|k| !k.ends_with("_provenance"))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// **No tick over an empty table.**
///
/// The specimen page calls an agent contracted when `a.output_contract IS NOT
/// NULL` — a column test that never looks inside. If the reader then finds no
/// rows, every verdict derived from the rows is vacuous: zero errors, so it
/// compiles.
///
/// The invariant is that the two cannot disagree: an agent the page is
/// willing to call contracted must have something to show for it.
///
/// `SHAPE_ONLY` is shrink-only. An entry declares a schema and no grounding
/// at all — it says what shape comes back and nothing about what any of it
/// can be trusted about — so the affirmation is still ahead of the evidence.
/// That is a narrower defect than the one this file was written for, and it
/// is listed rather than fixed so it is visible and counted.
#[test]
fn an_agent_called_contracted_has_something_to_show() {
    const SHAPE_ONLY: &[&str] = &["condition_forecaster", "wild_companion"];

    let disp = dispatchable();
    let mut shape_only_seen = Vec::new();

    for card in fleet() {
        let Some(oc) = card.output_contract.as_ref() else {
            continue; // declares_contract would be false; nothing is affirmed.
        };
        let reading = fermi::field_state::read_contract(&card.id, Some(oc), |t| disp.contains(t));

        if SHAPE_ONLY.contains(&card.id.as_str()) {
            shape_only_seen.push(card.id.clone());
            assert!(
                authored_blocks(oc).is_empty(),
                "{} is on SHAPE_ONLY but declares grounding blocks {:?}. The \
                 list excuses contracts that say nothing about trust; it must \
                 not come to excuse one that does and is being dropped.",
                card.id,
                authored_blocks(oc)
            );
            continue;
        }

        assert!(
            !reading.is_empty(),
            "{} has an output_contract, so `declares_contract` is true and the \
             specimen page will call it contracted and compute `compiles` from \
             its rows — of which there are none. A tick over an empty table. \
             Card: {}",
            card.id,
            card.path.display()
        );
        assert!(reading.grain.is_some(), "{}: rows without a grain", card.id);
    }

    // A shrink-only list that has silently emptied should stop being consulted.
    assert_eq!(
        shape_only_seen.len(),
        SHAPE_ONLY.len(),
        "SHAPE_ONLY names agents that are no longer in the fleet, or no longer \
         shape-only: expected {SHAPE_ONLY:?}, matched {shape_only_seen:?}. \
         Shrink the list rather than leaving it to rot."
    );
}

/// Every authored block reaches a state.
///
/// A dropped block reports a contract smaller than it is, which is the same
/// false negative one size down and much harder to see than a missing one.
/// `card_contract::validate` refuses an unrecognised status at publish, so
/// nothing in the fleet should be dropped here — this asserts that the gate
/// and the reader agree about the closed set.
#[test]
fn every_authored_block_reaches_a_state() {
    let disp = dispatchable();
    for card in fleet() {
        let Some(oc) = card.output_contract.as_ref() else {
            continue;
        };
        if fermi::grounding_trust::contracts_for(&card.id)
            .next()
            .is_some()
        {
            continue; // the table wins; the card's blocks are not the population
        }
        let authored = authored_blocks(oc);
        if authored.is_empty() {
            continue;
        }
        let reading = fermi::field_state::read_contract(&card.id, Some(oc), |t| disp.contains(t));
        let got: HashSet<&str> = reading.entries.iter().map(|e| e.path.as_str()).collect();
        for block in &authored {
            assert!(
                got.contains(block.as_str()),
                "{}.{block} is declared on the card and reached no state, so \
                 every count over this contract is short by one. Its status is \
                 {:?}, which `Declared::of_card_status` does not recognise — \
                 and `card_contract::GROUNDING_STATUSES` should have refused it \
                 at publish.",
                card.id,
                oc["grounding"][block]["status"]
            );
        }
    }
}

/// The card-only population is real, and reads as contracted.
///
/// Guards against the fix being undone by the other direction: if
/// `read_contract` stopped consulting cards, the two tests above would still
/// pass for every agent that has table rows, and this is the one that would
/// not.
#[test]
fn the_card_only_population_exists_and_is_read() {
    let disp = dispatchable();
    let mut card_only = Vec::new();

    for card in fleet() {
        let Some(oc) = card.output_contract.as_ref() else {
            continue;
        };
        let in_table = fermi::grounding_trust::contracts_for(&card.id)
            .next()
            .is_some();
        if in_table || authored_blocks(oc).is_empty() {
            continue;
        }
        let reading = fermi::field_state::read_contract(&card.id, Some(oc), |t| disp.contains(t));
        assert_eq!(
            reading.grain,
            Some(fermi::field_state::Grain::Block),
            "{} is card-only and must be counted per block",
            card.id
        );
        card_only.push(card.id);
    }

    assert!(
        !card_only.is_empty(),
        "no card-only agent found, so the card path proved nothing. Either the \
         fleet migrated wholesale into FIELD_CONTRACTS, or the card branch \
         stopped being reachable and both regressions are live with a green suite."
    );
}
