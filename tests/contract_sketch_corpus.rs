//! # Every sketch in the corpus still compiles, and every card still agrees
//!
//! `tests/equity_analyst_contract.rs` proves one migration in depth. This
//! file is the shallow check that has to hold for all of them, so migration
//! 87 does not need a new test file to be safe — dropping an
//! `output_contract.sketch.json` beside a card is enough to be covered.
//!
//! The `TYPED_TIER_EXEMPT` list has 85 names left. The intended shape of
//! burning it down is: write a sketch, compile, splice, remove the name,
//! lower `BASELINE`. This file is the part that stays true for free.

use fermi::contract_sketch::{Ontology, Sketch};
use serde_json::Value;

const ROOTS: &[&str] = &["agents/curated", "agents/templates"];

struct Sketched {
    id: String,
    dir: std::path::PathBuf,
}

fn sketches() -> Vec<Sketched> {
    let mut out = Vec::new();
    for root in ROOTS {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        for e in rd.flatten() {
            let dir = e.path();
            if dir.join("output_contract.sketch.json").exists()
                && dir.join("agent_card.json").exists()
            {
                out.push(Sketched {
                    id: e.file_name().to_string_lossy().into_owned(),
                    dir,
                });
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn read(p: &std::path::Path) -> Value {
    let raw = std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn tool_names(card: &Value) -> Vec<String> {
    card.pointer("/capabilities/mcp_tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The whole point of the compiler, asserted over whatever exists today: a
/// sketch either compiles to a contract the Admission gate accepts, or it
/// does not compile at all. There is no third state in which an author holds
/// something that looks finished and is not.
#[test]
fn every_sketch_compiles_and_the_card_matches_it() {
    let all = sketches();
    assert!(
        !all.is_empty(),
        "no output_contract.sketch.json found under {ROOTS:?} — if sketches were \
         removed, remove this test with them rather than leaving it vacuously green"
    );

    for s in &all {
        let card = read(&s.dir.join("agent_card.json"));
        let mut sketch = Sketch::from_json(&read(&s.dir.join("output_contract.sketch.json")))
            .unwrap_or_else(|f| panic!("{}: sketch does not parse:\n{f:#?}", s.id));

        let ont_path = s.dir.join("ontology.json");
        if ont_path.exists() {
            let ont = Ontology::from_json(&read(&ont_path))
                .unwrap_or_else(|e| panic!("{}: ontology: {e}", s.id));
            let errs = ont.expand(&mut sketch);
            assert!(errs.is_empty(), "{}: unresolved @refs:\n{errs:#?}", s.id);
        }

        let compiled = sketch
            .compile(&tool_names(&card))
            .unwrap_or_else(|f| panic!("{}: sketch does not compile:\n{f:#?}", s.id));

        assert_eq!(
            card.pointer("/capabilities/output_contract"),
            Some(&compiled.output_contract),
            "{}: the card has drifted from its sketch. The sketch is the source of \
             truth: `cargo run --bin contract-sketch -- {}` and splice the result.",
            s.id,
            s.id
        );

        let produces: Vec<String> = card
            .get("produces")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            produces, compiled.produces,
            "{}: ports must reference the declared type",
            s.id
        );
    }
}

/// The "Load worked example" button in the create wizard's contract builder
/// fetches `static/contract-examples/equity_evidence.sketch.json`. It is a
/// file rather than inline JavaScript precisely so this test can compile it.
///
/// A demo that ships broken is worse than no demo: the first thing a newcomer
/// clicks would show a wall of findings about the example rather than about
/// anything they did, and they would reasonably conclude the gate is noise.
#[test]
fn the_builders_worked_example_compiles() {
    const PATH: &str = "static/contract-examples/equity_evidence.sketch.json";
    let doc = read(std::path::Path::new(PATH));

    let tool_names: Vec<String> = doc
        .get("tool_names")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let sketch_val = doc
        .get("sketch")
        .unwrap_or_else(|| panic!("{PATH} has no `sketch` key"));
    let sketch =
        Sketch::from_json(sketch_val).unwrap_or_else(|f| panic!("{PATH}: does not parse:\n{f:#?}"));

    let compiled = sketch
        .compile(&tool_names)
        .unwrap_or_else(|f| panic!("{PATH}: the builder's example does not compile:\n{f:#?}"));

    // The example earns its place by showing all four stamp behaviours at
    // once. Asserted, because a well-meaning trim to "simplify the demo"
    // would quietly remove the only place a newcomer sees that `coverage`
    // changes the enum.
    let props = compiled
        .output_contract
        .pointer("/schema/properties")
        .and_then(|p| p.as_object())
        .unwrap();

    assert_eq!(
        props.get("profile_provenance"),
        Some(&serde_json::json!({ "enum": ["tool_verified", "tool_no_match"] })),
        "complete coverage: two verdicts"
    );
    assert_eq!(
        props.get("fundamentals_provenance"),
        Some(&serde_json::json!({
            "enum": ["tool_verified", "tool_no_match", "unavailable_no_tool_source"]
        })),
        "partial coverage: three verdicts — the distinction the demo exists to show"
    );
    assert_eq!(
        props.get("assessment_provenance"),
        Some(&serde_json::json!({ "const": "model_inference" })),
        "a judgement can never claim to be retrieved"
    );
    assert!(
        !props.contains_key("summary_provenance"),
        "a narrative block carries no stamp"
    );
}

/// An agent that has done the work must not still be taking the discount:
/// with the exemption in place `typed_tier_violations` returns empty whatever
/// the card says, so the gate has no opinion and the migration is invisible
/// to the thing it was for.
#[test]
fn a_sketched_agent_is_not_still_grandfathered() {
    for s in sketches() {
        assert!(
            !fermi::workflows::agent_contract::is_typed_tier_exempt(&s.id),
            "`{}` has a compiled contract but is still in TYPED_TIER_EXEMPT. Remove \
             it and lower BASELINE in the same commit — otherwise the Admission gate \
             is not the thing keeping this card correct.",
            s.id
        );
    }
}

/// A sketch whose schema uses a keyword `schema_validate` cannot evaluate
/// would make every document from that agent report
/// `unverified_unsupported_schema` at the delegation hop. The compiler's
/// mini-language cannot express such a keyword, so this is a guard on the
/// compiler rather than on authors — which is why it belongs here and not in
/// a review checklist.
#[test]
fn no_compiled_schema_can_defeat_the_validator() {
    const SUPPORTED: &[&str] = &[
        "type",
        "properties",
        "required",
        "additionalProperties",
        "enum",
        "const",
        "items",
        // Annotations `schema_validate::ANNOTATIONS` skips deliberately.
        "description",
        "title",
        "$schema",
        "$id",
    ];

    fn walk(v: &Value, path: &str, id: &str) {
        let Value::Object(m) = v else { return };
        for (k, sub) in m {
            assert!(
                SUPPORTED.contains(&k.as_str()),
                "{id}: {path}.{k} is a keyword src/schema_validate.rs neither \
                 implements nor ignores, so every document would report \
                 `unverified_unsupported_schema` — which is not a pass"
            );
            match k.as_str() {
                "properties" => {
                    if let Value::Object(props) = sub {
                        for (name, s) in props {
                            walk(s, &format!("{path}.{name}"), id);
                        }
                    }
                }
                "items" => walk(sub, &format!("{path}[]"), id),
                _ => {}
            }
        }
    }

    for s in sketches() {
        let card = read(&s.dir.join("agent_card.json"));
        if let Some(schema) = card.pointer("/capabilities/output_contract/schema") {
            walk(schema, "schema", &s.id);
        }
    }
}
