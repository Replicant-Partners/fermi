# Remote MCP servers as an ABW agent capability

**Status:** Implemented — client, dispatch, DB-backed config, API, UI.
**Code:** `src/agent_backend/mcp_client.rs`, `src/handlers/agents.rs`, `templates/agent_detail.html`

---

## What changed

ABW was an MCP **server** only, in two places:

- `src/bin/agent-mcp-server.rs` — SDK-based, stdio transport
- `src/handlers/mcp.rs` — hand-rolled JSON-RPC over HTTP, `/mcp/agents/:id`

There was no **client**, so an ABW agent could not consume tools from a
remote MCP server. Both directions now work, and they are separate
fields on the agent card.

| Card field | Direction | Meaning |
|---|---|---|
| `capabilities.mcp_tools` | ABW **serves** | Names of platform tools, resolved against the compile-time dispatch table in `tools_legacy::ToolRegistry::execute`. Doubles as the allowlist for `/mcp/agents/:id`. **Carries no endpoint.** |
| `capabilities.mcp_servers` | ABW **calls** | Remote MCP servers this agent may reach. Tools discovered at runtime. |

### The trap this removes

`mcp_tools` looks like it should point at a remote server. It never did —
`McpTool` is `{ name, description, input_schema }` with no URL. Declaring
a name there with no dispatch arm produced a **phantom tool**: the model
was told it existed, called it, and got `Unknown tool: X`.

(`agents.mcp_servers` also exists as a JSONB column. Despite the name it
is populated from `mcp_tools` at `src/handlers/agents.rs:963` and has
never been read back. It is *not* the source for this feature.)

---

## Adding an endpoint: a card edit, no Rust

```json
"capabilities": {
  "mcp_servers": [
    {
      "name": "polymonitor",
      "endpoint": "https://polymonitor.club/wm-api/mcp",
      "transport": "streamable_http",
      "auth": {
        "scheme": "bearer",
        "secret_key": "POLYMONITOR_API_KEY",
        "env": "POLYMONITOR_API_KEY"
      },
      "tool_allowlist": ["search_markets", "get_oracle_lifecycle"],
      "timeout_secs": 30
    }
  ]
}
```

At execution the platform calls `tools/list`, namespaces each result as
`polymonitor__get_oracle_lifecycle`, merges them into that agent's tool
schema, and routes calls back out via `tools/call`. **No new match arm,
no new Rust, no redeploy of tool code.**

### Fields

| Field | Required | Notes |
|---|---|---|
| `name` | yes | Namespace prefix. Sanitised to `[a-zA-Z0-9_-]`; a qualified name over 64 chars is skipped (both LLM APIs reject longer). |
| `endpoint` | yes | Streamable-HTTP JSON-RPC URL. |
| `transport` | no | Only `streamable_http`. `stdio` is not meaningful for a hosted platform. |
| `auth` | no | Omit for genuinely open servers. |
| `auth.scheme` | no | `bearer` (default) or `header` for a raw value. |
| `auth.header` | no | Defaults to `Authorization`. |
| `auth.secret_key` | no | Key in the agent's scoped encrypted secret store. **Preferred.** |
| `auth.env` | no | Process-env fallback for platform-funded integrations. |
| `tool_allowlist` | no | Empty = expose everything the server advertises. |
| `protocol_version` | no | Defaults to `2025-06-18`. |
| `timeout_secs` | no | Default 30. |

Secrets are **never** inlined in a card — only referenced.

---

## Design properties worth keeping

**1. Per-agent scoping is a real boundary.**
The catalogue is built from one card and carried on that agent's
`ToolContext`. This is deliberate, because builtin tools are *not*
scoped: `to_claude_tools_with_card` starts from every builtin and
`ToolRegistry::execute` performs no per-agent authorization, so every
agent currently receives all ~69 platform tools. Remote tools must not
inherit that, or one agent's third-party credential becomes every
agent's. An agent reaches a remote server only if its **own** card names
it.

**2. Builtins win.**
Remote dispatch is attempted only in the fallthrough of the builtin
`match`. A third-party server cannot shadow a platform tool by naming one
of its tools `execute_agent`.

**3. Partial failure degrades, it doesn't cascade.**
One dead endpoint costs that server's tools, not the agent's whole tool
surface. Failures are collected in `RemoteMcpCatalogue::failures` and
logged per agent rather than silently swallowed — a card that declares a
server which never resolves should be visibly broken, not quietly
toolless.

**4. Missing credentials fail closed.**
If a server declares `auth` and no credential resolves, discovery for
that server errors. It does **not** fall back to an anonymous call.

**5. `tools/list` is TTL-cached** (10 min, keyed by endpoint). Remote
catalogues change on deploy cadence, not request cadence.

**6. Both response shapes are handled.** Streamable HTTP permits a POST
to be answered with either `application/json` or `text/event-stream`;
`parse_jsonrpc` accepts a bare envelope or `data:` frames.

---

## Gotchas found in practice

- **The `MCP-Protocol-Version` header is mandatory.** Omitting it returns
  JSON-RPC `-32600 Unsupported or missing MCP-Protocol-Version`, not a
  4xx — easy to misread as a malformed body.
- **`tools/list` and `tools/call` can have different auth.** polymonitor
  leaves discovery open and gates calls. An agent will therefore list its
  tools happily and fail on first use if the key is missing. The error
  message names the server and the credential keys it looked for.
- **Remote IDs are not your IDs.** polymonitor's `market_id` is its own
  integer, not a Polymarket `conditionId`, `questionID`, or slug. Any
  agent bridging two systems must resolve and *verify* the mapping; a
  silent mis-join returns another entity's data, which is worse than an
  error. The shipped card enforces this in its system prompt.

