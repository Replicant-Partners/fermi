//! Product carbon accounting — the agent-driven half of the DPP's carbon block.
//!
//!   POST /api/workspaces/:id/actions/calculate_carbon
//!
//! ## Why this exists
//!
//! `dpp/composition.yaml` shipped with
//!
//! ```yaml
//! carbon_intensity:
//!   mode: synthetic
//!   value_kg_per_kg: 0.41
//!   scope_3_dominant: hibiscus_infusion
//! ```
//!
//! A person typed `0.41`. It is the third fixture in this App — after the
//! synthetic regulatory rulesets and the hardcoded BOM — and the one with a
//! compliance deadline behind it: carbon disclosure is already mandatory in the
//! EU Battery DPP, CBAM enters its definitive stage in 2026, and ESRS E1 is the
//! shape the food sector is being pulled toward.
//!
//! ## The division of labour, which is the whole design
//!
//! A product carbon figure is three claims wearing one number:
//!
//! | part | example | who produces it |
//! | --- | --- | --- |
//! | the emission factor | `dried hibiscus, Egypt: 2.1 kg CO2e/kg` | retrieved, cited |
//! | the arithmetic | `0.02805 kg x 2.1 = 0.0589 kg CO2e` | **this file, in Rust** |
//! | the scope attribution | `Scope 3, category 1` | the agent's judgement |
//!
//! The agent does not multiply. That is not a stylistic preference: a
//! confidently wrong product of two plausible numbers looks exactly like a
//! right one, and nobody recomputes a figure that came back formatted. It is
//! also the easy half. So [`normalise_reply`] echoes the authoritative BOM
//! quantity onto every line, and `grounding_trust::DERIVATIONS` performs the
//! multiplication inside `enforce` — overwriting whatever the reply carried
//! and reporting the disagreement count as `model_arithmetic_disagreements`.
//!
//! ## What is deliberately NOT done here
//!
//! No emission factors live in this file. There is no fallback table, no
//! "typical beverage" default, no proxy substitution. A hardcoded factor here
//! would be `mode: synthetic` again, one layer down and harder to see — and it
//! would be worse than the typed `0.41`, because it would arrive wearing a
//! provenance stamp.
//!
//! The handler orchestrates, echoes the BOM, does the arithmetic, gates and
//! persists. The corpus supplies the factors and the agent judges the scopes.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

use fermi_auth::AuthPrincipal;

use super::actions::resolve_workspace;
use crate::{grounding_trust, AppState};
use fermi::grounding_anomaly;

/// The agent that retrieves the factors and judges the scopes.
const ACCOUNTANT: &str = "carbon_accountant";

/// Wall-clock budget for one statement.
///
/// One run is up to four rounds of `web_search` — `ToolAwareExecutor` caps the
/// loop at five iterations — plus the reasoning over what came back, inside an
/// HTTP client that already allows 90s per hop. A six-line bill of materials
/// against three factor databases is minutes, not seconds, and saying so is
/// better than leaving a caller to guess whether the request has hung.
const RUN_TIMEOUT_SECS: u64 = 300;

/// `message_type` passed to `dispatch_rabble_action`.
///
/// Not `"calculate_carbon"`, and the reason is a constraint rather than taste.
/// `dispatch_rabble_action` assigns its `action_type` argument straight to
/// `workspace_messages.message_type`, which carries its own CHECK constraint
/// (mig-077) admitting a closed list that `calculate_carbon` is not on. The
/// insert is wrapped in `let _ =`, so a fresh token fails that CHECK silently
/// and the agent's reply is never posted to the workspace transcript —
/// which is what `evaluate_claim` does today, undiagnosed.
///
/// `agent_action` is already admitted and means exactly this. The action's own
/// identity is recorded where it belongs, in `workspace_action_log.action_type`
/// (mig-236), which is the audit anchor.
const DISPATCH_MESSAGE_TYPE: &str = "agent_action";

// ─── request ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CalculateCarbonRequest {
    /// System boundary to attempt. Defaults to `cradle_to_gate`, which is the
    /// only boundary a bill of materials can support on its own: nothing in
    /// `composition.yaml` describes distribution, use or disposal.
    pub boundary: Option<String>,
    /// Reporting region, used to bias factor geography. Defaults to `EU`.
    pub region: Option<String>,
    /// Recalculate even though `dpp/carbon/statement.yaml` already exists.
    #[serde(default)]
    pub force: bool,
    pub source_message_id: Option<String>,
}

const STATEMENT_PATH: &str = "dpp/carbon/statement.yaml";

// ─── the bill of materials, read in Rust ─────────────────────────────────────

/// One BOM line, with its quantity resolved to kilograms where that is
/// possible and left absent where it is not.
#[derive(Debug, Clone)]
struct BomLine {
    item_id: String,
    name: String,
    role: String,
    origin: Option<String>,
    quantity_declared: String,
    /// Kilograms of this material per serving. `None` for `trace` and for any
    /// quantity no basis can convert.
    qty_kg: Option<f64>,
    /// How `qty_kg` was arrived at, stated rather than implied. An unstated
    /// conversion assumption is a figure per kilogram that silently means
    /// something else.
    basis: Option<String>,
}

/// Parse a quantity string against a serving basis.
///
/// Two forms, and a third that is a real answer rather than a failure:
///
///   * `"8.5%"` — a percentage of the formulation. Needs a basis to convert,
///     and treats 1 ml as 1 g, which is defensible for an aqueous beverage and
///     indefensible if left unsaid. Hence the returned `basis` string.
///   * `"1.2 kg"` — absolute. No assumption needed.
///   * `"trace"`, `""`, anything else — `None`. The line still travels, with
///     `quantity_declared` carrying what the document actually holds, so the
///     agent can identify and source the material; it simply cannot be priced,
///     and `unpriced_items` will name it.
///
/// The parsing rule itself lives in [`super::bom_pricing::parse_quantity`] and
/// is called rather than restated. This function only converts its result to
/// kilograms and rejects what a footprint cannot use.
///
/// It was a second implementation until `price_bom` landed and moved the same
/// conversion server-side. Two readings of one BOM that disagree would put a
/// different mass behind the PRICE and the FOOTPRINT of the same product, and
/// each document would be internally consistent — the hardest kind of
/// disagreement to notice, and the one this repo keeps rediscovering. The
/// pricing module owns the rule because it got there first; carbon owns only
/// the unit conversion, which pricing does not need.
fn resolve_qty_kg(raw: &str, basis_ml: Option<f64>) -> (Option<f64>, Option<String>) {
    let Some((qty, unit, note)) = super::bom_pricing::parse_quantity(raw, basis_ml) else {
        return (None, None);
    };
    // A negative or non-finite quantity is not a small problem here: it
    // multiplies straight into the total and produces a negative footprint,
    // which is a removal claim with its own accounting rules and its own abuse.
    // Pricing can shrug at it; carbon cannot.
    if !qty.is_finite() || qty < 0.0 {
        return (None, None);
    }
    let kg = match unit {
        "kg" => qty,
        "g" => qty / 1000.0,
        "mg" => qty / 1_000_000.0,
        // Volumes, on the same aqueous assumption the percentage branch makes.
        "l" => qty,
        "ml" => qty / 1000.0,
        // `unit` and `pcs` are countable, not weighable. An emission factor is
        // per kilogram, so there is nothing to multiply and the line is
        // honestly unpriceable rather than guessed at a mass.
        _ => return (None, None),
    };
    let basis = note.unwrap_or_else(|| format!("{qty} {unit} as declared"));
    (
        Some(kg),
        Some(if unit == "l" || unit == "ml" || basis.contains('%') {
            format!("{basis}, 1 ml taken as 1 g")
        } else {
            basis
        }),
    )
}

