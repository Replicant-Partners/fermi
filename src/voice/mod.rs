pub mod cartesia;

pub use cartesia::CartesiaClient;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceAsset {
    pub asset_id: uuid::Uuid,
    pub object_type: String,
    pub object_id: String,
    pub provider: String,
    pub voice_id: Option<String>,
    pub duration_ms: Option<i32>,
    pub character_count: i32,
    pub storage_url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeRequest {
    pub text: String,
    pub voice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesizeResponse {
    pub audio_url: String,
    pub duration_ms: i32,
    pub character_count: i32,
}
