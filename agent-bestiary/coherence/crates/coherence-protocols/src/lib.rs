//! Integration protocol traits and adapters.
//!
//! Defines the [`CoherenceProvider`] trait — the common interface that all
//! protocol adapters implement to provide coherence evaluation capabilities.
//!
//! # Protocols
//!
//! - **MCP** (Model Context Protocol): Exposes the evaluator as a tool
//!   that LLM agents can invoke.
//! - **A2A** (Agent-to-Agent): Peer communication with other agents.
//! - **AKP**: Custom domain-specific protocol (to be defined).

mod mcp;
mod provider;

pub use mcp::McpToolDefinition;
pub use provider::CoherenceProvider;
