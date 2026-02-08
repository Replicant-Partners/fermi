//! Fermi Active Dreaming Memory
//!
//! Episodic and semantic memory storage for forecasting agents.
//!
//! This crate provides the foundational memory layer for Fermi agents,
//! implementing the Active Dreaming Memory (ADM) architecture with:
//! - Episodic memory (raw experiences)
//! - Semantic memory (consolidated rules)
//! - Knowledge graph (entities, facts, relationships)
//! - Bi-temporal tracking
//! - Vector similarity search

pub mod clustering;
pub mod consolidation;
pub mod embeddings;
pub mod error;
pub mod llm;
pub mod locking;
pub mod seed;
pub mod store;
pub mod types;

pub use clustering::{DBSCANClustering, EpisodeCluster};
pub use consolidation::{ConsolidationResult, ConsolidationWorker};
pub use embeddings::{
    AnthropicEmbeddings, EmbeddingGenerator, MistralEmbeddings, MockEmbeddings, OpenAIEmbeddings,
    QwenEmbeddings,
};
pub use error::{MemoryError, Result};
pub use llm::{
    generate_structured, AnthropicProvider, GenerationConfig, GenerationResponse, LLMProvider,
    LLMProviderConfig, LLMProviderFactory, Message, MessageRole, MistralProvider,
    OpenRouterProvider, ProviderType, QwenProvider, TokenUsage,
};
pub use locking::{ConsolidationLock, LockInfo};
pub use seed::SeedData;
pub use store::MemoryStore;
pub use types::*;
