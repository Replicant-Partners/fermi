//! # SimOps — Universal Resource Efficiency Engine
//!
//! SOSA-aligned, domain-agnostic SimOps engine for the Fermi / ΞSYSTEM platform.
//!
//! ## Modules
//! | Module | Purpose |
//! |---|---|
//! | [`process`] | `ProcessConfig`, `Stage`, `Resource` — data-driven process definition |
//! | [`cascade`] | Forward/backward multi-stage energy cascade + carbon intensity |
//! | [`kpi`] | NER, SEC, LCC, harvest intensity for a single batch observation |
//! | [`predictor`] | OLS regression: learn from SOSA observation history → predict yield |
//! | [`optimizer`] | What-if solver: given a target output, find required inputs |
//! | [`error`] | Shared error type |
//!
//! ## Quick start
//! ```rust
//! use simops::kpi::{BatchObservation, compute_kpis};
//!
//! let obs = BatchObservation {
//!     primary_energy_kwh: 120.0,
//!     climate_energy_kwh: 35.0,
//!     delivery_energy_kwh: 10.0,
//!     harvest_energy_kwh: 15.0,
//!     output_mass_kg: 4.5,
//!     caloric_density_kcal_g: 5.5,
//!     elec_price_per_kwh: 0.12,
//!     consumables_cost_usd: 8.50,
//!     capex_contribution_usd: 0.0,
//! };
//! let report = compute_kpis(&obs);
//! assert!(report.ner < 1.0); // indoor farms are energy sinks
//! ```

pub mod cascade;
pub mod cascade_v2;
pub mod error;
pub mod kpi;
pub mod optimizer;
pub mod predictor;
pub mod process;
pub mod process_v2;

// Convenience re-exports
pub use cascade::{cascade_backward, cascade_forward, CascadeResult, StageResult};
pub use error::SimOpsError;
pub use kpi::{compute_kpis, BatchObservation, EnergyStatus, KpiReport, KCAL_PER_KWH};
pub use optimizer::{scale_from_reference, single_input_solve, OptimizationResult};
pub use predictor::{Predictor, TrainingObservation};
pub use process::{CapexProfile, ProcessConfig, Resource, Sensor, Sidestream, Stage};
pub use process_v2::{
    CarbonIntensity, CascadeRequestEnvelope, CascadeRequestV2, Input, InputRole,
    MassBalanceMode, Output, OutputRole, PerBasis, ProcessConfigV2, ScaleRequest,
    SchemaVersionProbe, StageV2, Throughput,
};
pub use cascade_v2::{cascade_v2, CascadeError, CascadeResponseV2};
