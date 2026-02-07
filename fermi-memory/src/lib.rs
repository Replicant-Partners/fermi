//! # Fermi Memory
//!
//! Active Dreaming Memory (ADM) system for Fermi forecasting agents.
//!
//! This crate provides the core memory infrastructure for agents to:
//! - Store episodic memories (individual executions)
//! - Consolidate into semantic memories (learned rules)
//! - Build knowledge graphs (entities, relationships, facts)
//! - Track bi-temporal history (event time vs transaction time)
//!
//! ## Architecture
//!
//! ADM follows a sleep/wake cycle:
//!
//! **Wake Phase (Episodic Memory)**
//! - Agent executes tasks and stores episodes
//! - Each episode captures query, context, results, metrics
//! - Episodes remain unconsolidated initially
//!
//! **Sleep Phase (Consolidation)**
//! - Cluster similar episodes
//! - Extract semantic rules from patterns
//! - Build/update knowledge graph
//! - Verify rules against historical data
//! - Commit ontology changes to git
//!
//! ## Usage
//!
//! ```rust,no_run
//! use fermi_memory::{MemoryStore, Episode, ExecutionStatus};
//! use uuid::Uuid;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Connect to database
//!     let store = MemoryStore::new("postgresql://localhost/fermi").await?;
//!
//!     // Store an episode
//!     let agent_id = Uuid::new_v4();
//!     let episode = Episode::new(
//!         agent_id,
//!         "What is AMD's market share?".to_string(),
//!         serde_json::json!({"result": "15%"}),
//!         ExecutionStatus::Success,
//!     );
//!
//!     let episode_id = store.store_episode(episode).await?;
//!
//!     // Retrieve unconsolidated episodes
//!     let episodes = store.get_unconsolidated_episodes(agent_id, 100).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Phase 1 Status
//!
//! ✅ Core types defined (Episode, SemanticRule, Entity, Relationship, Fact)
//! ✅ MemoryStore with connection pooling
//! ✅ Episode storage and retrieval
//! ✅ Semantic rule storage and retrieval
//! ⏳ Embedding generation (Phase 2)
//! ⏳ Consolidation engine (Phase 3)
//! ⏳ Git integration (Phase 4)

pub mod error;
pub mod store;
pub mod types;

// Re-export main types
pub use error::{MemoryError, Result};
pub use store::MemoryStore;
pub use types::{
    Entity, Episode, ExecutionStatus, Fact, Relationship, SemanticRule, VerificationStatus,
};