---

## Known limitation

`agent_card_from_db` (`src/api_server.rs`) returns `mcp_tools: vec![]`
and `mcp_servers: vec![]`. `resolve_agent_card` prefers the filesystem
registry, so agents with an `agent_card.json` loaded at boot are
unaffected — but an agent existing **only** in the DB gets no platform
tools and no remote servers. Pre-existing behaviour for `mcp_tools`;
`mcp_servers` matches it rather than introducing a second, divergent
path. Fixing it requires correcting the `agents.mcp_servers` writer
first.

---

## Verifying

```bash
# Unit tests, incl. the card schema pinning
cargo test --lib mcp_client

# Live end-to-end discovery against a real third-party server
cargo test --lib mcp_client -- --ignored --nocapture
```

The live test uses polymonitor as a convenient real-world target: it
leaves `tools/list` open while gating `tools/call`, so it exercises
discovery, header handling, and response parsing with no credential, and
asserts the call fails with a clear auth error. It prints:

```
polymonitor__get_data_quality               ← get_data_quality
polymonitor__get_market_liquidity           ← get_market_liquidity
polymonitor__get_market_overview            ← get_market_overview
polymonitor__get_oracle_lifecycle           ← get_oracle_lifecycle
polymonitor__get_public_briefing            ← get_public_briefing
polymonitor__get_public_watchlist_snapshot  ← get_public_watchlist_snapshot
polymonitor__search_markets                 ← search_markets

expected auth error: MCP server 'polymonitor' error -32001: Sign in with
an administrator account or provide a scoped API key.
```

This is a **test target only**. No agent card ships against polymonitor:
its data surface is too immature to depend on (no fills/trade history, so
price must be taken on trust; no category-calibration reads; per-market
data quality unavailable — `get_data_quality` takes no arguments; briefing
tools require an out-of-band capability token; and its integer `market_id`
has no stable mapping to Polymarket identifiers). Deferred deliberately.

---

## Where config lives: the DB is the source of truth

Agent cards are files loaded into an in-memory registry at boot. They are
not writable at runtime (deployed filesystems are read-only), so the file
card is a **seed**, and `agents.mcp_servers` (JSONB) is authoritative.
`resolve_agent_card` bridges DB over card, exactly as it already did for
`provider`, `model`, `temperature`, and `system_prompt`.

Precedence is **override, not merge** — see `interpret_db_column`:

| Column state | Effect |
|---|---|
| `NULL` | Inherit whatever the file card declares. |
| `[]` or `{}` | Explicitly no servers, even if the card declares some. This is how the UI *removes* a file-declared server. |
| non-empty | Authoritative replacement. |
| unparseable, or legacy tool-shaped | Ignored; falls back to the file card, with a log line. |

A merge would leave no way to express removal and would make precedence
ambiguous on name collisions. The UI seeds the DB from the file card on
first save, so override costs nothing.

### The legacy `mcp_tools` spill

`create_agent` wrote the card's **`mcp_tools`** into `agents.mcp_servers`
for its entire life — wrong field, and invisible because nothing read the
column. 15 rows in production held tool declarations like
`[{"name":"execute_agent","description":...}]`.

Those parse cleanly as `RemoteMcpServer` (`name` has a default,
`endpoint` is optional), so making the column authoritative would have
produced phantom servers named after tools *and erased the real servers
file cards declare*. Two independent defences:

1. `interpret_db_column` ignores any column whose entries carry no
   `endpoint`/`url`/`streamable_url`/`command` — covers rows written
   before the cleanup and any future spill.
2. `migrations/177_agents_mcp_servers_legacy_cleanup.sql` nulls the
   legacy rows so the column means what it says.

The writer itself is fixed. Regression test:
`legacy_tool_shaped_db_column_is_ignored`.

---

## API

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/agents/:agent_id/mcp-servers` | Effective config + provenance + diagnostics. |
| `POST` | `/api/agents/:agent_id/mcp-servers/test` | Discover against a candidate config **before** saving. |
| `PUT` | `/api/agents/:agent_id` | Write, via the normal agent update with an `mcp_servers` field. |

Writes deliberately reuse the existing agent PUT so they inherit the RBAC
ladder, agent versioning, and the `agent_card.updated` broadcast. Both
new endpoints are edit-gated: endpoints and credential key names are
operational detail, not catalogue metadata.

**Credential values are never returned.** The GET projects key *names*
(`auth.secret_key`, `auth.env`, `credential_keys`) plus
`credential_resolved`, so an operator can see which servers are one
secret away from working without the API ever echoing a secret. The full
`auth` block minus values is projected so a read → edit → save round trip
is lossless — without `scheme`/`header` a client has to assume `bearer`
and would silently downgrade a server using a custom header.

`POST .../test` matters because every failure here is external and
otherwise silent: a wrong endpoint, a missing credential, a server whose
`tools/list` is open but whose `tools/call` is gated. It runs under the
agent owner's secret scope, so success means *this agent* can really
reach the server.

---

## UI

"Remote MCP Servers" panel on the agent detail page (Manage tab, which
already follows the per-section-Save pattern). Add/edit/remove servers,
test a connection and see the namespaced tool list it would expose, and
supply a missing secret — written to `/api/secrets` scoped to that agent
by `agent_name`, never displayed or pre-filled.

The panel distinguishes DB-owned config from card-inherited config,
because saving takes ownership: after the first save, edits to the file
card no longer take effect.

One sharp edge surfaced in the UI rather than hidden: `store_secret` is
`ON CONFLICT (user_id, secret_name) DO UPDATE ... scope = $5`, so storing
an agent-scoped secret whose name already exists globally **narrows** the
existing secret to that agent, affecting other agents relying on it.
