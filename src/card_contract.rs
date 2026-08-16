//! # Card-declared contracts — what an agent must state about itself
//!
//! [`crate::grounding_trust`] holds a Rust const table mapping output fields
//! to the tools that could supply them. That works for the handful of
//! curated agents someone has hand-written an entry for, and it cannot work
//! for anyone else: a third party publishing through `POST /api/agents` has
//! no way to add a line to a compiled const.
//!
//! So the map moves into the card, and Rust keeps only the checker. This
//! module is that checker.
//!
//! ## The declaration
//!
//! ```json
//! "output_contract": {
//!   "domain": "phylogenetics",
//!   "produces_schema": "rabble/phylogenetic_profile",
//!   "schema": { "type": "object", "properties": { ... } },
//!   "grounding": {
//!     "taxonomy":   { "status": "sourced",   "tool": "gbif_taxonomy_tree",
//!                     "response_field": "hierarchy", "why": "..." },
//!     "genome":     { "status": "unavailable", "why": "no genome database is wired up" },
//!     "threat_level": { "status": "inferred", "from": "taxonomy and proximity",
//!                       "why": "..." },
//!     "summary":    { "status": "narrative", "why": "..." }
//!   }
//! }
//! ```
//!
//! Every top-level property of `schema` must appear in `grounding`, and
//! every `sourced` entry must name a tool the agent actually declares. That
//! second check is the one with teeth: it is what stops an author writing
//! `"status": "sourced"` over a field nothing can source, which would
//! reproduce the original defect inside the mechanism built to prevent it.
//!
//! ## Why `why` is mandatory
//!
//! Every entry carries a justification, and short ones are rejected. An
//! unexplained disposition is how a contract rots: the next author cannot
//! tell a considered `unavailable` from a lazy one, so they copy whichever
//! is nearest. The Rust table has the same rule and it has already caught
//! two of my own entries.

use serde_json::Value;

/// Minimum length of a `why`. Short enough not to be tyrannical, long
/// enough that "n/a" and "tool" do not pass.
pub const MIN_WHY: usize = 40;

/// Dispositions an author may declare. Closed set: an open one would let
/// `"status": "estimated"` through, which is the fabrication reappearing as
/// a metadata value.
pub const GROUNDING_STATUSES: &[&str] = &["sourced", "inferred", "narrative", "unavailable"];

/// One violation of the card contract, phrased for the person who has to
/// fix it rather than for the person who wrote the checker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Stable machine name, also used as the publish-check name.
    pub check: &'static str,
    /// What is wrong and what to do about it.
    pub message: String,
}

fn f(check: &'static str, message: impl Into<String>) -> Finding {
    Finding {
        check,
        message: message.into(),
    }
}

