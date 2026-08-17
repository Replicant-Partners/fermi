//! How well-grounded was the evidence a memory was built from?
//!
//! # Why this is a trait and not a function
//!
//! The rules that answer the question live in `fermi::grounding_trust`: the
//! field contracts, the provenance vocabulary, the strength ordering, the
//! extraction ceiling. This crate cannot call them — `fermi` depends on
//! `agent-bestiary-memory`, so a direct call would be a dependency cycle.
//!
//! Copying the arithmetic here was the obvious alternative and would have been
//! a mistake. There would then be two `min` implementations and two copies of
//! the vocabulary, and this module's history says exactly what happens next:
//! cards said `gbif_verified` where the runtime emitted `tool_verified` and
//! nothing noticed until a guard was written for it. A second copy of a trust
//! calculation is a second answer to the same question, and the one that
//! disagrees is the one that gets believed, because it is the one nearest the
//! writer.
//!
//! So the shape is the one already used for `LLMProvider` and
//! `EmbeddingGenerator`: this crate declares what it needs, the upper crate
//! supplies the only implementation.
//!
//! # Why `None` is a third state
//!
//! Every method here can decline to answer, and declining is not a pass.
//! Three separate ways the honest answer is "unknown":
//!
//! * The subject agent has no field contract, so nothing is known about how it
//!   grounds anything. Silence is not a verdict.
//! * The episodes predate migration 199, which is when `episodes.response_text`
//!   began to be retained. The evidence is not unrecorded, it is *gone*.
//! * No oracle was injected, because the caller is a test or a path that has
//!   no database.
//!
//! All three produce `None`, which must reach storage as SQL NULL and be
//! excluded from grounded counts rather than assumed clean. A report that
//! counted NULL as grounded would show the corpus getting cleaner as coverage
//! got worse.

use crate::error::Result;
use uuid::Uuid;

/// The provenance floor of an extracted memory, plus the working.
///
/// Produced by a [`ProvenanceOracle`] and written to
/// `semantic_rules.provenance_floor` / `.provenance_floor_basis`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionFloor {
    /// Weakest provenance among the sources, capped at the extraction
    /// ceiling. `None` means unknown — see the module docs.
    pub floor: Option<String>,
    /// How the floor was reached, in enough detail to recompute and disagree:
    /// the per-source verdicts, whether the ceiling was binding, and why the
    /// answer is `None` when it is.
    ///
    /// A floor with no working shown is an assertion, and an assertion is the
    /// thing being replaced.
    pub basis: serde_json::Value,
}

impl ExtractionFloor {
    /// The honest answer when nothing can be established.
    ///
    /// Takes a `reason` because "unknown" with no reason is indistinguishable
    /// from a bug, and the three reasons imply completely different work:
    /// no contract means write one, no retained response means wait for
    /// episodes to accumulate, no oracle means fix the wiring.
    pub fn unknown(reason: &str) -> Self {
        Self {
            floor: None,
            basis: serde_json::json!({ "sources": [], "reason": reason }),
        }
    }
}

/// Resolves the provenance floor of a set of source episodes.
///
/// Implemented in `fermi`, which owns the contracts. See
/// `fermi::grounding_trust::extracted_floor` for the arithmetic and
/// `fermi::handlers::consolidation` for the wiring.
#[async_trait::async_trait]
pub trait ProvenanceOracle: Send + Sync {
    /// The floor for a rule extracted from `episode_ids`.
    ///
    /// Implementors MUST return `floor: None` rather than a value they cannot
    /// support, and MUST NOT return a floor stronger than the extraction
    /// ceiling: reading well-sourced episodes and writing a generalisation
    /// about them is judgement, and judgement does not inherit retrieval.
    ///
    /// An empty `episode_ids` MUST NOT produce a strong floor. `min` over an
    /// empty set has no answer and the identity element for `min` is the
    /// *maximum* value, so the natural implementation claims that a rule with
    /// no sources was measured. That is the single most likely way this
    /// calculation breaks, and it breaks in the direction that manufactures
    /// trust.
    async fn extraction_floor(&self, episode_ids: &[Uuid]) -> Result<ExtractionFloor>;
}
