//! BOM pricing — the supply-chain half of the DPP Studio.
//!
//!   POST /api/workspaces/:id/actions/price_bom
//!
//! ## Why this exists rather than the message path
//!
//! Pricing already worked, after a fashion: the browser posted an
//! `agent_invocation` message naming `supply_chain_oracle` and polled the
//! workspace messages for a reply. Four things follow from that shape, and
//! all four were felt as the feature being second-class:
//!
//!   1. **It needs a hire.** The message path joins `workspace_agents`
//!      (`messages.rs:318`) and posts a *system message* — not an HTTP error —
//!      when the agent is absent. So a mis-scoped hire surfaced as a run that
//!      quietly did nothing.
//!   2. **Nothing structured reaches the caller.** The reply is prose or JSON
//!      in a message body, matched by sender id and parsed client-side.
//!   3. **No action-log row.** So the run could not appear in the Activity
//!      panel after a reload, and the credits it spent landed in the ledger
//!      with nothing to attribute them to.
//!   4. **No grounding enforcement reaches the client.** `Pulse::grade` runs
//!      server-side and its verdict goes to the episode, never to the caller,
//!      so the prices rendered carried no provenance.
//!
//! `evaluate_claims` solved the same four for claims. This is that pattern,
//! applied to the BOM: `dispatch_rabble_action` (no hire), enforce, persist,
//! log.
//!
//! ## The conversion lives here now
//!
//! `scro/bom-query/1` wants a numeric `qty` and a `unit`. A DPP composition
//! states percentages of the formulation. The browser was doing that
//! conversion, which made it the only place that knew the basis — and a
//! second implementation would have to agree with it forever. The server owns
//! it, and states the assumption in the query so the agent prices the thing
//! the label describes.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use fermi_auth::AuthPrincipal;

use super::actions::resolve_workspace;
use super::claim_evaluation::{read_doc, resolve_search_credential};
use crate::{grounding_trust, AppState};
use fermi::grounding_anomaly;

const ORACLE: &str = "supply_chain_oracle";

/// One `web_search` per BOM line, inside a five-iteration tool loop. Six lines
/// is an ordinary product, so this is deliberately generous — the failure that
/// prompted this module was a 40-second client budget reporting impatience as
/// agent failure.
const PRICING_TIMEOUT_SECS: u64 = 300;

#[derive(Deserialize)]
pub struct PriceBomRequest {
    /// ISO 4217. Defaults to EUR, matching the oracle's own default.
    pub currency: Option<String>,
    /// `small_batch` | `pilot` | `industrial`. Absence is treated as
    /// `small_batch` by the oracle.
    pub production_scale: Option<String>,
    pub source_message_id: Option<String>,
}

// ─── the BOM → query conversion ──────────────────────────────────────────────

/// One priceable line, plus what we could not resolve about it.
struct BomLine {
    value: Value,
    /// `Some(reason)` when no numeric quantity could be derived.
    unresolved: Option<String>,
}

/// Parse a declared quantity into `(qty, unit)` against a serving basis.
///
/// Two forms appear in practice and a third must not be invented:
///   `"8.5%"`   — a share of the formulation, needs `basis_ml`
///   `"12 g"`   — already absolute
///   `"trace"`  — a real value in a real BOM, and not a number
///
/// The third returns `None`. A `trace` line still goes to the agent, because
/// the material is identifiable and sourceable; what it must not acquire is a
/// quantity nobody wrote.
fn parse_quantity(raw: &str, basis_ml: Option<f64>) -> Option<(f64, &'static str, Option<String>)> {
    let t = raw.trim();
    if let Some(pct) = t.strip_suffix('%') {
        let v: f64 = pct.trim().parse().ok()?;
        let basis = basis_ml?;
        // w/v against the serving basis, 1 ml treated as 1 g for an aqueous
        // beverage. Returned as a note rather than assumed silently.
        let grams = v / 100.0 * basis;
        return Some((
            (grams * 10_000.0).round() / 10_000.0,
            "g",
            Some(format!("{v}% w/v of a {basis} ml serving")),
        ));
    }
    // `12 g`, `0.5kg`, `330 ml`
    let split = t
        .find(|c: char| c.is_alphabetic())
        .filter(|i| *i > 0)?;
    let (num, unit) = t.split_at(split);
    let v: f64 = num.trim().parse().ok()?;
    let unit = match unit.trim().to_ascii_lowercase().as_str() {
        "kg" => "kg",
        "g" => "g",
        "mg" => "mg",
        "l" => "l",
        "ml" => "ml",
        "unit" | "units" | "pcs" => "unit",
        _ => return None,
    };
    Some((v, unit, None))
}

