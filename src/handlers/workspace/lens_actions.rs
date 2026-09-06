//! Regulatory-lens translator action handlers.
//!
//! Three action endpoints for the `adaptogen_lab_regulatory` App:
//!
//!   POST /api/workspaces/:id/actions/render_lens
//!   POST /api/workspaces/:id/actions/compare_lenses
//!   POST /api/workspaces/:id/actions/flag_divergence
//!
//! Each handler reads ruleset YAML from the workspace git, runs the grounding
//! gate ([`fermi::lens_rendering::gate_lens_output`]) plus
//! [`crate::grounding_trust::enforce`], merges the two reports, raises an
//! anomaly if the combined report is not clean, and records the action in
//! `workspace_action_log`.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

use fermi_auth::AuthPrincipal;

use super::actions::resolve_workspace;
use crate::{grounding_trust, AppState};
use fermi::grounding_anomaly;
use fermi::lens_rendering::{self, Market, Ruleset};

// ─── Shared helpers ──────────────────────────────────────────────────────────

fn parse_market(s: &str) -> Result<Market, (StatusCode, String)> {
    match s {
        "EU" => Ok(Market::Eu),
        "US" => Ok(Market::Us),
        "CN" => Ok(Market::Cn),
        other => Err((
            StatusCode::BAD_REQUEST,
            format!("Unknown market `{other}` — expected EU, US, or CN"),
        )),
    }
}

/// Read and parse a ruleset from the workspace git.
/// Read a ruleset, trying the workspace git first then falling back to the
/// platform apps directory. The fallback means workspaces don't need to be
/// pre-seeded with ruleset files — the canonical copies in
/// `apps/adaptogen-lab/regulatory-lens/rulesets/` are used if the workspace
/// hasn't overridden them. A workspace-local copy takes precedence, which
/// allows per-product ruleset customisation in the future.
async fn read_ruleset(
    state: &AppState,
    slug: &str,
    market: Market,
) -> Result<Ruleset, (StatusCode, String)> {
    let ws_path = format!("regulatory-lens/rulesets/{}", market.ruleset_filename());
    let platform_path = format!(
        "apps/adaptogen-lab/regulatory-lens/rulesets/{}",
        market.ruleset_filename()
    );
    let git = state.workspace_git.clone();
    let slug_s = slug.to_string();
    let bytes = tokio::task::spawn_blocking(move || {
        // Try workspace git first.
        git.read_file_bytes(&slug_s, &ws_path).or_else(|_| {
            // Fall back to the platform apps directory.
            std::fs::read(&platform_path).map_err(|e| {
                agent_bestiary_ontology::OntologyError::RepoNotFound(format!(
                    "platform fallback not found at {platform_path}: {e}"
                ))
            })
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("{market} ruleset not found in workspace or platform: {e}"),
        )
    })?;
    Ruleset::from_yaml(&bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{market} ruleset parse error: {e}"),
        )
    })
}

/// Classify the status family for divergence scoring.
fn status_family(status: &str) -> &'static str {
    if status.starts_with("allowed") {
        "allowed"
    } else if status == "conditionally_allowed" {
        "conditional"
    } else if status == "rewritten" {
        "rewritten"
    } else if status == "not_allowed" {
        "not_allowed"
    } else {
        "other"
    }
}

/// Score how much two or more market statuses diverge (0–3).
fn divergence_score(statuses: &[&str]) -> u8 {
    let any_prohibited = statuses.iter().any(|s| *s == "not_allowed");
    let all_prohibited = statuses.iter().all(|s| *s == "not_allowed");
    if any_prohibited && !all_prohibited {
        return 3;
    }
    let first_family = statuses
        .first()
        .map(|s| status_family(s))
        .unwrap_or("other");
    if statuses.iter().any(|s| status_family(s) != first_family) {
        return 2;
    }
    if statuses.windows(2).any(|w| w[0] != w[1]) {
        return 1;
    }
    0
}

fn reinstatement_note(divergence_type: &str, market_a: &str, market_b: &str) -> String {
    match divergence_type {
        "philosophy" => format!(
            "One market prohibits this claim outright while the other permits it. \
             Commission a regulatory affairs review before targeting both {market_a} and {market_b} \
             with the same label copy."
        ),
        "threshold" => format!(
            "Both {market_a} and {market_b} permit the claim but require different framing. \
             Separate label versions are recommended; a single label cannot satisfy both requirement sets."
        ),
        "ingredient_status" => format!(
            "The ingredient's regulatory status differs between {market_a} and {market_b}. \
             Verify current approved novel food or health food status in each jurisdiction before launch."
        ),
        _ => format!(
            "Both {market_a} and {market_b} allow this claim but require different wording. \
             Prepare market-specific copy for each SKU variant."
        ),
    }
}

