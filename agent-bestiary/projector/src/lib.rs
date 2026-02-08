pub mod cache;
pub mod error;
pub mod projection;
pub mod types;

pub use cache::{CacheKey, ProjectionCache};
pub use error::{ProjectorError, Result};
pub use projection::ProjectionEngine;
pub use types::{
    EmbeddingSource, PointMetadata, ProjectedPoint, ProjectionMethod, ProjectionResult,
    TemporalKeyframe, TemporalPoint, TemporalProjectionResult,
};
