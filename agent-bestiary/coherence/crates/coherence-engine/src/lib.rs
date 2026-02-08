//! Connectionist settling engine for Thagard's Theory of Explanatory Coherence.
//!
//! This crate implements the constraint-satisfaction network that computes
//! coherence scores for collaborative discourse. It takes a [`CoherenceSystem`]
//! with utterances and relations already populated, then iteratively settles
//! the activation values until the network converges.
//!
//! # The Settling Rule
//!
//! From Thagard (1989), adapted as implemented in ECHO:
//!
//! ```text
//! A_{t+1}(uᵢ) = clip[-1,1]( (1 − δ) · Aₜ(uᵢ)  +  η · Σⱼ wᵢⱼ · Aₜ(uⱼ) )
//! ```
//!
//! where:
//! - **δ** is the decay parameter (typically 0.05)
//! - **η** is the learning rate (typically 0.05)
//! - **wᵢⱼ** is the weight between nodes i and j
//! - Positive weights for coherence relations (R⁺)
//! - Negative weights for incoherence relations (R⁻)

mod scoring;
mod settling;

pub use scoring::PrincipleScorer;
pub use settling::{SettlingConfig, SettlingEngine, SettlingResult};
