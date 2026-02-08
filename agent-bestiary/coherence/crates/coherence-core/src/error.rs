//! Error types for the coherence-core crate.

use thiserror::Error;
use uuid::Uuid;

/// Errors that can occur within the core coherence model.
#[derive(Debug, Error)]
pub enum CoreError {
    /// An utterance was referenced that does not exist in the system.
    #[error("utterance not found: {0}")]
    UtteranceNotFound(Uuid),

    /// A participant was referenced that does not exist in the conversation.
    #[error("participant not found: {0}")]
    ParticipantNotFound(Uuid),

    /// A conversation was referenced that does not exist.
    #[error("conversation not found: {0}")]
    ConversationNotFound(Uuid),

    /// A message was referenced that does not exist.
    #[error("message not found: {0}")]
    MessageNotFound(Uuid),

    /// Attempted to create a relation between an utterance and itself.
    #[error("self-referential relation not allowed: utterance {0}")]
    SelfRelation(Uuid),

    /// Attempted to create a duplicate relation between two utterances.
    #[error("duplicate relation between {0} and {1}")]
    DuplicateRelation(Uuid, Uuid),

    /// A relation references an utterance not present in the system.
    #[error("dangling relation: utterance {0} not in system")]
    DanglingRelation(Uuid),

    /// An activation value was outside the valid range [-1, 1].
    #[error("activation value {0} out of range [-1, 1]")]
    ActivationOutOfRange(f64),

    /// A score value was outside the valid range [0, 1].
    #[error("score value {0} out of range [0, 1]")]
    ScoreOutOfRange(f64),

    /// A weight value was outside the valid range [-1, 1].
    #[error("weight value {0} out of range [-1, 1]")]
    WeightOutOfRange(f64),

    /// The coherence system is empty (no utterances to evaluate).
    #[error("coherence system is empty: no utterances to evaluate")]
    EmptySystem,

    /// A threshold value was outside the valid range [0, 1].
    #[error("threshold {name} value {value} out of range [0, 1]")]
    InvalidThreshold { name: String, value: f64 },

    /// The conversation has too many participants for the configured limit.
    #[error("participant limit exceeded: {count} > {limit}")]
    ParticipantLimitExceeded { count: usize, limit: usize },

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Generic internal error with context.
    #[error("internal error: {0}")]
    Internal(String),
}

/// A specialized `Result` type for coherence-core operations.
pub type CoreResult<T> = Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        let id = Uuid::new_v4();

        let err = CoreError::UtteranceNotFound(id);
        assert!(err.to_string().contains(&id.to_string()));

        let err = CoreError::ActivationOutOfRange(1.5);
        assert!(err.to_string().contains("1.5"));

        let err = CoreError::EmptySystem;
        assert!(err.to_string().contains("empty"));

        let err = CoreError::InvalidThreshold {
            name: "critical".to_string(),
            value: -0.1,
        };
        assert!(err.to_string().contains("critical"));
        assert!(err.to_string().contains("-0.1"));
    }

    #[test]
    fn result_type_works() {
        let ok: CoreResult<u32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: CoreResult<u32> = Err(CoreError::EmptySystem);
        assert!(err.is_err());
    }
}
