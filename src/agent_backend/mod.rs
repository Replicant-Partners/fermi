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
pub mod executor;
pub mod kg_context;
pub mod llm_executor;
pub mod multi_model_executor;
pub mod registry;
pub mod simops_tools;
pub mod tool_executor;
pub mod tools;

#[allow(ambiguous_glob_reexports)]
pub use agent_card::*;
pub use executor::*;
pub use llm_executor::*;
pub use multi_model_executor::*;
pub use registry::*;
pub use tool_executor::ToolAwareExecutor;
pub use tools::{ToolContext, ToolRegistry, SkillRegistry, validate_card_skills};
