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
///
/// Field names follow the MCP ecosystem convention (Claude Desktop,
/// Cursor, `mcp.json`) as closely as possible, because cards in this repo
/// were already hand-authored in that style before there was a client to
/// read them. `url` / `streamable_url` are accepted as aliases for
/// `endpoint`, and `command` / `env` are parsed rather than rejected.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RemoteMcpServer {
    /// Namespace prefix for this server's tools. Sanitised to
    /// `[a-zA-Z0-9_-]`; it becomes part of the tool name the model sees.
    ///
    /// Optional in the map form, where the map key supplies it.
    #[serde(default)]
    pub name: String,
    /// Streamable-HTTP JSON-RPC endpoint.
    ///
    /// Optional at parse time so a malformed or stdio-only entry produces
    /// a clear per-server failure at discovery instead of failing the
    /// whole card — one bad server should not cost an agent its identity.
    #[serde(
        default,
        alias = "url",
        alias = "streamable_url",
        alias = "streamableUrl",
        alias = "endpoint_url"
    )]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub transport: McpTransport,
    #[serde(default)]
    pub auth: Option<RemoteMcpAuth>,
    /// If non-empty, only these remote tool names are exposed. Use to
    /// narrow a broad third-party surface to what the agent actually
    /// needs — the remote server's own scoping is not under our control.
    #[serde(default, alias = "tools")]
    pub tool_allowlist: Vec<String>,
    /// Override the advertised MCP protocol revision.
    #[serde(default)]
    pub protocol_version: Option<String>,
    /// Per-call timeout in seconds. Default 30.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// stdio launch command, from the desktop-client convention. Parsed
    /// for round-tripping and diagnostics only — ABW does not spawn
    /// processes, so a server with only a `command` is reported as
    /// unsupported rather than silently ignored.
    #[serde(default)]
    pub command: Option<String>,
    /// Environment-variable map from the desktop-client convention, e.g.
    /// `{"SERVICE_API_KEY": "${SERVICE_API_KEY}"}`. When no explicit
    /// `auth` block is present these keys are used as bearer-credential
    /// candidates.
    #[serde(default)]
    pub env: HashMap<String, String>,
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
    ///
    /// Public alias so declaration-validation can compare a published
    /// `server__tool` name against the namespace this server will actually
    /// generate, rather than against the raw card `name`.
    pub fn namespace(&self) -> String {
        self.ns()
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

    /// Usable HTTP endpoint, or an explanation of why there isn't one.
    ///
    /// Public so operator-facing surfaces can render the reason a server
    /// is unusable instead of just showing zero tools.
    pub fn http_endpoint(&self) -> Result<&str, String> {
        match self.endpoint.as_deref() {
            Some(e) if e.starts_with("http://") || e.starts_with("https://") => Ok(e),
            Some(e) => Err(format!("endpoint '{e}' is not an http(s) URL")),
            None if self.command.is_some() => Err(format!(
                "declares a stdio `command` ({}) but no http endpoint; ABW does not spawn MCP \
                 processes — add a `streamable_url`/`endpoint` for the hosted transport",
                self.command.as_deref().unwrap_or("?")
            )),
            None => Err("no `endpoint` (or `url`/`streamable_url`) declared".to_string()),
        }
    }

    /// Every credential key this server might use, most specific first.
    ///
    /// Public so operator-facing surfaces can report *which* secret a
    /// server is waiting on without ever handling its value.
    pub fn credential_key_names(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if let Some(auth) = &self.auth {
            if let Some(k) = &auth.secret_key {
                keys.push(k.clone());
            }
            if let Some(k) = &auth.env {
                if !keys.contains(k) {
                    keys.push(k.clone());
                }
            }
        }
        for k in self.env_credential_candidates() {
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        keys
    }

    /// Env-var names implied by the desktop-convention `env` map.
    /// `"${FOO}"` and `"$FOO"` both yield `FOO`; a literal value yields
    /// its own key, since that is the variable the server expects.
    fn env_credential_candidates(&self) -> Vec<String> {
        self.env
            .iter()
            .map(|(k, v)| {
                let t = v.trim();
                let inner = t
                    .strip_prefix("${")
                    .and_then(|s| s.strip_suffix('}'))
                    .or_else(|| t.strip_prefix('$'));
                match inner {
                    Some(name) if !name.is_empty() => name.to_string(),
                    _ => k.clone(),
                }
            })
            .collect()
    }
}

/// Accept either the ecosystem-standard map form or a sequence.
///
/// Map form (matches Claude Desktop / Cursor / `mcp.json`, and what cards
/// in this repo already use) — the key supplies the namespace:
///
/// ```json
/// "mcp_servers": {
///   "bioportal": { "streamable_url": "https://bioportal.fastmcp.app/mcp" }
/// }
/// ```
///
/// Sequence form, where each entry carries its own `name`:
///
/// ```json
/// "mcp_servers": [
///   { "name": "bioportal", "endpoint": "https://bioportal.fastmcp.app/mcp" }
/// ]
/// ```
/// Interpret the `agents.mcp_servers` DB column.
///
/// Returns `None` when the column should be ignored in favour of the
/// filesystem card, and `Some(list)` when the DB is authoritative (an
/// empty list is meaningful — it means "explicitly no servers").
///
/// # Legacy data
///
/// Before this feature existed, `create_agent` wrote the card's
/// **`mcp_tools`** into this column (`handlers/agents.rs`, now fixed).
/// Many rows therefore hold *tool* declarations:
///
/// ```json
/// [{"name": "execute_agent", "description": "...", "inputSchema": {...}}]
/// ```
///
/// Those parse cleanly as `RemoteMcpServer` — `name` has a default and
/// `endpoint` is optional — producing phantom servers named after tools.
/// Left unchecked, the DB-overrides-file rule would then *erase* the real
/// servers a file card declares and flood the logs with "no endpoint"
/// failures.
///
/// So: an entry is only a server declaration if it carries an `endpoint`
/// (or alias) or a stdio `command`. If nothing in the column qualifies
/// while the column is non-empty, it is legacy tool data and we fall back
/// to the file card.
pub fn interpret_db_column(raw: &serde_json::Value) -> Option<Vec<RemoteMcpServer>> {
    if raw.is_null() {
        return None;
    }

    let parsed = deserialize_mcp_servers(raw.clone()).ok()?;

    // Explicit empty => the operator removed every server. Authoritative.
    if parsed.is_empty() {
        return Some(Vec::new());
    }

    let servers: Vec<RemoteMcpServer> = parsed
        .into_iter()
        .filter(|s| s.endpoint.is_some() || s.command.is_some())
        .collect();

    if servers.is_empty() {
        // Non-empty column, nothing server-shaped: legacy mcp_tools spill.
        return None;
    }

    Some(servers)
}

pub fn deserialize_mcp_servers<'de, D>(d: D) -> Result<Vec<RemoteMcpServer>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum MapOrSeq {
        Map(std::collections::BTreeMap<String, RemoteMcpServer>),
        Seq(Vec<RemoteMcpServer>),
    }

    Ok(match MapOrSeq::deserialize(d)? {
        MapOrSeq::Map(m) => m
            .into_iter()
            .map(|(key, mut s)| {
                // The key is authoritative for the namespace; an inline
                // `name` only fills in when the key is somehow empty.
                if !key.is_empty() {
                    s.name = key;
                }
                s
            })
            .collect(),
        MapOrSeq::Seq(v) => v,
    })
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
            // Validate transport before anything else so a stdio-only or
            // malformed entry gets an actionable message.
            if let Err(e) = server.http_endpoint() {
                cat.failures.push((server.name.clone(), e));
                continue;
            }

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
    // Candidate keys, most specific first. An explicit `auth` block wins;
    // otherwise fall back to the desktop-convention `env` map so cards
    // authored in the ecosystem style work without rewriting.
    let candidates = server.credential_key_names();

    // No auth block and no env map => the card is asserting this server
    // is open. Honour that rather than inventing a requirement.
    if candidates.is_empty() {
        return Ok(None);
    }

    for key in &candidates {
        // Agent-scoped encrypted store first — per-agent, rotatable.
        if let Some(v) = secrets.and_then(|s| s.get(key)) {
            if !v.trim().is_empty() {
                return Ok(Some(v.clone()));
            }
        }
        // Process env second — platform-funded fallback.
        if let Ok(v) = std::env::var(key) {
            if !v.trim().is_empty() {
                return Ok(Some(v));
            }
        }
    }

    Err(format!(
        "no credential available for MCP server '{}' (tried {}). Add it to the agent owner's \
         scoped secret store, or set it in the environment.",
        server.name,
        candidates.join(", ")
    ))
}

