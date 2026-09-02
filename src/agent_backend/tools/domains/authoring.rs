// src/agent_backend/tools/domains/authoring.rs
//
// Phase 2 domain migration: Authoring tools.
//
// Two tools (both requires_workspace: false):
//   validate_agent_card    — checks a draft card against the publish contract
//   build_output_contract  — compiles a sketch into a complete typed contract
//
// Each is a zero-size struct implementing PlatformTool. execute() delegates
// to the legacy ToolRegistry::standard() so that dispatch semantics are
// identical to the pre-migration path.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Authoring-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![Arc::new(ValidateAgentCard), Arc::new(BuildOutputContract)]
}

// ─── validate_agent_card ──────────────────────────────────────────────────────

struct ValidateAgentCard;

#[async_trait]
impl PlatformTool for ValidateAgentCard {
    fn name(&self) -> &'static str {
        "validate_agent_card"
    }

    fn description(&self) -> &'static str {
        "Check a draft agent card against the publish contract: typed output schema, ports that reference the declared type, and a grounding entry per output field saying where its value comes from. Returns every finding with the fix, or confirms it would publish. Use before proposing a card to a developer."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "agent_id": {
                    "type": "string",
                    "description": "Id of the agent being authored (checked against the grandfathering list)"
                },
                "output_contract": {
                    "type": "object",
                    "description": "The draft `capabilities.output_contract`: produces_schema, schema, grounding"
                },
                "produces": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Draft `produces` ports; each must equal the declared type name"
                },
                "tool_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tools the agent declares. A field marked `sourced` must name one of these."
                }
            },
            "required": ["agent_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Authoring
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::card_contract::execute_validate_tool(input)
    }
}

// ─── build_output_contract ────────────────────────────────────────────────────

struct BuildOutputContract;

#[async_trait]
impl PlatformTool for BuildOutputContract {
    fn name(&self) -> &'static str {
        "build_output_contract"
    }

    fn description(&self) -> &'static str {
        "Compile a short SKETCH into a complete, publishable typed output contract. You declare the three things that need judgement — the evidence blocks, their fields and types, and where each block's value comes from plus why — and this emits the JSON Schema, the narrowed per-block `_provenance` enums, the grounding map and the rewritten `produces`. Prefer this over hand-writing a contract: it emits schema and grounding from one pass, so they cannot disagree, and it refuses to return anything the publish gate would reject. It will NOT invent a `why`, and a block claiming to be `sourced` from a tool absent from `tool_names` is refused."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "sketch": {
                    "type": "object",
                    "description": "{domain, produces_schema (namespaced, e.g. `myapp/risk_assessment`), title?, description?, synthesis?, calibration?, blocks: [{name, source: {status: sourced|inferred|narrative|unavailable, tool?, response_field?, coverage?: complete|partial|deferred, from?, would_need?}, why (40+ chars, never generated), fields?: {name: type}, value?: type, required?}]}. Type syntax: string|integer|number|boolean|object, `enum:a|b|c`, `const:v`, or `@entity` to take the type from the ontology; suffix `[]` for array then `?` for nullable, in that order. `minimum`/`pattern` are deliberately unavailable — the platform validator cannot evaluate them, and a schema it cannot evaluate reports `unverified`, which is not a pass."
                },
                "tool_names": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tools the agent declares in `capabilities.mcp_tools`. Cross-checked: a `sourced` block must name one of these."
                },
                "ontology": {
                    "type": "object",
                    "description": "Optional agent ontology ({entities: [{id, properties: {definition, scale|categories}}]}). Resolves `@entity` field types so vocabulary is selected rather than reinvented."
                }
            },
            "required": ["sketch"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Authoring
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        crate::contract_sketch::execute_build_tool(input)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_authoring() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Authoring,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_two() {
        assert_eq!(tools().len(), 2);
    }

    #[test]
    fn none_require_workspace() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "tool `{}` should NOT require workspace",
                tool.name()
            );
        }
    }
}
