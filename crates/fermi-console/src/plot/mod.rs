//! `plot` — the GPUI-free core of the console's visualisations.
//!
//! # The split, and why
//!
//! Every chart has two halves: the arithmetic that decides *where
//! things go*, and the code that puts ink on the GPU. The old
//! `charts.rs` fused them inside plotters callbacks, which had three
//! consequences worth naming:
//!
//! * **Untestable.** The layout maths only existed inside a closure
//!   handed to `plotters`, so nothing about it could be asserted. The
//!   `trajectory_plot_bounds` duplicate existed precisely because the
//!   real geometry was unreachable.
//! * **Unidirectional.** A rasteriser maps data → pixels and stops
//!   there. Direct manipulation needs the inverse: pixels → data. You
//!   can't drag a threshold you can't invert.
//! * **Flickery.** See `viz/mod.rs` for the atlas-churn story.
//!
//! So the arithmetic lives here — plain `f64`, no `gpui`, in the lib
//! target where `cargo test -p fermi-console --lib` can reach it — and
//! the painting lives in the binary's `viz` module, which consumes
//! these types.
//!
//! ## Modules
//!
//! * [`scale`] — invertible data↔pixel mappings and nice-number ticks.
//! * [`frame`] — the one object that owns a chart's geometry, shared by
//!   painter and hit-tester so they cannot drift apart.
//! * [`density`] — honest distribution curves, tagged with the
//!   provenance of their shape.
//! * [`sobol`] — variance decomposition laid out for reading, including
//!   the first-order/interaction split and run-over-run movement.
//! * [`events`] — correlating trajectory events with the movement they
//!   caused, and packing their labels so they stay legible.
//! * [`format`] — scale-adaptive axis and readout formatting.
//! * [`distribution`], [`trajectory`], [`index`] — per-chart geometry:
//!   the layout, the scales, and the pixel→data inversion each chart's
//!   painter and hit-tester both consult.

pub mod curve;
pub mod density;
pub mod distribution;
pub mod events;
pub mod format;
pub mod frame;
pub mod index;
pub mod scale;
pub mod sobol;
pub mod trajectory;

pub use density::{Density, DensitySource};
pub use distribution::DistributionSpec;
pub use events::{correlate, Correlated};
pub use frame::{Frame, Margins, Rect};
pub use index::{IndexData, IndexSpec, IndexVersion};
pub use scale::{extent, LinearScale};
pub use sobol::{SobolBar, SobolLayout};
pub use trajectory::{TrajectoryData, TrajectorySpec};
