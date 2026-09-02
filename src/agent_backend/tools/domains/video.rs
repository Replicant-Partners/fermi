// src/agent_backend/tools/domains/video.rs
//
// Phase 4 domain migration: Video tools.
//
// Five tools, all requires_workspace: false:
//   reduct_list_projects
//   reduct_get_project
//   reduct_get_transcript
//   reduct_create_reel
//   reduct_add_block
//
// Each is a zero-size struct implementing PlatformTool. execute() calls
// a private function defined in this module.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::agent_backend::tools::platform_tool::{PlatformTool, ToolCategory};
use crate::agent_backend::tools::ToolContext;

/// All Video-category platform tools, in registration order.
pub fn tools() -> Vec<Arc<dyn PlatformTool>> {
    vec![
        Arc::new(ReductListProjects),
        Arc::new(ReductGetProject),
        Arc::new(ReductGetTranscript),
        Arc::new(ReductCreateReel),
        Arc::new(ReductAddBlock),
    ]
}

// ─── reduct_list_projects ─────────────────────────────────────────────────────

struct ReductListProjects;

#[async_trait]
impl PlatformTool for ReductListProjects {
    fn name(&self) -> &'static str {
        "reduct_list_projects"
    }

    fn description(&self) -> &'static str {
        "List all projects in the Reduct.video workspace. Returns project IDs, titles, and metadata."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Video
    }

    async fn execute(&self, _input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_reduct_list_projects(ctx).await
    }
}

// ─── reduct_get_project ───────────────────────────────────────────────────────

struct ReductGetProject;

#[async_trait]
impl PlatformTool for ReductGetProject {
    fn name(&self) -> &'static str {
        "reduct_get_project"
    }

    fn description(&self) -> &'static str {
        "Get details of a Reduct.video project including its recordings and reels."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The Reduct project ID"
                }
            },
            "required": ["project_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Video
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_reduct_get_project(input, ctx).await
    }
}

// ─── reduct_get_transcript ────────────────────────────────────────────────────

struct ReductGetTranscript;

#[async_trait]
impl PlatformTool for ReductGetTranscript {
    fn name(&self) -> &'static str {
        "reduct_get_transcript"
    }

    fn description(&self) -> &'static str {
        "Get the transcript of a recording in a Reduct.video project. Returns segments with start/end timestamps and speaker labels."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The Reduct project ID"
                },
                "recording_id": {
                    "type": "string",
                    "description": "The recording ID within the project"
                },
                "format": {
                    "type": "string",
                    "enum": ["json", "txt"],
                    "description": "Transcript format. 'json' carries per-segment start/end timestamps and is the only form clip boundaries may be taken from; 'txt' is prose with no timestamps. Default: json",
                    "default": "json"
                }
            },
            "required": ["project_id", "recording_id"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Video
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_reduct_get_transcript(input, ctx).await
    }
}

// ─── reduct_create_reel ───────────────────────────────────────────────────────

struct ReductCreateReel;

#[async_trait]
impl PlatformTool for ReductCreateReel {
    fn name(&self) -> &'static str {
        "reduct_create_reel"
    }

    fn description(&self) -> &'static str {
        "Create a new reel (highlight compilation) in a Reduct.video project. Returns the new reel ID."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The Reduct project ID"
                },
                "title": {
                    "type": "string",
                    "description": "Title for the new reel"
                }
            },
            "required": ["project_id", "title"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Video
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_reduct_create_reel(input, ctx).await
    }
}

// ─── reduct_add_block ─────────────────────────────────────────────────────────

struct ReductAddBlock;

