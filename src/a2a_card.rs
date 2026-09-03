//! # A2A AgentCard mapping — pure logic, no async, no AppState
//!
//! Converts ABW agent cards to A2A v1.0 `AgentCard` JSON.
//! Lives in the lib crate so it is testable and reusable.
//!
//! Design: `docs/DESIGN_a2a_provider.md §4`

use serde_json::{json, Value};

use crate::agent_backend::{agent_card::AgentCard, credentials::abw_base_url};

/// Build an A2A v1.0 `AgentCard` from an ABW agent card and DB row fields.
///
/// # Arguments
/// - `slug` — the agent's `agent_name`, used as the A2A slug and in URLs.
/// - `description` — resolved description (card metadata preferred, DB fallback).
/// - `version` — the card's semver string.
/// - `tags` — `card.metadata.tags`.
/// - `sample_queries` — `card.metadata.sample_queries`.
/// - `accepts` — `card.accepts` (schema IDs or free-text labels).
/// - `produces` — `card.produces`.
pub fn agent_card_to_a2a(
    slug: &str,
    description: &str,
    version: &str,
    tags: &[String],
    sample_queries: &[String],
    accepts: &[String],
    produces: &[String],
) -> Value {
    let base = abw_base_url();
    let agent_url = format!("{}/a2a/{}", base, slug);

    let (input_modes, output_modes) = derive_modes(accepts, produces);
    let skill = build_skill(
        slug,
        description,
        tags,
        sample_queries,
        &input_modes,
        &output_modes,
    );

    json!({
        // ── Identity ──────────────────────────────────────────────────
        "name": slug,
        "description": description,
        "version": version,

        // ── Provider ──────────────────────────────────────────────────
        "provider": {
            "organization": "Agent Bestiary",
            "url": "https://agent-bestiary.world"
        },
        "documentationUrl": format!("{}/agents/{}", base, slug),

        // ── Transport ─────────────────────────────────────────────────
        "supportedInterfaces": [{
            "url": agent_url,
            "protocolBinding": "HTTP+JSON",
            "protocolVersion": "1.0"
        }],

        // ── Capabilities ────────────────────────────────────────────────
        // All four A2A capability flags now active.
        "capabilities": {
            "streaming": true,
            "pushNotifications": true
        },

        // ── Auth ──────────────────────────────────────────────────────
        // Execution requires a Bearer API key with scope a2a:invoke:<slug>.
        // Discovery (this endpoint) is public — no auth required.
        "securitySchemes": {
            "bearerApiKey": {
                "httpAuthSecurityScheme": {
                    "scheme": "Bearer",
                    "bearerFormat": "ferm_<hex64>",
                    "description": format!(
                        "ABW API key with a2a:invoke scope. Generate at {}/settings/api-keys",
                        base
                    )
                }
            }
        },
        "securityRequirements": [{ "schemes": { "bearerApiKey": {} } }],

        // ── I/O modes ─────────────────────────────────────────────────
        "defaultInputModes": input_modes,
        "defaultOutputModes": output_modes,

        // ── Skills ────────────────────────────────────────────────────
        "skills": [skill],
    })
}

/// Convenience wrapper that takes the full resolved `AgentCard`.
pub fn agent_card_to_a2a_from_card(
    slug: &str,
    db_description: Option<&str>,
    card: &AgentCard,
) -> Value {
    let description = card.metadata.description.as_str();
    let description = if description.is_empty() {
        db_description.unwrap_or(slug)
    } else {
        description
    };

    agent_card_to_a2a(
        slug,
        description,
        &card.version,
        &card.metadata.tags,
        &card.metadata.sample_queries,
        &card.accepts,
        &card.produces,
    )
}

/// Derive A2A MIME-type input/output modes from ABW schema ID arrays.
///
/// ABW's `accepts`/`produces` are either:
/// - Schema IDs (`"scro/bom-query/1"`) → `"application/json"`
/// - Free-text labels (`"query"`, `"forecast-question"`) → `"text/plain"`
/// - Empty → both (permissive default)
pub fn derive_modes(accepts: &[String], produces: &[String]) -> (Vec<Value>, Vec<Value>) {
    let input_modes = if accepts.is_empty() {
        vec![json!("text/plain"), json!("application/json")]
    } else if accepts.iter().any(|a| is_schema_id(a)) {
        vec![json!("application/json")]
    } else {
        vec![json!("text/plain")]
    };

    let output_modes = if produces.is_empty() {
        vec![json!("text/plain"), json!("application/json")]
    } else if produces.iter().any(|p| is_schema_id(p)) {
        vec![json!("application/json")]
    } else {
        vec![json!("text/plain")]
    };

    (input_modes, output_modes)
}

