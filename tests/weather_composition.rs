//! # The weather composition type-checks end to end
//!
//! `weather_oracle` runs a four-stage pipeline over three members. With all
//! four typed, something becomes checkable that could not be checked before:
//! **does the coordinator read fields its members actually declare?**
//!
//! That question is the entire reason `produces_schema` is a type rather than
//! a label. A coordinator whose `forecast` block reads `raw_probability` from a
//! member that declares `probability` fails at runtime, silently, as a null —
//! and a null in a probability field reads as "the model had no view" rather
//! than as "the coordinator asked for the wrong name". Both sides look
//! correct in isolation. Only the pair is wrong, so only a test over the pair
//! can find it.
//!
//! ```text
//!   weather_oracle                       members
//!   ──────────────                       ───────
//!   forecast          <-- lifts from --  weather_ensemble_forecaster
//!   calibration_stage <-- lifts from --  weather_calibrator
//!   pricing           <-- lifts from --  weather_market_analyst
//!   challenge          (its own work, lifts from nobody)
//! ```
//!
//! ## Why the check is on leaf names rather than on paths
//!
//! A member's document is nested — `distribution.raw_probability`, not
//! `raw_probability` at the root. The coordinator's block flattens what it
//! lifts, because a coordinator that mirrored each member's internal structure
//! would break every time a member reorganised its blocks for reasons of its
//! own.
//!
//! So the convention this test enforces is: **every field a coordinator block
//! declares must exist as a leaf somewhere in the member's schema.** That is
//! weaker than a path match and stronger than nothing, and it is the property
//! that actually matters — the coordinator has to be able to find the value.

use serde_json::Value;
use std::collections::BTreeSet;

const COORDINATOR: &str = "weather_oracle";

/// `(coordinator block, member agent, member's declared type)`.
///
/// Written out rather than derived, because the mapping IS the composition and
/// deriving it from prose would mean this test believed whatever the cards
/// said about each other.
const PIPELINE: &[(&str, &str, &str)] = &[
    (
        "forecast",
        "weather_ensemble_forecaster",
        "fermi/weather_raw_forecast",
    ),
    (
        "calibration_stage",
        "weather_calibrator",
        "fermi/weather_calibrated_forecast",
    ),
    (
        "pricing",
        "weather_market_analyst",
        "fermi/weather_market_pricing",
    ),
];

fn card(agent: &str) -> Value {
    let path = format!("agents/curated/{agent}/agent_card.json");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn schema(agent: &str) -> Value {
    card(agent)
        .pointer("/capabilities/output_contract/schema")
        .cloned()
        .unwrap_or_else(|| panic!("{agent} declares no schema"))
}

fn declared_type(agent: &str) -> String {
    card(agent)
        .pointer("/capabilities/output_contract/produces_schema")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("{agent} declares no produces_schema"))
        .to_string()
}

/// Every leaf property name anywhere in a schema.
///
/// Leaves only: an object that has `properties` is structure, and the
/// coordinator lifts values rather than structure. `_provenance` siblings are
/// excluded because they are the platform's, not the member's, and a
/// coordinator reading one is reading a stamp rather than a value.
fn leaf_names(schema: &Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    fn walk(v: &Value, out: &mut BTreeSet<String>) {
        let Some(props) = v.get("properties").and_then(|p| p.as_object()) else {
            return;
        };
        for (name, sub) in props {
            if name.ends_with(fermi::contract_sketch::PROVENANCE_SUFFIX) {
                continue;
            }
            if sub.get("properties").is_some() {
                walk(sub, out);
            } else {
                out.insert(name.clone());
            }
        }
    }
    walk(schema, &mut out);
    out
}