#[async_trait]
impl PlatformTool for ReductAddBlock {
    fn name(&self) -> &'static str {
        "reduct_add_block"
    }

    fn description(&self) -> &'static str {
        "Add a block to a Reduct.video reel. Use type 'doc-range' for video clips (requires recording_id, start, end times) or type 'title' for title cards (requires text)."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "project_id": {
                    "type": "string",
                    "description": "The Reduct project ID"
                },
                "reel_id": {
                    "type": "string",
                    "description": "The reel ID to add the block to"
                },
                "block_type": {
                    "type": "string",
                    "description": "Block type: 'doc-range' for video clip, 'title' for title card"
                },
                "recording_id": {
                    "type": "string",
                    "description": "Recording ID (required for doc-range blocks)"
                },
                "start": {
                    "type": "number",
                    "description": "Start time in SECONDS as a number, e.g. 412.6 (required for doc-range blocks). Not a timecode string: '6:52' is rejected."
                },
                "end": {
                    "type": "number",
                    "description": "End time in SECONDS as a number, e.g. 448.2 (required for doc-range blocks). Must be greater than start."
                },
                "text": {
                    "type": "string",
                    "description": "Title text (required for title blocks)"
                }
            },
            "required": ["project_id", "reel_id", "block_type"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Video
    }

    async fn execute(&self, input: &Value, ctx: &ToolContext) -> Result<String, String> {
        execute_reduct_add_block(input, ctx).await
    }
}

// ─── Reduct.video API helpers ─────────────────────────────────────────────────
//
// Reduct's REST API is version 3 and lives under `/api/v3`. The interactive
// documentation is at `/backstage/api/`, which is a logged-in single-page app
// and NOT the request path — pointing a client at it yields a redirect to
// `/login`, which is worth recording because the two are one character apart in
// a card description and only one of them is callable.

const REDUCT_BASE_URL: &str = "https://app.reduct.video/api/v3";

/// Name of the credential, in both the scoped secret store and the env.
const REDUCT_KEY_NAME: &str = "REDUCT_API_KEY";

/// The workspace API key for this execution.
///
/// Scoped secret store first, process env second — the ordering
/// `RemoteMcpAuth` already documents (`secret_key`, then `env` "for
/// platform-owned integrations"). It matters here for a specific case rather
/// than for symmetry: `video_analyst` is `curated`, so
/// `resolve_agent_owner_secrets` returns `None` for it by design and the env
/// key is the correct source. A **fork** of it is owner-owned, carries its
/// owner's `REDUCT_API_KEY` in `user_secrets`, and — while these functions
/// took no `ToolContext` at all — could not reach it. That fork would then
/// have read someone else's workspace on the platform's key, which is the
/// cross-tenant leak SPEC_28 closed for LLM providers and had left open for
/// tool credentials.
///
/// `ctx` is `Option` so the two keyless call shapes in this file stay
/// possible; `None` means env only, which is what a context-free caller
/// honestly has.
fn reduct_api_key(ctx: Option<&ToolContext>) -> Result<String, String> {
    if let Some(k) = ctx
        .and_then(|c| c.user_secrets.as_ref())
        .and_then(|s| s.get(REDUCT_KEY_NAME))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        return Ok(k.to_string());
    }
    match std::env::var(REDUCT_KEY_NAME) {
        Ok(k) if !k.trim().is_empty() => Ok(k.trim().to_string()),
        // Owner-facing, and deliberately does not tell the reader to set an
        // env var: the person who can fix this for an owned agent is its
        // owner, on their profile page. Same rule as
        // `ExecutionError::Unfunded`.
        _ => Err(format!(
            "No {REDUCT_KEY_NAME} available, so the Reduct.video tools cannot \
             run. An agent's owner sets it under Profile → Agent Secrets at \
             {}/profile; for a platform-operated agent it is deployment \
             configuration. Generate the key from Reduct at \
             https://app.reduct.video/backstage/api/ (Professional or \
             Enterprise plan). Report this rather than describing clips you \
             could not read.",
            crate::agent_backend::credentials::abw_base_url(),
        )),
    }
}

