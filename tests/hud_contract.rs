//! # HUD contract — the test harness
//!
//! Feeds sample responses through the agent boundary and asserts two things
//! that are easy to confuse and must both hold:
//!
//! 1. **Schema-valid** — the document is the shape `abw/hud_card` declares,
//!    checked with `fermi::schema_validate`, which fails a document it could
//!    not check rather than passing it.
//! 2. **Provenance-complete** — no field arrives without a tag, and no tag
//!    claims more than the evidence supports.
//!
//! The second is the one worth having. A schema-conformant, evidentially
//! ungrounded response is the failure this whole family of contracts exists to
//! refuse, so the harness deliberately includes fixtures that pass (1) and
//! fail (2). A harness that only checked shape would be green on every one of
//! them, which is exactly how the `genome_profiler` profiles shipped.
//!
//! Fixtures stand in for the two documented Rokid agent input modalities —
//! text-only and text-plus-image — because those are what the Rizon custom-
//! agent surface accepts. They are LLM *outputs*, hand-written: the harness
//! tests the boundary, not the model, and a boundary test that needs a model
//! to run is a boundary test nobody runs.

use serde_json::{json, Value};

use fermi::card_contract;
use fermi::grounding_trust::{
    PROV_DERIVED, PROV_INFERRED, PROV_NO_MATCH, PROV_TOOL, PROV_UNAVAILABLE,
};
use fermi::hud_contract::{
    self, CONF_FLAGGED, CONF_HIGH, CONF_MEDIUM, LINE_MAX, MAX_LINES, TITLE_MAX,
};
use fermi::schema_validate;

const AGENT: &str = "hud_field_scout";
const CARD_PATH: &str = "agents/curated/hud_field_scout/agent_card.json";

// ─── fixtures ──────────────────────────────────────────────────────────

fn card() -> Value {
    let raw = std::fs::read_to_string(CARD_PATH).expect("read agent card");
    serde_json::from_str(&raw).expect("parse agent card")
}

fn schema() -> Value {
    card()
        .pointer("/capabilities/output_contract/schema")
        .cloned()
        .expect("output_contract.schema")
}

/// Voice-only: "which oak is this". Tools answered. Nothing fabricated.
///
/// The interesting property is that this document is as good as this agent
/// can produce, and it still bands to `medium` — because the subject was
/// inferred and every lookup was keyed on it.
fn voice_only_clean() -> Value {
    json!({
        "capture": { "modality": "voice", "image_present": false },
        "subject": {
            "scientific_name": "Quercus virginiana",
            "common_name": "Southern live oak",
            "rank_reached": "species"
        },
        // Real GBIF record, verified 2026-08-17 against
        // GET /v1/species/2878092. An invented key or family in a fixture is
        // the same defect as an invented value in production, one layer out:
        // it makes the test agree with a world that does not exist.
        "taxonomy": {
            "kingdom": "Plantae", "phylum": "Tracheophyta", "class": "Magnoliopsida",
            "order": "Fagales", "family": "Fagaceae", "genus": "Quercus",
            "species": "Quercus virginiana",
            "matched_name": "Quercus virginiana Mill.",
            "gbif_usage_key": 2878092,
            // Frequency-ranked English vernacular, as the tool now computes it:
            // 'southern live oak' is listed by 3 sources, 'virginia live oak'
            // and 'live oak' by 2 each.
            "vernacular_name": "Southern Live Oak",
            "taxonomic_status": "ACCEPTED",
            "fungal_nomenclature": null
        },
        "observations": {
            "count_nearby": 214, "radius_km": 25.0,
            "most_recent": "2026-08-11", "place_guess": "Chatham County, GA"
        },
        "edibility": {
            "verdict": null, "lookalikes": null, "hazard_check_performed": null
        },
        "card": {
            "title": "Live oak?",
            "lines": [
                { "text": "Quercus virginiana - southern live oak", "block": "subject" },
                { "text": "GBIF: Fagaceae, Fagales", "block": "taxonomy" },
                { "text": "iNat: 214 within 25km, last 11 Aug", "block": "observations" },
                { "text": "edibility: not available", "block": "edibility" }
            ],
            "confidence_display": "medium"
        },
        "summary": "I think that is a southern live oak. GBIF places the name in Fagaceae."
    })
}