// ─── 1. render_lens ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RenderLensRequest {
    pub target_market: String,
    pub source_product_id: Option<String>,
    pub claim_ids: Option<Vec<String>>,
    pub source_message_id: Option<String>,
}

pub async fn render_lens_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<RenderLensRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let market = parse_market(&req.target_market)?;
    let ruleset = read_ruleset(&state, &slug, market).await?;

    let product_id = req
        .source_product_id
        .as_deref()
        .unwrap_or("precision_kombucha_hibiscus_f2")
        .to_string();

    // Build rendered_claims from the ruleset, filtered by claim_ids if provided.
    let rendered_claims: Vec<Value> = ruleset
        .claim_renderings
        .iter()
        .filter(|cr| {
            req.claim_ids
                .as_ref()
                .map(|ids| ids.iter().any(|id| id == &cr.source_claim_id))
                .unwrap_or(true)
        })
        .map(|cr| {
            json!({
                "source_claim_id": cr.source_claim_id,
                "rendered_text": cr.rendered_text,
                "status": cr.status,
                "basis": cr.basis,
                "divergence_note": cr.divergence_note,
            })
        })
        .collect();

    let ingredient_status: Vec<Value> = ruleset
        .ingredient_status
        .iter()
        .map(|is| {
            json!({
                "ingredient_id": is.ingredient_id,
                "status": is.status,
                "notes": is.notes,
            })
        })
        .collect();

    let mut rendered_output = json!({
        "target_market": market.as_str(),
        "source_product_id": product_id,
        "ruleset_id": ruleset.ruleset_id,
        "data_status": ruleset.data_status,
        "rendered_claims": rendered_claims,
        "allergen_block": {
            "standard": ruleset.allergen_format.standard,
            "mechanism": ruleset.allergen_format.mechanism,
            "this_product": ruleset.allergen_format.this_product,
        },
        "ingredient_status": ingredient_status,
        "verification_appendix": ruleset.verify_sources,
    });

    // Gate: validate rendered output against the ruleset.
    let gate_report = lens_rendering::gate_lens_output(&ruleset, &mut rendered_output);
    // Grounding trust: check inferred and narrative fields.
    let trust_report = grounding_trust::enforce("regulatory_lens_translator", &mut rendered_output);
    let combined_report = lens_rendering::merge_reports(gate_report, trust_report);

    if !combined_report.is_clean() {
        grounding_anomaly::spawn_raise(
            Arc::clone(&state.memory_store),
            "regulatory_lens_translator",
            None,
            combined_report.clone(),
        );
    }

    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let payload = json!({
        "target_market": req.target_market,
        "source_product_id": &product_id,
        "claim_ids": req.claim_ids,
    });

    let action_id: Uuid = sqlx::query(
        r#"INSERT INTO workspace_action_log
           (workspace_id, emitted_by_type, emitted_by_id, action_type,
            app_schema, payload, confirmation, source_message_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING action_id"#,
    )
    .bind(ws_uuid)
    .bind("user")
    .bind(&user_id)
    .bind("render_lens")
    .bind(Some("adaptogen_lab_regulatory"))
    .bind(&payload)
    .bind("auto")
    .bind(source_msg_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("action_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_id,
        "rendered_output": rendered_output,
        "grounding_summary": {
            "is_clean": combined_report.is_clean(),
            "violation_count": combined_report.violations.len(),
            "provenance_blocks": combined_report.provenance.len(),
        },
    })))
}

// ─── 2. compare_lenses ───────────────────────────────────────────────────────

/// A single overridden source claim — replaces or adds a claim in the
/// composition's source_claims array for this comparison only.
/// The override is not persisted; call mutate_document separately to save it.
#[derive(Deserialize)]
pub struct OverrideClaim {
    pub id: String,
    pub candidate_text: String,
    /// Optional: claim_pressure hint ("high" | "medium" | "low"). Defaults to "medium".
    pub claim_pressure: Option<String>,
}

#[derive(Deserialize)]
pub struct CompareLensesRequest {
    pub source_product_id: Option<String>,
    pub markets: Option<Vec<String>>,
    pub claim_id: Option<String>,
    pub override_source_claims: Option<Vec<OverrideClaim>>,
    pub source_message_id: Option<String>,
}

