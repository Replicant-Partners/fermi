//! Conversation observer — utterance extraction and relation detection.
//!
//! This crate takes raw [`Message`](coherence_core::types::Message) objects
//! and produces classified [`Utterance`](coherence_core::types::Utterance)
//! and detected [`CoherenceRelation`](coherence_core::CoherenceRelation) /
//! [`IncoherenceRelation`](coherence_core::IncoherenceRelation) pairs.
//!
//! The current implementation uses heuristic keyword patterns. A future
//! version can plug in LLM-based classification via the protocols layer.

mod classifier;
mod detector;
mod observer;

pub use classifier::UtteranceClassifier;
pub use detector::RelationDetector;
pub use observer::ConversationObserver;

// Relevance gating lives in `coherence-core` so the settling engine's
// Symmetry scorer can share it; re-exported here for callers that think of it
// as an observation concern.
pub use coherence_core::relevance::{content_tokens, is_relevant, overlap, relevance};