async fn reduct_get(path: &str, ctx: Option<&ToolContext>) -> Result<Value, String> {
    let api_key = reduct_api_key(ctx)?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("X-Auth-Key", &api_key)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

async fn reduct_post(path: &str, body: &Value, ctx: Option<&ToolContext>) -> Result<Value, String> {
    let api_key = reduct_api_key(ctx)?;
    let url = format!("{}{}", REDUCT_BASE_URL, path);
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("X-Auth-Key", &api_key)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .map_err(|e| format!("Reduct API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("Reduct API error {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Reduct response: {}", e))
}

// ─── Private execute functions ────────────────────────────────────────────────

async fn execute_reduct_list_projects(ctx: &ToolContext) -> Result<String, String> {
    let data = reduct_get("/project", Some(ctx)).await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_project(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let data = reduct_get(&format!("/project/{}", project_id), Some(ctx)).await?;
    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_get_transcript(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let recording_id = input
        .get("recording_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: recording_id")?;

    // Anything other than an explicit `txt` is `json`, and that default is
    // load-bearing rather than tidy: only the JSON form carries segment
    // timestamps, and a transcript without timestamps is one a model can only
    // guess clip boundaries from. A typo in this argument must not silently
    // downgrade the caller to the representation that invites fabrication.
    let format = input
        .get("format")
        .and_then(|v| v.as_str())
        .unwrap_or("json");

    let ext = if format == "txt" { "txt" } else { "json" };
    let path = format!(
        "/project/{}/recording/{}/transcript.{}",
        project_id, recording_id, ext
    );

    if ext == "txt" {
        // Plain text transcript — fetch as text, not JSON
        let api_key = reduct_api_key(Some(ctx))?;
        let url = format!("{}{}", REDUCT_BASE_URL, path);
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header("X-Auth-Key", &api_key)
            .send()
            .await
            .map_err(|e| format!("Reduct API request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Reduct API error {}: {}", status, error_text));
        }

        response
            .text()
            .await
            .map_err(|e| format!("Failed to read transcript: {}", e))
    } else {
        let data = reduct_get(&path, Some(ctx)).await?;
        serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
    }
}

async fn execute_reduct_create_reel(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let title = input
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: title")?;

    let data = reduct_post(
        &format!("/project/{}/reel", project_id),
        &json!({ "title": title }),
        Some(ctx),
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
}

async fn execute_reduct_add_block(input: &Value, ctx: &ToolContext) -> Result<String, String> {
    let project_id = input
        .get("project_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: project_id")?;

    let reel_id = input
        .get("reel_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: reel_id")?;

    let block_type = input
        .get("block_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing required parameter: block_type")?;

    let body = match block_type {
        "doc-range" => {
            let recording_id = input
                .get("recording_id")
                .and_then(|v| v.as_str())
                .ok_or("doc-range block requires recording_id")?;
            // `as_f64` rejects `"412.6"` and `"6:52"` alike, and the error
            // below says which was wanted. A formatted timecode is the
            // characteristic mistake here — see `abw/video_highlight_reel`'s
            // `clips.start_seconds` — and it must fail at the call rather than
            // be coerced into a number that plays the wrong moment.
            let start = input
                .get("start")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires `start` as a NUMBER of seconds (e.g. 412.6), not a timecode string")?;
            let end = input
                .get("end")
                .and_then(|v| v.as_f64())
                .ok_or("doc-range block requires `end` as a NUMBER of seconds (e.g. 448.2), not a timecode string")?;
            if end <= start {
                return Err(format!(
                    "doc-range block has end ({end}) at or before start ({start}). \
                     Reduct would store a zero- or negative-length clip, which \
                     plays as nothing and reads in the reel as a clip that \
                     exists."
                ));
            }

            json!({
                "type": "doc-range",
                "recording": recording_id,
                "start": start,
                "end": end
            })
        }
        "title" => {
            let text = input
                .get("text")
                .and_then(|v| v.as_str())
                .ok_or("title block requires text")?;

            json!({
                "type": "title",
                "text": text
            })
        }
        other => {
            return Err(format!(
                "Unknown block type: {}. Use 'doc-range' or 'title'.",
                other
            ))
        }
    };

    let data = reduct_post(
        &format!("/project/{}/reel/{}/block", project_id, reel_id),
        &body,
        Some(ctx),
    )
    .await?;

    serde_json::to_string_pretty(&data).map_err(|e| format!("Serialization error: {}", e))
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
    fn all_categories_are_video() {
        for tool in tools() {
            assert_eq!(
                tool.category(),
                ToolCategory::Video,
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
    fn tool_count_is_five() {
        assert_eq!(tools().len(), 5);
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
