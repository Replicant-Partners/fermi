//! # The delegation envelope — making provenance survive a hop
//!
//! ## What crossed the seam before this
//!
//! When one agent delegates to another, `execute_agent` returned:
//!
//! ```json
//! { "agent": "...", "confidence": 0.7, "status": "Success",
//!   "response": "<metadata.reasoning>", "evidence": [ ... ] }
//! ```
//!
//! `response` is `metadata.reasoning` — the output of `parse_evidence_text`,
//! which is a **per-agent** extractor (it special-cases `genome_profiler` to
//! reach into a nested object). So what crossed the seam was not the
//! document the child produced but one hand-written *reading* of it, and the
//! reading changes retroactively whenever the parser does.
//!
//! Three consequences, all of which this module fixes:
//!
//! 1. **The document was discarded.** A caller could not validate what it
//!    received against the producer's declared type, because it never
//!    received the thing the type describes.
//! 2. **Grounding evaporated at the hop.** `grounding_trust::enforce` runs in
//!    the creature-module handlers. Delegation does not go through them, so a
//!    fabricated field was stripped when a handler called the agent directly
//!    and passed **completely freely** when one agent called another. The
//!    composition path — the one that matters for a fleet — was the
//!    unprotected one.
//! 3. **Nothing said where a value came from.** A coordinator combining five
//!    members' numbers could not tell a `tool_verified` measurement from a
//!    `model_inference` judgement, so neither could anything downstream of
//!    it: not the credit model, not the trust loop, not a human reading a
//!    receipt.
//!
//! ## Additive on purpose
//!
//! Every existing key is preserved byte-for-byte and the envelope is added
//! under its own `envelope` key. Coordinator prompts across the corpus expect
//! `response` and `evidence`; changing that shape would break every
//! composition at once in exchange for a property nothing consumes yet.
//! Producers go first, consumers follow.
//!
//! ## What an envelope asserts, and what it does not
//!
//! It asserts: *this is the producer's own document, enforced against its
//! declared grounding contract, and here is what was stripped.* It does not
//! assert the document is correct, or that it validates against a JSON
//! Schema — schema validation at the hop is the next increment and needs a
//! validator the workspace does not yet have.
//!
//! `type: null` means the producer declared no output type. That is an
//! absence, not a failure: silence must not read as a verdict.

use crate::agent_backend::executor::AgentOutput;
use serde_json::{json, Value};
use uuid::Uuid;