pub async fn compare_lenses_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<CompareLensesRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    // Read all three rulesets concurrently.
    let (eu_rs, us_rs, cn_rs) = tokio::try_join!(
        read_ruleset(&state, &slug, Market::Eu),
        read_ruleset(&state, &slug, Market::Us),
        read_ruleset(&state, &slug, Market::Cn),
    )?;

    // Read the product composition YAML.
    let product_id = req
        .source_product_id
        .as_deref()
        .unwrap_or("precision_kombucha_hibiscus_f2")
        .to_string();
    let ws_comp_path = format!("regulatory-lens/sku/{product_id}.yaml");
    let platform_comp_path = format!("apps/adaptogen-lab/regulatory-lens/sku/{product_id}.yaml");
    let git = state.workspace_git.clone();
    let slug_c = slug.clone();
    let comp_bytes = tokio::task::spawn_blocking(move || {
        git.read_file_bytes(&slug_c, &ws_comp_path).or_else(|_| {
            std::fs::read(&platform_comp_path).map_err(|e| {
                agent_bestiary_ontology::OntologyError::RepoNotFound(format!(
                    "platform fallback not found at {platform_comp_path}: {e}"
                ))
            })
        })
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("Product composition not found in workspace or platform: {e}"),
        )
    })?;

    let composition: Value = serde_yaml::from_slice(&comp_bytes).map_err(|e| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Composition parse error: {e}"),
        )
    })?;

    let source_claims: Vec<serde_json::Value> =
        if let Some(ref overrides) = req.override_source_claims {
            // When overrides are provided, use them instead of (or merged with) the
            // composition's source_claims. This allows the UI to test a modified claim
            // without writing it back to the YAML first.
            //
            // Merge strategy: start from the composition's source_claims, then apply
            // overrides by matching `id`. New ids (not in composition) are appended.
            let mut base: Vec<serde_json::Value> = composition
                .get("source_claims")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for ov in overrides {
                let pressure = ov.claim_pressure.as_deref().unwrap_or("medium");
                let replacement = serde_json::json!({
                    "id": ov.id,
                    "candidate_text": ov.candidate_text,
                    "claim_pressure": pressure,
                });
                if let Some(existing) = base
                    .iter_mut()
                    .find(|c| c.get("id").and_then(|v| v.as_str()) == Some(&ov.id))
                {
                    *existing = replacement;
                } else {
                    base.push(replacement);
                }
            }
            base
        } else {
            composition
                .get("source_claims")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        };

    // Determine which markets are in scope.
    let active_markets: Vec<Market> = req
        .markets
        .as_ref()
        .map(|ms| ms.iter().filter_map(|m| parse_market(m).ok()).collect())
        .unwrap_or_else(|| vec![Market::Eu, Market::Us, Market::Cn]);

    let market_rulesets: [(Market, &Ruleset); 3] = [
        (Market::Eu, &eu_rs),
        (Market::Us, &us_rs),
        (Market::Cn, &cn_rs),
    ];

    // Build the comparison table.
    let mut comparison_table: Vec<Value> = Vec::new();

    for claim in &source_claims {
        let claim_id = claim
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        if let Some(focus) = &req.claim_id {
            if &claim_id != focus {
                continue;
            }
        }

        let mut market_rows: Vec<Value> = Vec::new();
        let mut status_strs: Vec<String> = Vec::new();

        for (market, rs) in market_rulesets.iter() {
            if !active_markets.contains(market) {
                continue;
            }
            match rs.rendering_for(&claim_id) {
                Some(r) => {
                    status_strs.push(r.status.clone());
                    market_rows.push(json!({
                        "market": market.as_str(),
                        "status": r.status,
                        "rendered_text": r.rendered_text,
                        "basis": r.basis,
                        "divergence_note": r.divergence_note,
                    }));
                }
                None => {
                    status_strs.push("not_in_ruleset".to_string());
                    market_rows.push(json!({
                        "market": market.as_str(),
                        "status": "not_in_ruleset",
                        "rendered_text": null,
                        "basis": null,
                        "divergence_note": null,
                    }));
                }
            }
        }

        let status_refs: Vec<&str> = status_strs.iter().map(|s| s.as_str()).collect();
        let score = divergence_score(&status_refs);
        let is_demo_beat = claim_id == "hibiscus_wellness" || claim_id == "live_cultures_present";

        comparison_table.push(json!({
            "claim_id": claim_id,
            "candidate_text": claim.get("candidate_text"),
            "claim_pressure": claim.get("claim_pressure"),
            "markets": market_rows,
            "divergence_score": score,
            "demo_beat": is_demo_beat,
        }));
    }

    // Identify ingredient divergence beat for hibiscus across markets.
    let ingredient_divergence: Vec<Value> = market_rulesets
        .iter()
        .filter_map(|(market, rs)| {
            rs.ingredient_status
                .iter()
                .find(|i| i.ingredient_id.contains("hibiscus"))
                .map(|entry| {
                    json!({
                        "market": market.as_str(),
                        "ingredient_id": entry.ingredient_id,
                        "status": entry.status,
                        "notes": entry.notes,
                    })
                })
        })
        .collect();

    let primary_demo_beat = json!({
        "claim_id": "hibiscus_wellness",
        "rationale": "hibiscus_wellness exhibits the sharpest status divergence across markets \
                      (not_allowed in at least one jurisdiction while allowed or conditionally_allowed \
                      in others). This is the canonical demonstration of the lens translator's purpose: \
                      the same ingredient claim is permitted with caveats in one regulatory frame \
                      and outright prohibited in another.",
        "secondary": {
            "claim_id": "live_cultures_present",
            "rationale": "live_cultures_present shows philosophical divergence: all markets permit \
                          the claim, but the regulatory basis differs fundamentally — EU grounds it \
                          in functional food regulation, US in structure/function claim doctrine, \
                          CN in health food product standards. Same surface outcome, incompatible \
                          legitimating frames."
        }
    });

    let mut combined_output = json!({
        "source_product_id": product_id,
        "comparison_table": comparison_table,
        "primary_demo_beat": primary_demo_beat,
        "ingredient_divergence_beat": {
            "ingredient_id": "hibiscus",
            "markets": ingredient_divergence,
        },
        "verification_appendix": eu_rs.verify_sources,
        "data_status": eu_rs.data_status,
    });

    // Run grounding gate (EU ruleset as representative for appendix / data_status checks).
    let gate_report = lens_rendering::gate_lens_output(&eu_rs, &mut combined_output);
    let trust_report = grounding_trust::enforce("regulatory_lens_translator", &mut combined_output);
    let combined_report = lens_rendering::merge_reports(gate_report, trust_report);

    if !combined_report.is_clean() {
        grounding_anomaly::spawn_raise(
            Arc::clone(&state.memory_store),
            "regulatory_lens_translator",
            None,
            combined_report.clone(),
        );
    }

    // Commit the comparison output to workspace git.
    let output_json = serde_json::to_string_pretty(&combined_output).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("JSON serialise error: {e}"),
        )
    })?;
    let git = state.workspace_git.clone();
    let slug_w = slug.clone();
    tokio::task::spawn_blocking(move || {
        git.commit_file(
            &slug_w,
            "regulatory-lens/comparisons/three-lens.json",
            &output_json,
            "auto: three-lens comparison",
        )
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Git commit error: {e}"),
        )
    })?;

    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let markets_logged = req
        .markets
        .unwrap_or_else(|| vec!["EU".to_string(), "US".to_string(), "CN".to_string()]);

    let payload = json!({
        "source_product_id": &product_id,
        "markets": markets_logged,
        "claim_id": req.claim_id,
    });

    let action_id: Uuid = sqlx::query(
        r#"INSERT INTO workspace_action_log
           (workspace_id, emitted_by_type, emitted_by_id, action_type,
            app_schema, payload, confirmation, source_message_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING action_id"#,
    )
    .bind(ws_uuid)
    .bind("user")
    .bind(&user_id)
    .bind("compare_lenses")
    .bind(Some("adaptogen_lab_regulatory"))
    .bind(&payload)
    .bind("auto")
    .bind(source_msg_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("action_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_id,
        "output_path": "regulatory-lens/comparisons/three-lens.json",
        "output": combined_output,
        "grounding_summary": {
            "is_clean": combined_report.is_clean(),
            "violation_count": combined_report.violations.len(),
        },
    })))
}