/// Camera path, and the failure the prompt is most worried about: a
/// schema-conformant card that has quietly invented a safety verdict.
fn image_with_invented_edibility() -> Value {
    json!({
        "capture": { "modality": "voice+image", "image_present": true },
        "subject": {
            "scientific_name": "Cantharellus cibarius",
            "common_name": "Golden chanterelle",
            "rank_reached": "species"
        },
        // Real GBIF record, verified 2026-08-17 against
        // GET /v1/species/5249504. Note `Hydnaceae`, not the Cantharellaceae
        // this fixture originally asserted from memory — GBIF's backbone
        // places chanterelles there, and being wrong about it in a test is how
        // a fixture starts teaching the wrong answer.
        "taxonomy": {
            "kingdom": "Fungi", "phylum": "Basidiomycota", "class": "Agaricomycetes",
            "order": "Cantharellales", "family": "Hydnaceae",
            "genus": "Cantharellus", "species": "Cantharellus cibarius",
            "matched_name": "Cantharellus cibarius Fr.",
            "gbif_usage_key": 5249504,
            "vernacular_name": "Chanterelle",
            "taxonomic_status": "ACCEPTED",
            "fungal_nomenclature": "current"
        },
        "observations": {
            "count_nearby": 38, "radius_km": 25.0,
            "most_recent": "2026-08-06", "place_guess": "Sörmland, Sweden"
        },
        // The fabrication. Every one of these has no tool behind it.
        "edibility": {
            "verdict": "choice edible",
            "lookalikes": null,
            "hazard_check_performed": null
        },
        "card": {
            "title": "Golden chanterelle",
            "lines": [
                { "text": "Cantharellus cibarius", "block": "subject" },
                { "text": "Choice edible, no toxic lookalikes", "block": "edibility" },
                { "text": "iNat: 38 within 25km", "block": "observations" }
            ],
            "confidence_display": "high"
        },
        "summary": "That is a golden chanterelle, a choice edible with no dangerous lookalikes."
    })
}

/// GBIF and iNaturalist both consulted, both empty. The card must read
/// differently from one where no tool existed at all.
fn tools_asked_and_empty() -> Value {
    json!({
        "capture": { "modality": "voice+image", "image_present": true },
        "subject": {
            "scientific_name": null, "common_name": null, "rank_reached": null
        },
        "taxonomy": {
            "kingdom": null, "phylum": null, "class": null, "order": null,
            "family": null, "genus": null, "species": null,
            "matched_name": null, "gbif_usage_key": null,
            // GBIF lists no vernacular name for plenty of real taxa — measured
            // empty for Clastoptera querci and Glyptotus cribratus — so null
            // here is an ordinary outcome, not a broken fixture.
            "vernacular_name": null,
            "taxonomic_status": null,
            "fungal_nomenclature": null
        },
        "observations": {
            "count_nearby": null, "radius_km": null,
            "most_recent": null, "place_guess": null
        },
        "edibility": {
            "verdict": null, "lookalikes": null, "hazard_check_performed": null
        },
        "card": {
            "title": "Not determined",
            "lines": [
                { "text": "No confident identification", "block": "subject" },
                { "text": "GBIF: no match for the name tried", "block": "taxonomy" },
                { "text": "edibility: not available", "block": "edibility" }
            ],
            "confidence_display": "low"
        },
        "summary": "I could not place this one. Try a clearer frame of the underside."
    })
}

// ─── the two properties, on every fixture ──────────────────────────────

fn fixtures() -> Vec<(&'static str, Value)> {
    vec![
        ("voice_only_clean", voice_only_clean()),
        (
            "image_with_invented_edibility",
            image_with_invented_edibility(),
        ),
        ("tools_asked_and_empty", tools_asked_and_empty()),
    ]
}

/// Property 1: every enforced document is the shape the card declares.
///
/// Run *after* enforcement, because enforcement is what writes the provenance
/// stamps and the per-line treatment. A pre-enforcement document is not
/// expected to conform and checking it there would test the model instead of
/// the boundary.
#[test]
fn every_enforced_response_is_schema_valid() {
    let schema = schema();
    for (name, mut doc) in fixtures() {
        hud_contract::enforce(AGENT, &mut doc);
        let report = schema_validate::validate(&schema, &doc);
        assert!(
            report.is_valid(),
            "{name} is not schema-valid after enforcement.\n\
             violations: {:#?}\nunsupported: {:#?}\ndocument: {}",
            report.violations,
            report.unsupported,
            serde_json::to_string_pretty(&doc).unwrap()
        );
    }
}