/// Top-level property names of the declared output schema.
pub fn schema_properties(output_contract: &Value) -> Vec<String> {
    output_contract
        .get("schema")
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

/// Is there a real inline schema, as opposed to a name pointing at one?
pub fn is_typed(output_contract: &Value) -> bool {
    !schema_properties(output_contract).is_empty()
}

/// Check a card's `output_contract` against the typing and grounding rules.
///
/// `tool_names` is what the agent actually declares in `capabilities.mcp_tools`;
/// `produces` is its declared output ports. Both are needed because the
/// interesting failures are *cross-references*: a field claiming a tool the
/// agent does not have, a port naming a type the card does not declare.
///
/// Returns every finding rather than the first, so an author fixes a card in
/// one pass instead of playing whack-a-mole with a gate.
pub fn validate(
    output_contract: Option<&Value>,
    produces: &[String],
    tool_names: &[String],
) -> Vec<Finding> {
    let mut out = Vec::new();

    let Some(oc) = output_contract else {
        out.push(f(
            "output_contract_present",
            "No `output_contract`. Declare one with `produces_schema` (a namespaced \
             type name), `schema` (a JSON Schema for the document you return), and \
             `grounding` (one entry per top-level field saying where its value comes \
             from). See docs/guides/AGENT_CONTRACT_AUTHORING.md.",
        ));
        return out;
    };

    // ── the type itself ────────────────────────────────────────────
    let props = schema_properties(oc);
    if props.is_empty() {
        out.push(f(
            "output_contract_typed",
            "`output_contract` declares no schema. `produces_schema` is a *name*; a \
             name is only a contract once something can resolve it. Add \
             `schema: { \"type\": \"object\", \"properties\": { ... } }` describing the \
             document you actually return.",
        ));
    }

    let declared_type = oc.get("produces_schema").and_then(|v| v.as_str());
    match declared_type {
        None => out.push(f(
            "produces_schema_named",
            "`output_contract.produces_schema` is missing. Give your output type a \
             namespaced name, e.g. `myapp/risk_assessment`, so other agents can \
             reference it by identity rather than by string coincidence.",
        )),
        Some(name) if !name.contains('/') => out.push(f(
            "produces_schema_named",
            format!(
                "`produces_schema` is `{name}`, which has no namespace. Use \
                 `namespace/type` so two teams can both have a `summary` without \
                 colliding."
            ),
        )),
        Some(_) => {}
    }

    // ── the ports must reference the type ──────────────────────────
    if let Some(name) = declared_type {
        for p in produces {
            if p != name {
                out.push(f(
                    "produces_resolves",
                    format!(
                        "`produces` contains `{p}`, which is a free-text label, not a \
                         type. Every entry must be the declared type name (`{name}`) \
                         so a downstream agent matching on it is matching a type and \
                         not a string that happens to look familiar."
                    ),
                ));
            }
        }
    }
    if produces.is_empty() {
        out.push(f(
            "produces_resolves",
            "`produces` is empty, so nothing can compose with this agent.",
        ));
    }

    // ── the field-to-tool map ──────────────────────────────────────
    let grounding = oc.get("grounding").and_then(|g| g.as_object());
    let Some(grounding) = grounding else {
        out.push(f(
            "grounding_declared",
            format!(
                "`output_contract.grounding` is missing. Declare one entry per \
                 top-level output field ({}) stating where its value comes from: \
                 `sourced` (a tool returns it), `inferred` (you reason it out), \
                 `narrative` (prose), or `unavailable` (nothing can supply it, so it \
                 must be null).",
                if props.is_empty() {
                    "none declared yet".to_string()
                } else {
                    props.join(", ")
                }
            ),
        ));
        return out;
    };

    // Every declared field needs a disposition...
    for p in &props {
        if !grounding.contains_key(p) {
            out.push(f(
                "grounding_declared",
                format!(
                    "Output field `{p}` has no `grounding` entry. Every field needs \
                     one — a field nobody has classified is exactly the kind that \
                     gets filled from the model's memory and read as a measurement."
                ),
            ));
        }
    }
    // ...and every disposition needs a field.
    for k in grounding.keys() {
        if !props.contains(k) {
            out.push(f(
                "grounding_declared",
                format!(
                    "`grounding` names `{k}`, which is not a property of the declared \
                     schema. Either add it to the schema or remove the entry — a map \
                     to a field that does not exist protects nothing."
                ),
            ));
        }
    }

    for (field, spec) in grounding {
        let status = spec.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if !GROUNDING_STATUSES.contains(&status) {
            out.push(f(
                "grounding_status_valid",
                format!(
                    "`grounding.{field}.status` is `{status}`, which is not one of \
                     {GROUNDING_STATUSES:?}. The set is closed on purpose: an open one \
                     would admit `estimated`, and an estimate presented in a data \
                     field is the problem this contract exists to stop."
                ),
            ));
            continue;
        }

        let why = spec.get("why").and_then(|v| v.as_str()).unwrap_or("");
        if why.trim().len() < MIN_WHY {
            out.push(f(
                "grounding_explained",
                format!(
                    "`grounding.{field}.why` is missing or too short (needs {MIN_WHY}+ \
                     characters). Say why this field has the status it has. The next \
                     author cannot tell a considered `unavailable` from a lazy one, so \
                     they will copy whichever is nearest."
                ),
            ));
        }

        match status {
            "sourced" => {
                let tool = spec.get("tool").and_then(|v| v.as_str()).unwrap_or("");
                if tool.is_empty() {
                    out.push(f(
                        "grounding_sourced_names_tool",
                        format!("`grounding.{field}` is `sourced` but names no `tool`."),
                    ));
                } else if !tool_names.iter().any(|t| t == tool) {
                    // The check with teeth.
                    out.push(f(
                        "grounding_sourced_names_tool",
                        format!(
                            "`grounding.{field}` claims to be sourced from `{tool}`, \
                             but this agent does not declare that tool. Declared tools: \
                             {}. A field marked `sourced` against a tool the agent \
                             cannot call is the original defect, restated inside the \
                             mechanism built to catch it.",
                            if tool_names.is_empty() {
                                "(none)".to_string()
                            } else {
                                tool_names.join(", ")
                            }
                        ),
                    ));
                }
                if spec
                    .get("response_field")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .is_empty()
                {
                    out.push(f(
                        "grounding_sourced_names_tool",
                        format!(
                            "`grounding.{field}` is `sourced` but does not say which \
                             part of `{tool}`'s response supplies it. Name the field, \
                             so the claim is checkable against the tool's actual output."
                        ),
                    ));
                }
            }
            "inferred" => {
                if spec
                    .get("from")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .is_empty()
                {
                    out.push(f(
                        "grounding_inferred_names_basis",
                        format!(
                            "`grounding.{field}` is `inferred` but does not say what \
                             from. An inference over nothing is a guess; naming the \
                             basis is what distinguishes the two."
                        ),
                    ));
                }
            }
            _ => {}
        }
    }

    out
}

/// Does this agent declare a complete, self-consistent contract?
pub fn conforms(
    output_contract: Option<&Value>,
    produces: &[String],
    tool_names: &[String],
) -> bool {
    validate(output_contract, produces, tool_names).is_empty()
}

/// MCP tool body for `validate_agent_card`.
///
/// Lives here rather than in `tools_legacy.rs` for two reasons: it keeps the
/// footprint in that 6,000-line file to a declaration and a dispatch arm,
/// and it puts the tool immediately next to the rules it enforces, so the
/// two cannot drift.
///
/// The drift is the whole point. `xaman_ek` drafts agent cards for
/// developers; if it worked from a *description* of the contract in its
/// system prompt, that description would fall out of step with the gate and
/// the assistant would confidently produce unpublishable cards. Calling
/// [`validate`] means the advice and the gate are the same code.
pub fn execute_validate_tool(input: &Value) -> Result<String, String> {
    let agent_id = input
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or("agent_id is required")?;

    let produces: Vec<String> = input
        .get("produces")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let tool_names: Vec<String> = input
        .get("tool_names")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let oc = input.get("output_contract").filter(|v| !v.is_null());
    let findings = validate(oc, &produces, &tool_names);
    let exempt = crate::workflows::agent_contract::is_typed_tier_exempt(agent_id);

    let body = if findings.is_empty() {
        serde_json::json!({
            "agent_id": agent_id,
            "would_publish": true,
            "findings": [],
            "note": "Contract is complete: a typed schema, ports that reference it,                      and a grounding entry for every output field."
        })
    } else {
        serde_json::json!({
            "agent_id": agent_id,
            // Grandfathered agents are not blocked, but the findings are
            // still returned: the exemption is a deadline, not a dispensation.
            "would_publish": exempt,
            "grandfathered": exempt,
            "findings": findings
                .iter()
                .map(|f| serde_json::json!({ "check": f.check, "fix": f.message }))
                .collect::<Vec<_>>(),
            "note": if exempt {
                "This agent predates the typed tier and will still publish. The                  findings below are its migration list — the exemption list may                  only shrink."
            } else {
                "Publish is refused until every finding is resolved. See                  docs/guides/AGENT_CONTRACT_AUTHORING.md."
            },
            "guide": "docs/guides/AGENT_CONTRACT_AUTHORING.md"
        })
    };

    serde_json::to_string_pretty(&body).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tools() -> Vec<String> {
        vec!["gbif_taxonomy_tree".into(), "gbif_species_search".into()]
    }
    fn produces() -> Vec<String> {
        vec!["rabble/phylogenetic_profile".into()]
    }

    fn good() -> Value {
        json!({
            "produces_schema": "rabble/phylogenetic_profile",
            "schema": {
                "type": "object",
                "properties": { "taxonomy": {}, "genome": {}, "summary": {} }
            },
            "grounding": {
                "taxonomy": {
                    "status": "sourced", "tool": "gbif_taxonomy_tree",
                    "response_field": "hierarchy (kingdom..species)",
                    "why": "GBIF returns the full rank ladder with stable keys for the queried name."
                },
                "genome": {
                    "status": "unavailable",
                    "why": "No genome database is wired up; NCBI Assembly would be needed and most insects are unsequenced."
                },
                "summary": {
                    "status": "narrative",
                    "why": "Prose over whatever was retrieved; must not assert anything the sourced blocks cannot support."
                }
            }
        })
    }

    // ── the MCP tool xaman_ek calls ────────────────────────────────

    #[test]
    fn the_tool_reports_a_complete_contract_as_publishable() {
        let r = execute_validate_tool(&json!({
            "agent_id": "acme_new_agent",
            "output_contract": good(),
            "produces": produces(),
            "tool_names": tools(),
        }))
        .unwrap();
        let v: Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["would_publish"], json!(true));
        assert_eq!(v["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn the_tool_refuses_a_new_agent_and_says_how_to_fix_it() {
        let r = execute_validate_tool(&json!({ "agent_id": "acme_new_agent" })).unwrap();
        let v: Value = serde_json::from_str(&r).unwrap();
        assert_eq!(v["would_publish"], json!(false));
        let f = &v["findings"][0];
        assert_eq!(f["check"], json!("output_contract_present"));
        assert!(
            f["fix"].as_str().unwrap().contains("grounding"),
            "a refusal must carry the fix, or the author cannot act on it"
        );
        assert!(v["guide"]
            .as_str()
            .unwrap()
            .contains("AGENT_CONTRACT_AUTHORING"));
    }

    #[test]
    fn a_grandfathered_agent_still_gets_its_migration_list() {
        // The exemption is a deadline, not a dispensation: it must still be
        // told what is wrong, or the burn-down has no starting point.
        let r = execute_validate_tool(&json!({ "agent_id": "anomaly_triager" })).unwrap();
        let v: Value = serde_json::from_str(&r).unwrap();
        assert_eq!(
            v["would_publish"],
            json!(true),
            "must not block an existing agent"
        );
        assert_eq!(v["grandfathered"], json!(true));
        assert!(
            !v["findings"].as_array().unwrap().is_empty(),
            "silence would read as conformance"
        );
    }

    #[test]
    fn a_complete_contract_passes() {
        let v = validate(Some(&good()), &produces(), &tools());
        assert!(v.is_empty(), "{v:?}");
        assert!(conforms(Some(&good()), &produces(), &tools()));
    }

    #[test]
    fn a_missing_contract_says_what_to_add() {
        let v = validate(None, &produces(), &tools());
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].check, "output_contract_present");
        assert!(
            v[0].message.contains("grounding"),
            "the message must name all three required parts"
        );
    }

    #[test]
    fn a_schema_name_is_not_a_schema() {
        // The exact state 7 of 100 curated cards were in: produces_schema
        // present, schema absent, rendered under a heading saying "Schema".
        let mut oc = good();
        oc.as_object_mut().unwrap().remove("schema");
        oc["grounding"] = json!({});
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v.iter().any(|x| x.check == "output_contract_typed"));
    }

    #[test]
    fn claiming_a_tool_the_agent_does_not_have_is_rejected() {
        // The check with teeth. Without it an author can mark anything
        // `sourced` and the contract certifies the fabrication.
        let mut oc = good();
        oc["grounding"]["genome"] = json!({
            "status": "sourced", "tool": "ncbi_genome_search",
            "response_field": "assembly.size_mb",
            "why": "This looks entirely plausible and is completely untrue for this agent."
        });
        let v = validate(Some(&oc), &produces(), &tools());
        let hit = v
            .iter()
            .find(|x| x.check == "grounding_sourced_names_tool")
            .expect("must reject a tool the agent does not declare");
        assert!(hit.message.contains("ncbi_genome_search"));
        assert!(
            hit.message.contains("gbif_taxonomy_tree"),
            "the message must list what IS available, or the author cannot act on it"
        );
    }

    #[test]
    fn every_schema_field_needs_a_disposition() {
        let mut oc = good();
        oc["schema"]["properties"]["conservation"] = json!({});
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v
            .iter()
            .any(|x| x.check == "grounding_declared" && x.message.contains("conservation")));
    }

    #[test]
    fn a_disposition_for_a_field_that_does_not_exist_is_rejected() {
        let mut oc = good();
        oc["grounding"]["phylogeny"] = json!({
            "status": "unavailable",
            "why": "A leftover from an earlier draft of the schema, now protecting nothing at all."
        });
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v
            .iter()
            .any(|x| x.check == "grounding_declared" && x.message.contains("phylogeny")));
    }

    #[test]
    fn the_status_vocabulary_is_closed() {
        let mut oc = good();
        oc["grounding"]["genome"]["status"] = json!("estimated");
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v.iter().any(|x| x.check == "grounding_status_valid"));
    }

    /// `scripts/port_migrate.py` emits `NEEDS_AUTHOR` for every decision it
    /// refuses to make for you. That marker MUST fail this validator, or a
    /// draft could be pasted into a card and published — turning a migration
    /// aid into a fabrication engine with good manners.
    ///
    /// Named explicitly rather than relying on the closed-set test, so that
    /// anyone tempted to add it to `GROUNDING_STATUSES` has to delete a test
    /// that says why not.
    #[test]
    fn a_migration_draft_cannot_be_published() {
        assert!(
            !GROUNDING_STATUSES.contains(&"NEEDS_AUTHOR"),
            "NEEDS_AUTHOR must never become a valid status"
        );
        let mut oc = good();
        oc["grounding"]["genome"] = json!({
            "status": "NEEDS_AUTHOR",
            "why": "Field `genome` was found in the prompt's example document, so its NAME is evidence-backed. Where its VALUE comes from is not."
        });
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(
            v.iter().any(|x| x.check == "grounding_status_valid"),
            "a draft from port_migrate.py must be rejected by the gate it \
             migrates toward"
        );
    }

    #[test]
    fn a_lazy_justification_is_rejected() {
        let mut oc = good();
        oc["grounding"]["genome"]["why"] = json!("n/a");
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v.iter().any(|x| x.check == "grounding_explained"));
    }

    #[test]
    fn inferred_must_name_its_basis() {
        let mut oc = good();
        oc["grounding"]["summary"] = json!({
            "status": "inferred",
            "why": "A judgement the agent is asked to make, but with no stated basis for it."
        });
        let v = validate(Some(&oc), &produces(), &tools());
        assert!(v
            .iter()
            .any(|x| x.check == "grounding_inferred_names_basis"));
    }

    #[test]
    fn a_free_text_produces_label_is_rejected() {
        let v = validate(Some(&good()), &["phylogenetic_profile".into()], &tools());
        assert!(v.iter().any(|x| x.check == "produces_resolves"));
    }

    #[test]
    fn an_unnamespaced_type_is_rejected() {
        let mut oc = good();
        oc["produces_schema"] = json!("profile");
        let v = validate(Some(&oc), &["profile".into()], &tools());
        assert!(v.iter().any(|x| x.check == "produces_schema_named"));
    }

    #[test]
    fn all_findings_are_returned_not_just_the_first() {
        // An author should fix a card in one pass, not discover the next
        // problem only after fixing the previous one.
        let oc = json!({ "produces_schema": "x", "schema": {}, "grounding": {} });
        let v = validate(Some(&oc), &["y".into()], &tools());
        assert!(v.len() >= 3, "expected several findings, got {v:?}");
    }
}
