/// Agent Backend Module
///
/// This module implements the Agent Bestiary system:
/// - Agent Registry: Stores and manages agent cards
/// - Executors: Pluggable execution engines (LLM, MCP, Manual, Skill)
/// - Scheduler: Manages agent execution timing
/// - Ontology: Per-agent knowledge representation (future)
pub mod agent_card;
pub mod executor;
pub mod llm_executor;
pub mod multi_model_executor;
pub mod registry;

pub use agent_card::*;
pub use executor::*;
pub use llm_executor::*;
pub use multi_model_executor::*;
pub use registry::*;