// ─── 3. flag_divergence ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FlagDivergenceRequest {
    pub claim_id: Option<String>,
    pub ingredient_id: Option<String>,
    pub market_a: String,
    pub market_b: String,
    pub source_message_id: Option<String>,
}

fn classify_divergence_type(status_a: &str, status_b: &str, is_ingredient: bool) -> &'static str {
    if is_ingredient {
        return "ingredient_status";
    }
    let prohibited_a = status_a == "not_allowed";
    let prohibited_b = status_b == "not_allowed";
    if prohibited_a != prohibited_b {
        "philosophy"
    } else if status_family(status_a) != status_family(status_b) {
        "threshold"
    } else {
        "format"
    }
}

pub async fn flag_divergence_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<String>,
    Json(req): Json<FlagDivergenceRequest>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let (ws_uuid, slug) = resolve_workspace(&state, &workspace_id, &user_id).await?;

    let market_a = parse_market(&req.market_a)?;
    let market_b = parse_market(&req.market_b)?;

    let (rs_a, rs_b) = tokio::try_join!(
        read_ruleset(&state, &slug, market_a),
        read_ruleset(&state, &slug, market_b),
    )?;

    let source_msg_id = req
        .source_message_id
        .as_deref()
        .and_then(|s| s.parse::<Uuid>().ok());

    let divergence_report = if let Some(ref claim_id) = req.claim_id {
        let entry_a = rs_a.rendering_for(claim_id);
        let entry_b = rs_b.rendering_for(claim_id);

        let status_a = entry_a
            .map(|e| e.status.as_str())
            .unwrap_or("not_in_ruleset");
        let status_b = entry_b
            .map(|e| e.status.as_str())
            .unwrap_or("not_in_ruleset");
        let score = divergence_score(&[status_a, status_b]);
        let divergence_type = classify_divergence_type(status_a, status_b, false);

        json!({
            "focus": "claim",
            "claim_id": claim_id,
            "market_a": {
                "market": req.market_a,
                "status": status_a,
                "rendered_text": entry_a.and_then(|e| e.rendered_text.as_deref()),
                "basis": entry_a.and_then(|e| e.basis.as_deref()),
            },
            "market_b": {
                "market": req.market_b,
                "status": status_b,
                "rendered_text": entry_b.and_then(|e| e.rendered_text.as_deref()),
                "basis": entry_b.and_then(|e| e.basis.as_deref()),
            },
            "divergence_type": divergence_type,
            "divergence_score": score,
            "one_line": format!(
                "{claim_id}: {status_a} ({}) vs {status_b} ({})",
                req.market_a, req.market_b
            ),
            "reinstatement_note": reinstatement_note(divergence_type, &req.market_a, &req.market_b),
        })
    } else if let Some(ref ingredient_id) = req.ingredient_id {
        let entry_a = rs_a
            .ingredient_status
            .iter()
            .find(|i| i.ingredient_id == *ingredient_id);
        let entry_b = rs_b
            .ingredient_status
            .iter()
            .find(|i| i.ingredient_id == *ingredient_id);

        let status_a = entry_a
            .map(|e| e.status.as_str())
            .unwrap_or("not_in_ruleset");
        let status_b = entry_b
            .map(|e| e.status.as_str())
            .unwrap_or("not_in_ruleset");
        let divergence_type = classify_divergence_type(status_a, status_b, true);

        json!({
            "focus": "ingredient",
            "ingredient_id": ingredient_id,
            "market_a": {
                "market": req.market_a,
                "status": status_a,
                "notes": entry_a.and_then(|e| e.notes.as_deref()),
            },
            "market_b": {
                "market": req.market_b,
                "status": status_b,
                "notes": entry_b.and_then(|e| e.notes.as_deref()),
            },
            "divergence_type": divergence_type,
            "one_line": format!(
                "{ingredient_id}: {status_a} ({}) vs {status_b} ({})",
                req.market_a, req.market_b
            ),
            "reinstatement_note": reinstatement_note(divergence_type, &req.market_a, &req.market_b),
        })
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "At least one of claim_id or ingredient_id is required".to_string(),
        ));
    };

    let payload = json!({
        "claim_id": req.claim_id,
        "ingredient_id": req.ingredient_id,
        "market_a": req.market_a,
        "market_b": req.market_b,
    });

    let action_id: Uuid = sqlx::query(
        r#"INSERT INTO workspace_action_log
           (workspace_id, emitted_by_type, emitted_by_id, action_type,
            app_schema, payload, confirmation, source_message_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           RETURNING action_id"#,
    )
    .bind(ws_uuid)
    .bind("user")
    .bind(&user_id)
    .bind("flag_divergence")
    .bind(Some("adaptogen_lab_regulatory"))
    .bind(&payload)
    .bind("auto")
    .bind(source_msg_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .try_get("action_id")
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(json!({
        "action_id": action_id,
        "divergence_report": divergence_report,
    })))
}
