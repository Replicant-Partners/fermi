//! MCP (Model Context Protocol) tool definition.
//!
//! Defines the coherence evaluator as an MCP tool that LLM agents
//! (e.g. Claude) can invoke via tool-use.

use serde::{Deserialize, Serialize};

/// MCP tool definition for the coherence evaluator.
///
/// This struct can be serialized to JSON to register the evaluator
/// as an MCP tool with an LLM orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

impl McpToolDefinition {
    /// Create the standard MCP tool definition for coherence evaluation.
    pub fn coherence_evaluator() -> Self {
        Self {
            name: "evaluate_coherence".to_string(),
            description: "Evaluate the explanatory coherence of a multi-party conversation. \
                Returns a coherence score (0-1), per-principle scores based on Thagard's \
                Theory of Explanatory Coherence, and actionable feedback."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "conversation_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The ID of an existing evaluation session"
                    },
                    "messages": {
                        "type": "array",
                        "description": "Messages to add before evaluating",
                        "items": {
                            "type": "object",
                            "properties": {
                                "participant": {
                                    "type": "string",
                                    "description": "The name or ID of the participant"
                                },
                                "content": {
                                    "type": "string",
                                    "description": "The message text"
                                }
                            },
                            "required": ["participant", "content"]
                        }
                    }
                },
                "required": ["messages"]
            }),
        }
    }

    /// Create the MCP tool definition for getting a snapshot.
    pub fn get_snapshot() -> Self {
        Self {
            name: "get_coherence_snapshot".to_string(),
            description: "Get the current coherence evaluation state for a conversation \
                without re-evaluating."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "conversation_id": {
                        "type": "string",
                        "format": "uuid",
                        "description": "The ID of the evaluation session"
                    }
                },
                "required": ["conversation_id"]
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definition_serializes() {
        let tool = McpToolDefinition::coherence_evaluator();
        let json = serde_json::to_string_pretty(&tool).unwrap();
        assert!(json.contains("evaluate_coherence"));
        assert!(json.contains("messages"));

        // Round-trip
        let deserialized: McpToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "evaluate_coherence");
    }

    #[test]
    fn snapshot_tool_definition_serializes() {
        let tool = McpToolDefinition::get_snapshot();
        let json = serde_json::to_string_pretty(&tool).unwrap();
        assert!(json.contains("get_coherence_snapshot"));
    }
}