/// The fields a coordinator block declares, i.e. what it expects to be able
/// to fill from its member.
fn block_fields(coordinator_schema: &Value, block: &str) -> BTreeSet<String> {
    coordinator_schema
        .pointer(&format!("/properties/{block}/properties"))
        .and_then(|p| p.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

// ─── the composition ───────────────────────────────────────────────────

/// **The load-bearing test.** Every field the coordinator lifts exists in the
/// member that supplies it.
#[test]
fn every_field_the_coordinator_lifts_is_declared_by_its_member() {
    let coord = schema(COORDINATOR);

    for (block, member, _) in PIPELINE {
        let wanted = block_fields(&coord, block);
        assert!(
            !wanted.is_empty(),
            "{COORDINATOR}.{block} declares no fields, so the stage carries \
             nothing and the pipeline has a hole where a member's output \
             should be"
        );

        let available = leaf_names(&schema(member));
        let missing: Vec<&String> = wanted.difference(&available).collect();

        assert!(
            missing.is_empty(),
            "{COORDINATOR}.{block} reads {missing:?} from `{member}`, which \
             declares no such leaf.\n\
             At runtime those arrive as null, and a null in a probability or \
             an edge reads as `the member had no view` rather than as `the \
             coordinator asked for the wrong name`. Both cards look correct \
             alone; only the pair is wrong.\n\
             Member leaves: {available:?}"
        );
    }
}

/// The members declare the types the pipeline names. Cheap, and it is the
/// half that would silently rot if someone renamed a type while updating only
/// its own card.
#[test]
fn each_member_declares_the_type_the_pipeline_expects() {
    for (block, member, expected) in PIPELINE {
        assert_eq!(
            declared_type(member),
            *expected,
            "`{member}` (feeding {COORDINATOR}.{block}) has changed its \
             declared type. Every consumer matching on the old name now \
             matches nothing."
        );
    }
}

/// Ports reference the type, not a label. Checked here as well as by the
/// publish gate because a composition is where the cost of a free-text port
/// is actually paid: a coordinator cannot match on `raw_predictive_distribution`.
#[test]
fn every_member_port_references_its_type() {
    for (_, member, expected) in PIPELINE {
        let produces: Vec<String> = card(member)
            .get("produces")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            produces,
            vec![expected.to_string()],
            "`{member}` still advertises free-text ports"
        );
    }
}

// ─── the shape of a pipeline, asserted ─────────────────────────────────

/// Each member's document must be retrieval-first *or* judgement-first in the
/// way its role implies, and the pipeline's shape is the argument for its
/// existence. Stage 1 retrieves, Stage 2 decides, Stage 3 does both.
///
/// Asserted because the failure is quiet and expensive: if Stage 2's
/// calibrated probability were ever declared `sourced`, a modelling choice
/// would be presented to Stage 3 as a measurement, and Stage 3 sizes real
/// money against it.
#[test]
fn the_calibrated_probability_is_never_declared_as_a_retrieval() {
    let g = card("weather_calibrator")
        .pointer("/capabilities/output_contract/grounding")
        .and_then(|v| v.as_object())
        .cloned()
        .expect("weather_calibrator declares grounding");

    let status = g
        .get("calibration")
        .and_then(|b| b.get("status"))
        .and_then(|s| s.as_str());
    assert_eq!(
        status,
        Some("inferred"),
        "the calibrated probability is the number that gets traded. No tool \
         returns one — calibration is the modelling this agent exists to do — \
         so declaring it `sourced` would hand Stage 3 a judgement wearing a \
         measurement's clothes."
    );

    // And the schema must make it unrepresentable, not merely discouraged.
    let stamp = card("weather_calibrator")
        .pointer("/capabilities/output_contract/schema/properties/calibration_provenance")
        .cloned()
        .expect("the calibration block carries a provenance stamp");
    assert_eq!(
        stamp,
        serde_json::json!({ "const": "model_inference" }),
        "a constant, so no run can stamp a calibrated probability \
         `tool_verified`"
    );
}

/// The coordinator's own block is the only one it authors, and it must be a
/// judgement. A coordinator whose synthesis were `sourced` from its members
/// would be a pipeline script claiming to have had an opinion.
#[test]
fn the_coordinators_synthesis_is_its_own_judgement() {
    let g = card(COORDINATOR)
        .pointer("/capabilities/output_contract/grounding")
        .and_then(|v| v.as_object())
        .cloned()
        .expect("grounding");

    assert_eq!(
        g.get("challenge")
            .and_then(|b| b.get("status"))
            .and_then(|s| s.as_str()),
        Some("inferred"),
        "Stage 4 is the reason this agent exists rather than being a script"
    );

    // Exactly one authored block among the stages. If a second appeared, the
    // coordinator has started doing a member's job.
    let authored: Vec<&String> = g
        .iter()
        .filter(|(k, v)| {
            !k.ends_with(fermi::contract_sketch::PROVENANCE_SUFFIX)
                && v.get("status").and_then(|s| s.as_str()) == Some("inferred")
        })
        .map(|(k, _)| k)
        .collect();
    assert_eq!(
        authored,
        vec!["challenge"],
        "a coordinator should author exactly one block — its synthesis. \
         Anything else means it is doing work a member was delegated"
    );
}

/// The halt conditions live in the document as typed values, not as prose.
///
/// Both are conditions a *downstream* reader must be able to enforce: a
/// negative skill score requires stopping before pricing, and a failed
/// resolution audit invalidates the chain. A caveat in a summary cannot be
/// enforced by anything.
#[test]
fn the_halt_conditions_are_machine_readable() {
    let cal = leaf_names(&schema("weather_calibrator"));
    assert!(
        cal.contains("skill_score") && cal.contains("forecastable_with_skill"),
        "the negative-skill halt has to be checkable by whoever reads the \
         document, not trusted to the agent that produced it: {cal:?}"
    );

    let mkt = leaf_names(&schema("weather_market_analyst"));
    assert!(
        mkt.contains("resolution_audit_agrees"),
        "the audit that can invalidate the whole chain has to be a value: \
         {mkt:?}"
    );

    let coord = leaf_names(&schema(COORDINATOR));
    assert!(
        coord.contains("verdict") && coord.contains("edge_exceeds_uncertainty"),
        "the coordinator must publish the outcome of its own checks: {coord:?}"
    );
}

// ─── all four are live, not decoration ─────────────────────────────────

/// Every agent in the composition satisfies the publish gate with no
/// grandfathering discount, and its prompt asks for the document it declares.
///
/// The second half is what keeps a typed composition from being four
/// beautifully-typed cards that never emit a document. A schema checked
/// against prose reports `unverified_no_payload` for ever and reads as healthy.
#[test]
fn all_four_are_typed_ungrandfathered_and_actually_asked_for_it() {
    let mut agents = vec![COORDINATOR];
    agents.extend(PIPELINE.iter().map(|(_, m, _)| *m));

    for agent in agents {
        let c = card(agent);

        let produces: Vec<String> = c
            .get("produces")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let tools: Vec<String> = c
            .pointer("/capabilities/mcp_tools")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let findings = fermi::card_contract::validate(
            c.pointer("/capabilities/output_contract"),
            &produces,
            &tools,
        );
        assert!(
            findings.is_empty(),
            "`{agent}` would be refused at publish:\n{}",
            findings
                .iter()
                .map(|f| format!("  [{}] {}", f.check, f.message))
                .collect::<Vec<_>>()
                .join("\n")
        );

        assert!(
            !fermi::workflows::agent_contract::is_typed_tier_exempt(agent),
            "`{agent}` is still grandfathered, so the gate has no opinion \
             about it whatever its card says"
        );

        let prompt = c
            .get("system_prompt")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        for prop in schema(agent)
            .get("properties")
            .and_then(|p| p.as_object())
            .unwrap()
            .keys()
        {
            assert!(
                prompt.contains(prop),
                "`{agent}` declares `{prop}` and its prompt never mentions it, \
                 so nothing asks the model to produce it"
            );
        }
    }
}

/// The whole point, stated as a test: this is a composition in which every
/// hop is checkable. Before, `weather_oracle` delegated three times and every
/// returned document reported `unverified_no_schema` at the hop — which is not
/// a pass, and was indistinguishable from a healthy pipeline.
#[test]
fn every_hop_in_this_composition_is_now_checkable() {
    for (_, member, _) in PIPELINE {
        let oc = card(member)
            .pointer("/capabilities/output_contract")
            .cloned()
            .unwrap_or(Value::Null);
        assert!(
            oc.get("schema").is_some_and(|s| s.is_object()),
            "`{member}` has no inline schema, so `envelope::build` reports \
             `unverified_no_schema` for this hop — not a pass"
        );

        // And the schema must be one the validator can actually evaluate,
        // or every document reports `unverified_unsupported_schema` instead.
        let report =
            fermi::schema_validate::validate(oc.get("schema").unwrap(), &serde_json::json!({}));
        assert!(
            report.unsupported.is_empty(),
            "`{member}`'s schema uses keywords src/schema_validate.rs cannot \
             evaluate, which would make every document unverified: {:#?}",
            report.unsupported
        );
    }
}