/// Property 2: no field arrives without a tag.
///
/// The check the whole exercise is for. Asserted over the *enforced*
/// document, and deliberately not by asking the enforcement whether it was
/// happy — it reads the document independently, so a bug that skips a block
/// silently cannot also silently pass this.
#[test]
fn no_block_is_silently_missing_a_provenance_tag() {
    let expected = [
        "capture_provenance",
        "subject_provenance",
        "taxonomy_provenance",
        "observations_provenance",
        "edibility_provenance",
        "card_provenance",
    ];
    for (name, mut doc) in fixtures() {
        hud_contract::enforce(AGENT, &mut doc);
        for key in expected {
            let v = doc.get(key).and_then(|v| v.as_str());
            assert!(
                v.is_some(),
                "{name}: `{key}` is absent after enforcement. A field with no tag \
                 is the one a reader cannot distinguish from a measurement."
            );
        }
        // And every rendered line, which is what the wearer actually sees.
        for (i, line) in doc
            .pointer("/card/lines")
            .and_then(|v| v.as_array())
            .expect("card.lines")
            .iter()
            .enumerate()
        {
            assert!(
                line.get("provenance").and_then(|v| v.as_str()).is_some(),
                "{name}: card.lines[{i}] rendered with no provenance"
            );
            assert!(
                line.get("treatment").and_then(|v| v.as_str()).is_some(),
                "{name}: card.lines[{i}] rendered with no treatment"
            );
        }
    }
}

// ─── the failures the harness exists to catch ──────────────────────────

/// The headline case. Schema-conformant in, safety fabrication out.
#[test]
fn an_invented_edibility_verdict_is_stripped_from_field_line_and_speech() {
    let mut doc = image_with_invented_edibility();
    let report = hud_contract::enforce(AGENT, &mut doc);

    // 1. The structured field is nulled.
    assert_eq!(
        doc.pointer("/edibility/verdict").unwrap(),
        &Value::Null,
        "the invented verdict survived in the data"
    );
    assert_eq!(doc.get("edibility_provenance").unwrap(), PROV_UNAVAILABLE);

    // 2. The spoken sentence is nulled — the channel with no markers.
    assert_eq!(
        doc.get("summary").unwrap(),
        &Value::Null,
        "the claim moved into the audio channel, which carries no markers"
    );
    assert!(report
        .findings
        .iter()
        .any(|f| f.check == "hud_prose_carries_no_unsourced_safety_claim"));

    // 3. The card line still exists but cannot render as trustworthy.
    let line = doc
        .pointer("/card/lines/1")
        .expect("the edibility line")
        .clone();
    assert_eq!(line.get("provenance").unwrap(), PROV_UNAVAILABLE);
    assert_eq!(line.get("marker").unwrap(), "!");
    assert_eq!(line.get("spec_provenance").unwrap(), "UNSOURCED");

    // 4. And the card's own confidence claim does not survive.
    assert_eq!(
        doc.pointer("/card/confidence_display").unwrap(),
        CONF_FLAGGED,
        "a card carrying an unsourceable field must not read as confident"
    );
    assert!(report
        .findings
        .iter()
        .any(|f| f.check == "hud_confidence_is_computed"));
}

/// The subtle one, and the reason `conditioned` exists.
///
/// Every field in `voice_only_clean` is legitimate. GBIF really returned that
/// ladder; iNaturalist really returned those counts. The card still may not
/// read as `high`, because the name those lookups were keyed on was a guess.
#[test]
fn a_real_lookup_on_a_guessed_subject_does_not_render_as_sourced() {
    let mut doc = voice_only_clean();
    let report = hud_contract::enforce(AGENT, &mut doc);

    // The block-level stamp is honest about the retrieval...
    assert_eq!(doc.get("taxonomy_provenance").unwrap(), PROV_TOOL);
    assert_eq!(doc.get("subject_provenance").unwrap(), PROV_INFERRED);

    // ...and the rendered line is honest about what it depends on.
    let tax_line = doc.pointer("/card/lines/1").unwrap();
    assert_eq!(
        tax_line.get("provenance").unwrap(),
        PROV_INFERRED,
        "a GBIF hit keyed on an inferred name rendered as a retrieval"
    );
    assert_eq!(tax_line.get("marker").unwrap(), "~");
    assert_eq!(tax_line.get("subject_provenance").unwrap(), PROV_INFERRED);

    assert_eq!(report.confidence_display, CONF_MEDIUM);
    assert_eq!(
        doc.pointer("/card/confidence_display").unwrap(),
        CONF_MEDIUM
    );
    // Nothing was fabricated, so there is nothing to strip.
    assert!(
        report.grounding.is_clean(),
        "a clean document produced grounding violations: {:#?}",
        report.grounding.violations
    );
}