/// Read the bill of materials out of a parsed `dpp/composition.yaml`.
fn read_bom(composition: &Value) -> (Vec<BomLine>, Option<f64>) {
    let basis_ml = super::bom_pricing::serving_basis_ml(composition);

    let items = composition
        .get("consists_of")
        .or_else(|| composition.get("ingredients"))
        .and_then(|v| v.as_array());

    let mut out = Vec::new();
    for (i, it) in items.into_iter().flatten().enumerate() {
        let item_id = it
            .get("item_id")
            .or_else(|| it.get("id"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            // A line with no id still has to be accountable for, or it drops
            // out of the total without appearing in `unpriced_items`.
            .unwrap_or_else(|| format!("line_{i}"));
        let name = it
            .get("display_name")
            .or_else(|| it.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&item_id)
            .to_string();
        let quantity_declared = it
            .get("quantity")
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .unwrap_or_default();
        let (qty_kg, basis) = resolve_qty_kg(&quantity_declared, basis_ml);
        out.push(BomLine {
            item_id,
            name,
            role: it
                .get("role")
                .or_else(|| it.get("category"))
                .and_then(|v| v.as_str())
                .unwrap_or("unspecified")
                .to_string(),
            origin: it
                .get("origin")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            quantity_declared,
            qty_kg,
            basis,
        });
    }
    (out, basis_ml)
}

// ─── prompt construction ─────────────────────────────────────────────────────

/// The output shape, authored here rather than in the agent card.
///
/// Two reasons, and the second is the load-bearing one. It lives beside the
/// parser that reads it, so the repo's recurring failure — two copies of one
/// decision drifting apart — has one fewer place to happen. And a JSON
/// contract instruction in a system prompt trips `structured_output_trigger`
/// (`src/agent_backend/tool_executor.rs`), which makes `ToolAwareExecutor`
/// bypass the tool loop entirely: the agent would run single-shot with no
/// `web_search` at all and answer from training data while appearing to have
/// searched every factor database in the prompt.
fn output_shape(lines: &[BomLine]) -> String {
    let example = lines
        .iter()
        .find(|l| l.qty_kg.is_some())
        .or_else(|| lines.first());
    let example_id = example.map(|l| l.item_id.as_str()).unwrap_or("item_id");

    format!(
        r#"{{
  "inventory": {{
    "queries_run": ["the web_search queries you actually issued"],
    "items": [
      {{
        "item_id": "{example_id}",
        "material": "what you searched for — the material, not the product",
        "origin": "the BOM line's stated origin, echoed, or null",
        "factor_kg_co2e_per_kg": 0.0,
        "factor_unit": "kg CO2e/kg",
        "lca_basis": "cradle_to_gate | cradle_to_grave | gate_to_gate",
        "geography": "the geography the factor applies to — not the BOM origin",
        "reference_year": 2021,
        "dataset": "the database and version, e.g. ecoinvent 3.9.1",
        "source_url": "a URL that came back in one of your searches",
        "source_title": "the result title",
        "source_quote": "the retrieved text carrying the factor",
        "corroborating_value_kg_co2e_per_kg": 0.0,
        "corroborating_dataset": "a DIFFERENT publisher's name, or null if only one carries it",
        "corroborating_source_url": "a URL from one of your searches",
        "corroboration": null,
        "kg_co2e": null,
        "arithmetic": null
      }}
    ],
    "total_kg_co2e": null,
    "coverage": null,
    "unpriced_items": null
  }},
  "attribution": {{
    "items": [
      {{
        "item_id": "{example_id}",
        "scope": "scope_1 | scope_2 | scope_3 | out_of_scope",
        "ghg_protocol_category": 1,
        "rationale": "what in this BOM line drove the allocation"
      }}
    ],
    "scope_boundary_note": "where you assumed the organisational boundary sits, or null"
  }},
  "boundary": {{
    "declared": "cradle_to_gate | cradle_to_grave | gate_to_gate | indeterminate",
    "included": ["the life-cycle stages this figure covers"],
    "excluded": ["the stages it does not"],
    "exclusion_rationale": "why each exclusion is defensible, or that it is not",
    "standard_followed": "the methodology you read the factors against, or null"
  }},
  "assurance": {{
    "needs_expert": true,
    "verification_status": "unverified",
    "regulatory_fitness": [
      {{
        "regime": "CSRD/ESRS E1 | CBAM | EU ETS | EU Battery DPP | Green Claims Directive | ISO 14067",
        "fit": "adequate | screening_only | inadmissible",
        "why": "what about this evidence decides the fit"
      }}
    ],
    "blocking_gaps": ["what would have to be obtained to move up a tier"]
  }},
  "explanation": "Where this product's footprint concentrates, what the retrieval could and could not establish, and what a reader must not conclude from the number. This is the prose the operator reads."
}}"#
    )
}

fn describe_bom(lines: &[BomLine], basis_ml: Option<f64>, product: &Value) -> String {
    let name = product
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed product)");
    let pid = product
        .get("product_id")
        .and_then(|v| v.as_str())
        .unwrap_or("(no id)");

    let mut out = format!("Product: {name} ({pid})\n");
    match basis_ml {
        Some(ml) => out.push_str(&format!(
            "Serving basis: {ml} ml. Percentages below are read as w/v against \
             that serving, with 1 ml taken as 1 g.\n"
        )),
        None => out.push_str(
            "Serving basis: absent from the document. Percentage quantities \
             cannot be converted to a mass, so those lines carry no quantity \
             and cannot be priced.\n",
        ),
    }
    out.push_str("\nBill of materials, with the quantity the platform will multiply by:\n");
    for l in lines {
        out.push_str(&format!("  - {} [{}]", l.name, l.item_id));
        out.push_str(&format!("\n      role: {}", l.role));
        if let Some(o) = &l.origin {
            out.push_str(&format!("\n      origin: {o}"));
        }
        out.push_str(&format!("\n      declared: {}", l.quantity_declared));
        match l.qty_kg {
            Some(kg) => out.push_str(&format!("\n      activity_qty_kg: {kg}")),
            None => out.push_str(
                "\n      activity_qty_kg: none — this line cannot be priced, \
                 whatever factor you find. Report the factor anyway if you \
                 find one; it is worth caching.",
            ),
        }
        out.push('\n');
    }
    out
}