/// A schema ID contains `/` and is not a standard MIME type prefix.
/// e.g. "scro/bom-query/1", "kask_simops/action_block", "fermi/equity_evidence"
pub fn is_schema_id(s: &str) -> bool {
    s.contains('/')
        && !s.starts_with("text/")
        && !s.starts_with("application/")
        && !s.starts_with("image/")
        && !s.starts_with("audio/")
        && !s.starts_with("video/")
}

/// Build one A2A skill object from agent metadata.
pub fn build_skill(
    slug: &str,
    description: &str,
    tags: &[String],
    sample_queries: &[String],
    input_modes: &[Value],
    output_modes: &[Value],
) -> Value {
    // Human-readable name: underscores → spaces, title-case each word.
    let name = slug
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let examples: Vec<Value> = sample_queries.iter().take(5).map(|q| json!(q)).collect();

    json!({
        "id": slug,
        "name": name,
        "description": description,
        "tags": tags,
        "examples": examples,
        "inputModes": input_modes,
        "outputModes": output_modes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_ids_are_detected() {
        assert!(is_schema_id("scro/bom-query/1"));
        assert!(is_schema_id("kask_simops/action_block"));
        assert!(is_schema_id("fermi/equity_evidence"));
        assert!(!is_schema_id("text/plain"));
        assert!(!is_schema_id("application/json"));
        assert!(!is_schema_id("query"));
        assert!(!is_schema_id("forecast-question"));
    }

    #[test]
    fn modes_from_schema_ids() {
        let (input, output) = derive_modes(
            &["scro/bom-query/1".to_string()],
            &["scro/bom_response".to_string()],
        );
        assert_eq!(input, vec![json!("application/json")]);
        assert_eq!(output, vec![json!("application/json")]);
    }

    #[test]
    fn modes_from_text_labels() {
        let (input, output) = derive_modes(
            &["query".to_string(), "forecast-question".to_string()],
            &["narrative".to_string()],
        );
        assert_eq!(input, vec![json!("text/plain")]);
        assert_eq!(output, vec![json!("text/plain")]);
    }

    #[test]
    fn modes_from_empty_are_permissive() {
        let (input, output) = derive_modes(&[], &[]);
        assert!(input.contains(&json!("text/plain")));
        assert!(input.contains(&json!("application/json")));
        assert!(output.contains(&json!("text/plain")));
        assert!(output.contains(&json!("application/json")));
    }

    #[test]
    fn skill_name_is_title_cased() {
        let skill = build_skill(
            "supply_chain_oracle",
            "desc",
            &[],
            &[],
            &[json!("application/json")],
            &[json!("application/json")],
        );
        assert_eq!(skill["name"], json!("Supply Chain Oracle"));
    }

    #[test]
    fn skill_name_single_word() {
        let skill = build_skill(
            "fermi",
            "desc",
            &[],
            &[],
            &[json!("text/plain")],
            &[json!("text/plain")],
        );
        assert_eq!(skill["name"], json!("Fermi"));
    }

    #[test]
    fn a2a_card_has_required_fields() {
        let card = agent_card_to_a2a(
            "supply_chain_oracle",
            "Prices your BOM and flags supply chain risks.",
            "2.0.0",
            &["supply-chain".to_string(), "pricing".to_string()],
            &["{\"task\":\"resolve_bom\"}".to_string()],
            &["scro/bom-query/1".to_string()],
            &["scro/bom_response".to_string()],
        );
        // Required A2A fields
        assert!(card["name"].is_string());
        assert!(card["description"].is_string());
        assert!(card["version"].is_string());
        assert!(card["supportedInterfaces"].is_array());
        assert!(card["capabilities"].is_object());
        assert!(card["defaultInputModes"].is_array());
        assert!(card["defaultOutputModes"].is_array());
        assert!(card["skills"].is_array());
        // Content checks
        assert_eq!(card["name"], json!("supply_chain_oracle"));
        assert_eq!(card["defaultInputModes"][0], json!("application/json"));
        assert_eq!(card["defaultOutputModes"][0], json!("application/json"));
        assert_eq!(card["skills"][0]["id"], json!("supply_chain_oracle"));
        assert_eq!(card["skills"][0]["name"], json!("Supply Chain Oracle"));
        // securitySchemes declared
        assert!(card["securitySchemes"]["bearerApiKey"].is_object());
    }
}
