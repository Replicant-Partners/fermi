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
// ─── Shared helpers ────────────────────────────────────────────

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

// ─── stored agent evaluations ────────────────────────────────────────────────

/// One market's verdict for one claim, as written by
/// [`super::claim_evaluation`] after the grounding gate ran.
///
/// This is the primary source for a comparison. The ruleset YAMLs are a
/// fallback and a weaker one: they are hand-authored and declared
/// `synthetic_representative`, so a row served from them is a human's guess
/// about a regime, not a reading of it.
#[derive(Debug, serde::Deserialize)]
struct StoredEvaluation {
    status: String,
    #[serde(default)]
    basis: Option<String>,
    #[serde(default)]
    rendered_text: Option<String>,
    #[serde(default)]
    needs_expert: Option<bool>,
    #[serde(default)]
    provenance: StoredProvenance,
    #[serde(default)]
    citations: Vec<Value>,
    /// The searches that were actually issued.
    ///
    /// Carried through because it is a substantial part of why a reader should
    /// believe a verdict — "we searched the EFSA register for this wording and
    /// it returned nothing" is a much stronger statement than "not permitted",
    /// and it is the only field that makes a `tool_no_match` legible as work
    /// done rather than work skipped.
    #[serde(default)]
    queries_run: Vec<Value>,
    #[serde(default)]
    explanation: Option<String>,
    #[serde(default)]
    evaluated_at: Option<String>,
    /// A qualified reviewer's sign-off, or `None` if nobody has signed.
    /// Distinct from a rejection, which is why this is not a bool.
    #[serde(default)]
    endorsement: Option<Value>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct StoredProvenance {
    #[serde(default)]
    verdict: Option<String>,
    #[serde(default)]
    evidence: Option<String>,
}

/// Read a stored evaluation, if the evaluator has produced one.
///
/// Workspace git only — no platform fallback, deliberately. An evaluation is
/// something this workspace's agent did for this workspace's claim, against
/// the corpus at a particular moment. A shipped copy would be exactly the
/// pre-baked answer this handler is being fixed to stop serving.
async fn read_stored_evaluation(
    state: &AppState,
    slug: &str,
    market: Market,
    claim_id: &str,
) -> Option<StoredEvaluation> {
    let path = format!(
        "regulatory-lens/ontology/evaluated/{}/{claim_id}.yaml",
        market.as_str().to_lowercase()
    );
    let git = state.workspace_git.clone();
    let slug_s = slug.to_string();
    let bytes = tokio::task::spawn_blocking(move || git.read_file_bytes(&slug_s, &path).ok())
        .await
        .ok()
        .flatten()?;
    serde_yaml::from_slice(&bytes).ok()
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

    // Soft-fail on log INSERT (see compare_lenses_handler — migration 232 pending).
    let action_id = sqlx::query(
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
    .ok()
    .and_then(|r| r.try_get::<Uuid, _>("action_id").ok())
    .unwrap_or_else(Uuid::new_v4);

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
    /// Fall back to the hand-authored ruleset YAMLs for claims that have no
    /// stored evaluation.
    ///
    /// **Defaults to false, and that is a deliberate behaviour change.**
    ///
    /// The rulesets declare `data_status: synthetic_representative` — they are
    /// a person's sketch of what each regime would say, written to demonstrate
    /// the shape of the divergence. Serving them as the default made the app
    /// look like it had evaluated seven claims when it had looked up five
    /// hardcoded ids and could never have matched the other two, because the
    /// UI mints claim ids as `claim_<timestamp>`.
    ///
    /// So the default is now: a claim is `not_evaluated` until the evaluator
    /// has actually evaluated it. Pass `true` to see the seed rows, which
    /// arrive tagged `synthetic_seed` and must be rendered as such.
    #[serde(default)]
    pub include_synthetic_seed: bool,
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
    // Composition fallback chain:
    //   1. workspace: dpp/composition.yaml              (new DPP layout)
    //   2. workspace: regulatory-lens/sku/{id}.yaml     (legacy)
    //   3. platform:  apps/adaptogen-lab/dpp/composition.yaml  (new)
    //   4. platform:  apps/adaptogen-lab/regulatory-lens/sku/{id}.yaml  (legacy)
    let ws_comp_path = format!("regulatory-lens/sku/{product_id}.yaml");
    let platform_comp_path = format!("apps/adaptogen-lab/regulatory-lens/sku/{product_id}.yaml");
    let git = state.workspace_git.clone();
    let slug_c = slug.clone();
    let comp_bytes = tokio::task::spawn_blocking(move || {
        git.read_file_bytes(&slug_c, "dpp/composition.yaml")
            .or_else(|_| git.read_file_bytes(&slug_c, &ws_comp_path))
            .or_else(|_| {
                std::fs::read("apps/adaptogen-lab/dpp/composition.yaml").map_err(|e| {
                    agent_bestiary_ontology::OntologyError::RepoNotFound(e.to_string())
                })
            })
            .or_else(|_| {
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

    // Try to read source_claims from a separate claims.yaml first.
    // This is the new architecture: BOM and claims are separate documents.
    // Falls back to source_claims embedded in the composition YAML for compatibility.
    let claims_bytes = {
        let git2 = state.workspace_git.clone();
        let slug2 = slug.clone();
        tokio::task::spawn_blocking(move || {
            git2.read_file_bytes(&slug2, "dpp/claims.yaml")
                .or_else(|_| {
                    std::fs::read("apps/adaptogen-lab/dpp/claims.yaml").map_err(|e| {
                        agent_bestiary_ontology::OntologyError::RepoNotFound(e.to_string())
                    })
                })
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok() // None if not found — fall back to composition
    };

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
        } else if let Some(ref cb) = claims_bytes {
            // New: read source_claims from the separate claims.yaml document.
            let claims_doc: serde_json::Value =
                serde_yaml::from_slice(cb).unwrap_or(serde_json::Value::Null);
            claims_doc
                .get("source_claims")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default()
        } else {
            // Legacy: source_claims embedded directly in the composition YAML.
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

        let candidate_text = claim
            .get("candidate_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let mut market_rows: Vec<Value> = Vec::new();
        let mut status_strs: Vec<String> = Vec::new();

        // ── Where a row comes from, in order of authority ─────────────────────
        //
        //   1. a stored agent evaluation — the evaluator read the live corpus
        //      for THIS claim and the grounding gate passed on the result
        //   2. the ruleset YAML, only if the caller asked for it — a
        //      hand-authored synthetic sketch, tagged as one
        //   3. not_evaluated — nobody has looked yet
        //
        // What changed here, and why: this loop used to consult only (2), so a
        // claim absent from the YAML came back `not_in_ruleset` with no way to
        // ever become anything else, and the UI's prompt to "run
        // regulatory_scanner" was text with no code behind it. The evaluator
        // now has an endpoint (`POST /actions/evaluate_claims`) and writes
        // here, so (1) exists and this reads it.
        //
        // Still NOT the place for a prohibited-pattern table. A rule list in
        // this file would be the synthetic ruleset again, one layer down and
        // harder to see. The corpus decides, the agent judges, this reads.
        for (market, rs) in market_rulesets.iter() {
            if !active_markets.contains(market) {
                continue;
            }

            if let Some(ev) = read_stored_evaluation(&state, &slug, *market, &claim_id).await {
                status_strs.push(ev.status.clone());
                // `endorsement` outranks the agent's own stamp: a qualified
                // reviewer having signed the verdict is a stronger fact than
                // how the verdict was reached.
                let verdict_prov = if ev.endorsement.as_ref().is_some_and(|e| !e.is_null()) {
                    grounding_trust::PROV_HUMAN_ENDORSED.to_string()
                } else {
                    ev.provenance
                        .verdict
                        .clone()
                        .unwrap_or_else(|| grounding_trust::PROV_INFERRED.to_string())
                };
                market_rows.push(json!({
                    "market": market.as_str(),
                    "status": ev.status,
                    "rendered_text": ev.rendered_text,
                    "basis": ev.basis,
                    "divergence_note": ev.explanation,
                    "needs_expert": ev.needs_expert.unwrap_or(true),
                    // Two stamps, never collapsed into one. The verdict is a
                    // reading of the evidence and the evidence is what a tool
                    // returned; a surface that shows one number for both is
                    // claiming the reading was retrieved.
                    "source": "agent_evaluation",
                    "provenance": {
                        "verdict": verdict_prov,
                        "evidence": ev.provenance.evidence
                            .clone()
                            .unwrap_or_else(|| grounding_trust::PROV_UNAVAILABLE.to_string()),
                    },
                    "citations": ev.citations,
                    "queries_run": ev.queries_run,
                    "evaluated_at": ev.evaluated_at,
                    "endorsed": ev.endorsement.as_ref().is_some_and(|e| !e.is_null()),
                }));
                continue;
            }

            match rs
                .rendering_for(&claim_id)
                .filter(|_| req.include_synthetic_seed)
            {
                Some(r) => {
                    status_strs.push(r.status.clone());
                    market_rows.push(json!({
                        "market": market.as_str(),
                        "status": r.status,
                        "rendered_text": r.rendered_text,
                        "basis": r.basis,
                        "divergence_note": r.divergence_note,
                        "needs_expert": true,
                        // Named so a surface cannot render this like an
                        // evaluated row. It is a hand-authored sketch of the
                        // regime, and the ruleset says so in `data_status`.
                        "source": "synthetic_seed",
                        "provenance": {
                            "verdict": "synthetic_seed",
                            "evidence": grounding_trust::PROV_UNAVAILABLE,
                        },
                        // Emitted on every branch, so a consumer never has to
                        // tell an absent key from an empty one.
                        "citations": [],
                        "queries_run": [],
                        "evaluated_at": null,
                        "endorsed": false,
                    }));
                }
                None => {
                    // `not_evaluated`, not `not_in_ruleset`. The old token
                    // described where the handler had looked; this one
                    // describes what is true of the claim, and it is the token
                    // the evaluator also emits, so one vocabulary covers both.
                    status_strs.push("not_evaluated".to_string());
                    market_rows.push(json!({
                        "market": market.as_str(),
                        "status": "not_evaluated",
                        "rendered_text": null,
                        "basis": null,
                        "divergence_note": null,
                        "needs_expert": true,
                        "source": "none",
                        "provenance": {
                            "verdict": grounding_trust::PROV_UNAVAILABLE,
                            "evidence": grounding_trust::PROV_UNAVAILABLE,
                        },
                        "citations": [],
                        "queries_run": [],
                        "evaluated_at": null,
                        "endorsed": false,
                        "action_required": "evaluate_claims",
                    }));
                }
            }
        }

        let status_refs: Vec<&str> = status_strs.iter().map(|s| s.as_str()).collect();
        let is_demo_beat = claim_id == "hibiscus_wellness" || claim_id == "live_cultures_present";

        // How many of the markets in scope actually have a verdict.
        let evaluated_count = market_rows
            .iter()
            .filter(|r| r.get("status").and_then(|s| s.as_str()) != Some("not_evaluated"))
            .count();
        let fully_evaluated = evaluated_count == market_rows.len() && evaluated_count > 0;

        // A divergence score across unknowns is not a low score, it is not a
        // score. Three `not_evaluated` markets agree perfectly and mean
        // nothing; emitting 0 there rendered as "no divergence" — an answer —
        // when the truth is that nobody has looked. Null instead, so a surface
        // has to decide what to show rather than being handed a reassuring
        // number.
        let score = if fully_evaluated {
            Some(divergence_score(&status_refs))
        } else {
            None
        };

        comparison_table.push(json!({
            "claim_id": claim_id,
            "candidate_text": claim.get("candidate_text"),
            "claim_pressure": claim.get("claim_pressure"),
            "markets": market_rows,
            "divergence_score": score,
            "evaluated_markets": evaluated_count,
            "markets_in_scope": market_rows.len(),
            "fully_evaluated": fully_evaluated,
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

    // What the rows in this response actually rest on.
    //
    // `data_status` used to be copied straight from the EU ruleset, so every
    // response said `synthetic_representative` regardless of what it
    // contained. Now it reports the mix, because with the evaluator wired up a
    // single response can legitimately carry agent-evaluated rows, synthetic
    // seed rows and unevaluated ones at once, and a single label for all three
    // is false whichever one it picks.
    let mut row_sources: std::collections::BTreeMap<String, usize> = Default::default();
    for row in comparison_table
        .iter()
        .filter_map(|c| c.get("markets").and_then(|m| m.as_array()))
        .flatten()
    {
        let src = row
            .get("source")
            .and_then(|s| s.as_str())
            .unwrap_or("none")
            .to_string();
        *row_sources.entry(src).or_insert(0) += 1;
    }
    let has_synthetic = row_sources.contains_key("synthetic_seed");
    let has_evaluated = row_sources.contains_key("agent_evaluation");
    let data_status = match (has_evaluated, has_synthetic) {
        (true, true) => "mixed_agent_evaluated_and_synthetic_seed",
        (true, false) => "agent_evaluated_against_live_corpus",
        (false, true) => "synthetic_representative",
        (false, false) => "unevaluated",
    };

    let mut combined_output = json!({
        "source_product_id": product_id,
        "comparison_table": comparison_table,
        "primary_demo_beat": primary_demo_beat,
        "ingredient_divergence_beat": {
            "ingredient_id": "hibiscus",
            "markets": ingredient_divergence,
        },
        "verification_appendix": eu_rs.verify_sources,
        "data_status": data_status,
        "row_sources": row_sources,
        "ruleset_data_status": eu_rs.data_status,
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

    // Soft-fail on action log INSERT: the comparison result is the product;
    // the log entry is auditing infrastructure. If the constraint hasn't been
    // updated yet (migration 232 pending), we still return the comparison.
    let action_id = sqlx::query(
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
    .ok()
    .and_then(|r| r.try_get::<Uuid, _>("action_id").ok())
    .unwrap_or_else(Uuid::new_v4); // fallback id if log insert fails

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
    .ok()
    .and_then(|r| r.try_get::<Uuid, _>("action_id").ok())
    .unwrap_or_else(Uuid::new_v4); // soft-fail, see migration 232

    Ok(Json(json!({
        "action_id": action_id,
        "divergence_report": divergence_report,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// **The seam between the two handlers.**
    ///
    /// `claim_evaluation` writes the per-market YAML; this module reads it back
    /// to serve a comparison. Nothing else connects them — not a shared type,
    /// not a schema, just a file path and a field layout agreed in two places.
    ///
    /// That is exactly the shape of the bug this app already had once. The UI
    /// posted `{content, agent}` to a handler whose request struct had no
    /// `agent` field, serde dropped it silently, and the feature looked wired
    /// for as long as nobody checked. A writer and a reader that disagree about
    /// a key behave identically: the value simply arrives as `None`, the row
    /// renders as unevaluated, and the evaluation that cost real search calls
    /// is invisible with no error anywhere.
    ///
    /// So this test does not check the YAML looks reasonable. It checks that
    /// the bytes the writer actually produces deserialise into the reader's
    /// struct with every consequential field populated.
    #[test]
    fn a_written_evaluation_is_readable_by_the_comparison_handler() {
        let doc = json!({
            "claim_id": "claim_1788945073542",
            "candidate_text": "improves gut health",
            "eu": {
                "status": "not_allowed",
                "basis": "Reg 1924/2006 Art. 10(1)",
                "rendered_text": null,
                "needs_expert": false
            },
            "eu_evidence": {
                "queries_run": ["EFSA register gut health"],
                "citations": [{
                    "url": "https://ec.europa.eu/food/safety/labelling_nutrition/claims/register_en",
                    "title": "EU Register of nutrition and health claims",
                    "snippet": "Only authorised claims may be made.",
                    "provision": "Art. 10(1)"
                }]
            },
            "explanation": "Prohibited as worded in the EU.",
            "eu_provenance": "model_inference",
            "eu_evidence_provenance": "tool_verified"
        });

        let yaml = super::super::claim_evaluation::evaluation_yaml(
            &doc,
            "claim_1788945073542",
            "eu",
            Uuid::nil(),
            "model_inference",
            "tool_verified",
        );

        let read: StoredEvaluation = serde_yaml::from_str(&yaml)
            .expect("the comparison handler cannot read what the evaluator wrote");

        assert_eq!(read.status, "not_allowed");
        assert_eq!(read.basis.as_deref(), Some("Reg 1924/2006 Art. 10(1)"));
        assert_eq!(read.rendered_text, None, "a prohibited claim has no text");
        assert_eq!(read.needs_expert, Some(false));
        assert_eq!(read.citations.len(), 1, "the citation did not survive");

        // The two stamps must arrive separately. Collapsing them is the one
        // thing the document's shape exists to prevent.
        assert_eq!(read.provenance.verdict.as_deref(), Some("model_inference"));
        assert_eq!(read.provenance.evidence.as_deref(), Some("tool_verified"));

        // Nobody has signed this off, and that must be distinguishable from a
        // rejection rather than collapsing to `false`.
        assert!(
            read.endorsement.is_none() || read.endorsement == Some(Value::Null),
            "an unsigned evaluation must not read as endorsed"
        );
        assert!(read.evaluated_at.is_some(), "no timestamp was written");
    }

    /// A searched-and-found-nothing market must not read as a clearance once it
    /// has been through the file.
    #[test]
    fn an_empty_search_survives_the_round_trip_as_a_gap() {
        let doc = json!({
            "candidate_text": "proven to improve erectile dysfunction",
            "cn": {
                "status": "not_evaluated",
                "basis": null,
                "rendered_text": null,
                "needs_expert": true
            },
            "cn_evidence": { "queries_run": ["SAMR ..."], "citations": [] }
        });

        let yaml = super::super::claim_evaluation::evaluation_yaml(
            &doc,
            "claim_1788945159688",
            "cn",
            Uuid::nil(),
            "model_inference",
            "tool_no_match",
        );

        let read: StoredEvaluation = serde_yaml::from_str(&yaml).expect("unreadable");
        assert_eq!(read.status, "not_evaluated");
        assert!(read.citations.is_empty());
        assert_eq!(read.provenance.evidence.as_deref(), Some("tool_no_match"));
        assert_eq!(
            read.needs_expert,
            Some(true),
            "a gap that does not ask for a human is worse than no gap"
        );
    }

    /// **The non-terminating drain.**
    ///
    /// A caller pages by re-requesting while `remaining > 0`. If the evaluator
    /// omits a market block, and the writer therefore writes no file, that
    /// claim stays a candidate forever and the queue never empties — the
    /// client can only discover this by giving up. So a market the evaluator
    /// did not answer for must still produce a readable record.
    ///
    /// It must also record which of the two things happened. "Asked, got no
    /// answer for CN" and "nobody has asked yet" are different facts, and
    /// collapsing them is the failure this whole pipeline is built to avoid.
    #[test]
    fn a_market_the_evaluator_skipped_is_still_recorded() {
        // A reply with EU only. `cn` is absent entirely.
        let doc = json!({
            "candidate_text": "low in sugar",
            "eu": { "status": "conditionally_allowed", "needs_expert": true },
            "eu_evidence": { "citations": [] }
        });

        let yaml = super::super::claim_evaluation::evaluation_yaml(
            &doc,
            "low_sugar",
            "cn",
            Uuid::nil(),
            "unavailable_no_tool_source",
            "unavailable_no_tool_source",
        );

        let read: StoredEvaluation = serde_yaml::from_str(&yaml)
            .expect("a skipped market produced nothing readable — the drain cannot terminate");
        assert_eq!(
            read.status, "not_evaluated",
            "a market with no verdict must not inherit one"
        );
        assert!(read.citations.is_empty());
        assert_eq!(
            read.needs_expert,
            Some(true),
            "an unanswered market must ask for a human"
        );

        // And the record says the evaluator returned nothing for it, rather
        // than looking like a considered `not_evaluated`.
        let raw: serde_yaml::Value = serde_yaml::from_str(&yaml).expect("parse");
        assert_eq!(
            raw.get("market_block_returned").and_then(|v| v.as_bool()),
            Some(false)
        );

        // The market the evaluator DID answer for is marked the other way.
        let eu = super::super::claim_evaluation::evaluation_yaml(
            &doc,
            "low_sugar",
            "eu",
            Uuid::nil(),
            "model_inference",
            "tool_no_match",
        );
        let raw_eu: serde_yaml::Value = serde_yaml::from_str(&eu).expect("parse");
        assert_eq!(
            raw_eu.get("market_block_returned").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    /// `needs_expert` must default to the cautious value when the key is
    /// missing, not to `false` via `Option::unwrap_or_default`.
    #[test]
    fn a_missing_expert_flag_does_not_default_to_safe() {
        let yaml = "status: allowed\ncitations: []\n";
        let read: StoredEvaluation = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(read.needs_expert, None);
        // The handler substitutes `true` for `None` when building the row.
        assert!(
            read.needs_expert.unwrap_or(true),
            "an absent flag must be read as needing review"
        );
    }
}
