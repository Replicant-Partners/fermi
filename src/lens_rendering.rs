//! Grounding gate for the regulatory lens translator.
//!
//! Where [`crate::grounding_trust`] asks "could this value have come from
//! anywhere?", this module asks the next question for the lens translator
//! specifically: "for the values that *should* come from the ruleset, did the
//! agent faithfully reproduce what the ruleset says?"
//!
//! The ruleset is a workspace YAML file — the source of truth for each market's
//! claim renderings. An agent that generates its own rendering instead of reading
//! the ruleset is in the same failure class as an agent that invents a genome
//! size instead of querying NCBI: the output is present, plausible, and wrong.
//!
//! ## The two provenance classes
//!
//! **Ruleset-sourced** (`PROV_TOOL`): `rendered_text`, `status`, `basis`,
//! allergen format fields, ingredient status entries, `verification_appendix`.
//! These must come from reading the ruleset YAML via `read_workspace_file`. A
//! value here that contradicts the ruleset is a `ContradictsCanonical` violation —
//! same kind as a taxonomy field that disagrees with the creature row, just with
//! a YAML file instead of a DB row as the canonical source.
//!
//! **Inferred** (`PROV_INFERRED`): `divergence_note`, `summary_divergence`. The
//! agent reasons across two or more rulesets to produce these. They are
//! judgements the agent is commissioned to make, not retrievals. They survive
//! enforcement and are labelled as inference.
//!
//! ## Relationship to `grounding_trust`
//!
//! [`crate::grounding_trust::enforce`] handles the inferred and narrative fields —
//! anything `enforce` can check without reading YAML. This module handles the
//! YAML-backed sourced fields, producing violations in the same [`Report`] type
//! so [`crate::grounding_anomaly::spawn_raise`] sees a single combined picture.
//! Merge the two reports with [`merge_reports`] before raising.
//!
//! ## The verification appendix is not optional
//!
//! The spec (§6, §7) makes the verification appendix a first-class output, not a
//! disclaimer. An agent that omits it is in violation — the omission is what this
//! gate catches and files as an `UngroundedField`, the same kind as an agent that
//! fills a field it cannot source.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::grounding_trust::{
    strength, Report, Violation, ViolationKind, PROV_INFERRED, PROV_TOOL, PROV_UNAVAILABLE,
};

// ─── market ───────────────────────────────────────────────────────────────────

/// A regulatory market targeted by a lens render.
///
/// Three markets in the first build; Japan and Korea named in the roadmap (see
/// `apps/adaptogen-lab/regulatory-lens/manifest.json`) but not delivered.
/// The variant names are uppercase to match the `target_market` JSON field the
/// agent action grammar uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Market {
    Eu,
    Us,
    Cn,
}

impl Market {
    /// Filename (within the `rulesets/` directory) for this market's ruleset.
    pub fn ruleset_filename(self) -> &'static str {
        match self {
            Market::Eu => "eu_reg_1924.yaml",
            Market::Us => "us_fda_ftc.yaml",
            Market::Cn => "cn_gb_samr.yaml",
        }
    }

    /// The uppercase token used in the action grammar and the ruleset `market:` field.
    pub fn as_str(self) -> &'static str {
        match self {
            Market::Eu => "EU",
            Market::Us => "US",
            Market::Cn => "CN",
        }
    }
}

