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
use serde_json::{json, Map, Value};
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
///
/// `pub(super)` so `super::carbon` converts the same way rather than parsing
/// percentages a second time. Two readings of one BOM that disagree would put a
/// different mass behind the PRICE and the FOOTPRINT of the same product, and
/// each document would be internally consistent — the hardest kind of
/// disagreement to notice.
pub(super) fn parse_quantity(
    raw: &str,
    basis_ml: Option<f64>,
) -> Option<(f64, &'static str, Option<String>)> {
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
    let split = t.find(|c: char| c.is_alphabetic()).filter(|i| *i > 0)?;
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

// ─── the platform's arithmetic ──────────────────────────────────────────────────────
//
// The card asked the oracle for `total_bom_cost: <sum of qty * unit_cost>`, so
// the model did the multiplication. That is the one thing this App does not let
// an agent do — `carbon_accountant` retrieves factors and the platform
// multiplies, because a confidently wrong product of two plausible numbers looks
// exactly like a right one. Pricing had the same exposure plus a worse one: the
// oracle quotes per `kg|L|g|unit`, the quantities are in grams, and nothing
// reconciled them. The Studio papered over it by printing every price as €/g
// and dividing the model's total by 1000.
//
// So each line's cost and the total are derived here, with the unit conversion
// written out, and the model's own total is kept only as an audit figure.

/// `(dimension, factor to that dimension's base unit)`. Mass base is grams,
/// volume base is millilitres.
fn unit_base(u: &str) -> Option<(&'static str, f64)> {
    match u.trim().to_ascii_lowercase().as_str() {
        "mg" => Some(("mass", 0.001)),
        "g" => Some(("mass", 1.0)),
        "kg" => Some(("mass", 1000.0)),
        "ml" => Some(("volume", 1.0)),
        "l" => Some(("volume", 1000.0)),
        "unit" | "units" | "each" | "pcs" => Some(("count", 1.0)),
        _ => None,
    }
}

/// Express `qty qty_unit` in `price_unit`.
///
/// Mass and volume are different dimensions and are not converted into each
/// other — except on a line whose quantity the platform itself derived from a
/// percentage of the serving, where it has ALREADY declared 1 ml = 1 g for this
/// aqueous product. Converting there is consistent with a stated assumption;
/// converting anywhere else would invent a density.
fn qty_in(qty: f64, qty_unit: &str, price_unit: &str, aqueous: bool) -> Result<f64, String> {
    let (qd, qf) = unit_base(qty_unit)
        .ok_or_else(|| format!("quantity unit `{qty_unit}` is not one the platform converts"))?;
    let (pd, pf) = unit_base(price_unit)
        .ok_or_else(|| format!("price unit `{price_unit}` is not one the platform converts"))?;
    let same = qd == pd;
    let bridged = aqueous && ((qd == "mass" && pd == "volume") || (qd == "volume" && pd == "mass"));
    if !same && !bridged {
        return Err(format!(
            "priced per {price_unit} but the BOM states {qty_unit}; converting {qd} to {pd} needs a \
             density nobody has given"
        ));
    }
    Ok(qty * qf / pf)
}

fn round6(v: f64) -> Value {
    serde_json::Number::from_f64((v * 1_000_000.0).round() / 1_000_000.0)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Overwrite each priced item's quantity and cost, and the total, with the
/// platform's values. Pure over the BOM and the enforced reply.
fn derive_costs(lines: &[BomLine], doc: &mut Value) {
    let by_name = |name: &str| {
        let n = name.trim().to_ascii_lowercase();
        lines.iter().find(|l| {
            l.value["name"]
                .as_str()
                .map(|s| s.trim().to_ascii_lowercase())
                == Some(n.clone())
                || l.value["item_id"].as_str().map(|s| s.to_ascii_lowercase()) == Some(n.clone())
        })
    };

    let mut subtotal = 0.0_f64;
    let mut priced: Vec<String> = Vec::new();
    let mut unpriced: Vec<String> = Vec::new();

    if let Some(items) = doc.get_mut("items").and_then(|v| v.as_array_mut()) {
        for item in items.iter_mut() {
            let Some(obj) = item.as_object_mut() else {
                continue;
            };
            let name = obj
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let line = by_name(&name);

            // The quantity is the BOM's. A quantity the model restated is one
            // nobody wrote, and it multiplies straight into the total.
            let qty = line.and_then(|l| l.value["qty"].as_f64());
            let qty_unit = line
                .and_then(|l| l.value["unit"].as_str())
                .map(str::to_string);
            let aqueous = line.is_some_and(|l| l.value.get("basis").is_some());
            obj.insert("qty".into(), qty.map(round6).unwrap_or(Value::Null));
            obj.insert(
                "qty_unit".into(),
                qty_unit.clone().map(Value::String).unwrap_or(Value::Null),
            );
            // Grams, when the line can be expressed in grams at all — what an
            // operator-declared price per gram multiplies against.
            let qty_g = match (qty, qty_unit.as_deref()) {
                (Some(q), Some(u)) => qty_in(q, u, "g", aqueous).ok(),
                _ => None,
            };
            obj.insert("qty_g".into(), qty_g.map(round6).unwrap_or(Value::Null));

            let unit_cost = obj
                .get("unit_cost")
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0);
            let price_unit = obj.get("unit").and_then(|v| v.as_str()).map(str::to_string);

            let outcome: Result<(f64, String), String> = match (
                line,
                qty,
                qty_unit.as_deref(),
                unit_cost,
                price_unit.as_deref(),
            ) {
                (None, ..) => Err(
                    "not a line in dpp/composition.yaml, so there is no quantity to price".into(),
                ),
                (_, None, ..) | (_, _, None, ..) => {
                    Err("the BOM states no numeric quantity for this line".into())
                }
                (_, _, _, None, _) => Err("no price retrieved".into()),
                (_, _, _, _, None) => Err("price has no unit, so it cannot be multiplied".into()),
                (_, Some(q), Some(qu), Some(c), Some(pu)) => qty_in(q, qu, pu, aqueous).map(|n| {
                    let cost = n * c;
                    let bridge =
                        if aqueous && unit_base(qu).map(|x| x.0) != unit_base(pu).map(|x| x.0) {
                            " (1 ml taken as 1 g, as for the serving basis)"
                        } else {
                            ""
                        };
                    (
                        cost,
                        format!("{q} {qu} = {n:.6} {pu}{bridge} × {c} = {cost:.6}"),
                    )
                }),
            };
            match outcome {
                Ok((cost, arith)) => {
                    subtotal += cost;
                    priced.push(name.clone());
                    obj.insert("line_cost".into(), round6(cost));
                    obj.insert("arithmetic".into(), Value::String(arith));
                    obj.remove("cost_refusal");
                }
                Err(why) => {
                    unpriced.push(name.clone());
                    obj.insert("line_cost".into(), Value::Null);
                    obj.insert("arithmetic".into(), Value::Null);
                    obj.insert("cost_refusal".into(), Value::String(why));
                }
            }
        }
    }

    // A BOM line with a quantity that the oracle did not return at all is as
    // unpriced as one it returned without a price, and must count against
    // completeness rather than vanish from it.
    for l in lines {
        let n = l.value["name"].as_str().unwrap_or("").to_string();
        if l.value["qty"].is_number() && !priced.contains(&n) && !unpriced.contains(&n) {
            unpriced.push(n);
        }
    }

    let coverage = if priced.is_empty() {
        "none"
    } else if unpriced.is_empty() {
        "complete"
    } else {
        "partial"
    };
    let mut summary = match doc.get("summary") {
        Some(Value::Object(m)) => m.clone(),
        Some(Value::String(s)) => {
            let mut m = Map::new();
            m.insert("oracle_note".into(), Value::String(s.clone()));
            m
        }
        _ => Map::new(),
    };
    let model_total = summary.remove("total_bom_cost").unwrap_or(Value::Null);
    // Null unless complete: a sum over some lines is a subtotal, and printing
    // it where a total goes is the misreading this field exists to prevent.
    summary.insert(
        "total_bom_cost".into(),
        if coverage == "complete" {
            round6(subtotal)
        } else {
            Value::Null
        },
    );
    summary.insert(
        "priced_subtotal".into(),
        if priced.is_empty() {
            Value::Null
        } else {
            round6(subtotal)
        },
    );
    summary.insert("coverage".into(), json!(coverage));
    summary.insert("unpriced_items".into(), json!(unpriced));
    summary.insert("arithmetic".into(), json!("platform_derived"));
    summary.insert("model_total_bom_cost".into(), model_total);
    if let Value::Object(d) = doc {
        d.insert("summary".into(), Value::Object(summary));
    }
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

pub(super) fn serving_basis_ml(composition: &Value) -> Option<f64> {
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

    // Hiring is enforced server-side. This App already had a Hire button for
    // the oracle, which is why its card reads "not hired" — but the button
    // guarded the UI, not the endpoint.
    super::require_hired_agent(&state, ws_uuid, ORACLE).await?;

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

    // After the gate, before persistence: a price the gate stripped is not
    // multiplied, and the file records the platform's figures, not the model's.
    derive_costs(&lines, &mut doc);

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
    let priced_count = doc
        .get("items")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);

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

    /// Lines from the real composition, priced the way the oracle quotes —
    /// per kilogram — against quantities the BOM states in grams.
    fn priced(items: Value) -> Value {
        let lines = build_lines(&composition(), Some(330.0));
        let mut doc = json!({ "items": items, "summary": { "total_bom_cost": 999.0 } });
        derive_costs(&lines, &mut doc);
        doc
    }

    /// The multiplication is the platform's, across the unit the oracle quoted.
    ///
    /// Black tea is 0.8% of 330 ml = 2.64 g. Priced at €12/kg that is
    /// 0.00264 kg × 12 = €0.03168. The Studio used to print €12 as "/g" and
    /// divide the model's total by 1000 — right only by accident of which unit
    /// the model happened to pick.
    #[test]
    fn line_cost_is_converted_into_the_quoted_unit_and_multiplied_by_the_platform() {
        let doc = priced(json!([
            { "name": "Black tea (Camellia sinensis)", "unit_cost": 12.0, "unit": "kg" }
        ]));
        let tea = &doc["items"][0];
        assert_eq!(tea["qty_g"].as_f64(), Some(2.64));
        assert_eq!(tea["line_cost"].as_f64(), Some(0.03168));
        assert!(tea["arithmetic"].as_str().unwrap().contains("kg"));
        // The model's 999 is kept for audit and is not the total.
        assert_eq!(doc["summary"]["model_total_bom_cost"].as_f64(), Some(999.0));
        assert_eq!(doc["summary"]["arithmetic"], "platform_derived");
    }

    /// A total over some lines is a subtotal, and it must not sit where a
    /// total goes. Five BOM lines carry quantities; one is priced.
    #[test]
    fn a_total_is_null_until_every_quantified_line_is_priced() {
        let doc = priced(json!([
            { "name": "Black tea (Camellia sinensis)", "unit_cost": 12.0, "unit": "kg" },
            { "name": "Hibiscus infusion", "unit_cost": null, "unit": "kg" }
        ]));
        let s = &doc["summary"];
        assert!(s["total_bom_cost"].is_null());
        assert_eq!(s["priced_subtotal"].as_f64(), Some(0.03168));
        assert_eq!(s["coverage"], "partial");
        let unpriced: Vec<&str> = s["unpriced_items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(
            unpriced.contains(&"Hibiscus infusion"),
            "returned without a price"
        );
        assert!(
            unpriced.contains(&"Cane sugar (residual post-F1)"),
            "a quantified BOM line the oracle never returned still counts against completeness"
        );
    }

    /// Mass is not converted into volume unless the platform itself declared
    /// the equivalence. Water's quantity came from the serving basis (1 ml =
    /// 1 g, stated), so a per-litre price converts. A line stated in grams
    /// outright, priced per litre, would need a density nobody gave.
    #[test]
    fn volume_pricing_converts_only_where_the_basis_already_declared_1ml_is_1g() {
        let doc =
            priced(json!([{ "name": "Water (process grade)", "unit_cost": 0.002, "unit": "L" }]));
        let w = &doc["items"][0];
        assert_eq!(w["line_cost"].as_f64(), Some(0.000583));
        assert!(w["arithmetic"]
            .as_str()
            .unwrap()
            .contains("1 ml taken as 1 g"));

        assert!(
            qty_in(10.0, "g", "l", false).is_err(),
            "no density, no conversion"
        );
        assert_eq!(qty_in(500.0, "mg", "g", false).unwrap(), 0.5);
    }

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
            .find(|l| {
                l.value["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("scoby")
            })
            .expect("scoby line");

        assert!(
            scoby.value["qty"].is_null(),
            "a number was invented for `trace`"
        );
        assert!(scoby.value["unit"].is_null());
        assert_eq!(
            scoby.value["quantity_declared"].as_str(),
            Some("trace"),
            "the declared value must survive so the agent can see what the document says"
        );
        assert!(
            scoby
                .unresolved
                .as_deref()
                .is_some_and(|u| u.contains("trace")),
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
        assert_eq!(
            parse_quantity("12 g", None).map(|(q, u, _)| (q, u)),
            Some((12.0, "g"))
        );
        assert_eq!(
            parse_quantity("0.5kg", None).map(|(q, u, _)| (q, u)),
            Some((0.5, "kg"))
        );
        assert_eq!(
            parse_quantity("330 ml", None).map(|(q, u, _)| (q, u)),
            Some((330.0, "ml"))
        );
        // Not a unit the oracle's schema names, so it is not guessed at.
        assert!(parse_quantity("2 handfuls", None).is_none());
        assert!(parse_quantity("trace", None).is_none());
        assert!(parse_quantity("", None).is_none());
        // A percentage with no basis is unresolvable, not zero.
        assert!(parse_quantity("8.5%", None).is_none());
    }
}
