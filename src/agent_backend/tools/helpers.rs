// src/agent_backend/tools/helpers.rs
//
// Phase 4: shared helpers used by multiple domain execute implementations.
//
// Kept minimal — only helpers that are genuinely cross-domain belong here.
// Domain-specific helpers (reduct_get, intention_ctx, etc.) live in their
// respective domain module files.

use super::ToolContext;
use serde_json::Value;
use uuid::Uuid;

/// Resolve an agent identifier field to a UUID.
///
/// Accepts either a bare UUID string or an agent name slug.  Name lookup is
/// async because it reads the agents table.  Used by the coordination,
/// observability, and platform domain tools that accept `agent_id` or
/// `agent_name` interchangeably.
pub(crate) async fn resolve_agent_id(
    input: &Value,
    field: &str,
    ctx: &ToolContext,
) -> Result<Uuid, String> {
    let s = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {}", field))?;

    if let Ok(uuid) = Uuid::parse_str(s) {
        return Ok(uuid);
    }

    // Treat as a name slug and look up the UUID in the agents table.
    ctx.memory_store
        .get_agent_by_name(s)
        .await
        .map(|a| a.agent_id)
        .map_err(|e| format!("Agent '{}' not found (tried as name slug): {}", s, e))
}

/// Parse a UUID from a named field in the input JSON.
///
/// Used by observability tools (query_eval_signals, classify_anomaly, etc.).
pub(crate) fn parse_uuid_field(input: &Value, field: &str) -> Result<Uuid, String> {
    let s = input
        .get(field)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Missing required parameter: {}", field))?;
    Uuid::parse_str(s).map_err(|e| format!("Invalid UUID for {field}: {e}"))
}
