//! Phase 5 — Intervention feedback loop
//!
//! Implements the five-step corrective-signal flow from the architecture doc
//! (Plane D, "Intervention feedback flow"):
//!
//! ```text
//!   Step 1  Reviewer acts (HITL queue — handled in the HTTP layer)
//!   Step 2  InterventionEncoder  → EncodedIntervention
//!   Step 3  CoherenceGate        → GateOutcome  (synchronous for AgentWide;
//!                                                settler mode otherwise)
//!   Step 4  TwoWriteMemory       → TwoWriteReceipt  (annotation + synthetic
//!                                                     episode)
//!   Step 5  Loop closes          → persona_version bump for AgentWide scope
//! ```
//!
//! See:
//! - `docs/architecture/social_agent_observability_architecture.html` (Plane D)
//! - `docs/architecture/OBSERVABILITY_IMPL.md` (Phase 5)

pub mod encoder;
pub mod error;
pub mod gate;
pub mod two_write;

#[cfg(test)]
mod tests;

pub use encoder::{EncodedIntervention, InterventionEncoder, InterventionRequest};
pub use error::GateError;
pub use gate::{CoherenceGate, GateOutcome, GateVerdict};
pub use two_write::{TwoWriteMemory, TwoWriteReceipt};

// Re-export the scope / classification enums so callers don't need to
// pull in `agent-bestiary-memory` directly.
pub use agent_bestiary_memory::{CorrectionClassification, CorrectionScope};
