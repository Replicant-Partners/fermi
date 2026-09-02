// src/agent_backend/tools/registry.rs
//
// Phase 1 scaffold for PlatformToolRegistry.
//
// The execute() method is the final design from
// docs/plans/TOOL_REGISTRY_REFACTOR.md §2.2. During Phase 1 it always falls
// through to the remote-MCP / "Unknown tool" branch because all_tools() is
// empty. No tool call goes through this registry until Phase 3 switches
// ToolAwareExecutor to use it.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::agent_backend::agent_card::AgentCard;
use crate::agent_backend::llm_executor::ClaudeTool;
use crate::agent_backend::mcp_client::RemoteMcpCatalogue;
use crate::agent_backend::multi_model_executor::{OpenAIFunction, OpenAITool};

use super::platform_tool::{PlatformTool, ToolCatalogueEntry};
use super::{all_tools, ToolContext};

/// Registry backed by a `HashMap<name, Arc<dyn PlatformTool>>`.
///
/// Three constructors mirror `ToolRegistry`'s three configurations so that
/// Phase 3 is a mechanical type-name substitution in `ToolAwareExecutor`.
pub struct PlatformToolRegistry {
    tools: HashMap<&'static str, Arc<dyn PlatformTool>>,
}

impl PlatformToolRegistry {
    /// Full registry — all tools including workspace and delegation.
    pub fn all() -> Self {
        Self::build(true, true)
    }

    /// Standard registry — no workspace tools, no delegation.
    /// Used for single-turn fallback execution.
    pub fn standard() -> Self {
        Self::build(false, false)
    }

    /// Workspace registry without delegation tools.
    /// Used for recursive execute_agent calls to prevent cycles.
    pub fn workspace_no_delegation() -> Self {
        Self::build(true, false)
    }

    fn build(include_workspace: bool, include_delegation: bool) -> Self {
        let tools: HashMap<&'static str, Arc<dyn PlatformTool>> = all_tools()
            .into_iter()
            .filter(|t| include_workspace || !t.requires_workspace())
            .filter(|t| include_delegation || !t.is_delegation())
            .map(|t| (t.name(), t))
            .collect();
        Self { tools }
    }

    /// Dispatch a tool call. Enforces workspace and credential guards before
    /// calling `execute()`. Falls through to `ctx.remote_mcp` for any name
    /// not found in `self.tools`, using the exact same error-message format
    /// as the legacy match fallthrough (§4.5).
    pub async fn execute(
        &self,
        tool_name: &str,
        input: &Value,
        ctx: &ToolContext,
    ) -> Result<String, String> {
        match self.tool(tool_name) {
            Some(tool) => {
                // Workspace guard
                if tool.requires_workspace() && ctx.workspace_id.is_none() {
                    return Err(format!(
                        "Tool `{tool_name}` requires a workspace context."
                    ));
                }
                // Credential guard
                if let Some(key) = tool.required_credential() {
                    if ctx
                        .user_secrets
                        .as_ref()
                        .and_then(|s| s.get(key))
                        .is_none()
                    {
                        return Err(format!(
                            "Tool `{tool_name}` requires credential `{key}` \
                             which is not configured for this agent."
                        ));
                    }
                }
                tool.execute(input, ctx).await
            }
            None => {
                // Remote MCP fallthrough — preserved verbatim from ToolRegistry (§4.5).
                match ctx.remote_mcp.as_ref() {
                    Some(cat) if cat.get(tool_name).is_some() => {
                        cat.call(tool_name, input).await
                    }
                    Some(cat) if !cat.is_empty() => Err(format!(
                        "Unknown tool: {tool_name}. Remote MCP tools: {}",
                        cat.tools()
                            .iter()
                            .map(|t| t.qualified_name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    _ => Err(format!("Unknown tool: {tool_name}")),
                }
            }
        }
    }

    /// Tool schema for Claude (LLM-visible tools only), merged with card-declared
    /// MCP tools and remote MCP tools.
    ///
    /// Ordering is deliberate: platform tools, then card MCP tools, then remote
    /// tools — later groups skip names already claimed, so a remote server cannot
    /// shadow a platform tool by naming one of its tools `execute_agent`.
    pub(crate) fn to_claude_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&RemoteMcpCatalogue>,
    ) -> Vec<ClaudeTool> {
        let mut tools: Vec<ClaudeTool> = self
            .tools
            .values()
            .filter(|t| t.is_llm_visible())
            .map(|t| ClaudeTool {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.input_schema(),
            })
            .collect();

        let mut claimed: std::collections::HashSet<String> =
            tools.iter().map(|t| t.name.clone()).collect();

        for mcp in &card.capabilities.mcp_tools {
            if let Some(ref schema) = mcp.input_schema {
                if claimed.insert(mcp.name.clone()) {
                    tools.push(ClaudeTool {
                        name: mcp.name.clone(),
                        description: mcp.description.clone(),
                        input_schema: schema.clone(),
                    });
                }
            }
        }
        if let Some(cat) = remote {
            for rt in cat.tools() {
                if claimed.insert(rt.qualified_name.clone()) {
                    tools.push(ClaudeTool {
                        name: rt.qualified_name.clone(),
                        description: rt.description.clone(),
                        input_schema: rt.input_schema.clone(),
                    });
                }
            }
        }
        tools
    }

    /// OpenAI-format counterpart of `to_claude_tools_with_card_and_remote`.
    pub(crate) fn to_openai_tools_with_card_and_remote(
        &self,
        card: &AgentCard,
        remote: Option<&RemoteMcpCatalogue>,
    ) -> Vec<OpenAITool> {
        self.to_claude_tools_with_card_and_remote(card, remote)
            .into_iter()
            .map(|t| OpenAITool {
                tool_type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name,
                    description: t.description,
                    parameters: t.input_schema,
                },
            })
            .collect()
    }

    /// All tool names dispatchable by this registry instance.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.keys().copied().collect()
    }

    /// Look up a tool by name.
    pub fn tool(&self, name: &str) -> Option<Arc<dyn PlatformTool>> {
        self.tools.get(name).cloned()
    }

    /// All tools across all categories — used for the UI catalogue endpoint
    /// (Phase 5). Returns entries in registration order.
    pub fn all_tools_catalogue() -> Vec<ToolCatalogueEntry> {
        all_tools()
            .iter()
            .map(|t| ToolCatalogueEntry {
                name: t.name(),
                description: t.description(),
                category: t.category(),
                requires_workspace: t.requires_workspace(),
                is_llm_visible: t.is_llm_visible(),
                required_credential: t.required_credential(),
            })
            .collect()
    }
}
