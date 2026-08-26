//! # `equity_analyst`: the first sketch-compiled contract, checked end to end
//!
//! `equity_analyst` was one of the 86 agents `agent_contract::TYPED_TIER_EXEMPT`
//! grandfathered — a Fermi orchestra member with nine real data tools and four
//! free-text `produces` labels, which is the most common shape in the corpus
//! and therefore the right one to migrate first.
//!
//! Its contract is not hand-written. It is compiled from
//! `agents/curated/equity_analyst/output_contract.sketch.json` by
//! `contract_sketch`, and this file is what stops the two diverging.
//!
//! ## What "participates in the loop and gate infrastructure" means here
//!
//! A typed schema in this codebase is reachable from three places, and a
//! contract that satisfies only the first is the cosmetic kind the
//! verification paper complains about. So each gets a test:
//!
//! | Where | Mechanism | Test below |
//! |---|---|---|
//! | The **Admission gate**, at publish | `card_contract::validate` via `publish_pipeline` | `the_card_would_pass_the_admission_gate` |
//! | The **delegation hop**, per composition | `envelope::build` → `schema_validate::validate` | `a_conforming_document_reports_valid_at_the_hop` and the four beside it |
//! | The **declaration census**, on the loops surface | `declaration_ladder::CENSUS_SQL` reads `output_contract ? 'produces_schema'` and `jsonb_typeof(schema) = 'object'` | `the_contract_satisfies_the_declaration_census_predicates` |
//!
//! The hop tests are the ones that matter most, because they are the only
//! place the schema meets a real document. They check both directions: that a
//! conforming document reports `valid`, and — the half that is usually
//! missing — that the specific fabrications this contract exists to stop
//! report `invalid` rather than `unverified`. Those are different verdicts
//! with different fixes, and treating the second as a pass is the defect
//! `envelope.rs` was written to close.

use fermi::{card_contract, contract_sketch::Sketch, schema_validate};
use serde_json::{json, Value};

const DIR: &str = "agents/curated/equity_analyst";

