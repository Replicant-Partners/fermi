//! Error type for the coherence-gate crate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GateError {
    /// The coherence engine refused to settle (e.g. empty system).
    #[error("coherence settling failed: {0}")]
    SettlingFailed(String),

    /// The gate blocked the write because the proposed update conflicts
    /// with the agent's existing coherence model beyond the threshold.
    #[error("coherence gate blocked: gamma={gamma:.3} < threshold={threshold:.3}, tensions={tensions:?}")]
    Blocked {
        gamma: f64,
        threshold: f64,
        tensions: Vec<String>,
    },

    /// Two-reviewer consensus is required for `AgentWide` scope but only
    /// one reviewer has acted.
    #[error("agent_wide intervention requires two-reviewer consensus; first review recorded, awaiting second reviewer")]
    AwaitingSecondReviewer,

    /// Memory store error propagated upward.
    #[error("memory store error: {0}")]
    Storage(#[from] agent_bestiary_memory::MemoryError),

    /// The referenced episode was not found.
    #[error("episode not found: {0}")]
    EpisodeNotFound(uuid::Uuid),

    /// Invalid input (bad scope, missing classification for AgentWide, etc.).
    #[error("invalid intervention request: {0}")]
    InvalidRequest(String),
}