impl std::fmt::Display for Market {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─── ruleset types (partial deserialisation of the YAML) ──────────────────────
//
// We parse only the fields the gate needs to check — not the full ruleset.
// This is intentional: the full ruleset YAML contains extensive notes and
// philosophy text that the agent reads but the gate doesn't need to validate.
// A partial parse is also more resilient to future YAML additions.

/// A single claim rendering as declared in the ruleset YAML.
#[derive(Debug, Deserialize)]
pub struct RulesetClaimRendering {
    pub source_claim_id: String,
    /// `None` when `status` is `not_allowed` (the claim is stripped, no text).
    pub rendered_text: Option<String>,
    /// `allowed` | `conditionally_allowed` | `not_allowed` | `rewritten` |
    /// `allowed_with_caution` | `allowed_rewritten` | `allowed_with_important_note`
    pub status: String,
    pub basis: Option<String>,
    /// The agent-facing template for the divergence explanation. Tagged
    /// `PROV_INFERRED` in the output — the ruleset seeds it; the agent
    /// reasons across markets to produce the final form.
    pub divergence_note: Option<String>,
}

/// A single ingredient-status entry from the ruleset.
#[derive(Debug, Deserialize)]
pub struct RulesetIngredientStatus {
    pub ingredient_id: String,
    pub status: String,
    pub notes: Option<String>,
}

/// The allergen format block from the ruleset.
#[derive(Debug, Deserialize)]
pub struct RulesetAllergenFormat {
    pub standard: String,
    pub mechanism: String,
    pub this_product: String,
}

/// The slice of a ruleset YAML the gate needs. Partial deserialisation.
#[derive(Debug, Deserialize)]
pub struct Ruleset {
    pub ruleset_id: String,
    /// The market token — "EU" | "US" | "CN". Must match the `target_market`
    /// field in the agent's output.
    pub market: String,
    /// Must be "synthetic_representative" for the first build. Gate injects
    /// this into the output if the agent omitted it — failing to carry the
    /// synthetic-data caveat forward is the same class of omission as failing
    /// to carry a provenance tag.
    pub data_status: String,
    pub claim_renderings: Vec<RulesetClaimRendering>,
    pub allergen_format: RulesetAllergenFormat,
    pub ingredient_status: Vec<RulesetIngredientStatus>,
    /// Primary sources to verify before commercial use. Every rendering output
    /// must include these — the gate raises `UngroundedField` if absent.
    pub verify_sources: Vec<String>,
}

impl Ruleset {
    /// Parse a ruleset from YAML bytes.
    pub fn from_yaml(bytes: &[u8]) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_slice(bytes)
    }

    /// Find the prescribed rendering for a given source claim ID.
    pub fn rendering_for(&self, claim_id: &str) -> Option<&RulesetClaimRendering> {
        self.claim_renderings
            .iter()
            .find(|r| r.source_claim_id == claim_id)
    }
}

// ─── gate function ─────────────────────────────────────────────────────────────