fn read(path: &str) -> Value {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

fn card() -> Value {
    read(&format!("{DIR}/agent_card.json"))
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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

fn schema() -> Value {
    card()
        .pointer("/capabilities/output_contract/schema")
        .cloned()
        .expect("the card declares a schema")
}

/// A document that conforms: every key present, every stamp a verdict its
/// block may honestly hold.
fn conforming() -> Value {
    json!({
        "profile": {
            "symbol": "AAPL", "company_name": "Apple Inc.", "sector": "Technology",
            "industry": "Consumer Electronics", "price_usd": 168.2,
            "market_cap_usd": 2.61e12, "beta": 1.24
        },
        "profile_provenance": "tool_verified",
        "valuation_multiples": {
            "period": "annual", "price_to_earnings": 28.4, "price_to_book": 39.1,
            "price_to_sales": 6.8, "dividend_yield": 0.0055
        },
        "valuation_multiples_provenance": "tool_verified",
        "intrinsic_value": { "dcf_per_share_usd": 142.0, "price_at_dcf_date_usd": 168.2 },
        "intrinsic_value_provenance": "tool_verified",
        "fundamentals": {
            "enterprise_value_usd": 2.68e12, "return_on_equity": 1.47,
            "return_on_invested_capital": 0.56, "free_cash_flow_yield": 0.031,
            "debt_to_equity": 1.79
        },
        "fundamentals_provenance": "tool_verified",
        "analyst_consensus": {
            "estimate_date": "2026-09-30", "revenue_avg_usd": 4.1e11, "eps_avg": 7.21,
            "eps_low": 6.60, "eps_high": 7.90, "analyst_count": 31
        },
        "analyst_consensus_provenance": "tool_verified",
        "assessment": {
            "direction": "overvalued",
            "multiplier_p50": 0.75, "multiplier_p5": 0.40, "multiplier_p95": 1.20,
            "confidence": 0.72,
            "key_findings": [
                "[BASE RATE] S&P 500 quarterly EPS beat rate 73%",
                "[MULTIPLIER] Suggested p50: 0.75 (p5: 0.40, p95: 1.20) - stretched valuation"
            ]
        },
        "assessment_provenance": "model_inference",
        "summary": "Trading above FMP's DCF fair value on a forward-multiple premium."
    })
}

// ─── the sketch is the source of truth ─────────────────────────────────

/// The card is the compiler's output. If someone hand-edits the contract, the
/// sketch stops describing the agent and every future recompile silently
/// reverts their change — which is how generated artefacts in this repo have
/// previously rotted. Caught here instead.
#[test]
fn the_card_is_exactly_what_the_sketch_compiles_to() {
    let card = card();
    let sketch = Sketch::from_json(&read(&format!("{DIR}/output_contract.sketch.json")))
        .expect("the sketch parses");
    let compiled = sketch
        .compile(&tool_names(&card))
        .unwrap_or_else(|f| panic!("the sketch does not compile:\n{f:#?}"));

    assert_eq!(
        card.pointer("/capabilities/output_contract"),
        Some(&compiled.output_contract),
        "the card's output_contract has drifted from the sketch. The sketch is the \
         source of truth: `cargo run --bin contract-sketch -- equity_analyst` and \
         splice the result (see the header of scripts/contract_sketch.rs)."
    );
    assert_eq!(strings(card.get("produces")), compiled.produces);
}

/// Six authored blocks, six derived stamps. Pinned because the ratio is the
/// argument for the sketch existing at all: if the compiler ever stopped
/// deriving these, the authoring cost would quietly return.
#[test]
fn the_compiler_derived_half_of_the_declared_surface() {
    let card = card();
    let sketch = Sketch::from_json(&read(&format!("{DIR}/output_contract.sketch.json"))).unwrap();
    let compiled = sketch.compile(&tool_names(&card)).unwrap();

    let props = compiled
        .output_contract
        .pointer("/schema/properties")
        .and_then(|p| p.as_object())
        .unwrap()
        .len();

    assert_eq!(props, 13);
    assert_eq!(compiled.generated_properties.len(), 6);
    assert_eq!(
        compiled
            .output_contract
            .get("grounding")
            .and_then(|g| g.as_object())
            .unwrap()
            .len(),
        13,
        "grounding must cover every property — it is emitted from the same \
         traversal, so a mismatch means the compiler has a bug rather than the \
         author"
    );
}

// ─── the Admission gate ────────────────────────────────────────────────

#[test]
fn the_card_would_pass_the_admission_gate() {
    let card = card();
    let findings = card_contract::validate(
        card.pointer("/capabilities/output_contract"),
        &strings(card.get("produces")),
        &tool_names(&card),
    );
    assert!(
        findings.is_empty(),
        "publish would be refused:\n{}",
        findings
            .iter()
            .map(|f| format!("  [{}] {}", f.check, f.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The exemption must be gone, or the gate above is passing an agent it was
/// never going to refuse and this file proves nothing.
#[test]
fn the_agent_no_longer_takes_the_grandfathering_discount() {
    assert!(
        !fermi::workflows::agent_contract::is_typed_tier_exempt("equity_analyst"),
        "equity_analyst is still in TYPED_TIER_EXEMPT, so `typed_tier_violations` \
         returns empty for it regardless of its card and the Admission gate has no \
         opinion. Remove it from the list."
    );
}

/// Every `sourced` block names a tool the agent actually declares. This is
/// checked by `card_contract` too; it is repeated here against the real card
/// because it is the one check whose failure mode is a plausible-looking
/// contract, and a plausible contract is indistinguishable from a true one.
#[test]
fn every_sourced_block_names_a_tool_this_agent_can_call() {
    let card = card();
    let tools = tool_names(&card);
    let grounding = card
        .pointer("/capabilities/output_contract/grounding")
        .and_then(|g| g.as_object())
        .expect("grounding");

    let mut sourced = 0;
    for (field, spec) in grounding {
        if spec.get("status").and_then(|s| s.as_str()) != Some("sourced") {
            continue;
        }
        sourced += 1;
        let tool = spec.get("tool").and_then(|t| t.as_str()).unwrap_or("");
        assert!(
            tools.iter().any(|t| t == tool),
            "`{field}` claims to be sourced from `{tool}`, which this agent does not \
             declare"
        );
        assert!(
            spec.get("response_field")
                .and_then(|r| r.as_str())
                .is_some_and(|r| !r.trim().is_empty()),
            "`{field}` is sourced but does not say which part of `{tool}`'s response \
             supplies it, so the claim cannot be checked against the tool's output"
        );
    }
    assert_eq!(
        sourced, 5,
        "five blocks are retrieved from FMP; if this changed, the provenance enums \
         below changed with it"
    );
}

// ─── the delegation hop ────────────────────────────────────────────────

/// The precondition for everything else on this surface. A schema carrying
/// one keyword `schema_validate` cannot evaluate makes every document report
/// `unverified_unsupported_schema` — which is not a pass, and is strictly
/// worse than having declared nothing, because it looks like coverage.
#[test]
fn the_schema_uses_only_keywords_the_validator_implements() {
    let report = schema_validate::validate(&schema(), &conforming());
    assert!(
        report.unsupported.is_empty(),
        "the schema uses keywords src/schema_validate.rs cannot evaluate: {:#?}\n\
         Note `minimum`/`maximum` in particular: the multiplier bound belongs in a \
         description until the validator can enforce it.",
        report.unsupported
    );
}

#[test]
fn a_conforming_document_reports_valid_at_the_hop() {
    let report = schema_validate::validate(&schema(), &conforming());
    assert!(report.is_valid(), "{report:#?}");
}

/// The fabrication the contract exists to stop: a block filled from the
/// model's memory and stamped as though a tool had answered. The schema
/// cannot see *that* — grounding does — but it can and must refuse the
/// adjacent move, which is claiming a verdict the block may not hold.
#[test]
fn a_stamp_the_block_may_not_hold_is_a_contradiction_not_an_unknown() {
    let mut doc = conforming();
    // `profile` has complete coverage: FMP either answers or has no such
    // symbol. "No source exists" is not one of its outcomes.
    doc["profile_provenance"] = json!("unavailable_no_tool_source");

    let report = schema_validate::validate(&schema(), &doc);
    assert!(
        report.is_contradiction(),
        "a sourced block with complete coverage claiming there is no source must be \
         refused, not merely unverified: {report:#?}"
    );
}

/// And the converse, which is the more dangerous direction: a block whose
/// tool genuinely may not cover it must be *allowed* to say so. A schema that
/// refused this would push the agent toward a confident null.
#[test]
fn a_block_with_partial_coverage_may_admit_it_has_no_source() {
    let mut doc = conforming();
    doc["fundamentals_provenance"] = json!("unavailable_no_tool_source");
    assert!(
        schema_validate::validate(&schema(), &doc).is_valid(),
        "fundamentals is declared `partial` precisely so this verdict is reachable"
    );
}

/// A judgement may never present itself as a retrieval, and the `const` is
/// what makes that unrepresentable rather than discouraged.
#[test]
fn the_judgement_block_can_never_claim_to_be_tool_verified() {
    let mut doc = conforming();
    doc["assessment_provenance"] = json!("tool_verified");
    assert!(
        schema_validate::validate(&schema(), &doc).is_contradiction(),
        "assessment is the agent's own reasoning; a run that stamped it \
         tool_verified would be laundering an inference through the composition path"
    );
}

#[test]
fn an_invented_key_is_refused_because_the_document_is_closed() {
    let mut doc = conforming();
    doc.as_object_mut()
        .unwrap()
        .insert("insider_sentiment".into(), json!({ "score": 0.8 }));
    assert!(
        schema_validate::validate(&schema(), &doc).is_contradiction(),
        "an extra top-level key is a field nobody classified, arriving by the one \
         route the grounding bijection cannot see"
    );
}

#[test]
fn a_missing_block_is_refused_so_absence_must_be_stated() {
    let mut doc = conforming();
    doc.as_object_mut().unwrap().remove("intrinsic_value");
    assert!(
        schema_validate::validate(&schema(), &doc).is_contradiction(),
        "every key is required and nullable where it can be empty. Dropping a key \
         makes `I looked and found nothing` indistinguishable from `I did not look`."
    );
}

#[test]
fn a_null_block_is_accepted_because_that_is_the_honest_empty_answer() {
    let mut doc = conforming();
    doc["intrinsic_value"] = json!({
        "dcf_per_share_usd": null, "price_at_dcf_date_usd": null
    });
    doc["intrinsic_value_provenance"] = json!("tool_no_match");
    assert!(
        schema_validate::validate(&schema(), &doc).is_valid(),
        "the tool was asked and had nothing — the single most common real outcome, \
         and it must be cheaper to state than to paper over"
    );
}

#[test]
fn an_out_of_vocabulary_direction_is_refused() {
    let mut doc = conforming();
    doc["assessment"]["direction"] = json!("probably_fine");
    assert!(schema_validate::validate(&schema(), &doc).is_contradiction());
}

// ─── the loops surface ─────────────────────────────────────────────────

/// `declaration_ladder::CENSUS_SQL` counts an agent as having reached the
/// `output_schema` rung when `output_contract ? 'produces_schema'` and
/// `jsonb_typeof(output_contract -> 'schema') = 'object'`. Asserted against
/// the card so the agent is visible on `/api/declarations` as typed, rather
/// than typed in a way only this test can see.
#[test]
fn the_contract_satisfies_the_declaration_census_predicates() {
    let card = card();
    let oc = card
        .pointer("/capabilities/output_contract")
        .expect("output_contract");
    assert!(
        oc.get("produces_schema")
            .and_then(|v| v.as_str())
            .is_some_and(|s| s.contains('/')),
        "the census keys on `produces_schema`, and the gate wants it namespaced"
    );
    assert!(
        oc.get("schema").is_some_and(|s| s.is_object()),
        "the census requires jsonb_typeof(schema) = 'object'"
    );
}

/// The agent is a Fermi orchestra member, so its contract must say how it is
/// eventually scored. Without this the typed document is composable and
/// unfalsifiable, which is a strange combination to ship.
#[test]
fn the_contract_declares_how_it_gets_scored() {
    let card = card();
    let cal = card
        .pointer("/capabilities/output_contract/calibration")
        .expect("a Fermi member declares a calibration signal");
    assert_eq!(
        cal.get("signal").and_then(|s| s.as_str()),
        Some("brier_forecast")
    );
    assert_eq!(
        card.pointer("/capabilities/output_contract/synthesis")
            .and_then(|s| s.as_str()),
        Some("cep_weighted"),
        "how a coordinator combines members' documents"
    );
}

// ─── the contract is not decoration ────────────────────────────────────

/// The failure this whole line of work exists to prevent: a beautifully typed
/// contract on an agent nobody ever asked to produce that type. The schema
/// would then be checked against prose forever and report
/// `unverified_no_payload`, which reads as "fine" from any distance.
#[test]
fn the_prompt_actually_asks_for_the_document_the_card_declares() {
    let card = card();
    let prompt = card
        .get("system_prompt")
        .and_then(|p| p.as_str())
        .expect("system_prompt");

    let props: Vec<String> = schema()
        .get("properties")
        .and_then(|p| p.as_object())
        .unwrap()
        .keys()
        .cloned()
        .collect();

    for p in &props {
        assert!(
            prompt.contains(p),
            "the schema declares `{p}` but the system prompt never mentions it, so \
             nothing asks the model to produce it. An unrequested field is a \
             contract clause with no counterparty."
        );
    }

    assert!(
        prompt.contains("fermi/equity_evidence"),
        "the prompt should name the type it is being asked to emit"
    );
    assert!(
        prompt.contains("[MULTIPLIER]"),
        "the orchestra's existing text parser keys on this label; typing the \
         document must not silently drop it"
    );
}
