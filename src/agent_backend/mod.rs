/// Agent Backend Module
///
/// # Primitives model
///
/// - **Agent**: execution unit with identity, persona, capabilities, and memory
/// - **Composition**: goal-bearing assemblage of agents with a coordination strategist
/// - **Tool**: LLM-callable capability (platform-builtin or MCP/card-declared)
/// - **Skill**: deterministic capability declared on the card, validated at startup
/// - **RSI loops**: cascade (member learning) and tune-team (composition evolution)
///
/// # Module layout
///
/// - `tools/` — platform tools, MCP integrations, and deterministic skills
///   - `tools/platform/` — always-available infrastructure (memory, workspace, coherence)
///   - `tools/mcp/` — external API tools (Polymarket, FMP, Reduct, etc.)
///   - `tools/skills/` — deterministic computations (spatial, simulation, SimOps, bio)
/// - `tool_executor.rs` — Anthropic tool-use loop
/// - `multi_model_executor.rs` — OpenAI-compatible tool-use loop
/// - `llm_executor.rs` — simple LLM executor (no tools)
/// - `simops_tools.rs` — SimOps deterministic implementations (used by skills/simops.rs)
pub mod agent_card;
/// Per-execution provider credentials (SPEC_28). The single path by which
/// an executor obtains an LLM API key.
pub mod credentials;
pub mod executor;
/// SPEC_28 acceptance tests: an agent's funding must not depend on the
/// shape of its output. Test-only module.
#[cfg(test)]
mod funding_parity_tests;
pub mod kg_context;
pub mod llm_executor;
/// Outbound MCP client: lets an agent consume tools from remote MCP
/// servers declared on its card. See module docs for the
/// server-vs-client history.
pub mod mcp_client;
pub mod multi_model_executor;
pub mod registry;
pub mod simops_tools;
pub mod tool_executor;
pub mod tools;
/// Weather forecasting + prediction-market tools backing the `weather_oracle`
/// composition. Research provenance: `docs/WEATHER_MARKETS_RESEARCH.md`.
pub mod weather_tools;

#[allow(ambiguous_glob_reexports)]
pub use agent_card::*;
pub use executor::*;
pub use llm_executor::*;
pub use multi_model_executor::*;
pub use registry::*;
pub use tool_executor::ToolAwareExecutor;
pub use tools::{validate_card_skills, SkillRegistry, ToolContext, ToolRegistry};