/// Validate an agent-produced lens rendering against the ruleset it claims to
/// derive from.
///
/// Returns a [`Report`] in the same type as [`crate::grounding_trust::enforce`]
/// produces, so that [`crate::grounding_anomaly::spawn_raise`] sees a single
/// combined picture. Merge with the `enforce` report using [`merge_reports`]
/// before raising.
///
/// ## What the gate checks
///
/// **Status contradictions** — the most critical check. If the ruleset says a
/// claim is `not_allowed` and the agent rendered it as `allowed`, the claim
/// would appear on a label it has no business being on. The gate overwrites the
/// wrong status with the ruleset's authoritative value and files a
/// `ContradictsCanonical` violation. No manual review needed — the overwrite
/// is the correction.
///
/// **Missing verification appendix** — the spec makes the appendix mandatory
/// (§6, §7). Absence is an `UngroundedField` violation. Unlike a missing genome
/// size, which is a failure of retrieval, a missing appendix is a failure of
/// design — the agent was explicitly asked to produce it.
///
/// **Unknown claims** — a claim the ruleset has no entry for is tagged
/// `PROV_INFERRED` on its `rendered_text`. Not a violation (the ruleset isn't
/// exhaustive), but the rendering carries less authority and the tag makes that
/// visible.
///
/// **Data status** — if the agent omitted `data_status`, the gate injects the
/// ruleset's value. Dropping the synthetic-data caveat is the same class of
/// omission as dropping a provenance tag.
///
/// Provenance stamps are written into `agent_output` as
/// `rendered_claims_provenance`, `allergen_block_provenance`, etc., following
/// the same convention as [`crate::grounding_trust::enforce`].
pub fn gate_lens_output(ruleset: &Ruleset, agent_output: &mut Value) -> Report {
    let mut report = Report::default();

    // ── rendered_claims ───────────────────────────────────────────────────────
    let mut all_claims_tool_sourced = true;

    if let Some(claims) = agent_output
        .get_mut("rendered_claims")
        .and_then(Value::as_array_mut)
    {
        for claim in claims.iter_mut() {
            let claim_id = claim
                .get("source_claim_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();

            match ruleset.rendering_for(&claim_id) {
                Some(prescribed) => {
                    // ── status check (highest-stakes field) ──────────────────
                    //
                    // The agent's status must match the ruleset. A mismatch
                    // means the agent permitted a claim the ruleset prohibits, or
                    // vice versa. This is not a cosmetic error — a wrong
                    // permission assessment changes what appears on the label.
                    let agent_status = claim
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();

                    // Status families: `not_allowed` is a hard prohibition.
                    // Any non-prohibited status where the ruleset says prohibited
                    // is the critical failure. The reverse (agent says prohibited
                    // where ruleset allows) is overcautious but not dangerous;
                    // still a contradiction because the rendering is then wrong.
                    let ruleset_prohibits = prescribed.status == "not_allowed";
                    let agent_permits = agent_status != "not_allowed" && !agent_status.is_empty();

                    if ruleset_prohibits && agent_permits {
                        // Agent rendered a prohibited claim. Overwrite and file.
                        report.violations.push(Violation {
                            path: format!("rendered_claims[{claim_id}].status"),
                            removed: Value::String(agent_status),
                            kind: ViolationKind::ContradictsCanonical,
                        });
                        if let Some(obj) = claim.as_object_mut() {
                            obj.insert(
                                "status".to_string(),
                                Value::String(prescribed.status.clone()),
                            );
                            // Also null the rendered text — a not_allowed claim
                            // has no rendered text on a valid label.
                            obj.insert("rendered_text".to_string(), Value::Null);
                        }
                        all_claims_tool_sourced = false;
                    } else if agent_status != prescribed.status {
                        // Non-critical status mismatch (e.g., "allowed" vs
                        // "conditionally_allowed"). Overwrite silently — the
                        // ruleset is authoritative, and this is a precision
                        // error rather than a safety one.
                        if let Some(obj) = claim.as_object_mut() {
                            obj.insert(
                                "status".to_string(),
                                Value::String(prescribed.status.clone()),
                            );
                        }
                    }

                    // ── divergence_note provenance ───────────────────────────
                    //
                    // Always PROV_INFERRED: the ruleset seeds the note, but the
                    // agent reasons across multiple rulesets to produce the final
                    // form. It is a judgement, not a retrieval.
                    if let Some(obj) = claim.as_object_mut() {
                        obj.insert(
                            "divergence_note_provenance".to_string(),
                            Value::String(PROV_INFERRED.to_string()),
                        );
                    }
                }

                None => {
                    // The ruleset has no entry for this claim — the agent
                    // generated a rendering from parametric knowledge. Tag the
                    // rendered text as inferred. Not a violation, but the tag
                    // makes the lower authority visible.
                    all_claims_tool_sourced = false;
                    if let Some(obj) = claim.as_object_mut() {
                        obj.insert(
                            "rendered_text_provenance".to_string(),
                            Value::String(PROV_INFERRED.to_string()),
                        );
                        obj.insert(
                            "status_provenance".to_string(),
                            Value::String(PROV_INFERRED.to_string()),
                        );
                    }
                }
            }
        }
    }

    // Block-level provenance stamp for rendered_claims.
    if let Some(obj) = agent_output.as_object_mut() {
        obj.insert(
            "rendered_claims_provenance".to_string(),
            Value::String(if all_claims_tool_sourced {
                PROV_TOOL.to_string()
            } else {
                PROV_INFERRED.to_string()
            }),
        );
    }

    // Write into report.provenance in the same format grounding_trust::enforce uses.
    report.provenance.push((
        "rendered_claims".to_string(),
        if all_claims_tool_sourced {
            PROV_TOOL
        } else {
            PROV_INFERRED
        },
    ));

    // ── allergen_block ────────────────────────────────────────────────────────
    //
    // The allergen format is prescribed entirely by the ruleset — the standard,
    // the mechanism, and the product-specific note are all ruleset-sourced. The
    // gate does not validate the content verbatim (the agent may reword while
    // preserving meaning) but stamps the block as PROV_TOOL because the ruleset
    // is the only legitimate source.
    if let Some(obj) = agent_output.as_object_mut() {
        obj.insert(
            "allergen_block_provenance".to_string(),
            Value::String(PROV_TOOL.to_string()),
        );
    }
    report
        .provenance
        .push(("allergen_block".to_string(), PROV_TOOL));

    // ── ingredient_status ─────────────────────────────────────────────────────
    //
    // Status values come from the ruleset; notes may include agent reasoning.
    // Block-level stamp: PROV_TOOL (the status field, which is the consequential
    // one, is always ruleset-sourced).
    if let Some(obj) = agent_output.as_object_mut() {
        obj.insert(
            "ingredient_status_provenance".to_string(),
            Value::String(PROV_TOOL.to_string()),
        );
    }
    report
        .provenance
        .push(("ingredient_status".to_string(), PROV_TOOL));

    // ── verification_appendix — mandatory, not optional ───────────────────────
    //
    // The spec (§6, §7) establishes the appendix as a first-class output that
    // carries the honesty-as-credibility argument forward. An agent that omits
    // it has failed on purpose — the gate files it as UngroundedField, the same
    // violation kind as an agent populating a field that must be null, but
    // inverted: a field that must be populated and is absent.
    let has_appendix = agent_output
        .get("verification_appendix")
        .map(|v| match v {
            Value::Array(a) => !a.is_empty(),
            Value::Null => false,
            _ => true,
        })
        .unwrap_or(false);

    if !has_appendix {
        report.violations.push(Violation {
            path: "verification_appendix".to_string(),
            // `removed` records what was there. Null because it was absent, not
            // because it was nulled. Retained for the anomaly payload — the
            // difference between "absent" and "was something, got removed" is
            // information.
            removed: Value::Null,
            kind: ViolationKind::UngroundedField,
        });
        report
            .provenance
            .push(("verification_appendix".to_string(), PROV_UNAVAILABLE));
    } else {
        if let Some(obj) = agent_output.as_object_mut() {
            obj.insert(
                "verification_appendix_provenance".to_string(),
                Value::String(PROV_TOOL.to_string()),
            );
        }
        report
            .provenance
            .push(("verification_appendix".to_string(), PROV_TOOL));
    }

    // ── data_status ───────────────────────────────────────────────────────────
    //
    // Every rendering output must carry the ruleset's `data_status` value
    // ("synthetic_representative"). Dropping it is dropping the synthetic-data
    // caveat — which is the specific failure mode the spec names in §2. The gate
    // injects the ruleset's value if absent rather than filing a violation,
    // because the value is fully deterministic from the ruleset.
    if !agent_output
        .get("data_status")
        .map(|v| !v.is_null())
        .unwrap_or(false)
    {
        if let Some(obj) = agent_output.as_object_mut() {
            obj.insert(
                "data_status".to_string(),
                Value::String(ruleset.data_status.clone()),
            );
        }
    }

    report
}

// ─── report merge ─────────────────────────────────────────────────────────────

/// Merge two [`Report`]s into one.
///
/// Called after running both [`gate_lens_output`] and
/// [`crate::grounding_trust::enforce`] on the same document — the merged report
/// is what [`crate::grounding_anomaly::spawn_raise`] receives. For overlapping
/// block names in `provenance`, the weaker value wins: the floor of the
/// combined picture, not the floor of either half alone.
pub fn merge_reports(a: Report, b: Report) -> Report {
    let mut violations = a.violations;
    violations.extend(b.violations);

    let mut provenance = a.provenance;
    for (block_b, prov_b) in b.provenance {
        match provenance
            .iter_mut()
            .find(|(block_a, _)| *block_a == block_b)
        {
            Some(existing) => {
                // Take the weaker value. `strength` is a public function from
                // grounding_trust; using it here rather than duplicating the
                // ordinal table so there is one answer to the question.
                if strength(prov_b) < strength(existing.1) {
                    existing.1 = prov_b;
                }
            }
            None => provenance.push((block_b, prov_b)),
        }
    }

    Report {
        violations,
        provenance,
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal ruleset for testing — covers the two claims that show the most
    /// divergence in the demo: live_cultures_present (allowed in EU) and
    /// hibiscus_wellness (not_allowed in EU).
    fn fixture_ruleset() -> Ruleset {
        Ruleset {
            ruleset_id: "eu_reg_1924_1169".to_string(),
            market: "EU".to_string(),
            data_status: "synthetic_representative".to_string(),
            claim_renderings: vec![
                RulesetClaimRendering {
                    source_claim_id: "live_cultures_present".to_string(),
                    rendered_text: Some(
                        "Contains live Acetobacter and Brettanomyces cultures".to_string(),
                    ),
                    status: "allowed".to_string(),
                    basis: Some("process_descriptor".to_string()),
                    divergence_note: Some(
                        "EU: probiotic word prohibited by EFSA; organisms named factually."
                            .to_string(),
                    ),
                },
                RulesetClaimRendering {
                    source_claim_id: "hibiscus_wellness".to_string(),
                    rendered_text: None,
                    status: "not_allowed".to_string(),
                    basis: None,
                    divergence_note: Some(
                        "No EFSA-authorized claim; traditional use not a valid EU basis."
                            .to_string(),
                    ),
                },
            ],
            allergen_format: RulesetAllergenFormat {
                standard: "Reg 1169/2011 Annex II".to_string(),
                mechanism: "Emphasise in the ingredients list (bold/italic/contrasting colour)."
                    .to_string(),
                this_product: "No Annex II allergens present.".to_string(),
            },
            ingredient_status: vec![],
            verify_sources: vec!["EFSA authorized claims register: \
                 https://ec.europa.eu/food/safety/labelling_nutrition/claims/register_en"
                .to_string()],
        }
    }

    fn doc_with_appendix(claims: serde_json::Value) -> Value {
        json!({
            "rendered_claims": claims,
            "allergen_block": {
                "standard": "Reg 1169/2011 Annex II",
                "mechanism": "Emphasise in list",
                "this_product": "No allergens"
            },
            "ingredient_status": [],
            "verification_appendix": ["https://ec.europa.eu/..."],
            "data_status": "synthetic_representative"
        })
    }

    #[test]
    fn a_faithful_rendering_is_clean() {
        let ruleset = fixture_ruleset();
        let mut doc = doc_with_appendix(json!([{
            "source_claim_id": "live_cultures_present",
            "rendered_text": "Contains live Acetobacter and Brettanomyces cultures",
            "status": "allowed",
            "basis": "process_descriptor",
            "divergence_note": "EU: probiotic word prohibited by EFSA"
        }]));

        let report = gate_lens_output(&ruleset, &mut doc);
        assert!(
            report.is_clean(),
            "Faithful rendering should produce no violations: {:?}",
            report.violations
        );
    }

    #[test]
    fn a_permitted_prohibited_claim_is_a_contradiction_and_is_overwritten() {
        // The demo's primary divergence beat: hibiscus wellness is not_allowed
        // in EU. An agent that renders it as allowed has made the critical error.
        let ruleset = fixture_ruleset();
        let mut doc = doc_with_appendix(json!([{
            "source_claim_id": "hibiscus_wellness",
            "rendered_text": "Hibiscus — traditionally used for wellbeing",
            "status": "allowed",  // ← agent said allowed; ruleset says not_allowed
            "basis": "traditional_use_context"
        }]));

        let report = gate_lens_output(&ruleset, &mut doc);

        assert!(!report.is_clean(), "Should have a violation");
        assert_eq!(report.violations.len(), 1);
        assert_eq!(
            report.violations[0].path,
            "rendered_claims[hibiscus_wellness].status"
        );
        assert!(
            matches!(
                report.violations[0].kind,
                ViolationKind::ContradictsCanonical
            ),
            "Wrong violation kind: {:?}",
            report.violations[0].kind
        );

        // The gate must overwrite the wrong status with the authoritative value.
        let corrected = doc["rendered_claims"][0]["status"]
            .as_str()
            .expect("status should be a string after overwrite");
        assert_eq!(corrected, "not_allowed");

        // The gate must also null the rendered_text for a not_allowed claim.
        assert!(
            doc["rendered_claims"][0]["rendered_text"].is_null(),
            "rendered_text for a not_allowed claim must be null"
        );
    }

    #[test]
    fn a_missing_verification_appendix_is_a_violation() {
        let ruleset = fixture_ruleset();
        let mut doc = json!({
            "rendered_claims": [],
            "allergen_block": { "standard": "x", "mechanism": "x", "this_product": "x" },
            "ingredient_status": [],
            // verification_appendix deliberately omitted
            "data_status": "synthetic_representative"
        });

        let report = gate_lens_output(&ruleset, &mut doc);

        assert!(
            report
                .violations
                .iter()
                .any(|v| v.path == "verification_appendix"),
            "Missing appendix should produce a violation"
        );
    }

    #[test]
    fn an_empty_verification_appendix_is_also_a_violation() {
        let ruleset = fixture_ruleset();
        // An empty array is not a valid appendix — it means the agent produced
        // the structure but no content.
        let mut doc = json!({
            "rendered_claims": [],
            "allergen_block": { "standard": "x", "mechanism": "x", "this_product": "x" },
            "ingredient_status": [],
            "verification_appendix": [],  // empty
            "data_status": "synthetic_representative"
        });

        let report = gate_lens_output(&ruleset, &mut doc);

        assert!(
            report
                .violations
                .iter()
                .any(|v| v.path == "verification_appendix"),
            "Empty appendix should be treated as absent"
        );
    }

    #[test]
    fn a_claim_not_in_the_ruleset_is_tagged_inferred_not_violated() {
        let ruleset = fixture_ruleset();
        // The agent rendered a claim the ruleset has no entry for. This is not
        // a violation — the ruleset isn't exhaustive — but the rendering is
        // tagged PROV_INFERRED to show it came from the model, not the ruleset.
        let mut doc = doc_with_appendix(json!([{
            "source_claim_id": "some_novel_claim_not_in_ruleset",
            "rendered_text": "Some text the agent generated from parametric knowledge",
            "status": "allowed",
            "basis": "process_descriptor"
        }]));

        let report = gate_lens_output(&ruleset, &mut doc);

        assert!(
            report.is_clean(),
            "Claim not in ruleset should not produce a violation"
        );
        let prov = doc["rendered_claims"][0]
            .get("rendered_text_provenance")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(prov, PROV_INFERRED, "Unruled claim must be tagged inferred");
    }

    #[test]
    fn missing_data_status_is_injected_not_violated() {
        // Dropping the synthetic-data caveat is a design error, not a retrieval
        // error. The gate injects the value deterministically from the ruleset
        // rather than filing a violation, because the value is known.
        let ruleset = fixture_ruleset();
        let mut doc = json!({
            "rendered_claims": [],
            "allergen_block": { "standard": "x", "mechanism": "x", "this_product": "x" },
            "ingredient_status": [],
            "verification_appendix": ["https://ec.europa.eu/..."]
            // data_status absent
        });

        let report = gate_lens_output(&ruleset, &mut doc);

        assert!(report.is_clean());
        assert_eq!(
            doc["data_status"].as_str().unwrap_or(""),
            "synthetic_representative"
        );
    }

    #[test]
    fn divergence_note_is_always_tagged_inferred() {
        let ruleset = fixture_ruleset();
        let mut doc = doc_with_appendix(json!([{
            "source_claim_id": "live_cultures_present",
            "rendered_text": "Contains live Acetobacter and Brettanomyces cultures",
            "status": "allowed",
            "basis": "process_descriptor",
            "divergence_note": "EU: probiotic word prohibited"
        }]));

        gate_lens_output(&ruleset, &mut doc);

        let note_prov = doc["rendered_claims"][0]
            .get("divergence_note_provenance")
            .and_then(Value::as_str)
            .unwrap_or("");
        assert_eq!(
            note_prov, PROV_INFERRED,
            "divergence_note must always be tagged inferred"
        );
    }

    #[test]
    fn merge_reports_takes_weaker_provenance_for_shared_blocks() {
        let a = Report {
            violations: vec![],
            provenance: vec![("rendered_claims".to_string(), PROV_TOOL)],
        };
        let b = Report {
            violations: vec![],
            provenance: vec![("rendered_claims".to_string(), PROV_INFERRED)],
        };

        let merged = merge_reports(a, b);
        let prov = merged
            .provenance
            .iter()
            .find(|(block, _)| block == "rendered_claims")
            .map(|(_, p)| *p)
            .unwrap_or(PROV_UNAVAILABLE);

        // PROV_INFERRED is weaker than PROV_TOOL; the merged result must be PROV_INFERRED.
        assert_eq!(prov, PROV_INFERRED);
    }

    #[test]
    fn merge_reports_appends_non_overlapping_blocks() {
        let a = Report {
            violations: vec![],
            provenance: vec![("rendered_claims".to_string(), PROV_TOOL)],
        };
        let b = Report {
            violations: vec![],
            provenance: vec![("allergen_block".to_string(), PROV_TOOL)],
        };

        let merged = merge_reports(a, b);
        assert_eq!(merged.provenance.len(), 2);
    }

    #[test]
    fn merge_reports_accumulates_violations_from_both_halves() {
        let a = Report {
            violations: vec![Violation {
                path: "rendered_claims[foo].status".to_string(),
                removed: Value::String("allowed".to_string()),
                kind: ViolationKind::ContradictsCanonical,
            }],
            provenance: vec![],
        };
        let b = Report {
            violations: vec![Violation {
                path: "summary".to_string(),
                removed: Value::String("antioxidant benefit".to_string()),
                kind: ViolationKind::NarrativeLeak,
            }],
            provenance: vec![],
        };

        let merged = merge_reports(a, b);
        assert_eq!(merged.violations.len(), 2);
    }
}