fn build_query(
    lines: &[BomLine],
    basis_ml: Option<f64>,
    product: &Value,
    req_boundary: &str,
    region: &str,
    hints: &std::collections::HashMap<String, FactorHint>,
) -> String {
    let priceable = lines.iter().filter(|l| l.qty_kg.is_some()).count();

    let mut q = String::new();
    q.push_str(
        "Produce a product carbon statement for this bill of materials, against \
         the published life-cycle inventory corpus.\n\n",
    );
    q.push_str(&describe_bom(lines, basis_ml, product));
    q.push_str(&format!(
        "\nBoundary requested: {req_boundary}\nReporting region: {region}\n"
    ));

    if !hints.is_empty() {
        q.push_str(
            "\nWhat earlier runs already established\n\
             For these lines a previous run identified the material and the \
             dataset row. The VALUE is deliberately not repeated here: you must \
             read it yourself. Two runs agreeing because the second was handed \
             the first's number is not evidence of anything, and the platform \
             compares independently retrieved values to catch a wrong factor. \
             What you are saved is the expensive part — working out which \
             dataset row applies.\n",
        );
        for l in lines {
            let Some(h) = hints.get(&l.item_id) else {
                continue;
            };
            q.push_str(&format!(
                "  - {} [{}]: previously identified as \"{}\"{} for {} ({}){}\n",
                l.name,
                l.item_id,
                h.material,
                h.dataset
                    .as_deref()
                    .map(|d| format!(" in {d}"))
                    .unwrap_or_default(),
                h.geography,
                h.reference_year,
                h.source_url
                    .as_deref()
                    .map(|u| format!("\n      {u}"))
                    .unwrap_or_default(),
            ));
        }
        q.push_str(
            "  If a line is not in fact that material, ignore the hint and \
             search afresh — it is keyed on an identifier local to a product, \
             not on the substance.\n",
        );
    }

    q.push_str(
        "\nHow to proceed\n\
         Search the corpus. One or more web_search calls per line, on the \
         material and its geography rather than on the product. Where a hint \
         above names the dataset row, go to that source and read the figure \
         off it rather than searching for the material again. \
         The tool loop is capped at five iterations, so you will not get an \
         unlimited number of rounds: spend them on the lines that carry the \
         mass. ",
    );
    q.push_str(&format!(
        "{priceable} of {} lines have a quantity and can affect the total; the \
         rest cannot move it whatever factor you find.\n\n",
        lines.len()
    ));

    q.push_str("Reply with a single fenced json block in this shape:\n\n```json\n");
    q.push_str(&output_shape(lines));
    q.push_str("\n```\n\n");

    q.push_str(
        "Rules that decide whether this statement is usable\n\
         - Leave `kg_co2e`, `arithmetic`, `total_kg_co2e`, `coverage` and \
           `unpriced_items` null. The platform multiplies the quantity above by \
           the factor you retrieve, in Rust, and writes those fields over \
           whatever you put there. If you fill them the disagreement is counted \
           and reported, so guessing shows up as a number rather than as a \
           wrong footprint.\n\
         - Seek a SECOND reading of each factor from a different publisher, \
           and record it in the `corroborating_*` fields. Different publisher \
           is the whole requirement: ecoinvent against Agribalyse, or against \
           a DEFRA factor, or against a supplier EPD. The same dataset quoted \
           twice is not a second reading, it is the first one fetched again, \
           and the platform counts those as a corroboration that decorrelated \
           nothing.\n\
           Why this is asked for: the platform compares factors across runs to \
           catch a wrong one, and two of your runs are not independent — same \
           corpus, same ranking, same prior — so you could agree with yourself \
           while both readings were wrong. Two publishers in one run cannot \
           agree by that mechanism. Leave `corroboration` null: the platform \
           decides whether the two match, on a 30% band, because two \
           publishers legitimately differ and because deciding whether your \
           own two numbers agree is not a retrieval.\n\
         - If only one inventory carries the material, say so by leaving the \
           `corroborating_*` fields null. That is a real and common answer, \
           and it is recorded as `single_source` rather than as a failure. Do \
           not invent a second citation to satisfy the rule — a fabricated \
           corroboration is worse than an honest one, because it converts a \
           known weakness into a false assurance.\n\
         - Every `source_url` must be a result you actually received on this \
           run. A plausible ecoinvent process URL is indistinguishable from a \
           real one to anyone without a licence, which is most readers.\n\
         - A factor with no dataset, geography and reference year beside it is \
           not usable in a disclosure. Nobody can then tell whether a 2014 \
           European average is standing in for a named Egyptian supplier.\n\
         - If nothing bears on a line, set `factor_kg_co2e_per_kg` to null and \
           say in the same entry what you searched for. Do not substitute a \
           proxy material without labelling it a proxy, and do not fall back on \
           what you remember. A line with no factor is unknown, never zero — \
           and zero is the one value that would make the total wrong in the \
           direction nobody checks.\n\
         - `geography` is the factor's geography, not the BOM line's origin. \
           They are often different and that is fine; conflating them is not.\n\
         - Set `needs_expert: true` whenever coverage is incomplete, the \
           geographies or years do not match the BOM, the bases are mixed, or \
           any line comes back `diverging` — two publishers more than 30% \
           apart on the same material means at least one of them does not \
           describe what you think it does, and that is a question for a \
           person rather than a number to average. Under-flagging is the \
           expensive direction here.\n\
         - `verification_status` is `unverified`. You are not an accredited \
           verifier. Do not state or imply that the product is carbon neutral, \
           climate neutral or offset.\n\
         - Do not write workspace files for this task. The platform runs the \
           grounding gate over your reply, does the arithmetic, persists the \
           enforced document, and appends every fully-identified factor to its \
           own ledger so that a later run resolving the same material, \
           geography, year and dataset can be compared against yours. Anything \
           you wrote directly would bypass all of that.\n\
         - A factor is only comparable, and therefore only checkable, if it \
           carries all four of material, geography, reference_year and dataset. \
           One missing field does not make the factor useless, but it does take \
           it out of the evidence base, and the response says how many were \
           dropped for that reason.\n",
    );
    q
}

// ─── normalisation: the platform's half of the document ──────────────────────

/// How many lines the model priced itself, and how far off it was.
struct ArithmeticAudit {
    /// Lines where the model wrote a `kg_co2e` at all.
    attempted: usize,
    /// Of those, how many disagreed with `qty * factor` beyond rounding.
    disagreements: usize,
    /// The worst relative error seen, for the action log.
    worst_relative_error: Option<f64>,
}

/// Make the reply into a document the contract can be enforced over.
///
/// Four things happen here and each is a refusal to trust the reply about
/// something the platform already knows:
///
///   1. **The line set is the BOM's, not the model's.** Lines are matched by
///      `item_id`; a line the reply omitted is added empty, and an `item_id`
///      the BOM does not contain is dropped. Otherwise a statement could omit
///      an ingredient and report `coverage: complete`, or invent one.
///   2. **`activity_qty_kg` is overwritten from the BOM.** A quantity the
///      model restated is a quantity nobody wrote, and here it multiplies
///      straight into the total.
///   3. **The derived keys are seeded.** `set_path` can finish a block but not
///      build one, and a well-behaved reply leaves these null or absent.
///   4. **The model's own arithmetic is measured before it is discarded**, so
///      the discipline the design depends on is observed rather than assumed.
fn normalise_reply(doc: &mut Value, lines: &[BomLine]) -> ArithmeticAudit {
    let mut audit = ArithmeticAudit {
        attempted: 0,
        disagreements: 0,
        worst_relative_error: None,
    };

    let inventory = doc
        .as_object_mut()
        .expect("caller guarantees an object")
        .entry("inventory")
        .or_insert_with(|| json!({}));
    if !inventory.is_object() {
        *inventory = json!({});
    }

    let replied: Vec<Value> = inventory
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut rebuilt = Vec::with_capacity(lines.len());
    for line in lines {
        let mut item = replied
            .iter()
            .find(|it| {
                it.get("item_id")
                    .and_then(|v| v.as_str())
                    .is_some_and(|id| id == line.item_id)
            })
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !item.is_object() {
            item = json!({});
        }

        // Measure before overwriting. `qty * factor` uses the BOM quantity
        // rather than the model's, because the question is whether the model
        // multiplied correctly, not whether it also mis-copied the quantity.
        if let (Some(claimed), Some(qty), Some(factor)) = (
            item.get("kg_co2e").and_then(|v| v.as_f64()),
            line.qty_kg,
            item.get("factor_kg_co2e_per_kg").and_then(|v| v.as_f64()),
        ) {
            audit.attempted += 1;
            let truth = qty * factor;
            let err = (claimed - truth).abs();
            let tolerance = (0.005 * truth.abs()).max(1e-9);
            if err > tolerance {
                audit.disagreements += 1;
                let rel = if truth.abs() > 0.0 {
                    err / truth.abs()
                } else {
                    f64::INFINITY
                };
                audit.worst_relative_error = Some(match audit.worst_relative_error {
                    Some(prev) if prev >= rel => prev,
                    _ => rel,
                });
            }
        }

        let obj = item.as_object_mut().expect("normalised to object");
        obj.insert("item_id".into(), json!(line.item_id));
        obj.insert(
            "activity_qty_kg".into(),
            line.qty_kg
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        );
        obj.insert(
            "activity_basis".into(),
            line.basis
                .as_deref()
                .map(|b| Value::String(b.to_string()))
                .unwrap_or(Value::Null),
        );
        obj.insert("bom_name".into(), json!(line.name));
        obj.insert(
            "bom_origin".into(),
            line.origin
                .as_deref()
                .map(|o| Value::String(o.to_string()))
                .unwrap_or(Value::Null),
        );
        obj.entry("kg_co2e").or_insert(Value::Null);
        obj.entry("arithmetic").or_insert(Value::Null);
        rebuilt.push(item);
    }

    let inv = inventory.as_object_mut().expect("normalised to object");
    inv.insert("items".into(), Value::Array(rebuilt));
    // Seeded so the derivations have somewhere to write. A reply that obeyed
    // the prompt omits all three.
    for key in ["total_kg_co2e", "coverage", "unpriced_items"] {
        inv.insert(key.into(), Value::Null);
    }
    audit
}

