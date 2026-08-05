//! Outbound MCP client — lets an ABW agent *consume* tools from remote
//! MCP servers.
//!
//! # Why this exists
//!
//! Before this module ABW was an MCP **server** only, in two places:
//! `src/bin/agent-mcp-server.rs` (SDK, stdio) and the hand-rolled
//! JSON-RPC surface at `src/handlers/mcp.rs`. There was no client.
//!
//! The field that looked like the mechanism wasn't one:
//! `AgentCapabilities::mcp_tools` is a `Vec<McpTool>` of
//! name/description/input_schema with **no endpoint**, resolved against
//! the compile-time `match` in `tools_legacy::ToolRegistry::execute`.
//! Declaring a name with no match arm produced a *phantom tool* — the
//! LLM was told it existed, called it, and got `Unknown tool: X`.
//! (`agents.mcp_servers` JSONB existed in the DB but was written and
//! never read.)
//!
//! # The model
//!
//! An agent card declares zero or more **remote MCP servers**:
//!
//! ```json
//! "capabilities": {
//!   "mcp_servers": [{
//!     "name": "polymonitor",
//!     "endpoint": "https://polymonitor.club/wm-api/mcp",
//!     "auth": { "scheme": "bearer", "secret_key": "POLYMONITOR_API_KEY" }
//!   }]
//! }
//! ```
//!
//! At execution time we `tools/list` each server (TTL-cached), namespace
//! the results as `server__tool`, merge them into that agent's tool
//! schema, and route calls back out via `tools/call`.
//!
//! **Adding a new MCP endpoint to ABW is a card edit. No Rust changes,
//! no redeploy of tool code, no new match arm.**
//!
//! # Two properties worth preserving
//!
//! 1. **Per-agent scoping.** The catalogue is built from *one card* and
//!    lives on that agent's `ToolContext`. This matters because the
//!    builtin tool surface is global — `to_claude_tools_with_card`
//!    starts from every builtin and `execute` performs no per-agent
//!    authorization. Remote tools deliberately do not inherit that
//!    behaviour: an agent reaches a remote server only if its own card
//!    declares it.
//! 2. **Builtins win.** Remote dispatch is attempted only in the
//!    fallthrough of the builtin `match`, so a remote server cannot
//!    shadow a platform tool by naming a tool `execute_agent`.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// MCP revision we advertise. The spec requires this header on every
/// Streamable-HTTP request; omitting it gets you a JSON-RPC -32600
/// `Unsupported or missing MCP-Protocol-Version` rather than a 4xx,
/// which is easy to misread as a malformed body.
pub const DEFAULT_PROTOCOL_VERSION: &str = "2025-06-18";

/// How long a `tools/list` result stays fresh. Remote catalogues change
/// on deploy cadence, not request cadence.
const TOOLS_CACHE_TTL: Duration = Duration::from_secs(600);

/// Separator between server namespace and remote tool name. Anthropic
/// and OpenAI both constrain tool names to `[a-zA-Z0-9_-]{1,64}`, so a
/// dot is not usable.
pub const NS_SEP: &str = "__";

// ─── Card configuration ─────────────────────────────────────────────

/// Transport for a remote MCP server. Only Streamable HTTP is
/// implemented; `stdio` would require process management and is not
/// meaningful for a hosted platform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    #[default]
    StreamableHttp,
}

/// How to authenticate to a remote MCP server.
///
/// Secrets are never written into the card. `secret_key` names an entry
/// in the agent's already-scoped encrypted secret store (resolved via
/// `fermi_auth::get_secrets_for_agent` into
/// `ToolContext::user_secrets`); `env` is a process-level fallback for
/// platform-owned integrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMcpAuth {
    /// `"bearer"` (default) or `"header"` for a raw header value.
    #[serde(default = "default_scheme")]
    pub scheme: String,
    /// Header to carry the credential. Defaults to `Authorization`.
    #[serde(default)]
    pub header: Option<String>,
    /// Key to look up in the agent's scoped secret store.
    #[serde(default)]
    pub secret_key: Option<String>,
    /// Environment variable fallback.
    #[serde(default)]
    pub env: Option<String>,
}

fn default_scheme() -> String {
    "bearer".to_string()
}