fn apply_auth(
    mut req: reqwest::RequestBuilder,
    server: &RemoteMcpServer,
    credential: Option<&str>,
) -> reqwest::RequestBuilder {
    if let Some(cred) = credential {
        // Default to bearer on the Authorization header: that is what an
        // `env`-only card (desktop convention, no explicit auth block)
        // means in practice for a hosted HTTP server.
        let (header, scheme) = match &server.auth {
            Some(a) => (
                a.header.as_deref().unwrap_or("Authorization"),
                a.scheme.as_str(),
            ),
            None => ("Authorization", "bearer"),
        };
        let value = if scheme.eq_ignore_ascii_case("bearer") {
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
    let key = format!("{}|{}", server.http_endpoint()?, server.ns());
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
        .post(server.http_endpoint()?)
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

    /// The load-bearing claim of this module: adding a third-party MCP
    /// endpoint to ABW is a card edit, with no Rust change. This pins the
    /// exact card shape the UI writes and the loader reads — if the schema
    /// drifts, this fails rather than silently producing an agent with no
    /// tools.
    #[test]
    fn card_capabilities_block_deserialises() {
        let raw = r#"{
          "capabilities": {
            "executor": "llm",
            "mcp_tools": [],
            "mcp_servers": [
              {
                "name": "example_service",
                "endpoint": "https://example.test/mcp",
                "transport": "streamable_http",
                "auth": {
                  "scheme": "bearer",
                  "secret_key": "EXAMPLE_SERVICE_API_KEY"
                },
                "tool_allowlist": ["search", "lookup"],
                "timeout_secs": 30
              },
              { "name": "open_service", "endpoint": "https://open.test/mcp" }
            ]
          }
        }"#;

        let card: serde_json::Value = serde_json::from_str(raw).expect("card should be valid JSON");
        let servers: Vec<RemoteMcpServer> =
            serde_json::from_value(card["capabilities"]["mcp_servers"].clone())
                .expect("mcp_servers should deserialise into RemoteMcpServer");

        assert_eq!(servers.len(), 2);

        let a = &servers[0];
        assert_eq!(a.name, "example_service");
        assert_eq!(a.endpoint.as_deref(), Some("https://example.test/mcp"));
        assert_eq!(a.transport, McpTransport::StreamableHttp);
        assert_eq!(a.tool_allowlist.len(), 2);
        let auth = a.auth.as_ref().expect("auth block");
        assert_eq!(auth.scheme, "bearer");
        assert_eq!(auth.secret_key.as_deref(), Some("EXAMPLE_SERVICE_API_KEY"));
        assert_eq!(a.timeout_secs, Some(30));

        // Everything except name + endpoint is optional, and the defaults
        // are the safe ones: HTTP transport, no auth, no allowlist filter.
        let b = &servers[1];
        assert_eq!(b.transport, McpTransport::StreamableHttp);
        assert!(b.auth.is_none());
        assert!(b.tool_allowlist.is_empty());
        assert_eq!(b.protocol(), DEFAULT_PROTOCOL_VERSION);

        // Credentials are referenced by key, never inlined in a card.
        assert!(!raw.contains("Bearer "));
    }

    /// The ecosystem-standard map form, which is what cards in this repo
    /// were already hand-authored in (see biotech_analyst) before there
    /// was a client to read them. The map key supplies the namespace, and
    /// `streamable_url` is accepted as an alias for `endpoint`.
    #[test]
    fn map_form_and_ecosystem_aliases_deserialise() {
        let raw = r#"{
          "bioportal": {
            "command": "bioportal-mcp",
            "env": { "BIOPORTAL_API_KEY": "${BIOPORTAL_API_KEY}" },
            "streamable_url": "https://bioportal.fastmcp.app/mcp"
          }
        }"#;

        let mut de = serde_json::Deserializer::from_str(raw);
        let servers = deserialize_mcp_servers(&mut de).expect("map form should deserialise");

        assert_eq!(servers.len(), 1);
        let s = &servers[0];
        // Namespace comes from the map key.
        assert_eq!(s.name, "bioportal");
        // streamable_url aliased onto endpoint.
        assert_eq!(
            s.endpoint.as_deref(),
            Some("https://bioportal.fastmcp.app/mcp")
        );
        assert!(s.http_endpoint().is_ok());
        // command is retained for diagnostics, not executed.
        assert_eq!(s.command.as_deref(), Some("bioportal-mcp"));
        // "${VAR}" is unwrapped to the variable name.
        assert_eq!(
            s.env_credential_candidates(),
            vec!["BIOPORTAL_API_KEY".to_string()]
        );
        // No explicit auth block, but env implies a credential is needed,
        // so an unset variable must be an error rather than an anonymous
        // call.
        assert!(resolve_credential(s, None).is_err());
    }

    /// A stdio-only server must fail with an actionable message, not be
    /// silently skipped or attempted over HTTP.
    #[test]
    fn stdio_only_server_reports_unsupported_transport() {
        let raw = r#"{ "local_thing": { "command": "some-mcp-binary" } }"#;
        let mut de = serde_json::Deserializer::from_str(raw);
        let servers = deserialize_mcp_servers(&mut de).unwrap();

        let err = servers[0]
            .http_endpoint()
            .expect_err("should be unsupported");
        assert!(err.contains("does not spawn MCP processes"), "got: {err}");
    }

    /// Every curated card on disk must still parse. This is the regression
    /// guard for the schema: `mcp_servers` already existed in the wild in
    /// map form, and a Vec-only schema silently broke biotech_analyst.
    #[test]
    fn all_curated_cards_still_parse_mcp_servers() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/agents/curated");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).expect("curated dir") {
            let path = entry.expect("entry").path().join("agent_card.json");
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&path).expect("read card");
            let v: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
            let Some(ms) = v.get("capabilities").and_then(|c| c.get("mcp_servers")) else {
                continue;
            };
            let servers: Vec<RemoteMcpServer> = deserialize_mcp_servers(ms.clone())
                .unwrap_or_else(|e| panic!("{} mcp_servers failed: {e}", path.display()));
            for s in &servers {
                assert!(
                    !s.name.is_empty(),
                    "{} has an unnamed server",
                    path.display()
                );
            }
            checked += 1;
        }
        assert!(
            checked > 0,
            "expected at least one card declaring mcp_servers"
        );
    }

    /// Regression guard for the legacy `mcp_tools` spill. 8+ agents in
    /// production hold tool declarations in `agents.mcp_servers` because
    /// the old create path wrote the wrong field. These must be ignored,
    /// not treated as endpoint-less servers — otherwise the
    /// DB-overrides-file rule erases real file-card servers.
    #[test]
    fn legacy_tool_shaped_db_column_is_ignored() {
        let legacy = serde_json::json!([
            {
                "name": "execute_agent",
                "description": "Invoke a member agent.",
                "inputSchema": { "type": "object" }
            },
            { "name": "ask_xaman_ek", "description": "Navigator." }
        ]);
        assert!(
            interpret_db_column(&legacy).is_none(),
            "tool-shaped rows must fall back to the filesystem card"
        );
    }

    /// The three states the column encodes.
    #[test]
    fn db_column_precedence_states() {
        // NULL => inherit from the file card.
        assert!(interpret_db_column(&serde_json::Value::Null).is_none());

        // Explicit empty => authoritative "no servers", which is how the
        // UI removes a server a file card declared.
        assert!(interpret_db_column(&serde_json::json!([]))
            .expect("empty array is authoritative")
            .is_empty());
        assert!(interpret_db_column(&serde_json::json!({}))
            .expect("empty map is authoritative")
            .is_empty());

        // Real server => authoritative replacement.
        let real = serde_json::json!({
            "svc": { "streamable_url": "https://svc.test/mcp" }
        });
        let got = interpret_db_column(&real).expect("should be authoritative");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "svc");

        // Mixed: keep the real server, drop the tool-shaped noise.
        let mixed = serde_json::json!([
            { "name": "execute_agent", "description": "tool, not a server" },
            { "name": "svc", "endpoint": "https://svc.test/mcp" }
        ]);
        let got = interpret_db_column(&mixed).expect("should keep the real one");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "svc");
    }

    /// Published tool names must resolve to something dispatchable, or they
    /// become phantom tools: advertised to the model and over
    /// `/mcp/agents/:id`, called, then answered `Unknown tool: X`.
    #[test]
    fn tool_declaration_validation() {
        use crate::agent_backend::tools::{invalid_tool_declarations, ToolDeclarationError};

        let servers = vec![RemoteMcpServer {
            name: "my.svc".into(), // sanitises to my_svc
            endpoint: Some("https://svc.test/mcp".into()),
            ..Default::default()
        }];

        // A real platform tool passes.
        assert!(invalid_tool_declarations(&["execute_agent".to_string()], &servers).is_empty());

        // A remote tool passes when its server is declared — matched on the
        // SANITISED namespace, which is what dispatch actually generates.
        assert!(
            invalid_tool_declarations(&["my_svc__search".to_string()], &servers).is_empty(),
            "namespace should be compared post-sanitisation"
        );

        // A remote tool whose server isn't declared is rejected, and names
        // the server so the error is actionable.
        let bad = invalid_tool_declarations(&["other__search".to_string()], &servers);
        assert_eq!(
            bad,
            vec![(
                "other__search".to_string(),
                ToolDeclarationError::UnknownRemoteServer {
                    server: "other".to_string()
                }
            )]
        );

        // A plain invented name is rejected.
        let bad = invalid_tool_declarations(&["totally_made_up_tool".to_string()], &servers);
        assert_eq!(bad.len(), 1);
        assert_eq!(bad[0].1, ToolDeclarationError::NotDispatchable);

        // Empty publishes nothing and is always valid.
        assert!(invalid_tool_declarations(&[], &servers).is_empty());
    }

    /// The dispatch table must be non-empty and self-consistent — this is
    /// what validation is measured against, so a regression here silently
    /// disables the guard.
    #[test]
    fn platform_tool_catalogue_is_populated_and_unique() {
        use crate::agent_backend::tools::platform_tool_names;
        let names = platform_tool_names();
        assert!(names.len() > 20, "expected a populated catalogue");
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "duplicate builtin tool name");
        // Spot-check a few the platform depends on.
        for expected in ["execute_agent", "delegate_to_agent", "search_knowledge"] {
            assert!(names.contains(&expected), "missing builtin: {expected}");
        }
    }

    #[test]
    fn namespace_is_sanitised_for_tool_name_charset() {
        let s = RemoteMcpServer {
            name: "poly.monitor club".into(),
            endpoint: Some("https://example.test/mcp".into()),
            ..Default::default()
        };
        assert_eq!(s.ns(), "poly_monitor_club");
    }

    #[test]
    fn missing_credential_is_an_error_not_a_silent_anonymous_call() {
        let s = RemoteMcpServer {
            name: "needs_auth".into(),
            endpoint: Some("https://example.test/mcp".into()),
            auth: Some(RemoteMcpAuth {
                scheme: "bearer".into(),
                header: None,
                secret_key: Some("DEFINITELY_NOT_SET_XYZ".into()),
                env: Some("ALSO_NOT_SET_XYZ".into()),
            }),
            ..Default::default()
        };
        assert!(resolve_credential(&s, None).is_err());
    }

    /// Live end-to-end discovery against a real third-party MCP server.
    ///
    /// Network-dependent, so ignored by default. Run with:
    ///   `cargo test --lib mcp_client -- --ignored --nocapture`
    ///
    /// polymonitor leaves `tools/list` unauthenticated while gating
    /// `tools/call` behind an `mcp:read` key, which makes it a useful
    /// no-credential check that discovery, header handling, and response
    /// parsing all work against a server we don't control.
    #[tokio::test]
    #[ignore]
    async fn live_discovery_against_polymonitor() {
        let server = RemoteMcpServer {
            name: "polymonitor".into(),
            endpoint: Some("https://polymonitor.club/wm-api/mcp".into()),
            timeout_secs: Some(20),
            ..Default::default()
        };

        let cat = RemoteMcpCatalogue::discover(std::slice::from_ref(&server), None).await;
        assert!(
            cat.failures.is_empty(),
            "discovery reported failures: {:?}",
            cat.failures
        );
        assert!(!cat.is_empty(), "expected a non-empty remote catalogue");

        for t in cat.tools() {
            println!("{}  ← {}", t.qualified_name, t.remote_name);
            assert!(t.qualified_name.starts_with("polymonitor__"));
            assert!(t.qualified_name.len() <= 64);
        }

        // The oracle lifecycle read is the one that would replace Fermi's
        // price-threshold resolution heuristic, so assert it specifically.
        assert!(
            cat.get("polymonitor__get_oracle_lifecycle").is_some(),
            "expected get_oracle_lifecycle; got {:?}",
            cat.tools()
                .iter()
                .map(|t| &t.qualified_name)
                .collect::<Vec<_>>()
        );

        // Unauthenticated tools/call must surface a clear auth error
        // rather than silently returning something empty.
        let err = cat
            .call("polymonitor__get_data_quality", &serde_json::json!({}))
            .await
            .expect_err("expected AUTH_REQUIRED without a key");
        println!("expected auth error: {err}");
        assert!(err.to_ascii_lowercase().contains("auth"));
    }

    #[test]
    fn absent_auth_block_means_anonymous_is_intended() {
        let s = RemoteMcpServer {
            name: "open".into(),
            endpoint: Some("https://example.test/mcp".into()),
            ..Default::default()
        };
        assert_eq!(resolve_credential(&s, None).unwrap(), None);
    }
}