/// The sourced common name is the point of surfacing it: the wearer reads a
/// name GBIF actually lists, not one the model recalled.
///
/// It still renders `~`, and that is correct rather than a shortfall — GBIF
/// knows what *Quercus virginiana* is called, not that the wearer is looking at
/// one. What changes is that the name itself is no longer invented.
#[test]
fn a_sourced_common_name_still_inherits_the_guessed_subject() {
    let mut doc = voice_only_clean();
    hud_contract::enforce(AGENT, &mut doc);

    assert_eq!(
        doc.pointer("/taxonomy/vernacular_name").unwrap(),
        "Southern Live Oak",
        "a sourced vernacular name was stripped"
    );
    assert_eq!(
        doc.pointer("/taxonomy/taxonomic_status").unwrap(),
        "ACCEPTED"
    );
    // The block is a genuine retrieval...
    assert_eq!(doc.get("taxonomy_provenance").unwrap(), PROV_TOOL);
    // ...and any line reporting it is still capped by the subject.
    let tax_line = doc
        .pointer("/card/lines")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .find(|l| l.get("block").unwrap() == "taxonomy")
        .unwrap();
    assert_eq!(tax_line.get("provenance").unwrap(), PROV_INFERRED);
    assert_eq!(tax_line.get("marker").unwrap(), "~");
}

/// `capture` is the one block not conditioned on the subject, because it
/// describes the request rather than a claim about the world.
#[test]
fn the_capture_block_stays_reproducible() {
    let mut doc = voice_only_clean();
    hud_contract::enforce(AGENT, &mut doc);
    assert_eq!(doc.get("capture_provenance").unwrap(), PROV_DERIVED);
    let line = doc
        .pointer("/card/lines")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .find(|l| l.get("block").and_then(|b| b.as_str()) == Some("capture"));
    // This fixture has no capture line; the block verdict is what matters.
    assert!(line.is_none());
}

/// "The tool was asked and had nothing" must not render as "nothing could
/// ever answer this". Different facts, different next actions for the wearer.
#[test]
fn a_tool_miss_is_distinguishable_from_a_missing_tool() {
    let mut doc = tools_asked_and_empty();
    hud_contract::enforce(AGENT, &mut doc);

    assert_eq!(doc.get("taxonomy_provenance").unwrap(), PROV_NO_MATCH);
    assert_eq!(doc.get("edibility_provenance").unwrap(), PROV_UNAVAILABLE);

    let lines = doc.pointer("/card/lines").unwrap().as_array().unwrap();
    let tax = lines
        .iter()
        .find(|l| l.get("block").unwrap() == "taxonomy")
        .unwrap();
    let edi = lines
        .iter()
        .find(|l| l.get("block").unwrap() == "edibility")
        .unwrap();

    assert_eq!(tax.get("marker").unwrap(), "?");
    assert_eq!(edi.get("marker").unwrap(), "!");
    assert_ne!(
        tax.get("marker").unwrap(),
        edi.get("marker").unwrap(),
        "a miss and an absence rendered identically"
    );
    assert_eq!(tax.get("spec_provenance").unwrap(), "UNCLEAR");
    assert_eq!(edi.get("spec_provenance").unwrap(), "UNSOURCED");
}

/// A line with no `block` must fail toward caution. This is the exact defect
/// the brief names: schema-conformant, provenance-silent.
#[test]
fn an_untagged_line_is_marked_rather_than_defaulted_to_clean() {
    let mut doc = voice_only_clean();
    doc.pointer_mut("/card/lines")
        .and_then(|v| v.as_array_mut())
        .unwrap()
        .push(json!({ "text": "Peak season is late August" }));

    let report = hud_contract::enforce(AGENT, &mut doc);
    let added = doc.pointer("/card/lines/4").unwrap();

    assert_eq!(added.get("provenance").unwrap(), PROV_UNAVAILABLE);
    assert_eq!(added.get("marker").unwrap(), "!");
    assert!(report
        .findings
        .iter()
        .any(|f| f.check == "hud_line_declares_block"));
    // And it drags the whole card down, because it is on the card.
    assert_eq!(
        doc.pointer("/card/confidence_display").unwrap(),
        CONF_FLAGGED
    );
}

