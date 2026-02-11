use serde::{Deserialize, Serialize};
use std::error::Error;

/// Cartesia Sonic voice styles
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceStyle {
    Narrator,
    Conversational,
    Storyteller,
}

impl VoiceStyle {
    pub fn to_voice_id(&self) -> &str {
        match self {
            VoiceStyle::Narrator => "79a125e8-cd45-4c13-8a67-188112f4dd22", // British Narrator
            VoiceStyle::Conversational => "a0e99841-438c-4a64-b679-ae501e7d6091", // Friendly Guy
            VoiceStyle::Storyteller => "71a7ad14-091c-4e8e-a314-022ece01c121", // Calm Woman
        }
    }
}

impl Default for VoiceStyle {
    fn default() -> Self {
        VoiceStyle::Narrator
    }
}

#[derive(Debug, Serialize)]
struct CartesiaTTSRequest {
    model_id: String,
    transcript: String,
    voice: CartesiaVoice,
    output_format: CartesiaOutputFormat,
}

#[derive(Debug, Serialize)]
struct CartesiaVoice {
    mode: String,
    id: String,
}

#[derive(Debug, Serialize)]
struct CartesiaOutputFormat {
    container: String,
    encoding: String,
    sample_rate: u32,
}

#[derive(Debug, Deserialize)]
struct CartesiaResponse {
    #[serde(default)]
    audio: Option<String>, // Base64 encoded audio
}

pub struct CartesiaClient {
    api_key: String,
    base_url: String,
}

impl CartesiaClient {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.cartesia.ai".to_string(),
        }
    }

    pub async fn synthesize(
        &self,
        text: &str,
        voice_style: VoiceStyle,
    ) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let request = CartesiaTTSRequest {
            model_id: "sonic-english".to_string(),
            transcript: text.to_string(),
            voice: CartesiaVoice {
                mode: "id".to_string(),
                id: voice_style.to_voice_id().to_string(),
            },
            output_format: CartesiaOutputFormat {
                container: "raw".to_string(),
                encoding: "pcm_f32le".to_string(),
                sample_rate: 44100,
            },
        };

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/tts/bytes", self.base_url))
            .header("X-API-Key", &self.api_key)
            .header("Cartesia-Version", "2024-06-10")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();
            return Err(format!("Cartesia API error {}: {}", status, error_body).into());
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }

    pub fn estimate_duration_ms(&self, text: &str) -> i32 {
        // Rough estimate: ~150 words per minute = 2.5 words per second
        // Average word length ~5 chars, so ~12.5 chars per second
        let chars = text.len() as f64;
        let seconds = chars / 12.5;
        (seconds * 1000.0) as i32
    }
}