// ─── the factor ledger ───────────────────────────────────────────────────────
//
// `carbon_emission_factors` (mig-238) is the platform's second copy of a fact
// it cannot otherwise hold. Every factor a run retrieves is appended with the
// key that makes it comparable — (material, geography, reference_year,
// dataset) — so the second run to resolve the same key produces a pair, and
// `inventory.items[].factor_kg_co2e_per_kg`'s `cross_check_sql` counts the
// pairs that disagree. That is what discharged six exemptions down to five.
//
// ## The hint mechanism, and why it withholds the number
//
// Retrieving a factor is the expensive half of this action, and factors are
// reused across products, so a cache is worth having. But the obvious cache —
// hand the agent the value it found last time — would destroy the check it
// sits beside: the second run would echo the first, the two rows would agree
// by construction, and agreement built from a number we supplied is the `xgd`
// trap. Three numbers we made consistent are evidence of nothing.
//
// So the hint carries the IDENTIFICATION and withholds the VALUE. A previous
// run's material name, dataset, geography, year and URL are passed forward;
// the number is not. The agent is spared the expensive part — working out
// which dataset row applies to "Hibiscus infusion" — and must still read the
// figure itself. Every value in the ledger is therefore independently
// retrieved, which is the property the cross-check rests on.

/// The comparison key, normalised so that "Dried hibiscus  calyces" and
/// "dried hibiscus calyces" are one key rather than two that never meet.
fn factor_key(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// What a previous run established about a BOM line, minus the number.
struct FactorHint {
    material: String,
    dataset: Option<String>,
    geography: String,
    reference_year: i32,
    source_url: Option<String>,
}

/// Identifications from earlier runs, keyed by `item_id`.
///
/// Keyed on `item_id` rather than on the material, and that is forced by
/// ordering: the material name is the agent's output, so it does not exist
/// before the agent runs. The item id does. It is a hint and is labelled as
/// one in the query — the agent is told to search afresh if the line is not
/// the same material — because an id is a workspace-local token and two
/// products may spell the same word differently.
async fn load_factor_hints(
    state: &AppState,
    item_ids: &[String],
) -> std::collections::HashMap<String, FactorHint> {
    let mut out = std::collections::HashMap::new();
    if item_ids.is_empty() {
        return out;
    }
    // Most recent resolution per item_id. `DISTINCT ON` needs the ordering to
    // lead with the partition key.
    let rows = sqlx::query(
        "SELECT DISTINCT ON (item_id)
                item_id, material, dataset, geography, reference_year, source_url
           FROM carbon_emission_factors
          WHERE item_id = ANY($1) AND retrieval = 'search'
          ORDER BY item_id, resolved_at DESC",
    )
    // Owned rather than borrowed: `&[String]` relies on a blanket array
    // encoding that is easy to trip over, and a `Vec<String>` binds as TEXT[]
    // unambiguously. This query is soft-failed into an empty map, so getting
    // it wrong would present as "no hints ever" rather than as an error.
    .bind(item_ids.to_vec())
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for r in rows {
        let Ok(id) = r.try_get::<String, _>("item_id") else {
            continue;
        };
        out.insert(
            id,
            FactorHint {
                material: r.try_get("material").unwrap_or_default(),
                dataset: r.try_get("dataset").ok().flatten(),
                geography: r.try_get("geography").unwrap_or_default(),
                reference_year: r.try_get("reference_year").unwrap_or_default(),
                source_url: r.try_get("source_url").ok().flatten(),
            },
        );
    }
    out
}

/// How much of this run went into the ledger.
struct LedgerOutcome {
    recorded: usize,
    /// Factors that resolved but could not be made comparable.
    incomplete: usize,
}

/// Append every fully-identified factor from the enforced document.
///
/// Incomplete rows are skipped rather than stored with nulls, and the reason
/// is the check rather than tidiness: a factor with no dataset cannot be
/// compared against anything without either being excluded or being allowed to
/// join a key it may not belong to. The first loses nothing; the second
/// invents agreement. They are counted and reported so that "we retrieved six
/// factors and could compare two of them" is visible rather than inferred.
async fn record_factors(
    state: &AppState,
    doc: &Value,
    ws_uuid: Uuid,
    action_id: Uuid,
) -> LedgerOutcome {
    let mut out = LedgerOutcome {
        recorded: 0,
        incomplete: 0,
    };
    let Some(items) = doc.pointer("/inventory/items").and_then(|v| v.as_array()) else {
        return out;
    };

    for it in items {
        let Some(value) = it.get("factor_kg_co2e_per_kg").and_then(|v| v.as_f64()) else {
            continue; // no factor at all — an honest gap, not a ledger row
        };
        if !value.is_finite() || value < 0.0 {
            out.incomplete += 1;
            continue;
        }
        let material = it
            .get("material")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let geography = it
            .get("geography")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let dataset = it
            .get("dataset")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let year = it
            .get("reference_year")
            .and_then(|v| v.as_i64())
            .or_else(|| {
                it.get("reference_year")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.trim().parse::<i64>().ok())
            });

        let (Some(material), Some(geography), Some(dataset), Some(year)) =
            (material, geography, dataset, year)
        else {
            out.incomplete += 1;
            continue;
        };

        let res = sqlx::query(
            "INSERT INTO carbon_emission_factors
                 (material_key, geography, reference_year, dataset_key,
                  value_kg_co2e_per_kg, material, dataset, lca_basis,
                  source_url, source_title, retrieval, agent_name,
                  workspace_id, action_id, item_id)
             VALUES ($1,$2,$3,$4,$5::numeric,$6,$7,$8,$9,$10,'search',$11,$12,$13,$14)",
        )
        .bind(factor_key(material))
        .bind(geography.trim().to_uppercase())
        .bind(year as i32)
        .bind(factor_key(dataset))
        .bind(value)
        .bind(material.trim())
        .bind(dataset.trim())
        .bind(it.get("lca_basis").and_then(|v| v.as_str()))
        .bind(it.get("source_url").and_then(|v| v.as_str()))
        .bind(it.get("source_title").and_then(|v| v.as_str()))
        .bind(ACCOUNTANT)
        .bind(ws_uuid)
        .bind(action_id)
        .bind(it.get("item_id").and_then(|v| v.as_str()))
        .execute(&state.db)
        .await;

        match res {
            Ok(_) => out.recorded += 1,
            // Soft-fail for the same reason the action log does: the statement
            // is the product. A missing migration must not lose work that has
            // already cost real searches. The count in the response is what
            // says the evidence did not accrue.
            Err(_) => out.incomplete += 1,
        }
    }
    out
}

