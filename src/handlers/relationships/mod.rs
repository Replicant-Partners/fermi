//! Forecast relationship groups — Spec 25.
//!
//! Group-tag model replaces the per-relationship ID-list model from mig 150.
//! Members are discovered by querying fermi_forecasts.relationship_groups
//! instead of explicit forecast_ids arrays.

pub mod groups;
pub mod membership;
pub mod propagation;
pub mod apply;
pub mod undo;
pub mod requeue;
pub mod legacy;
pub mod recompose;

pub use propagation::{dispatch_propagation, dispatch_propagation_group, PropagateRequest, PropagateResult, DeltaEntry};
pub use apply::ApplyDismissRequest;
pub use legacy::{CreateRelationshipRequest};
