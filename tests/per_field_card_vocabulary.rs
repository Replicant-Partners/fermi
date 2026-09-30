//! **Prototype: can a card express what `FIELD_CONTRACTS` expresses?**
//!
//! `docs/DESIGN_a2a_contracting.md` 7.6 wants the Rust table to shrink to zero
//! as declarations migrate onto agent cards. Two things blocked that, and one
//! is now gone: the SQL cross-checks used to be a field on `FieldContract`, so
//! migrating a declaration deleted the platform's only falsifiable check of it.
//! They live in `CROSS_CHECKS` keyed by `(agent_id, path)` and the two move
//! independently.
//!
//! The remaining blocker is vocabulary. A card's `grounding` map is per BLOCK
//! and the table is per PATH, so ~105 dotted declarations have nowhere to land.
//! Worse than nowhere: a single status over a mixed block is an **overclaim**.
//!
//! This file prototypes the extension — a block may carry a `fields` sub-map —
//! and answers the only question that matters before migrating anything:
//! **does the card form reproduce the table form exactly?**
//!
//! `football_analyst` is the subject because it declares in both homes, so the
//! two readings are directly comparable, and because its `advanced_metrics`
//! block is the worked overclaim: one `sourced` stamp over a retrieval, a
//! computation and a field no tool will ever carry.
//!
//! The prototype map is `agents/curated/football_analyst/output_contract.per_field.prototype.json`.
//! It is NOT live; the card still carries the per-block map.

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

const AGENT: &str = "football_analyst";

fn dispatchable() -> HashSet<&'static str> {
    fermi::agent_backend::tools::dispatchable_tool_names()
        .into_iter()
        .collect()
}

/// The proposed per-field map, wrapped as an `output_contract` would be.
fn prototype() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("agents/curated/football_analyst/output_contract.per_field.prototype.json");
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
    )
    .expect("prototype is JSON");
    serde_json::json!({ "grounding": v["grounding"].clone() })
}

/// The live per-block map, for the comparison that shows what is lost.
fn live_card() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("agents/curated/football_analyst/agent_card.json");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    v["capabilities"]["output_contract"].clone()
}

/// `path -> state token`, from whichever home.
fn states(reading: &fermi::field_state::ContractReading) -> BTreeMap<String, &'static str> {
    reading
        .entries
        .iter()
        .map(|e| (e.path.clone(), e.state.token()))
        .collect()
}

fn table_reading() -> fermi::field_state::ContractReading {
    let disp = dispatchable();
    // `None` for the card, so this is the registered table and nothing else.
    fermi::field_state::read_contract(AGENT, None, |t| disp.contains(t))
}

fn prototype_reading() -> fermi::field_state::ContractReading {
    let disp = dispatchable();
    let proto = prototype();
    // Read as a card would be read, bypassing the table's precedence so the
    // two can be compared rather than one shadowing the other.
    fermi::field_state::read_contract("a_hypothetical_agent", Some(&proto), |t| disp.contains(t))
}

/// **The result.** Path for path, state for state.
///
/// If this passes, the 13 registered rows for this agent are redundant and can
/// be deleted — the card says the same thing. If it fails, the vocabulary is
/// still short and the diff names exactly what by.
#[test]
fn the_per_field_card_form_reproduces_the_table_exactly() {
    let table = states(&table_reading());
    let proto = states(&prototype_reading());

    assert_eq!(
        table.len(),
        13,
        "fixture drift: football_analyst no longer declares 13 paths in \
         FIELD_CONTRACTS, so the prototype is being compared against a \
         different contract than it was written for. Table now: {table:#?}"
    );

    let missing: Vec<_> = table.keys().filter(|k| !proto.contains_key(*k)).collect();
    let extra: Vec<_> = proto.keys().filter(|k| !table.contains_key(*k)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "the two homes declare different paths.\n  only in FIELD_CONTRACTS: \
         {missing:?}\n  only on the card: {extra:?}"
    );

    let disagree: Vec<String> = table
        .iter()
        .filter(|(p, s)| proto.get(*p) != Some(*s))
        .map(|(p, s)| format!("{p}: table={s} card={}", proto[p]))
        .collect();
    assert!(
        disagree.is_empty(),
        "same paths, different trust states: {disagree:#?}"
    );
}

/// The grain claim is the point of the exercise.
///
/// A refined card declares per path, which is the unit the table uses. Note
/// that the table is itself mixed — `league_context` is a bare block beside
/// the dotted `advanced_metrics.xg` — so `Field` does not mean "every row is
/// a leaf", it means "the finest declaration here is a field".
#[test]
fn a_refined_card_reads_at_field_grain_like_the_table() {
    use fermi::field_state::Grain;

    assert_eq!(table_reading().grain, Some(Grain::Field));
    assert_eq!(prototype_reading().grain, Some(Grain::Field));

    // And an unrefined card is unchanged: every other card in the fleet keeps
    // reading per block, so this extension is additive.
    let disp = dispatchable();
    let live = live_card();
    let live_reading =
        fermi::field_state::read_contract("a_hypothetical_agent", Some(&live), |t| {
            disp.contains(t)
        });
    assert_eq!(live_reading.grain, Some(Grain::Block));
}