// ─── persistence ─────────────────────────────────────────────────────────────

/// The statement, as the workspace's carbon memory.
///
/// Written by the platform after `enforce`, for the same reason the regulatory
/// evaluations are: persisting the raw reply would persist fields the gate is
/// about to strip, and every later read would serve them as though they had
/// passed.
fn statement_yaml(
    doc: &Value,
    product_id: &str,
    action_id: Uuid,
    audit: &ArithmeticAudit,
) -> String {
    let record = json!({
        "product_id": product_id,
        "inventory": doc.get("inventory").cloned().unwrap_or(Value::Null),
        "attribution": doc.get("attribution").cloned().unwrap_or(Value::Null),
        "boundary": doc.get("boundary").cloned().unwrap_or(Value::Null),
        "assurance": doc.get("assurance").cloned().unwrap_or(Value::Null),
        "explanation": doc.get("explanation").cloned().unwrap_or(Value::Null),
        // One stamp per block, never one for the document. The retrieval and
        // the judgements over it have different strengths and a consumer has
        // to be able to see both.
        "provenance": {
            "inventory": doc.get("inventory_provenance").cloned().unwrap_or(Value::Null),
            "attribution": doc.get("attribution_provenance").cloned().unwrap_or(Value::Null),
            "boundary": doc.get("boundary_provenance").cloned().unwrap_or(Value::Null),
            "assurance": doc.get("assurance_provenance").cloned().unwrap_or(Value::Null),
            "arithmetic": grounding_trust::PROV_DERIVED,
        },
        "model_arithmetic": {
            "lines_the_model_priced": audit.attempted,
            "disagreements": audit.disagreements,
            "worst_relative_error": audit.worst_relative_error,
            "note": "The platform's values are authoritative and are what appear \
                     above. This records how often the model multiplied anyway and \
                     was wrong — a discipline signal the derivation would otherwise \
                     hide by making every stored row correct by construction.",
        },
        "calculated_by": ACCOUNTANT,
        "calculated_at": chrono::Utc::now().to_rfc3339(),
        "action_id": action_id.to_string(),
        // Set by a reviewer through the endorsement path. `null` means nobody
        // has signed this off, which is different from a rejection.
        "endorsement": Value::Null,
    });

    let body = serde_yaml::to_string(&record).unwrap_or_default();
    format!(
        "# Product carbon statement — written by the platform, not by hand.\n\
         #\n\
         # Produced by `{ACCOUNTANT}` via POST /actions/calculate_carbon and\n\
         # persisted AFTER `grounding_trust::enforce` ran over the reply. Any\n\
         # field the agent could not ground has already been nulled.\n\
         #\n\
         # The ARITHMETIC in `inventory` is the platform's, computed in Rust from\n\
         # the bill-of-materials quantity and the retrieved factor. Every\n\
         # `kg_co2e` is reproducible by hand: it is `activity_qty_kg` times\n\
         # `factor_kg_co2e_per_kg`, and `arithmetic` spells it out.\n\
         #\n\
         # The FACTORS are retrieved and cited but NOT independently verified —\n\
         # the platform holds no copy of ecoinvent, Agribalyse or the DEFRA\n\
         # factors. See CROSS_CHECK_EXEMPTIONS in src/grounding_trust.rs for\n\
         # what would close that, and read `provenance.inventory` for whether a\n\
         # tool answered at all.\n\
         #\n\
         # `inventory.coverage: partial` means the total is a SUBSET of the\n\
         # product's footprint. `unpriced_items` names what is missing. A subset\n\
         # read as a footprint is the error this file is laid out to prevent.\n\
         #\n\
         # Edit `endorsement` to record a qualified reviewer's sign-off.\n\
         \n{body}"
    )
}

/// Replace the `carbon_intensity:` block in `composition.yaml`, preserving
/// every other line — including the comments, which are most of the file's
/// value.
///
/// A round-trip through `serde_yaml` would be shorter and would delete all of
/// them, and the argument of this whole App is that an operator can tell a
/// retrieved number from a typed one. That argument is carried by the prose in
/// that file as much as by the data.
///
/// Returns `None` when there is no top-level `carbon_intensity:` key, which is
/// a real state (a composition that never carried the fixture) and not an
/// error.
fn rewrite_carbon_intensity(yaml: &str, replacement: &str) -> Option<String> {
    let lines: Vec<&str> = yaml.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("carbon_intensity:"))?;
    // Consume the block: its indented continuation lines, and any blank lines
    // inside it. Stops at the next line that begins in column zero.
    let mut end = start + 1;
    while end < lines.len() {
        let l = lines[end];
        if l.trim().is_empty() || l.starts_with(' ') || l.starts_with('\t') {
            end += 1;
        } else {
            break;
        }
    }
    // Trailing blank lines belong to the gap between blocks, not to the block.
    let mut body_end = end;
    while body_end > start + 1 && lines[body_end - 1].trim().is_empty() {
        body_end -= 1;
    }

    let mut out: Vec<String> = lines[..start].iter().map(|s| s.to_string()).collect();
    out.extend(replacement.lines().map(|s| s.to_string()));
    out.extend(lines[body_end..].iter().map(|s| s.to_string()));
    let mut joined = out.join("\n");
    if yaml.ends_with('\n') {
        joined.push('\n');
    }
    Some(joined)
}

/// The replacement block, with `mode` now meaning something.
///
/// `mode: synthetic` was a person typing `0.41`. The vocabulary is widened
/// rather than the block deleted, because an operator has to be able to tell a
/// retrieved number from a typed one and from a supplier's declaration — which
/// is the whole argument of this App. A calculated mode therefore carries a
/// pointer to the statement and the coverage of the figure, because an
/// intensity without its coverage is a subset wearing a total's clothes.
fn carbon_intensity_block(doc: &Value, product_mass_kg: Option<f64>) -> String {
    let total = doc
        .pointer("/inventory/total_kg_co2e")
        .and_then(|v| v.as_f64());
    let coverage = doc
        .pointer("/inventory/coverage")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let intensity = match (total, product_mass_kg) {
        (Some(t), Some(m)) if m > 0.0 => Some(((t / m) * 1_000_000.0).round() / 1_000_000.0),
        _ => None,
    };
    let dominant = doc
        .pointer("/inventory/items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some((
                        i.get("item_id")?.as_str()?.to_string(),
                        i.get("kg_co2e")?.as_f64()?,
                    ))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(id, _)| id)
        });

    let mut s = String::from(
        "carbon_intensity:\n\
         \x20 # mode vocabulary:\n\
         \x20 #   synthetic           a person typed the number. Not evidence.\n\
         \x20 #   agent_calculated    retrieved factors x BOM quantities, the\n\
         \x20 #                       arithmetic performed by the platform. See\n\
         \x20 #                       statement_ref for the factors and their URLs.\n\
         \x20 #   supplier_declared   a supplier's own figure, ideally an EPD.\n\
         \x20 mode: agent_calculated\n",
    );
    match intensity {
        Some(v) => s.push_str(&format!("  value_kg_per_kg: {v}\n")),
        None => s.push_str(
            "  value_kg_per_kg: null   # no line resolved, or no serving mass to \
             divide by. Null, not zero.\n",
        ),
    }
    s.push_str(&format!("  coverage: {coverage}\n"));
    if coverage != "complete" {
        s.push_str(
            "  # coverage is not complete: this intensity is a SUBSET of the \
             product's\n  # footprint. statement_ref names which lines are missing.\n",
        );
    }
    match dominant {
        Some(id) => s.push_str(&format!("  scope_3_dominant: {id}\n")),
        None => s.push_str("  scope_3_dominant: null\n"),
    }
    s.push_str(&format!("  statement_ref: {STATEMENT_PATH}\n"));
    s
}