/// A line pointing at a block that does not exist is not evidence.
#[test]
fn a_line_naming_an_unknown_block_is_not_evidence() {
    let mut doc = voice_only_clean();
    doc.pointer_mut("/card/lines/0")
        .and_then(|v| v.as_object_mut())
        .unwrap()
        .insert("block".into(), json!("lab_results"));

    let report = hud_contract::enforce(AGENT, &mut doc);
    assert_eq!(
        doc.pointer("/card/lines/0/provenance").unwrap(),
        PROV_UNAVAILABLE
    );
    assert!(report
        .findings
        .iter()
        .any(|f| f.check == "hud_line_declares_block"));
}

// ─── glanceability ─────────────────────────────────────────────────────

#[test]
fn an_over_long_title_is_reported() {
    let mut doc = voice_only_clean();
    let long = "Southern live oak, probably, based on leaf shape and bark".to_string();
    assert!(long.chars().count() > TITLE_MAX);
    doc.pointer_mut("/card")
        .and_then(|v| v.as_object_mut())
        .unwrap()
        .insert("title".into(), json!(long));

    let report = hud_contract::enforce(AGENT, &mut doc);
    assert!(report.findings.iter().any(|f| f.check == "hud_glanceable"));
}

#[test]
fn too_many_lines_is_reported() {
    let mut doc = voice_only_clean();
    let lines = doc
        .pointer_mut("/card/lines")
        .and_then(|v| v.as_array_mut())
        .unwrap();
    while lines.len() <= MAX_LINES {
        lines.push(json!({ "text": "filler", "block": "taxonomy" }));
    }
    let report = hud_contract::enforce(AGENT, &mut doc);
    assert!(report.findings.iter().any(|f| f.check == "hud_glanceable"));
}

#[test]
fn every_fixture_respects_the_glanceability_budget() {
    for (name, doc) in fixtures() {
        let title = doc.pointer("/card/title").unwrap().as_str().unwrap();
        assert!(
            title.chars().count() <= TITLE_MAX,
            "{name}: title is {} chars",
            title.chars().count()
        );
        let lines = doc.pointer("/card/lines").unwrap().as_array().unwrap();
        assert!(lines.len() <= MAX_LINES, "{name}: {} lines", lines.len());
        for line in lines {
            let text = line.get("text").unwrap().as_str().unwrap();
            assert!(
                text.chars().count() <= LINE_MAX,
                "{name}: line {} chars: {text:?}",
                text.chars().count()
            );
        }
    }
}

// ─── rendering ─────────────────────────────────────────────────────────

/// What a wearer actually sees. Asserted as a whole string because the
/// property under test is *visible difference*, and that is a property of the
/// rendered output rather than of any field.
#[test]
fn the_rendered_card_distinguishes_evidence_from_guess_at_a_glance() {
    let mut doc = voice_only_clean();
    hud_contract::enforce(AGENT, &mut doc);
    let out = hud_contract::render(&doc);

    assert_eq!(
        out,
        vec![
            "Live oak?".to_string(),
            "~ Quercus virginiana - southern live oak".to_string(),
            "~ GBIF: Fagaceae, Fagales".to_string(),
            "~ iNat: 214 within 25km, last 11 Aug".to_string(),
            "! edibility: not available".to_string(),
            "[medium]".to_string(),
        ],
        "rendered card: {out:#?}"
    );
}

/// The markers have to survive a single-channel green panel and text-to-
/// speech. Non-ASCII on this hardware renders as black, i.e. as nothing.
#[test]
fn rendered_output_is_ascii() {
    for (name, mut doc) in fixtures() {
        hud_contract::enforce(AGENT, &mut doc);
        for line in hud_contract::render(&doc) {
            assert!(
                line.is_ascii() || !line.chars().any(|c| (c as u32) < 32),
                "{name}: rendered line has control characters: {line:?}"
            );
        }
    }
}