/// **What the per-block form was getting wrong, stated as a number.**
///
/// Not a style complaint. Under the live card, `advanced_metrics` is one
/// `sourced` stamp, so a reader is told that everything in it came back from
/// `call_football_api`. Two of the three fields are not retrievals, and one of
/// them — `ppda` — has no source at all, which means a fabricated value there
/// read as tool-verified.
#[test]
fn the_per_block_form_overclaims_and_the_refinement_is_what_fixes_it() {
    let disp = dispatchable();
    let live = live_card();
    let live_states = states(&fermi::field_state::read_contract(
        "a_hypothetical_agent",
        Some(&live),
        |t| disp.contains(t),
    ));
    let proto = states(&prototype_reading());

    // The live card says the whole block is a resolved retrieval.
    assert_eq!(
        live_states.get("advanced_metrics"),
        Some(&"resolved"),
        "fixture drift: the live card no longer declares advanced_metrics \
         sourced, so there is no overclaim left to demonstrate"
    );
    assert!(
        !live_states.contains_key("advanced_metrics.ppda"),
        "the live per-block card cannot mention a field; if it now can, this \
         prototype has already shipped"
    );

    // The refined card tells the truth about each one.
    assert_eq!(proto.get("advanced_metrics.xg"), Some(&"resolved"));
    assert_eq!(proto.get("advanced_metrics.xgd"), Some(&"derived"));
    assert_eq!(proto.get("advanced_metrics.ppda"), Some(&"pending"));

    // `pending` is the load-bearing one: it means the field MUST be null and a
    // value in it is the violation. Under the block stamp that same value was
    // `resolved` — the difference between "the API said so" and "the model
    // made it up", on the same bytes.
    assert_ne!(
        proto.get("advanced_metrics.ppda"),
        live_states.get("advanced_metrics"),
        "the refinement must disagree with the block stamp somewhere, or it \
         buys nothing"
    );
}

/// The cross-check survives the migration it was blocking.
///
/// This is what the `CROSS_CHECKS` split bought, checked rather than asserted:
/// the platform's only falsifiable statement about `advanced_metrics.xgd` is
/// keyed by `(agent_id, path)`, so deleting the `FieldContract` row that used
/// to carry it leaves the check standing.
#[test]
fn the_declaration_can_migrate_without_taking_its_cross_check() {
    let check = fermi::grounding_trust::cross_check_for(AGENT, "advanced_metrics.xgd")
        .expect("xgd must carry the self-consistency check; it is this agent's only one");
    assert!(
        check.contains("xgd") && check.contains("xga"),
        "the check no longer compares xgd against xg - xga"
    );

    // The prototype declares the same path, so after migration the check still
    // names something declared — which is what
    // `every_cross_check_names_a_field_somebody_declares` requires.
    assert!(
        states(&prototype_reading()).contains_key("advanced_metrics.xgd"),
        "the card form drops the path the cross-check is keyed to, so \
         migrating would leave a query running against nothing and reporting \
         `ok` for want of rows"
    );
}

/// **The gate `derived` needs before this can ship.**
///
/// `card_contract::GROUNDING_STATUSES` deliberately excludes `derived`:
/// `PLATFORM_ASSIGNED_ONLY` says an author cannot assert that the platform
/// computes their agent's field. The prototype needs the token for `xgd`, so
/// the proposal is a gate rather than an exemption — an author may write
/// `derived` only where the platform already keeps the promise.
///
/// This test states the rule and checks `xgd` satisfies it. It does NOT yet
/// run at publish; wiring it into `card_contract::validate` is the next step,
/// and until then the prototype map would be refused.
#[test]
fn derived_is_only_declarable_where_the_platform_keeps_the_promise() {
    let proto = prototype();
    let blocks = proto["grounding"].as_object().unwrap();

    let mut checked = 0usize;
    for (block, spec) in blocks {
        let Some(fields) = spec.get("fields").and_then(|f| f.as_object()) else {
            continue;
        };
        for (field, fspec) in fields {
            if fspec["status"].as_str() != Some("derived") {
                continue;
            }
            let path = format!("{block}.{field}");
            let computed = fermi::grounding_trust::DERIVATIONS
                .iter()
                .any(|(a, p, _)| *a == AGENT && *p == path);
            let checked_by_us = fermi::grounding_trust::cross_check_for(AGENT, &path).is_some();
            assert!(
                computed || checked_by_us,
                "{AGENT}.{path} is declared `derived` on the card and the \
                 platform neither computes it (DERIVATIONS) nor checks it \
                 (CROSS_CHECKS). `derived` asserts reproducibility; an author \
                 asserting it about their own agent with nothing backing it is \
                 exactly what PLATFORM_ASSIGNED_ONLY exists to refuse."
            );
            checked += 1;
        }
    }

    assert_eq!(
        checked, 1,
        "expected exactly one `derived` field in the prototype (xgd); the gate \
         is going vacuous if there are none"
    );

    // And the token is still not admitted at publish, which is the honest
    // state of this prototype. When this assertion starts failing, the
    // vocabulary has shipped and the gate above must have moved into
    // `card_contract::validate` first.
    assert!(
        !fermi::card_contract::GROUNDING_STATUSES.contains(&"derived"),
        "`derived` is now a publishable status. The gate in this test must be \
         enforced by card_contract::validate before that is safe, or any card \
         can claim the platform computes its fields."
    );
}