// ─── handler ─────────────────────────────────────────────────────────────────

pub async fn calculate_carbon_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<CalculateCarbonRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let request_started = std::time::Instant::now();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let boundary = req
        .boundary
        .as_deref()
        .unwrap_or("cradle_to_gate")
        .to_string();
    let region = req.region.as_deref().unwrap_or("EU").to_string();

    // ── Preflight: refuse rather than degrade ────────────────────────────
    //
    // Three things must be true before spending anything, and each refusal is
    // cheaper than the document it prevents.
    let db_agent = crate::resolve_agent(&state, ACCOUNTANT)
        .await
        .map_err(|(_code, msg)| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "Carbon accounting is unavailable: `{ACCOUNTANT}` is not \
                     installed on this platform ({msg}). Its agent card must \
                     load and be seeded before it can be run."
                ),
            )
        })?;

    // `web_search` reads its key at call time and, when absent, returns the
    // words "BRAVE_SEARCH_API_KEY environment variable not set" as its tool
    // RESULT rather than raising. The model is handed that string and does the
    // only thing it can: answers from training data. The run SUCCEEDS, every
    // factor arrives uncited, the block is stamped `tool_no_match` — "the
    // datasets were searched and had nothing" — when they were never reached.
    //
    // A missing key is an operator problem with a one-line fix. A workspace of
    // carbon statements that quietly came from model memory is not fixable at
    // all, because nothing separates them from the real ones. So: refuse.
    if super::claim_evaluation::resolve_search_credential(&state, &db_agent)
        .await
        .is_none()
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            format!(
                "Carbon accounting is unavailable: no `brave_search` credential \
                 is reachable, so web_search cannot query the factor databases. \
                 Running anyway would produce emission factors from the model's \
                 training data, and a plausible factor is indistinguishable from \
                 a retrieved one to anyone without a dataset licence.\n\n\
                 Put the key in the credential store as (principal `{}`, \
                 provider `brave_search`), which is where \
                 docs/specs/AGENT_CREDENTIAL_MODEL.md §2 requires it to live. \
                 Setting BRAVE_SEARCH_API_KEY in the environment also works and \
                 seeds the store once at startup, but it is a bootstrap seed \
                 rather than the source of truth.",
                crate::funding_principal_for(&db_agent).unwrap_or_else(|| "abw-system".to_string()),
            ),
        ));
    }

    // ── The bill of materials. Refuse rather than cost a fixture. ─────────
    let raw = super::claim_evaluation::read_doc(
        &state,
        &slug,
        "dpp/composition.yaml",
        "apps/adaptogen-lab/dpp/composition.yaml",
    )
    .await
    .ok_or((
        StatusCode::NOT_FOUND,
        "No dpp/composition.yaml in this workspace. A carbon figure is a \
         retrieved factor times a bill-of-materials quantity; with no BOM there \
         is nothing to multiply, and a run would cost real searches to produce \
         a document about a product nobody described."
            .to_string(),
    ))?;
    let composition: Value = serde_yaml::from_slice(&raw).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("dpp/composition.yaml did not parse as YAML: {e}"),
        )
    })?;

    let (lines, basis_ml) = read_bom(&composition);
    if lines.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "dpp/composition.yaml carries no `consists_of` entries. An empty BOM \
             priced silently is how the hardcoded fixture went unnoticed for as \
             long as it did."
                .to_string(),
        ));
    }
    let product_id = composition
        .get("product_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown_product")
        .to_string();

    if !req.force {
        let git = state.workspace_git.clone();
        let slug_s = slug.clone();
        let exists = tokio::task::spawn_blocking(move || {
            git.read_file_bytes(&slug_s, STATEMENT_PATH).is_ok()
        })
        .await
        .unwrap_or(false);
        if exists {
            return Ok(Json(json!({
                "action_type": "calculate_carbon",
                "skipped": true,
                "reason": "already_calculated",
                "statement_path": STATEMENT_PATH,
                "duration_ms": request_started.elapsed().as_millis() as u64,
                "note": "A statement already exists. Emission factors are \
                         expensive to retrieve and do not change between runs, \
                         so this is a cache read by default. Pass `force: true` \
                         to recalculate.",
            })));
        }
    }

    // ── Log the action first, so the persisted statement can reference it ──
    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());
    let action_id = super::actions::log_action(
        &state,
        ws_uuid,
        "calculate_carbon",
        "user",
        &user_id,
        Some("adaptogen_lab_regulatory"),
        &json!({
            "product_id": product_id,
            "boundary": boundary,
            "region": region,
            "bom_lines": lines.len(),
            "priceable_lines": lines.iter().filter(|l| l.qty_kg.is_some()).count(),
            "force": req.force,
            "accountant": ACCOUNTANT,
        }),
        "auto",
        source_msg_id,
    )
    .await
    // Soft-fail, matching the evaluator: the statement is the product and the
    // log row is auditing infrastructure. A missing migration must not lose
    // work that has already cost real searches and tokens. The consequence is
    // named in the response, because an `action_id` in no table is an audit
    // anchor pointing at nothing.
    .map_err(|_| ())
    .unwrap_or_else(|_| Uuid::new_v4());

    // ── Run the agent ──────────────────────────────────────────────────────
    //
    // What earlier runs identified, minus the numbers they found. See the
    // ledger section for why the value is withheld: handing it back would make
    // the cross-check compare a number against a copy of itself.
    let hints = load_factor_hints(
        &state,
        &lines.iter().map(|l| l.item_id.clone()).collect::<Vec<_>>(),
    )
    .await;
    let query = build_query(&lines, basis_ml, &composition, &boundary, &region, &hints);
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(RUN_TIMEOUT_SECS),
        crate::handlers::rabble_workspace::dispatch_rabble_action(
            &state,
            ws_uuid,
            ACCOUNTANT,
            DISPATCH_MESSAGE_TYPE,
            &query,
            &user_id,
            None,
        ),
    )
    .await
    .map_err(|_| {
        (
            StatusCode::GATEWAY_TIMEOUT,
            format!("Carbon calculation timed out after {RUN_TIMEOUT_SECS}s."),
        )
    })?
    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;

    // Parse with the same scanner the delegation hop uses rather than a second
    // copy of it. An unreadable reply becomes a document that says nothing,
    // never one that says everything is fine — note that the fallback carries
    // no `inventory` items at all, so the total is null and coverage is `none`
    // rather than a zero that would read as a measured footprint.
    // An unreadable reply becomes a document that says nothing, never one that
    // says everything is fine — note the fallback carries no `inventory` items
    // at all, so the total is null and coverage is `none` rather than a zero
    // that would read as a measured footprint.
    //
    // The diagnostic is kept OUT of the document and returned beside it. The
    // schema declares `additionalProperties: false`, so a `parse_failure` key
    // inside the statement would make every parse failure also a schema
    // violation — two unrelated faults reported as one, and the second of them
    // caused by the error handling rather than by the agent. It is also the
    // wrong place on its own terms: a client branching on "did this parse"
    // should not have to read the document to find out.
    let mut parse_failure: Option<String> = None;
    let mut doc = fermi::agent_backend::envelope::extract_json(&reply).unwrap_or_else(|| {
        parse_failure = Some(format!(
            "The accountant replied but the reply could not be read as a \
             document, so no factor was recorded. First 400 characters: {}",
            reply.chars().take(400).collect::<String>()
        ));
        json!({ "explanation": Value::Null })
    });
    if !doc.is_object() {
        parse_failure = Some("The reply parsed to a non-object.".to_string());
        doc = json!({ "explanation": Value::Null });
    }

    // The platform's half: the BOM is authoritative, the arithmetic is ours,
    // and the model's own attempt at it is measured before it is discarded.
    let audit = normalise_reply(&mut doc, &lines);

    // The gate. Nulls ungrounded fields, performs the registered derivations,
    // stamps every block, and scans the prose for a dataset nothing came back
    // from.
    let report = grounding_trust::enforce(ACCOUNTANT, &mut doc);
    if !report.is_clean() {
        grounding_anomaly::spawn_raise(
            Arc::clone(&state.memory_store),
            ACCOUNTANT,
            None,
            report.clone(),
        );
    }

    // Append the retrieved factors to the ledger, from the ENFORCED document
    // rather than the reply: a factor the gate stripped is not evidence, and
    // recording it would put a value into the comparison base that the
    // statement itself does not stand behind.
    let ledger = record_factors(&state, &doc, ws_uuid, action_id).await;

    // ── Persist ────────────────────────────────────────────────────────────
    let mut files: Vec<(String, String)> = vec![(
        STATEMENT_PATH.to_string(),
        statement_yaml(&doc, &product_id, action_id, &audit),
    )];

    // And retire the fixture. 1 ml taken as 1 g again, consistently with the
    // quantity conversion, so the intensity denominator matches the numerator.
    let product_mass_kg = basis_ml.map(|ml| ml / 1000.0);
    let composition_updated = match String::from_utf8(raw.clone()).ok().and_then(|text| {
        rewrite_carbon_intensity(&text, &carbon_intensity_block(&doc, product_mass_kg))
    }) {
        Some(updated) => {
            files.push(("dpp/composition.yaml".to_string(), updated));
            true
        }
        None => false,
    };

    let git = state.workspace_git.clone();
    let slug_w = slug.clone();
    let commit_files = files.clone();
    let msg = format!("{ACCOUNTANT}: carbon statement for {product_id}");
    let commit = tokio::task::spawn_blocking(move || {
        git.commit_files_as(&slug_w, &commit_files, &msg, None)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let written: Vec<String> = match commit {
        Ok(_) => files.iter().map(|(p, _)| p.clone()).collect(),
        Err(_) => Vec::new(),
    };

    // Persist what the run cost onto the action row that is already its audit
    // anchor. Soft-fail, like the insert.
    let outcome = json!({
        "duration_ms": request_started.elapsed().as_millis() as u64,
        "coverage": doc.pointer("/inventory/coverage"),
        "total_kg_co2e": doc.pointer("/inventory/total_kg_co2e"),
        "unpriced_items": doc.pointer("/inventory/unpriced_items"),
        "inventory_provenance": doc.get("inventory_provenance"),
        "model_arithmetic_disagreements": audit.disagreements,
        "violations": report.violations.len(),
        "written_paths": written.len(),
        "factors_recorded": ledger.recorded,
        "factors_not_comparable": ledger.incomplete,
        "hints_offered": hints.len(),
    });
    let _ = sqlx::query(
        "UPDATE workspace_action_log
            SET apply_result = $1, applied = TRUE, applied_at = NOW()
          WHERE action_id = $2",
    )
    .bind(&outcome)
    .bind(action_id)
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "calculate_carbon",
        "product_id": product_id,
        "statement": doc,
        // Null on a normal run. Non-null means the reply was unreadable and the
        // statement below is the empty one, not a footprint of zero.
        "parse_failure": parse_failure,
        "statement_path": STATEMENT_PATH,
        "written_paths": written,
        "composition_updated": composition_updated,
        "boundary_requested": boundary,
        "region": region,
        "grounding_summary": {
            "is_clean": report.is_clean(),
            "violation_count": report.violations.len(),
            "provenance": report.provenance.iter()
                .map(|(b, v)| json!({ "block": b, "verdict": v }))
                .collect::<Vec<_>>(),
        },
        // The arithmetic above is the platform's. This says how often the model
        // did it anyway and was wrong — reported rather than hidden, because a
        // derivation makes every stored row correct by construction and would
        // otherwise conceal exactly the behaviour the design forbids.
        "model_arithmetic": {
            "lines_the_model_priced": audit.attempted,
            "disagreements": audit.disagreements,
            "worst_relative_error": audit.worst_relative_error,
        },
        // What this run contributed to the evidence base that makes a factor
        // falsifiable at all. `recorded` are factors carrying all four key
        // fields, so a later run resolving the same one can be compared against
        // them; `not_comparable` retrieved a number without enough
        // identification to place it, which is usable in this statement and
        // invisible to the cross-check. `hints_offered` is how many lines this
        // run was spared searching from scratch.
        "factor_ledger": {
            "recorded": ledger.recorded,
            "not_comparable": ledger.incomplete,
            "hints_offered": hints.len(),
            "note": "Appended to carbon_emission_factors. Two runs resolving the \
                     same (material, geography, reference_year, dataset) are two \
                     readings of one published figure and must agree — that \
                     comparison is this agent's only cross-check, and it is why \
                     the hint above withholds the value it already knows.",
        },
        "duration_ms": request_started.elapsed().as_millis() as u64,
        // Credits are charged by `dispatch_rabble_action` in a background task
        // AFTER this response is built, so no cost can honestly be reported
        // here. Quoting an estimate beside a real duration would read as though
        // both were measured.
        "cost": {
            "credits_charged": Value::Null,
            "where": "GET /api/workspaces/{workspace_id}/budget",
            "note": "Charged asynchronously after this response. The ledger \
                     entry carries the agent, action and token count.",
        },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPOSITION: &str = r#"# a comment that must survive
item_type: bom:Item
product_id: precision_kombucha_hibiscus_f2
display_name: "Precision Kombucha — Hibiscus, Cold F2"

serving:
  volume_ml: 330
  servings_per_unit: 1

consists_of:

  - item_id: hibiscus_infusion
    display_name: "Hibiscus infusion"
    role: consumable
    quantity: "8.5%"
    origin: "Egypt / Sudan"

  - item_id: starter_scoby
    display_name: "SCOBY starter culture"
    role: catalyst
    quantity: "trace"

carbon_intensity:
  mode: synthetic
  value_kg_per_kg: 0.41
  scope_3_dominant: hibiscus_infusion

allergens:
  present: []
"#;

    /// The conversion the browser already does, done again server-side against
    /// the committed document — and it has to land on the same numbers, or the
    /// pricing query and the carbon statement describe different products.
    #[test]
    fn percentages_resolve_against_the_serving_and_trace_does_not() {
        let comp: Value = serde_yaml::from_str(COMPOSITION).unwrap();
        let (lines, basis) = read_bom(&comp);
        assert_eq!(basis, Some(330.0));
        assert_eq!(lines.len(), 2);

        // 8.5% of 330 ml = 28.05 g = 0.02805 kg. The same 28.05 g that
        // `buildBomItems()` sends to the pricing oracle.
        let h = &lines[0];
        assert_eq!(h.item_id, "hibiscus_infusion");
        assert!((h.qty_kg.unwrap() - 0.02805).abs() < 1e-12);
        assert!(
            h.basis.as_deref().unwrap().contains("1 ml taken as 1 g"),
            "the conversion assumption must travel with the number: an \
             unstated one is a figure per kilogram that silently means \
             something else"
        );

        // `trace` is a real answer and not a parse failure. The line still
        // travels so the agent can identify and cache a factor for it; it
        // simply cannot be priced, and `unpriced_items` will name it.
        let s = &lines[1];
        assert_eq!(s.qty_kg, None);
        assert_eq!(s.quantity_declared, "trace");
    }

    #[test]
    fn a_percentage_with_no_serving_basis_yields_no_quantity() {
        // Not a guess at a default serving size. A basis nobody stated is the
        // assumption that makes a footprint mean something other than it says.
        assert_eq!(resolve_qty_kg("8.5%", None), (None, None));
        assert_eq!(resolve_qty_kg("", Some(330.0)), (None, None));
        assert_eq!(resolve_qty_kg("a splash", Some(330.0)), (None, None));
        assert_eq!(resolve_qty_kg("1.2 kg", None).0, Some(1.2));
        assert_eq!(resolve_qty_kg("250 g", None).0, Some(0.25));
    }

    /// **The BOM decides the line set, not the reply.**
    ///
    /// Without this a statement could omit an ingredient and still derive
    /// `coverage: complete`, which is the subset-as-footprint failure the
    /// coverage field exists to prevent — arriving through the one door that
    /// bypasses it.
    #[test]
    fn the_bom_decides_the_lines_and_the_platform_owns_the_quantity() {
        let comp: Value = serde_yaml::from_str(COMPOSITION).unwrap();
        let (lines, _) = read_bom(&comp);

        let mut doc = json!({
            "inventory": {
                "items": [
                    // Right id, wrong quantity — off by a factor of ten, which
                    // would multiply straight into the total.
                    { "item_id": "hibiscus_infusion", "activity_qty_kg": 0.2805,
                      "factor_kg_co2e_per_kg": 2.1, "kg_co2e": 0.589 },
                    // A line the BOM does not contain.
                    { "item_id": "invented_ingredient", "activity_qty_kg": 1.0,
                      "factor_kg_co2e_per_kg": 9.9 }
                ]
            }
        });
        let audit = normalise_reply(&mut doc, &lines);

        let items = doc.pointer("/inventory/items").unwrap().as_array().unwrap();
        assert_eq!(
            items.len(),
            2,
            "the BOM has two lines, so the document has two"
        );
        let ids: Vec<&str> = items
            .iter()
            .map(|i| i["item_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, vec!["hibiscus_infusion", "starter_scoby"]);
        assert!(
            (items[0]["activity_qty_kg"].as_f64().unwrap() - 0.02805).abs() < 1e-12,
            "the quantity is the BOM's; a quantity the model restated is a \
             quantity nobody wrote"
        );
        // The omitted line is present and unpriceable, so it will be named in
        // `unpriced_items` rather than vanishing from the denominator.
        assert!(items[1]["activity_qty_kg"].is_null());

        // Measured before discarded: 0.589 against 0.02805 x 2.1 = 0.0589.
        assert_eq!(audit.attempted, 1);
        assert_eq!(audit.disagreements, 1);
        assert!(audit.worst_relative_error.unwrap() > 8.0);

        // And the derivations now have somewhere to write.
        for k in ["total_kg_co2e", "coverage", "unpriced_items"] {
            assert!(doc.pointer(&format!("/inventory/{k}")).is_some());
        }
    }

    /// End to end over the real gate: normalise, enforce, and check the
    /// statement is reproducible by hand from a cited factor and a BOM
    /// quantity — which is acceptance criterion 1 for this work.
    #[test]
    fn the_statement_is_reproducible_by_hand() {
        let comp: Value = serde_yaml::from_str(COMPOSITION).unwrap();
        let (lines, basis) = read_bom(&comp);
        let mut doc = json!({
            "inventory": {
                "queries_run": ["ecoinvent dried hibiscus Egypt"],
                "items": [{
                    "item_id": "hibiscus_infusion",
                    "factor_kg_co2e_per_kg": 2.1,
                    "lca_basis": "cradle_to_gate",
                    "geography": "EG",
                    "reference_year": 2021,
                    "dataset": "ecoinvent 3.9.1",
                    "source_url": "https://example.invalid/hibiscus"
                }]
            },
            "attribution": { "items": [] },
            "boundary": { "declared": "cradle_to_gate", "excluded": ["use"] },
            "assurance": { "needs_expert": true, "verification_status": "unverified" },
            "explanation": "One line resolved; the starter culture is a named gap."
        });
        normalise_reply(&mut doc, &lines);
        grounding_trust::enforce(ACCOUNTANT, &mut doc);

        let line = &doc.pointer("/inventory/items/0").unwrap();
        let qty = line["activity_qty_kg"].as_f64().unwrap();
        let factor = line["factor_kg_co2e_per_kg"].as_f64().unwrap();
        let kg = line["kg_co2e"].as_f64().unwrap();
        assert!(
            (kg - qty * factor).abs() < 1e-9,
            "every kg_co2e must be reproducible by hand from the cited factor \
             and the BOM quantity: {qty} x {factor} != {kg}"
        );
        assert_eq!(
            doc.pointer("/inventory/coverage").and_then(|v| v.as_str()),
            Some("partial")
        );
        assert_eq!(
            doc.pointer("/inventory/unpriced_items"),
            Some(&json!(["starter_scoby"]))
        );

        // And the composition block that replaces the fixture reports the same
        // figure with its coverage attached.
        let block = carbon_intensity_block(&doc, basis.map(|ml| ml / 1000.0));
        assert!(block.contains("mode: agent_calculated"));
        assert!(block.contains("coverage: partial"));
        assert!(
            block.contains("SUBSET"),
            "an intensity whose coverage is partial must say so where it is \
             read, not only in the statement: {block}"
        );
        assert!(block.contains(STATEMENT_PATH));
    }

    /// The fixture is replaced in place, and every comment in the file
    /// survives — which is the reason this is a line rewrite and not a
    /// `serde_yaml` round-trip.
    #[test]
    fn rewriting_the_fixture_preserves_the_rest_of_the_document() {
        let out =
            rewrite_carbon_intensity(COMPOSITION, "carbon_intensity:\n  mode: agent_calculated\n")
                .expect("the block is there");
        assert!(!out.contains("mode: synthetic"), "the fixture must be gone");
        assert!(out.contains("mode: agent_calculated"));
        assert!(out.contains("# a comment that must survive"));
        assert!(out.contains("allergens:"), "the following block survives");
        assert!(out.contains("volume_ml: 330"));
        assert!(
            !out.contains("value_kg_per_kg: 0.41"),
            "the typed number is what this whole endpoint exists to retire"
        );
        // Still parses, which a naive line splice is quite capable of breaking.
        let parsed: Value = serde_yaml::from_str(&out).expect("still valid YAML");
        assert_eq!(
            parsed
                .pointer("/carbon_intensity/mode")
                .and_then(|v| v.as_str()),
            Some("agent_calculated")
        );
        assert_eq!(
            parsed.pointer("/allergens/present"),
            Some(&json!([])),
            "the block after the rewrite must still be reachable"
        );

        // A composition that never carried the fixture is a real state, not an
        // error: nothing to replace, and the statement is still written.
        assert!(rewrite_carbon_intensity("product_id: x\n", "carbon_intensity:\n").is_none());
    }
}