/// Build the `bom_items` array from the composition document.
fn build_lines(composition: &Value, basis_ml: Option<f64>) -> Vec<BomLine> {
    let items = composition
        .get("consists_of")
        .or_else(|| composition.get("ingredients"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    items
        .iter()
        .filter_map(|it| {
            let name = it
                .get("display_name")
                .or_else(|| it.get("name"))
                .and_then(|v| v.as_str())
                .or_else(|| it.get("item_id").and_then(|v| v.as_str()))?
                .trim()
                .to_string();
            if name.is_empty() {
                return None;
            }
            let declared = it
                .get("quantity")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();

            let role = it
                .get("role")
                .or_else(|| it.get("category"))
                .and_then(|v| v.as_str())
                .unwrap_or("unspecified");

            let mut line = json!({
                "name": name,
                "role": role,
                "quantity_declared": if declared.is_empty() { Value::Null } else { json!(declared) },
            });
            if let Some(o) = it.get("origin").and_then(|v| v.as_str()) {
                line["origin"] = json!(o);
            }
            if let Some(pn) = it.get("part_number").and_then(|v| v.as_str()) {
                line["part_number"] = json!(pn);
            }
            if let Some(id) = it.get("item_id").and_then(|v| v.as_str()) {
                line["item_id"] = json!(id);
            }

            let unresolved = match parse_quantity(&declared, basis_ml) {
                Some((qty, unit, basis_note)) => {
                    line["qty"] = json!(qty);
                    line["unit"] = json!(unit);
                    if let Some(b) = basis_note {
                        line["basis"] = json!(b);
                    }
                    None
                }
                None => {
                    // Explicit nulls rather than absent keys: the oracle must
                    // be able to tell "no quantity was stated" from "the
                    // field did not travel".
                    line["qty"] = Value::Null;
                    line["unit"] = Value::Null;
                    Some(format!(
                        "{name}: {}",
                        if declared.is_empty() {
                            "no quantity stated".to_string()
                        } else {
                            declared.clone()
                        }
                    ))
                }
            };
            Some(BomLine {
                value: line,
                unresolved,
            })
        })
        .collect()
}

fn serving_basis_ml(composition: &Value) -> Option<f64> {
    composition
        .get("serving")
        .and_then(|s| s.get("volume_ml"))
        .and_then(|v| v.as_f64())
}

// ─── handler ─────────────────────────────────────────────────────────────────

pub async fn price_bom_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<PriceBomRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let started = std::time::Instant::now();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    // The oracle must exist, and the corpus must be reachable. Same two
    // preflights as the claim evaluator, for the same reason: `items` on this
    // agent's contract is `sourced` from `web_search`, so without a search
    // credential the prices would come from the model's memory and be stamped
    // `tool_no_match` — "searched, found nothing" — which is indistinguishable
    // from a real search that came up empty.
    let db_agent = crate::resolve_agent(&state, ORACLE)
        .await
        .map_err(|(_c, msg)| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("BOM pricing is unavailable: `{ORACLE}` is not installed ({msg})."),
            )
        })?;

    if resolve_search_credential(&state, &db_agent).await.is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "BOM pricing is unavailable: no `brave_search` credential is reachable, \
             so web_search cannot look up market prices. Running anyway would \
             return prices from the model's training data stamped `tool_no_match`, \
             which is indistinguishable from a search that found nothing. See \
             docs/specs/AGENT_CREDENTIAL_MODEL.md §2."
                .to_string(),
        ));
    }

    // The BOM the user actually wrote.
    let comp_bytes = read_doc(
        &state,
        &slug,
        "dpp/composition.yaml",
        "apps/adaptogen-lab/dpp/composition.yaml",
    )
    .await
    .ok_or((
        StatusCode::NOT_FOUND,
        "No composition document found. Write a bill of materials to \
         dpp/composition.yaml first."
            .to_string(),
    ))?;

    let composition: Value = serde_yaml::from_slice(&comp_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("dpp/composition.yaml parse error: {e}"),
        )
    })?;

    let basis_ml = serving_basis_ml(&composition);
    let lines = build_lines(&composition, basis_ml);

    // Refuse rather than price nothing. An empty BOM priced silently is how
    // the hardcoded fixture this replaced survived for as long as it did.
    if lines.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "The composition has no `consists_of` entries to price.".to_string(),
        ));
    }

    let unresolved: Vec<String> = lines.iter().filter_map(|l| l.unresolved.clone()).collect();
    let bom_items: Vec<Value> = lines.iter().map(|l| l.value.clone()).collect();
    let currency = req.currency.as_deref().unwrap_or("EUR").to_string();
    let scale = req
        .production_scale
        .as_deref()
        .unwrap_or("small_batch")
        .to_string();

    let product_name = composition
        .get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unnamed product")
        .to_string();

    let mut notes: Vec<String> = Vec::new();
    if basis_ml.is_none() {
        notes.push(
            "No serving.volume_ml in the composition, so percentage quantities \
             could not be converted to mass."
                .to_string(),
        );
    }
    if !unresolved.is_empty() {
        notes.push(format!(
            "Quantities not resolvable to a number: {}.",
            unresolved.join("; ")
        ));
    }

    let query_doc = json!({
        "task": "resolve_bom",
        "process_context": {
            "process_name": product_name,
            "production_scale": scale,
            "basis": match basis_ml {
                Some(b) => format!(
                    "one {b} ml serving; percentages read as w/v, 1 ml treated as 1 g"
                ),
                None => "unstated — no serving volume in the composition".to_string(),
            },
            "source_document": "dpp/composition.yaml",
            "notes": if notes.is_empty() { Value::Null } else { json!(notes.join(" ")) },
        },
        "bom_items": bom_items,
        "currency": currency,
    });

    // Log before running, so the stored output can reference the action.
    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());
    let action_id = super::actions::log_action(
        &state,
        ws_uuid,
        "price_bom",
        "user",
        &user_id,
        Some("adaptogen_lab_regulatory"),
        &json!({
            "lines": bom_items.len(),
            "currency": currency,
            "production_scale": scale,
            "basis_ml": basis_ml,
            "unresolved": unresolved.len(),
            "agent": ORACLE,
        }),
        "auto",
        source_msg_id,
    )
    .await
    .unwrap_or_else(|_| Uuid::new_v4());

    let query = serde_json::to_string(&query_doc).unwrap_or_default();
    let reply = tokio::time::timeout(
        std::time::Duration::from_secs(PRICING_TIMEOUT_SECS),
        crate::handlers::rabble_workspace::dispatch_rabble_action(
            &state,
            ws_uuid,
            ORACLE,
            "resolve_bom",
            &query,
            &user_id,
            None,
        ),
    )
    .await
    .map_err(|_| {
        (
            StatusCode::GATEWAY_TIMEOUT,
            format!("BOM pricing timed out after {PRICING_TIMEOUT_SECS}s."),
        )
    })?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let mut doc = fermi::agent_backend::envelope::extract_json(&reply).unwrap_or_else(|| {
        json!({
            "items": [],
            "risks": [],
            "summary": format!(
                "The oracle replied but the reply could not be read as a document, \
                 so nothing was priced. First 400 characters: {}",
                reply.chars().take(400).collect::<String>()
            ),
        })
    });
    if !doc.is_object() {
        doc = json!({
            "items": [], "risks": [],
            "summary": "The oracle's reply parsed to a non-object, so nothing was priced.",
        });
    }

    // Enforce against the card's compiled contract.
    //
    // `supply_chain_oracle` has no `FIELD_CONTRACTS` entry — its typing lives
    // entirely in the grounding map compiled onto its card, which declares
    // `items` as `sourced` from `web_search`. So `enforce` alone would find
    // nothing and return a clean report over unstamped values;
    // `enforce_from_output_contract` with the card is what actually applies
    // it. `FIELD_CONTRACTS` still wins if one is ever added.
    let card = crate::resolve_agent_card(&state, &db_agent);
    let report = grounding_trust::enforce_from_output_contract(
        ORACLE,
        card.capabilities.output_contract.as_ref(),
        &mut doc,
    );
    if !report.is_clean() {
        grounding_anomaly::spawn_raise(
            Arc::clone(&state.memory_store),
            ORACLE,
            None,
            report.clone(),
        );
    }

    // Persist the enforced document, so a reload does not have to re-run a
    // paid search to see what it said.
    let statement = json!({
        "product": product_name,
        "currency": currency,
        "production_scale": scale,
        "basis_ml": basis_ml,
        "unresolved_quantities": unresolved,
        "priced": doc,
        "provenance": {
            "items": doc.get("items_provenance").cloned().unwrap_or(Value::Null),
            "risks": doc.get("risks_provenance").cloned().unwrap_or(Value::Null),
        },
        "priced_by": ORACLE,
        "priced_at": chrono::Utc::now().to_rfc3339(),
        "action_id": action_id.to_string(),
        "source_document": "dpp/composition.yaml",
    });
    let yaml = serde_yaml::to_string(&statement).unwrap_or_default();
    let body = format!(
        "# BOM pricing — written by the platform, not by hand.\n\
         #\n\
         # Produced by `{ORACLE}` via POST /actions/price_bom and persisted\n\
         # AFTER the grounding gate ran. `provenance.items` says whether a\n\
         # tool really returned these prices: `tool_no_match` means the search\n\
         # happened and found nothing, which is a gap and not a price of zero.\n\
         #\n\
         # Quantities are derived from dpp/composition.yaml percentages against\n\
         # the serving basis recorded above. Change the composition, re-run.\n\
         \n{yaml}"
    );
    let git = state.workspace_git.clone();
    let slug_w = slug.clone();
    let persisted = tokio::task::spawn_blocking(move || {
        git.commit_file(
            &slug_w,
            "dpp/pricing/bom_pricing.yaml",
            &body,
            "supply_chain_oracle: BOM pricing",
        )
    })
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false);

    let duration_ms = started.elapsed().as_millis() as u64;
    let priced_count = doc.get("items").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);

    // Outcome back onto the action row — the only workspace-scoped record of
    // how long this took. See the same note in `claim_evaluation.rs`.
    let _ = sqlx::query(
        "UPDATE workspace_action_log
            SET apply_result = $1, applied = TRUE, applied_at = NOW()
          WHERE action_id = $2",
    )
    .bind(json!({
        "duration_ms": duration_ms,
        "lines": bom_items.len(),
        "priced": priced_count,
        "unresolved": unresolved.len(),
        "violations": report.violations.len(),
        "persisted": persisted,
    }))
    .bind(action_id)
    .execute(&state.db)
    .await;

    Ok(Json(json!({
        "action_id": action_id,
        "action_type": "price_bom",
        "product": product_name,
        "currency": currency,
        "basis_ml": basis_ml,
        "lines_sent": bom_items.len(),
        "priced": priced_count,
        "unresolved_quantities": unresolved,
        "output": doc,
        "written_path": if persisted { json!("dpp/pricing/bom_pricing.yaml") } else { Value::Null },
        "duration_ms": duration_ms,
        "grounding_summary": {
            "is_clean": report.is_clean(),
            "violation_count": report.violations.len(),
            "provenance": report.provenance.iter()
                .map(|(b, v)| json!({"block": b, "verdict": v}))
                .collect::<Vec<_>>(),
        },
        // Same honesty as evaluate_claims: the charge lands in a background
        // task after this response is built, so no figure exists yet.
        "cost": {
            "credits_charged": Value::Null,
            "where": "GET /api/workspaces/{workspace_id}/budget",
        },
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composition() -> Value {
        serde_yaml::from_slice(
            &std::fs::read("apps/adaptogen-lab/dpp/composition.yaml").expect("read composition"),
        )
        .expect("parse composition")
    }

    /// **The conversion the browser used to own.**
    ///
    /// The hardcoded fixture this replaces carried 2.64 g of black tea and
    /// 28.05 g of hibiscus. Those were never arbitrary — they are this
    /// composition's percentages times its 330 ml serving — which is exactly
    /// why the fixture went unnoticed. Reproducing them from the document is
    /// the evidence that the server-side conversion is the same arithmetic,
    /// now applied to whatever the user actually wrote.
    #[test]
    fn percentages_become_the_masses_the_fixture_hardcoded() {
        let comp = composition();
        let basis = serving_basis_ml(&comp).expect("serving volume");
        assert_eq!(basis, 330.0);
        let lines = build_lines(&comp, Some(basis));

        let by_name = |needle: &str| -> Value {
            lines
                .iter()
                .find(|l| {
                    l.value["name"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(needle)
                })
                .unwrap_or_else(|| panic!("no line matching {needle}"))
                .value
                .clone()
        };

        assert_eq!(by_name("black tea")["qty"].as_f64(), Some(2.64));
        assert_eq!(by_name("hibiscus")["qty"].as_f64(), Some(28.05));
        assert_eq!(by_name("citric")["qty"].as_f64(), Some(0.99));
        // Water was omitted from the fixture entirely — 88.3% of the product,
        // left out of its own bill of materials.
        assert_eq!(by_name("water")["qty"].as_f64(), Some(291.39));
        assert_eq!(by_name("water")["unit"].as_str(), Some("g"));
    }

    /// A real BOM contains quantities that are not numbers, and no number may
    /// be invented for them.
    #[test]
    fn a_trace_quantity_is_carried_without_a_number() {
        let comp = composition();
        let lines = build_lines(&comp, Some(330.0));
        let scoby = lines
            .iter()
            .find(|l| l.value["name"].as_str().unwrap_or("").to_lowercase().contains("scoby"))
            .expect("scoby line");

        assert!(scoby.value["qty"].is_null(), "a number was invented for `trace`");
        assert!(scoby.value["unit"].is_null());
        assert_eq!(
            scoby.value["quantity_declared"].as_str(),
            Some("trace"),
            "the declared value must survive so the agent can see what the document says"
        );
        assert!(
            scoby.unresolved.as_deref().is_some_and(|u| u.contains("trace")),
            "an unresolvable quantity must be reported, not silently dropped"
        );
    }

    /// Without a basis, a percentage cannot become a mass — and must not.
    #[test]
    fn percentages_without_a_basis_resolve_to_nothing() {
        let comp = composition();
        let lines = build_lines(&comp, None);
        assert!(!lines.is_empty());
        for l in &lines {
            let declared = l.value["quantity_declared"].as_str().unwrap_or("");
            if declared.ends_with('%') {
                assert!(
                    l.value["qty"].is_null(),
                    "{declared} became a mass with no serving basis to convert against"
                );
                assert!(l.unresolved.is_some());
            }
        }
    }

    #[test]
    fn absolute_quantities_pass_through_with_their_unit() {
        assert_eq!(parse_quantity("12 g", None).map(|(q, u, _)| (q, u)), Some((12.0, "g")));
        assert_eq!(parse_quantity("0.5kg", None).map(|(q, u, _)| (q, u)), Some((0.5, "kg")));
        assert_eq!(parse_quantity("330 ml", None).map(|(q, u, _)| (q, u)), Some((330.0, "ml")));
        // Not a unit the oracle's schema names, so it is not guessed at.
        assert!(parse_quantity("2 handfuls", None).is_none());
        assert!(parse_quantity("trace", None).is_none());
        assert!(parse_quantity("", None).is_none());
        // A percentage with no basis is unresolvable, not zero.
        assert!(parse_quantity("8.5%", None).is_none());
    }
}