/// A remote MCP server an agent is authorised to call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMcpServer {
    /// Namespace prefix for this server's tools. Must be
    /// `[a-zA-Z0-9_-]+`; it becomes part of the tool name the model sees.
    pub name: String,
    /// Streamable-HTTP JSON-RPC endpoint.
    pub endpoint: String,
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub auth: Option<RemoteMcpAuth>,
    /// If non-empty, only these remote tool names are exposed. Use to
    /// narrow a broad third-party surface to what the agent actually
    /// needs — the remote server's own scoping is not under our control.
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    /// Override the advertised MCP protocol revision.
    #[serde(default)]
    pub protocol_version: Option<String>,
    /// Per-call timeout in seconds. Default 30.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl RemoteMcpServer {
    fn protocol(&self) -> &str {
        self.protocol_version
            .as_deref()
            .unwrap_or(DEFAULT_PROTOCOL_VERSION)
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs.unwrap_or(30))
    }

    /// Sanitised namespace, safe for a tool name.
    fn ns(&self) -> String {
        self.name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }
}

// ─── Discovered tools ───────────────────────────────────────────────

/// A tool as advertised by a remote server's `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Option<serde_json::Value>,
}

/// One routable entry: the qualified name the model sees, plus enough
/// context to dispatch it.
#[derive(Debug, Clone)]
pub struct RoutedTool {
    /// `server__tool`, as presented to the model.
    pub qualified_name: String,
    /// Bare name as the remote server knows it.
    pub remote_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub server: RemoteMcpServer,
    /// Resolved bearer/header credential, if the server needs one.
    pub credential: Option<String>,
}

/// Per-agent catalogue of remote MCP tools.
///
/// Built once per execution from a single agent card. Cheap to clone
/// into an `Arc` and hang off `ToolContext`.
#[derive(Debug, Clone, Default)]
pub struct RemoteMcpCatalogue {
    routes: HashMap<String, RoutedTool>,
    /// Servers that failed discovery, with the reason. Surfaced to the
    /// operator rather than silently dropped — a card that declares a
    /// server which never resolves should be visibly broken, not quietly
    /// toolless.
    pub failures: Vec<(String, String)>,
}

impl RemoteMcpCatalogue {
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Every routable tool, sorted for deterministic prompt ordering.
    pub fn tools(&self) -> Vec<&RoutedTool> {
        let mut v: Vec<&RoutedTool> = self.routes.values().collect();
        v.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
        v
    }

    pub fn get(&self, qualified_name: &str) -> Option<&RoutedTool> {
        self.routes.get(qualified_name)
    }

    /// Discover tools across every server declared on a card.
    ///
    /// Failure of one server never blocks the others: a dead endpoint
    /// costs that server's tools, not the agent's whole tool surface.
    pub async fn discover(
        servers: &[RemoteMcpServer],
        secrets: Option<&HashMap<String, String>>,
    ) -> Self {
        let mut cat = Self::default();
        if servers.is_empty() {
            return cat;
        }

        for server in servers {
            let credential = match resolve_credential(server, secrets) {
                Ok(c) => c,
                Err(e) => {
                    cat.failures.push((server.name.clone(), e));
                    continue;
                }
            };

            let tools = match list_tools_cached(server, credential.as_deref()).await {
                Ok(t) => t,
                Err(e) => {
                    cat.failures.push((server.name.clone(), e));
                    continue;
                }
            };

            let ns = server.ns();
            for t in tools {
                if !server.tool_allowlist.is_empty()
                    && !server.tool_allowlist.iter().any(|a| a == &t.name)
                {
                    continue;
                }
                // A tool with no schema can't be offered to the model —
                // both APIs require one.
                let schema = match t.input_schema.clone() {
                    Some(s) => s,
                    None => serde_json::json!({ "type": "object", "properties": {} }),
                };
                let qualified = format!("{ns}{NS_SEP}{}", t.name);
                if qualified.len() > 64 {
                    cat.failures.push((
                        server.name.clone(),
                        format!("tool name '{qualified}' exceeds the 64-char API limit; skipped"),
                    ));
                    continue;
                }
                cat.routes.insert(
                    qualified.clone(),
                    RoutedTool {
                        qualified_name: qualified,
                        remote_name: t.name.clone(),
                        description: if t.description.is_empty() {
                            format!("[{}] {}", server.name, t.name)
                        } else {
                            format!("[{}] {}", server.name, t.description)
                        },
                        input_schema: schema,
                        server: server.clone(),
                        credential: credential.clone(),
                    },
                );
            }
        }

        cat
    }

    /// Dispatch a qualified tool call to its remote server.
    pub async fn call(
        &self,
        qualified_name: &str,
        args: &serde_json::Value,
    ) -> Result<String, String> {
        let route = self.routes.get(qualified_name).ok_or_else(|| {
            format!("Remote MCP tool '{qualified_name}' is not available to this agent")
        })?;
        call_tool(route, args).await
    }
}

// ─── Credential resolution ──────────────────────────────────────────

