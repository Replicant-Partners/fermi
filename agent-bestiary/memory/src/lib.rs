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

pub mod bundle;
pub mod clustering;
pub mod consolidation;
pub mod dyad;
pub mod embeddings;
pub mod error;
pub mod llm;
pub mod locking;
pub mod provenance;
pub mod seed;
pub mod store;
pub mod types;

pub use bundle::{AgentCardSnapshot, EpisodeBundle, TranscriptRole, TranscriptTurn};
pub use clustering::{DBSCANClustering, EpisodeCluster};
pub use consolidation::{
    extractor_self_knowledge, ConsolidationResult, ConsolidationWorker, ExtractorUsage,
};
pub use dyad::{
    agent_id_from_dyad, dyad_id, eval_dyad_id, human_id_from_dyad, is_eval_dyad, is_real_dyad,
};
pub use embeddings::{
    AnthropicEmbeddings, EmbeddingGenerator, MistralEmbeddings, MockEmbeddings, NomicEmbeddings,
    OpenAIEmbeddings, ProvenancedEmbedding, QwenEmbeddings,
};
pub use error::{MemoryError, Result};
pub use llm::{
    generate_structured, generate_structured_with_usage, AnthropicProvider, GenerationConfig,
    GenerationResponse, LLMProvider, LLMProviderConfig, LLMProviderFactory, Message, MessageRole,
    MistralProvider, OpenRouterProvider, ProviderType, QwenProvider, TokenUsage,
};
pub use locking::{ConsolidationLock, LockInfo};
pub use provenance::{ExtractionFloor, ProvenanceOracle};
pub use seed::SeedData;
pub use store::MemoryStore;
pub use types::*;
