//! # coherence-core
//!
//! Core types and data model for the Collaboration Coherence Evaluator.
//!
//! Implements the formal tuple **C = ⟨U, E, R⁺, R⁻, A, σ⟩** from the
//! Thagard (1989) TEC adaptation for multi-party conversation analysis.

pub mod error;
pub mod principles;
pub mod relations;
pub mod system;
pub mod types;

// Re-export primary types for convenience
pub use error::CoreError;
pub use principles::{Principle, PrincipleScore, PrincipleScores};
pub use relations::{CoherenceRelation, IncoherenceRelation, RelationStrength};
pub use system::{Activation, CoherenceSnapshot, CoherenceSystem, GlobalCoherence};
pub use types::{
    Conversation, ConversationId, Message, MessageId, Participant, ParticipantId, Utterance,
    UtteranceId, UtteranceKind,
};