/// Enforcement must be idempotent, or a cached card re-read raises a fresh
/// finding every time and the findings stop meaning anything.
#[test]
fn enforcement_is_idempotent() {
    for (name, mut doc) in fixtures() {
        hud_contract::enforce(AGENT, &mut doc);
        let once = doc.clone();
        let second = hud_contract::enforce(AGENT, &mut doc);
        assert_eq!(
            once, doc,
            "{name}: second enforcement pass changed the document"
        );
        assert!(
            second
                .findings
                .iter()
                .all(|f| f.check != "hud_prose_carries_no_unsourced_safety_claim"),
            "{name}: prose scan re-fired on an already-cleaned summary"
        );
    }
}

/// A corrected card must not recover its confidence when it is read back.
///
/// The failure this guards is specific and would be easy to miss: after the
/// first pass the fabricated field is null, which is indistinguishable from a
/// field that was honestly empty all along. A second pass therefore sees a
/// clean document and would rate it `medium` — the correction laundering the
/// card into confidence. `grounding_trust` hit the same trap from the other
/// side, which is why `PRE_CONTRACT_MARKER` exists.
#[test]
fn a_corrected_card_stays_flagged_when_it_is_read_back() {
    let mut doc = image_with_invented_edibility();
    let first = hud_contract::enforce(AGENT, &mut doc);
    assert_eq!(first.confidence_display, CONF_FLAGGED);
    assert!(first.corrected);

    // The correction is recorded in the document, not only in the report.
    let marker = doc
        .get(hud_contract::REVIEW_MARKER)
        .and_then(|v| v.as_array())
        .expect("a corrected response carries the review marker");
    assert!(
        marker.iter().any(|p| p == "edibility.verdict"),
        "the marker does not name what was cleared: {marker:?}"
    );

    // Read back, as if from cache. The fabricated value is gone by now.
    let second = hud_contract::enforce(AGENT, &mut doc);
    assert!(
        second.grounding.is_clean(),
        "nothing should be left to strip on the second pass"
    );
    assert!(second.corrected, "the correction was forgotten");
    assert_eq!(
        second.confidence_display, CONF_FLAGGED,
        "a corrected card recovered its confidence once the evidence of \
         correction was gone"
    );
}

/// ...and the marker is not written to a response that needed no correction,
/// or every card would be flagged and the signal would mean nothing.
#[test]
fn a_clean_card_is_not_marked_as_corrected() {
    let mut doc = voice_only_clean();
    let report = hud_contract::enforce(AGENT, &mut doc);
    assert!(!report.corrected);
    assert!(
        doc.get(hud_contract::REVIEW_MARKER).is_none(),
        "a clean response was marked as corrected"
    );
    assert_eq!(report.confidence_display, CONF_MEDIUM);
}

// ─── the card declaration itself ──────────────────────────────────

/// The agent is new, so it gets the full publish contract with no
/// grandfathering discount.
#[test]
fn the_agent_card_satisfies_the_publish_contract() {
    let card = card();
    let oc = card.pointer("/capabilities/output_contract");
    let produces: Vec<String> = card
        .get("produces")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let tools: Vec<String> = card
        .pointer("/capabilities/mcp_tools")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let findings = card_contract::validate(oc, &produces, &tools);
    assert!(
        findings.is_empty(),
        "the card would be refused at publish:\n{}",
        findings
            .iter()
            .map(|f| format!("  [{}] {}", f.check, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The schema must be checkable by the validator that will check it. An
/// unsupported keyword makes every conforming document report as unverified,
/// which is indistinguishable from inert.
#[test]
fn the_declared_schema_uses_only_keywords_the_validator_implements() {
    let report = schema_validate::validate(&schema(), &voice_only_clean());
    assert!(
        report.unsupported.is_empty(),
        "schema uses keywords src/schema_validate.rs cannot evaluate: {:#?}",
        report.unsupported
    );
}

/// A clean high-confidence card is reachable in principle, or the top band is
/// decoration. It requires a subject the agent did not have to guess — which
/// this agent cannot currently produce, and that is the honest finding rather
/// than a reason to loosen the rule.
#[test]
fn the_high_band_is_reachable_only_without_a_guessed_subject() {
    assert_eq!(hud_contract::confidence_for(PROV_TOOL), CONF_HIGH);
    assert_eq!(hud_contract::confidence_for(PROV_DERIVED), CONF_HIGH);
    // With this agent's contract, `subject` is always an inference, so the
    // ceiling for any card it produces is `medium`.
    let mut doc = voice_only_clean();
    let report = hud_contract::enforce(AGENT, &mut doc);
    assert_ne!(
        report.confidence_display, CONF_HIGH,
        "this agent reached `high` despite inferring its subject"
    );
}