/// Pull the first JSON object out of a model response.
///
/// Handles the shapes models actually emit: bare JSON, a ```json fence, and
/// prose wrapped around either.
///
/// NOTE: `handlers::creatures::agent_modules::parse_agent_json` does the same
/// job for the creature modules. Two implementations of one rule is the drift
/// this repo keeps finding, and the reason there are two here is directional:
/// that one lives in the binary and cannot be called from the library. The
/// convergence is to have it call this. Tracked rather than left silent.
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        if v.is_object() {
            return Some(v);
        }
    }
    // Balanced brace scan — take the largest object, which for a document
    // wrapped in prose is the document rather than a nested fragment.
    let bytes = t.as_bytes();
    let mut best: Option<Value> = None;
    let mut best_len = 0usize;
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            b'}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let span = &t[start..=i];
                    if span.len() > best_len {
                        if let Ok(v) = serde_json::from_str::<Value>(span) {
                            if v.is_object() {
                                best_len = span.len();
                                best = Some(v);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    best
}

/// The type this agent declares it produces, if any.
///
/// Takes the `output_contract` rather than the whole card: the card is moved
/// into the execution context at the delegation call site, and an envelope
/// has no business holding a card alive.
fn declared_type(output_contract: Option<&Value>) -> Option<String> {
    output_contract?
        .get("produces_schema")?
        .as_str()
        .map(str::to_string)
}

/// Build the additive `envelope` value for a delegated execution.
///
/// The payload is the **enforced** document: ungrounded fields already
/// nulled, provenance already stamped. A consumer receives data that has been
/// through the grounding contract rather than data it must trust.
pub fn build(
    agent_name: &str,
    output_contract: Option<&Value>,
    output: &AgentOutput,
    episode_id: Uuid,
) -> Value {
    let ty = declared_type(output_contract);

    // The document, if the producer emitted one. `raw_response` is the
    // verbatim final text (mig-199); before it existed there was nothing here
    // to enforce against.
    let mut payload = output.raw_response.as_deref().and_then(extract_json);

    // Enforce the producer's grounding contract at the hop. This is the
    // protection that delegation never had.
    let report = match payload.as_mut() {
        Some(doc) => crate::grounding_trust::enforce(agent_name, doc),
        None => crate::grounding_trust::Report::default(),
    };

    let has_contract = crate::grounding_trust::contracts_for(agent_name)
        .next()
        .is_some();

    // Validate the enforced payload against the producer's declared schema.
    //
    // Order matters: grounding runs FIRST, then validation. A schema that
    // pins an unsourceable field to `"type": "null"` would otherwise reject
    // a document that grounding was about to clean, and the producer would
    // be blamed for something the platform then fixed. Enforce, then verify
    // what remains.
    //
    // Three outcomes, kept distinct because they need different fixes:
    //   valid        checked and conforming
    //   invalid      the document contradicts the declared type
    //   unverified   no schema, no payload, or a schema keyword the
    //                validator cannot evaluate — NOT a pass
    let schema = output_contract
        .and_then(|oc| oc.get("schema"))
        .filter(|s| s.is_object());
    let (validation_status, schema_violations, unsupported) = match (schema, payload.as_ref()) {
        (Some(sch), Some(doc)) => {
            let r = crate::schema_validate::validate(sch, doc);
            let status = if r.is_valid() {
                "valid"
            } else if r.is_contradiction() {
                "invalid"
            } else {
                "unverified_unsupported_schema"
            };
            (
                status,
                r.violations
                    .iter()
                    .map(|v| json!({ "path": v.path, "message": v.message }))
                    .collect::<Vec<_>>(),
                r.unsupported.clone(),
            )
        }
        (None, _) => ("unverified_no_schema", vec![], vec![]),
        (Some(_), None) => ("unverified_no_payload", vec![], vec![]),
    };

    json!({
        "type": ty,
        // Stated rather than inferred from `type == null`, so a consumer does
        // not have to guess whether an absent type means "undeclared" or
        // "declared but unresolvable".
        "type_status": if ty_is_some(&ty) { "declared" } else { "undeclared" },
        "payload": payload,
        "payload_status": payload_status(output),
        // Whether the payload was checked against `type`, and what happened.
        // `unverified_*` is never a pass: a consumer that treats it as one
        // has reintroduced the defect this envelope exists to close.
        "validation": {
            "status": validation_status,
            "violations": schema_violations,
            "unsupported": unsupported,
        },
        "provenance": {
            "producer": agent_name,
            "episode_id": episode_id,
            // Whether a grounding contract exists for this producer at all.
            // False is not a pass — it means nobody has written one, which
            // `scripts/port_census.py` reports and this must not disguise.
            "grounding_enforced": has_contract,
            "blocks": report
                .provenance
                .iter()
                .map(|(block, verdict)| json!({ "block": block, "provenance": verdict }))
                .collect::<Vec<_>>(),
            "violations": report
                .violations
                .iter()
                .map(|v| json!({
                    "path": v.path,
                    "kind": format!("{:?}", v.kind),
                }))
                .collect::<Vec<_>>(),
        }
    })
}

fn ty_is_some(ty: &Option<String>) -> bool {
    ty.as_deref().is_some_and(|s| !s.trim().is_empty())
}

/// Why there is or is not a payload, in the producer's terms.
fn payload_status(output: &AgentOutput) -> &'static str {
    match output.raw_response.as_deref() {
        None => "no_raw_response",
        Some(t) if t.trim().is_empty() => "empty_response",
        Some(t) if extract_json(t).is_some() => "document",
        // A prose agent. Legitimate, and explicitly non-composable rather
        // than a failure.
        Some(_) => "prose_only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_backend::executor::{AgentMetadata, AgentStatus};

    fn output_with(raw: Option<&str>) -> AgentOutput {
        AgentOutput {
            agent_name: "genome_profiler".into(),
            agent_type: "research".into(),
            timestamp: chrono::Utc::now(),
            status: AgentStatus::Success,
            evidence: vec![],
            confidence: 0.5,
            sources_consulted: vec![],
            execution_time_ms: 10,
            tokens_used: Some(10),
            input_tokens: Some(5),
            output_tokens: Some(5),
            raw_response: raw.map(str::to_string),
            metadata: AgentMetadata::default(),
            tool_invocations: vec![],
            loop_iterations: 1,
        }
    }

    /// The `output_contract` from a real card on disk, so these tests break
    /// if a card stops declaring its type rather than passing on a fixture.
    fn contract_for(agent_id: &str) -> Option<Value> {
        let path = format!("agents/curated/{agent_id}/agent_card.json");
        let json = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let card: Value = serde_json::from_str(&json).unwrap();
        card.get("capabilities")
            .and_then(|c| c.get("output_contract"))
            .cloned()
            .filter(|v| !v.is_null())
    }

    #[test]
    fn extracts_a_document_from_prose_and_a_fence() {
        let bare = r#"{"a":1}"#;
        assert!(extract_json(bare).is_some());
        let fenced = "Here you go:\n```json\n{\"a\":1,\"b\":2}\n```\nhope that helps";
        assert_eq!(extract_json(fenced).unwrap()["b"], json!(2));
        assert!(extract_json("no json at all").is_none());
    }

    #[test]
    fn the_largest_object_wins_not_the_first() {
        // A nested fragment must not be mistaken for the document.
        let t =
            r#"note {"x":1} then the real one {"taxonomy":{"order":"Lepidoptera"},"summary":"s"}"#;
        let v = extract_json(t).unwrap();
        assert!(v.get("taxonomy").is_some(), "got {v}");
    }

    #[test]
    fn the_envelope_carries_the_producers_declared_type() {
        let oc = contract_for("genome_profiler");
        let env = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some(
                r#"{"taxonomy":{"order":"Lepidoptera"},"summary":"A nymphalid."}"#,
            )),
            Uuid::new_v4(),
        );
        assert_eq!(env["type"], json!("rabble/phylogenetic_profile"));
        assert_eq!(env["type_status"], json!("declared"));
        assert_eq!(env["payload_status"], json!("document"));
        assert_eq!(env["provenance"]["producer"], json!("genome_profiler"));
        assert_eq!(env["provenance"]["grounding_enforced"], json!(true));
    }

    #[test]
    fn grounding_is_enforced_at_the_hop() {
        // This is the property delegation never had: a fabricated field
        // passed freely between agents even after the creature handlers
        // started stripping it.
        let oc = contract_for("genome_profiler");
        let raw = r#"{"taxonomy":{"order":"Lepidoptera"},
                      "conservation":{"iucn_status":"Least Concern"},
                      "summary":"A nymphalid."}"#;
        let env = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );

        assert!(
            env["payload"]["conservation"]["iucn_status"].is_null(),
            "an unsourceable status must be stripped before it crosses the seam"
        );
        let violations = env["provenance"]["violations"].as_array().unwrap();
        assert!(
            violations
                .iter()
                .any(|v| v["path"] == "conservation.iucn_status"),
            "and the strip must be reported, not silent: {violations:?}"
        );
    }

    #[test]
    fn provenance_names_each_block_so_a_coordinator_can_weigh_it() {
        let oc = contract_for("genome_profiler");
        let raw = r#"{"taxonomy":{"order":"Lepidoptera","species":"Apatura iris"},
                      "phylogeny":{"sister_taxa":["Apatura ilia"]},
                      "summary":"GBIF places it in Apatura."}"#;
        let env = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );
        let blocks = env["provenance"]["blocks"].as_array().unwrap();
        let tax = blocks
            .iter()
            .find(|b| b["block"] == "taxonomy")
            .expect("taxonomy block verdict missing");
        assert_eq!(tax["provenance"], json!("tool_verified"));
        let cons = blocks
            .iter()
            .find(|b| b["block"] == "conservation")
            .expect("conservation block verdict missing");
        assert_eq!(
            cons["provenance"],
            json!("unavailable_no_tool_source"),
            "an empty block must say WHY it is empty, or a coordinator reads \
             it as a measurement of nothing"
        );
    }

    #[test]
    fn a_conforming_payload_is_reported_valid() {
        let oc = contract_for("genome_profiler");
        let raw = r#"{
            "taxonomy": {"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta",
                         "order":"Lepidoptera","family":"Nymphalidae",
                         "genus":"Apatura","species":"Apatura iris"},
            "taxonomy_provenance": "gbif_verified",
            "genome": {"estimated_size_mb":245.2,"chromosome_count":30,
                       "assembly_name":"MEX_DaPlex","assembly_accession":"GCA_018135715.1",
                       "notable_genes":null,"ploidy":null},
            "genome_provenance": "tool_verified",
            "phylogeny": {"sister_taxa":["Apatura ilia"],"superorder":"Holometabola",
                          "divergence_mya":null,"defining_traits":null},
            "phylogeny_provenance": "platform_derived",
            "conservation": {"iucn_status":null,"population_trend":null,
                             "genetic_diversity_notes":null},
            "conservation_provenance": "unavailable_no_tool_source",
            "summary": "GBIF places Apatura iris in Nymphalidae."
        }"#;
        let env = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );
        assert_eq!(
            env["validation"]["status"],
            json!("valid"),
            "violations: {}",
            env["validation"]["violations"]
        );
    }

    #[test]
    fn a_type_violation_is_reported_not_swallowed() {
        // A genome size as a range string — the shape that shipped for 56
        // episodes. Grounding no longer strips it (the field is sourced
        // now), so the SCHEMA is what has to catch it.
        let oc = contract_for("genome_profiler");
        let raw = r#"{"taxonomy":{},"genome":{"estimated_size_mb":"420-480"},"summary":"x"}"#;
        let env = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );
        assert_eq!(env["validation"]["status"], json!("invalid"));
        let v = env["validation"]["violations"].as_array().unwrap();
        assert!(
            v.iter().any(|x| x["path"] == "genome.estimated_size_mb"),
            "{v:?}"
        );
    }

    /// The first sketch-compiled contract, checked through the hop rather
    /// than against `schema_validate` directly — so this covers the ordering
    /// (`grounding_trust::enforce` first, then validate) as well as the
    /// verdict.
    #[test]
    fn a_sketch_compiled_contract_validates_at_the_hop() {
        let oc = contract_for("equity_analyst");
        let raw = r#"Here is the analysis.
```json
{
  "profile": {"symbol":"AAPL","company_name":"Apple Inc.","sector":"Technology",
              "industry":"Consumer Electronics","price_usd":168.2,
              "market_cap_usd":2610000000000,"beta":1.24},
  "profile_provenance": "tool_verified",
  "valuation_multiples": {"period":"annual","price_to_earnings":28.4,
                          "price_to_book":39.1,"price_to_sales":6.8,
                          "dividend_yield":0.0055},
  "valuation_multiples_provenance": "tool_verified",
  "intrinsic_value": {"dcf_per_share_usd":142.0,"price_at_dcf_date_usd":168.2},
  "intrinsic_value_provenance": "tool_verified",
  "fundamentals": {"enterprise_value_usd":2680000000000,"return_on_equity":1.47,
                   "return_on_invested_capital":0.56,"free_cash_flow_yield":null,
                   "debt_to_equity":1.79},
  "fundamentals_provenance": "unavailable_no_tool_source",
  "analyst_consensus": {"estimate_date":"2026-09-30","revenue_avg_usd":410000000000,
                        "eps_avg":7.21,"eps_low":6.6,"eps_high":7.9,
                        "analyst_count":31},
  "analyst_consensus_provenance": "tool_verified",
  "assessment": {"direction":"overvalued","multiplier_p50":0.75,"multiplier_p5":0.4,
                 "multiplier_p95":1.2,"confidence":0.72,
                 "key_findings":["[MULTIPLIER] Suggested p50: 0.75 (p5: 0.40, p95: 1.20)"]},
  "assessment_provenance": "model_inference",
  "summary": "Trading above FMP's DCF fair value."
}
```"#;
        let env = build(
            "equity_analyst",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );
        assert_eq!(
            env["validation"]["status"],
            json!("valid"),
            "violations: {}",
            env["validation"]["violations"]
        );
        assert_eq!(env["type"], json!("fermi/equity_evidence"));
        assert_eq!(env["payload_status"], json!("document"));

        // Honest about what is still missing. The card now types the
        // document, but nobody has written a `grounding_trust` contract for
        // this producer, so the hop checks shape and not sourcing. Asserted
        // rather than left implicit: `grounding_enforced: false` is exactly
        // the reading that must not be mistaken for a pass, and pinning it
        // here means the day someone writes the contract this test tells
        // them to update the claim.
        assert_eq!(env["provenance"]["grounding_enforced"], json!(false));
    }

    /// A judgement stamped as a retrieval is refused at the hop. This is the
    /// composition-path protection: the value is caught crossing the seam,
    /// which is where a coordinator would otherwise have weighted it as
    /// measured data.
    #[test]
    fn a_reasoned_block_claiming_to_be_retrieved_is_refused_at_the_hop() {
        let oc = contract_for("equity_analyst");
        let raw = r#"{"assessment_provenance":"tool_verified","summary":"s"}"#;
        let env = build(
            "equity_analyst",
            oc.as_ref(),
            &output_with(Some(raw)),
            Uuid::new_v4(),
        );
        assert_eq!(env["validation"]["status"], json!("invalid"));
        let v = env["validation"]["violations"].as_array().unwrap();
        assert!(
            v.iter().any(|x| x["path"] == "assessment_provenance"),
            "{v:?}"
        );
    }

    #[test]
    fn an_untyped_producer_is_unverified_never_valid() {
        // The failure mode to avoid: an agent with no schema must not look
        // like an agent that passed one.
        let env = build(
            "anomaly_triager",
            None,
            &output_with(Some(r#"{"anything":true}"#)),
            Uuid::new_v4(),
        );
        assert_eq!(env["validation"]["status"], json!("unverified_no_schema"));
        assert_ne!(env["validation"]["status"], json!("valid"));
    }

    #[test]
    fn a_prose_agent_gets_an_honest_empty_envelope() {
        // Most agents return narrative. That is legitimate and must not read
        // as a failure — it reads as non-composable.
        let oc = contract_for("anomaly_triager");
        let env = build(
            "anomaly_triager",
            oc.as_ref(),
            &output_with(Some("Three L1 signals, nothing routed to HITL.")),
            Uuid::new_v4(),
        );
        assert_eq!(env["payload_status"], json!("prose_only"));
        assert!(env["payload"].is_null());
        assert_eq!(env["type_status"], json!("undeclared"));
        assert_eq!(
            env["provenance"]["grounding_enforced"],
            json!(false),
            "no contract exists for this agent, and that must be stated \
             rather than disguised as a pass"
        );
    }

    #[test]
    fn a_missing_raw_response_is_distinguishable_from_an_empty_one() {
        let oc = contract_for("genome_profiler");
        let none = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(None),
            Uuid::new_v4(),
        );
        assert_eq!(none["payload_status"], json!("no_raw_response"));
        let empty = build(
            "genome_profiler",
            oc.as_ref(),
            &output_with(Some("   ")),
            Uuid::new_v4(),
        );
        assert_eq!(empty["payload_status"], json!("empty_response"));
    }
}
