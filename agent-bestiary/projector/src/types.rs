use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which table/type this embedding came from
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EmbeddingSource {
    Episode,
    SemanticRule,
    Entity,
    Community,
}

impl std::fmt::Display for EmbeddingSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbeddingSource::Episode => write!(f, "episode"),
            EmbeddingSource::SemanticRule => write!(f, "semantic_rule"),
            EmbeddingSource::Entity => write!(f, "entity"),
            EmbeddingSource::Community => write!(f, "community"),
        }
    }
}

/// A single point projected from high-D to 2D/3D
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectedPoint {
    pub id: Uuid,
    pub source: EmbeddingSource,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub z: Option<f32>,
    pub metadata: PointMetadata,
    pub timestamp: DateTime<Utc>,
}

/// Source-specific metadata attached to each point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointMetadata {
    pub agent_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    // Episode fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consolidated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<Uuid>,
    // Rule fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_status: Option<String>,
    // Entity fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extraction_confidence: Option<f64>,
    // Community fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i32>,
}

/// Projection method selection
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectionMethod {
    Pca,
    Tsne { perplexity: f64 },
}

impl ProjectionMethod {
    pub fn name(&self) -> &str {
        match self {
            ProjectionMethod::Pca => "pca",
            ProjectionMethod::Tsne { .. } => "tsne",
        }
    }
}

/// Result of a projection computation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionResult {
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub method: String,
    pub dimensions: u8,
    pub point_count: usize,
    pub points: Vec<ProjectedPoint>,
    pub computed_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explained_variance: Option<Vec<f64>>,
}

/// A temporal keyframe for animation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalKeyframe {
    pub timestamp: DateTime<Utc>,
    pub label: String,
    pub point_count: usize,
    pub points: Vec<TemporalPoint>,
}

/// Minimal point data for temporal keyframes (just id + coords)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPoint {
    pub id: Uuid,
    pub source: EmbeddingSource,
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub z: Option<f32>,
}

/// Result of a temporal projection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalProjectionResult {
    pub agent_id: Uuid,
    pub method: String,
    pub dimensions: u8,
    pub total_points: usize,
    pub keyframes: Vec<TemporalKeyframe>,
    pub computed_at: DateTime<Utc>,
}

/// Internal: an embedding with its metadata, before projection
#[derive(Debug, Clone)]
pub struct EmbeddingRecord {
    pub id: Uuid,
    pub source: EmbeddingSource,
    pub label: String,
    pub embedding: Vec<f32>,
    pub metadata: PointMetadata,
    pub timestamp: DateTime<Utc>,
}
