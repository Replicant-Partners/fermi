//! # A2A Task JSON builders
//!
//! Converts ABW execution results (AgentOutput + episode_id) to A2A v1.0
//! Task objects. Pure logic — no async, no AppState.
//!
//! Design: `docs/DESIGN_a2a_provider.md §5`

use serde_json::{json, Value};
use uuid::Uuid;

/// A2A TaskState enum values (A2A v1.0 protobuf names).
pub mod state {
    pub const SUBMITTED: &str = "TASK_STATE_SUBMITTED";
    pub const WORKING: &str = "TASK_STATE_WORKING";
    pub const COMPLETED: &str = "TASK_STATE_COMPLETED";
    pub const FAILED: &str = "TASK_STATE_FAILED";
    pub const CANCELED: &str = "TASK_STATE_CANCELED";
}

/// Wrap a completed agent execution as a Task response body.
///
/// The `raw_response` is placed in an Artifact:
/// - If it is valid JSON → `Part { data: <json> }`
/// - Otherwise → `Part { text: "<string>" }`
///
/// `context_id` is typically the caller's user_id (groups this caller's tasks).
pub fn completed_task(episode_id: Uuid, context_id: &str, raw_response: Option<&str>) -> Value {
    let artifact = build_artifact(episode_id, raw_response);
    json!({
        "task": {
            "id": episode_id.to_string(),
            "contextId": context_id,
            "status": {
                "state": state::COMPLETED,
                "timestamp": chrono::Utc::now().to_rfc3339()
            },
            "artifacts": [artifact]
        }
    })
}

/// A Task that failed execution.
pub fn failed_task(episode_id: Uuid, context_id: &str, reason: &str) -> Value {
    json!({
        "task": {
            "id": episode_id.to_string(),
            "contextId": context_id,
            "status": {
                "state": state::FAILED,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "message": {
                    "role": "ROLE_AGENT",
                    "messageId": Uuid::new_v4().to_string(),
                    "parts": [{ "text": reason }]
                }
            },
            "artifacts": []
        }
    })
}

/// A Task that was just submitted (non-blocking path — poll via GET /tasks/:id).
pub fn submitted_task(episode_id: Uuid, context_id: &str) -> Value {
    json!({
        "task": {
            "id": episode_id.to_string(),
            "contextId": context_id,
            "status": {
                "state": state::SUBMITTED,
                "timestamp": chrono::Utc::now().to_rfc3339()
            },
            "artifacts": []
        }
    })
}

/// A Task that is currently executing (poll response when still in-progress).
pub fn working_task(episode_id: Uuid, context_id: &str) -> Value {
    json!({
        "task": {
            "id": episode_id.to_string(),
            "contextId": context_id,
            "status": {
                "state": state::WORKING,
                "timestamp": chrono::Utc::now().to_rfc3339()
            },
            "artifacts": []
        }
    })
}

/// Build an Artifact for use in `artifactUpdate` SSE events (Phase 3).
/// Public so the stream handler can construct it without duplicating the logic.
pub fn build_stream_artifact(episode_id: Uuid, raw_response: Option<&str>) -> Value {
    build_artifact(episode_id, raw_response)
}

/// Build one Artifact from a raw response string.
fn build_artifact(episode_id: Uuid, raw_response: Option<&str>) -> Value {
    let artifact_id = Uuid::new_v4();
    let parts = match raw_response {
        None | Some("") => vec![json!({ "text": "" })],
        Some(text) => {
            // Try to find and extract the first JSON object from the response
            // (agents often wrap JSON in markdown code blocks or prose).
            if let Some(json_val) = extract_json(text) {
                vec![json!({ "data": json_val })]
            } else {
                vec![json!({ "text": text })]
            }
        }
    };
    json!({
        "artifactId": artifact_id.to_string(),
        "name": "agent_response",
        "parts": parts,
        "metadata": {
            "abw_episode_id": episode_id.to_string()
        }
    })
}

/// Extract the first JSON object from a string that may contain markdown
/// code blocks or prose. Returns None if no valid JSON object is found.
pub fn extract_json(text: &str) -> Option<Value> {
    // Strip markdown code fences if present.
    let stripped = if let Some(start) = text.find("```json") {
        if let Some(end) = text[start..].find("\n```") {
            &text[start + 7..start + end]
        } else {
            text
        }
    } else {
        text
    };

    // Find the first `{` and try to parse from there.
    if let Some(start) = stripped.find('{') {
        if let Some(end) = find_matching_brace(stripped, start) {
            if let Ok(val) = serde_json::from_str(&stripped[start..=end]) {
                return Some(val);
            }
        }
    }
    None
}

/// Find the closing brace that matches the opening brace at `start`.
fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if escape_next {
            escape_next = false;
            continue;
        }
        match b {
            b'\\' if in_string => escape_next = true,
            b'"' => in_string = !in_string,
            b'{' if !in_string => depth += 1,
            b'}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_task_has_required_fields() {
        let id = Uuid::new_v4();
        let t = completed_task(id, "user_123", Some(r#"{"items":[],"oracle_note":"ok"}"#));
        let task = &t["task"];
        assert_eq!(task["id"], json!(id.to_string()));
        assert_eq!(task["status"]["state"], json!(state::COMPLETED));
        assert!(task["artifacts"].is_array());
        assert_eq!(
            task["artifacts"][0]["parts"][0]["data"]["oracle_note"],
            json!("ok")
        );
    }

    #[test]
    fn prose_response_becomes_text_part() {
        let id = Uuid::new_v4();
        let t = completed_task(id, "user_123", Some("This is a prose response."));
        assert_eq!(
            t["task"]["artifacts"][0]["parts"][0]["text"],
            json!("This is a prose response.")
        );
    }

    #[test]
    fn json_in_markdown_fence_is_extracted() {
        let raw = "Here is the result:\n```json\n{\"answer\": 42}\n```\nDone.";
        let val = extract_json(raw);
        assert!(val.is_some());
        assert_eq!(val.unwrap()["answer"], json!(42));
    }

    #[test]
    fn failed_task_has_failed_state() {
        let id = Uuid::new_v4();
        let t = failed_task(id, "user_123", "Something went wrong");
        assert_eq!(t["task"]["status"]["state"], json!(state::FAILED));
    }
}