fn resolve_credential(
    server: &RemoteMcpServer,
    secrets: Option<&HashMap<String, String>>,
) -> Result<Option<String>, String> {
    let Some(auth) = &server.auth else {
        return Ok(None);
    };

    // Agent-scoped encrypted store first, process env second.
    if let Some(key) = &auth.secret_key {
        if let Some(v) = secrets.and_then(|s| s.get(key)) {
            if !v.trim().is_empty() {
                return Ok(Some(v.clone()));
            }
        }
        // A secret_key that also names an env var is a common and
        // harmless overlap; try it before giving up.
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Ok(Some(v));
            }
        }
    }

    if let Some(env_key) = &auth.env {
        if let Ok(v) = std::env::var(env_key) {
            if !v.trim().is_empty() {
                return Ok(Some(v));
            }
        }
    }

    Err(format!(
        "no credential available for MCP server '{}' (looked for secret_key={:?}, env={:?}). \
         Add it to the agent's scoped secret store or the process environment.",
        server.name, auth.secret_key, auth.env
    ))
}

fn apply_auth(
    mut req: reqwest::RequestBuilder,
    server: &RemoteMcpServer,
    credential: Option<&str>,
) -> reqwest::RequestBuilder {
    if let (Some(auth), Some(cred)) = (&server.auth, credential) {
        let header = auth.header.as_deref().unwrap_or("Authorization");
        let value = if auth.scheme.eq_ignore_ascii_case("bearer") {
            format!("Bearer {cred}")
        } else {
            cred.to_string()
        };
        req = req.header(header, value);
    }
    req
}

// ─── JSON-RPC over Streamable HTTP ──────────────────────────────────

fn tools_cache() -> &'static DashMap<String, (Instant, Vec<RemoteTool>)> {
    static CACHE: OnceLock<DashMap<String, (Instant, Vec<RemoteTool>)>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

async fn list_tools_cached(
    server: &RemoteMcpServer,
    credential: Option<&str>,
) -> Result<Vec<RemoteTool>, String> {
    let key = format!("{}|{}", server.endpoint, server.ns());
    if let Some(entry) = tools_cache().get(&key) {
        let (at, tools) = entry.value();
        if at.elapsed() < TOOLS_CACHE_TTL {
            return Ok(tools.clone());
        }
    }

    let tools = list_tools(server, credential).await?;
    tools_cache().insert(key, (Instant::now(), tools.clone()));
    Ok(tools)
}

/// `tools/list` against a remote server.
pub async fn list_tools(
    server: &RemoteMcpServer,
    credential: Option<&str>,
) -> Result<Vec<RemoteTool>, String> {
    let result = rpc(server, credential, "tools/list", serde_json::json!({})).await?;

    let arr = result
        .get("tools")
        .and_then(|t| t.as_array())
        .ok_or_else(|| format!("MCP server '{}' returned no tools array", server.name))?;

    Ok(arr
        .iter()
        .filter_map(|t| serde_json::from_value::<RemoteTool>(t.clone()).ok())
        .collect())
}

/// `tools/call` against a remote server, flattened to text.
pub async fn call_tool(route: &RoutedTool, args: &serde_json::Value) -> Result<String, String> {
    let result = rpc(
        &route.server,
        route.credential.as_deref(),
        "tools/call",
        serde_json::json!({
            "name": route.remote_name,
            "arguments": args,
        }),
    )
    .await?;

    // MCP tools/call → { content: [{type, text}], isError? }
    let is_error = result
        .get("isError")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let text = result
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| {
                    b.get("text")
                        .and_then(|t| t.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            // Non-text blocks (image/resource) — keep a
                            // structural trace rather than dropping them
                            // silently, so the model isn't misled into
                            // thinking the result was empty.
                            b.get("type")
                                .and_then(|t| t.as_str())
                                .map(|t| format!("[{t} content omitted]"))
                        })
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        // Some servers return structured results with no content block.
        .unwrap_or_else(|| result.to_string());

    if is_error {
        return Err(format!(
            "remote MCP tool '{}' reported an error: {text}",
            route.qualified_name
        ));
    }
    Ok(text)
}

