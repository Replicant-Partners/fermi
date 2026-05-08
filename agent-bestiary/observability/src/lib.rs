//! Plane C — longitudinal observability.
//!
//! See:
//! - [`docs/architecture/social_agent_observability_architecture.html`](../../../docs/architecture/social_agent_observability_architecture.html)
//! - [`docs/architecture/OBSERVABILITY_IMPL.md`](../../../docs/architecture/OBSERVABILITY_IMPL.md) (Phase 3)
//!
//! ## Architecture
//!
//! ```text
//!   Episode + AggregatedSignal (Phase 2)
//!         │
//!         ├──── EpisodeScorer.write_inline ───► agent_timeline_entries (fast path)
//!         │
//!         └──── ObservabilityWorker.scan ─────► drift, anomaly, dyad updates
//!                                          │
//!                                          ├─ PersonaDriftMonitor.compute
//!                                          ├─ SocialInteractionTracker.update_from
//!                                          ├─ AnomalyDetector.scan
//!                                          └─ TrendAnalyzer.compute (on-demand only)
//! ```
//!
//! Per Q4 (c) — hybrid scheduling: timeline entries are written
//! inline at episode-store time so the dashboard never lags; drift +
//! anomaly + dyad updates run in the background scanner. The scanner
//! cadence mirrors the consolidation worker's on-demand pattern: it
//! is invoked per-agent (HTTP-trigger or post-eval-run hook) and scans
//! all timeline entries since its last checkpoint.

pub mod anomaly;
pub mod drift;
pub mod error;
pub mod scorer;
pub mod social;
pub mod trend;
pub mod worker;

#[cfg(test)]
mod tests;

pub use anomaly::{AnomalyDetector, AnomalyKind, DetectedAnomaly};
pub use drift::{DriftThreshold, DriftVector, PersonaDriftMonitor};
pub use error::ObservabilityError;
pub use scorer::EpisodeScorer;
pub use social::{SocialInteractionTracker, SocialUpdate};
pub use trend::{TrendAnalyzer, TrendSeries, TrendWindow};
pub use worker::{ObservabilityWorker, ScanReport};

// Re-export the storage shape consumers will most often touch.
pub use agent_bestiary_memory::{
    AgentObservabilityState, AnomalyEvent, DyadState, TimelineEntry,
};
