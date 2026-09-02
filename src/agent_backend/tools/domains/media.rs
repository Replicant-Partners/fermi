// src/agent_backend/tools/domains/media.rs
//
// Phase 4 domain migration: Media tools.
//
// Three tools, all requires_workspace: false:
//   generate_image
//   edit_image
//   speak_text
//
// Each is a zero-size struct implementing PlatformTool. execute() calls
// a private function defined in this module.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;
use crate::voice::{cartesia::VoiceStyle, CartesiaClient};

/// All Media-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(GenerateImage),
        Arc::new(EditImage),
        Arc::new(SpeakText),
    ]
}

// ─── generate_image ───────────────────────────────────────────────────────────

struct GenerateImage;

#[async_trait]
impl PlatformTool for GenerateImage {
    fn name(&self) -> &'static str {
        "generate_image"
    }

    fn description(&self) -> &'static str {
        "Generate an image from a text prompt using Gemini. Returns the image as base64-encoded data with its MIME type."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text description of the image to generate"
                }
            },
            "required": ["prompt"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Media
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_generate_image(input).await
    }
}

// ─── edit_image ───────────────────────────────────────────────────────────────

struct EditImage;

#[async_trait]
impl PlatformTool for EditImage {
    fn name(&self) -> &'static str {
        "edit_image"
    }

    fn description(&self) -> &'static str {
        "Edit/transform an image using a text prompt and a reference image URL via Gemini. Useful for style transfer, modifications, and artistic transformations. Returns the edited image as base64-encoded data."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "Text description of the desired edit/transformation"
                },
                "image_url": {
                    "type": "string",
                    "description": "URL of the source image to edit"
                }
            },
            "required": ["prompt", "image_url"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Media
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_edit_image(input).await
    }
}

// ─── speak_text ───────────────────────────────────────────────────────────────

struct SpeakText;

#[async_trait]
impl PlatformTool for SpeakText {
    fn name(&self) -> &'static str {
        "speak_text"
    }

    fn description(&self) -> &'static str {
        "Convert text to natural speech using Cartesia Sonic. Returns audio as base64-encoded PCM data."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to convert to speech (max 5000 characters)"
                },
                "voice": {
                    "type": "string",
                    "description": "Voice style: narrator (British), conversational (friendly), or storyteller (calm)",
                    "enum": ["narrator", "conversational", "storyteller"],
                    "default": "narrator"
                }
            },
            "required": ["text"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Media
    }

    async fn execute(&self, input: &Value, _ctx: &ToolContext) -> Result<String, String> {
        execute_speak_text(input).await
    }
}

// ─── Gemini response types ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct GeminiToolResponse {
    candidates: Vec<GeminiToolCandidate>,
}

#[derive(serde::Deserialize)]
struct GeminiToolCandidate {
    content: GeminiToolContent,
}

#[derive(serde::Deserialize)]
struct GeminiToolContent {
    parts: Vec<GeminiToolPart>,
}

#[derive(serde::Deserialize)]
struct GeminiToolPart {
    text: Option<String>,
    #[serde(rename = "inlineData")]
    inline_data: Option<GeminiToolInlineData>,
}

#[derive(serde::Deserialize)]
struct GeminiToolInlineData {
    #[serde(rename = "mimeType")]
    mime_type: String,
    data: String,
}

const GEMINI_IMAGE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-image:generateContent";

// ─── Private execute functions ────────────────────────────────────────────────

async fn execute_generate_image(input: &Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image generation unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let body = json!({
        "contents": [{
            "parts": [{ "text": prompt }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"]
        }
    });

    let client = reqwest::Client::new();
    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image data from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

async fn execute_edit_image(input: &Value) -> Result<String, String> {
    let api_key = std::env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY not set — image editing unavailable")?;

    let prompt = input
        .get("prompt")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: prompt")?;

    let image_url = input
        .get("image_url")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: image_url")?;

    // Fetch the source image and convert to base64
    let client = reqwest::Client::new();
    let img_response = client
        .get(image_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch source image: {}", e))?;

    if !img_response.status().is_success() {
        return Err(format!(
            "Failed to fetch image ({}): {}",
            img_response.status(),
            image_url
        ));
    }

    let content_type = img_response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    let img_bytes = img_response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image bytes: {}", e))?;

    use base64::Engine;
    let img_b64 = base64::engine::general_purpose::STANDARD.encode(&img_bytes);

    let body = json!({
        "contents": [{
            "parts": [
                { "text": prompt },
                {
                    "inline_data": {
                        "mime_type": content_type,
                        "data": img_b64
                    }
                }
            ]
        }],
        "generationConfig": {
            "responseModalities": ["TEXT", "IMAGE"]
        }
    });

    let response = client
        .post(GEMINI_IMAGE_URL)
        .header("x-goog-api-key", &api_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gemini API request failed: {}", e))?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Gemini API error: {}", error_text));
    }

    let gemini_resp: GeminiToolResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Gemini response: {}", e))?;

    // Extract image + text from response
    for candidate in &gemini_resp.candidates {
        for part in &candidate.content.parts {
            if let Some(ref inline_data) = part.inline_data {
                let result = json!({
                    "image": {
                        "mime_type": inline_data.mime_type,
                        "data": inline_data.data,
                    },
                    "description": candidate.content.parts.iter()
                        .filter_map(|p| p.text.as_deref())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                return serde_json::to_string_pretty(&result)
                    .map_err(|e| format!("Serialization error: {}", e));
            }
        }
    }

    Err("Gemini returned no image data".to_string())
}

async fn execute_speak_text(input: &Value) -> Result<String, String> {
    let text = input
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: text")?;

    if text.len() > 5000 {
        return Err("Text exceeds maximum length of 5000 characters".to_string());
    }

    let voice_str = input
        .get("voice")
        .and_then(|v| v.as_str())
        .unwrap_or("narrator");

    let voice_style = match voice_str {
        "conversational" => VoiceStyle::Conversational,
        "storyteller" => VoiceStyle::Storyteller,
        _ => VoiceStyle::Narrator,
    };

    let api_key = std::env::var("CARTESIA_API_KEY")
        .map_err(|_| "CARTESIA_API_KEY not set — voice synthesis unavailable".to_string())?;

    let client = CartesiaClient::new(api_key);

    let audio_bytes = client
        .synthesize(text, voice_style)
        .await
        .map_err(|e| format!("Cartesia API error: {}", e))?;

    let duration_ms = client.estimate_duration_ms(text);

    // Encode as base64 for transport
    use base64::Engine;
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&audio_bytes);

    Ok(json!({
        "audio": audio_base64,
        "format": "pcm_f32le",
        "sample_rate": 44100,
        "duration_ms": duration_ms,
        "character_count": text.len(),
    })
    .to_string())
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_names_are_dispatchable() {
        for tool in tools() {
            assert!(!tool.name().is_empty(), "tool has empty name");
        }
    }

    #[test]
    fn all_categories_are_media() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Media,
                "tool `{}` has wrong category",
                tool.name()
            );
        }
    }

    #[test]
    fn input_schemas_are_objects() {
        for tool in tools() {
            let schema = tool.input_schema();
            assert_eq!(
                schema["type"],
                "object",
                "tool `{}` input_schema missing \"type\": \"object\"",
                tool.name()
            );
        }
    }

    #[test]
    fn tool_count_is_three() {
        assert_eq!(tools().len(), 3);
    }

    #[test]
    fn no_tool_requires_workspace() {
        for tool in tools() {
            assert!(
                !tool.requires_workspace(),
                "tool `{}` should NOT require workspace",
                tool.name()
            );
        }
    }
}