/// One JSON-RPC round trip, returning the `result` object.
async fn rpc(
    server: &RemoteMcpServer,
    credential: Option<&str>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(server.timeout())
        .user_agent("AgentBestiary/1.0 (+https://agent-bestiary.world)")
        .build()
        .map_err(|e| format!("http client build failed: {e}"))?;

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });

    let req = client
        .post(&server.endpoint)
        .header("Content-Type", "application/json")
        // Streamable HTTP servers may answer either shape; accept both.
        .header("Accept", "application/json, text/event-stream")
        .header("MCP-Protocol-Version", server.protocol())
        .json(&body);

    let resp = apply_auth(req, server, credential)
        .send()
        .await
        .map_err(|e| format!("MCP server '{}' unreachable: {e}", server.name))?;

    let status = resp.status();
    let raw = resp
        .text()
        .await
        .map_err(|e| format!("MCP server '{}' body read failed: {e}", server.name))?;

    if !status.is_success() && raw.trim().is_empty() {
        return Err(format!(
            "MCP server '{}' returned HTTP {status}",
            server.name
        ));
    }

    let envelope: serde_json::Value = parse_jsonrpc(&raw).ok_or_else(|| {
        format!(
            "MCP server '{}' returned an unparseable response (HTTP {status}): {}",
            server.name,
            raw.chars().take(300).collect::<String>()
        )
    })?;

    if let Some(err) = envelope.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error");
        // -32001 with AUTH_REQUIRED is the common misconfiguration; make
        // it actionable rather than opaque.
        let hint = if msg.to_ascii_lowercase().contains("auth")
            || err
                .get("data")
                .and_then(|d| d.get("code"))
                .and_then(|c| c.as_str())
                .map(|c| c.contains("AUTH"))
                .unwrap_or(false)
        {
            format!(
                " \u{2014} check the credential for server '{}' (auth={:?})",
                server.name,
                server
                    .auth
                    .as_ref()
                    .map(|a| (&a.scheme, &a.secret_key, &a.env))
            )
        } else {
            String::new()
        };
        return Err(format!(
            "MCP server '{}' error {code}: {msg}{hint}",
            server.name
        ));
    }

    envelope
        .get("result")
        .cloned()
        .ok_or_else(|| format!("MCP server '{}' returned no result", server.name))
}

/// Parse either a bare JSON-RPC object or an SSE stream carrying one.
///
/// Streamable HTTP permits the server to answer a POST with
/// `text/event-stream`, in which case the payload arrives as one or more
/// `data:` lines. We take the last frame that parses and carries a
/// JSON-RPC envelope.
fn parse_jsonrpc(raw: &str) -> Option<serde_json::Value> {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
            return Some(v);
        }
    }

    let mut found = None;
    for line in raw.lines() {
        let line = line.trim();
        let Some(payload) = line.strip_prefix("data:") else {
            continue;
        };
        let payload = payload.trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
            if v.get("result").is_some() || v.get("error").is_some() {
                found = Some(v);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json() {
        let v = parse_jsonrpc(r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#).unwrap();
        assert!(v.get("result").is_some());
    }

    #[test]
    fn parses_sse_frames_and_takes_the_envelope() {
        let raw =
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let v = parse_jsonrpc(raw).unwrap();
        assert_eq!(v["result"]["ok"], serde_json::json!(true));
    }

    #[test]
    fn ignores_sse_noise_without_an_envelope() {
        assert!(parse_jsonrpc("data: {\"note\":\"ping\"}\n").is_none());
    }

    #[test]
    fn namespace_is_sanitised_for_tool_name_charset() {
        let s = RemoteMcpServer {
            name: "poly.monitor club".into(),
            endpoint: "https://example.test/mcp".into(),
            transport: McpTransport::default(),
            auth: None,
            tool_allowlist: vec![],
            protocol_version: None,
            timeout_secs: None,
        };
        assert_eq!(s.ns(), "poly_monitor_club");
    }

    #[test]
    fn missing_credential_is_an_error_not_a_silent_anonymous_call() {
        let s = RemoteMcpServer {
            name: "needs_auth".into(),
            endpoint: "https://example.test/mcp".into(),
            transport: McpTransport::default(),
            auth: Some(RemoteMcpAuth {
                scheme: "bearer".into(),
                header: None,
                secret_key: Some("DEFINITELY_NOT_SET_XYZ".into()),
                env: Some("ALSO_NOT_SET_XYZ".into()),
            }),
            tool_allowlist: vec![],
            protocol_version: None,
            timeout_secs: None,
        };
        assert!(resolve_credential(&s, None).is_err());
    }

    #[test]
    fn absent_auth_block_means_anonymous_is_intended() {
        let s = RemoteMcpServer {
            name: "open".into(),
            endpoint: "https://example.test/mcp".into(),
            transport: McpTransport::default(),
            auth: None,
            tool_allowlist: vec![],
            protocol_version: None,
            timeout_secs: None,
        };
        assert_eq!(resolve_credential(&s, None).unwrap(), None);
    }
}
