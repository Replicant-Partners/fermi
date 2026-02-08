//! The common [`CoherenceProvider`] trait.
//!
//! All protocol adapters implement this trait to provide a uniform interface
//! for coherence evaluation requests.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use coherence_core::types::ConversationId;
use coherence_core::CoherenceSnapshot;

/// A request to evaluate coherence for a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateRequest {
    /// The conversation to evaluate.
    pub conversation_id: ConversationId,

    /// Optional: raw message texts to add before evaluating.
    #[serde(default)]
    pub messages: Vec<MessageInput>,
}

/// A raw message to be classified and added to the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageInput {
    /// The participant name or ID.
    pub participant: String,

    /// The message text.
    pub content: String,
}

/// The result of a coherence evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateResponse {
    /// The coherence snapshot.
    pub snapshot: CoherenceSnapshot,

    /// Human-readable summary.
    pub summary: String,
}

/// Trait for protocol adapters that provide coherence evaluation.
///
/// Implementations handle protocol-specific serialization, transport,
/// and authentication, while delegating the actual evaluation to the
/// engine and observer crates.
#[async_trait]
pub trait CoherenceProvider: Send + Sync {
    /// Evaluate coherence for a conversation.
    async fn evaluate(&self, request: EvaluateRequest) -> anyhow::Result<EvaluateResponse>;

    /// Get the current snapshot for a conversation without re-evaluating.
    async fn get_snapshot(
        &self,
        conversation_id: ConversationId,
    ) -> anyhow::Result<Option<CoherenceSnapshot>>;

    /// Protocol name for logging and diagnostics.
    fn protocol_name(&self) -> &'static str;
}
