# ABW as A2A Provider

**Status:** Design — approved for implementation  
**Date:** 2026-08-30  
**Author:** Ivan Labra  
**Scope:** Expose every published ABW agent as an A2A v1.0-conformant endpoint,
callable by external frameworks (Google ADK, LangGraph, CrewAI, Copilot, etc.)
with full billing, discovery, and execution parity with internal calls.

---

## 1. What we are building

ABW agents become first-class participants in the open agentic ecosystem.
Any external framework that speaks A2A can discover an ABW agent via its
`agent-card.json`, invoke it via `POST /message:send`, poll or stream its
result, and be billed per-execution. The agent owner earns a royalty. The
platform takes a fee. The existing internal execution pipeline — grounding
gates, output_contract validation, episode recording — runs unchanged; the
A2A layer is a translation boundary, not a replacement.

No existing internal mechanism changes. External callers get a standard
interface; internal callers continue using execute_agent and workspace messages.

---

## 2. URL structure

All A2A endpoints live under `/a2a/:agent_slug/`. This scopes each agent
cleanly and lets a caller discover any ABW agent given its slug.

```
GET  /a2a/:slug/agent-card.json        Agent Card (public, no auth)
POST /a2a/:slug/message:send           Execute (sync or async) — requires auth
POST /a2a/:slug/message:stream         Execute with SSE streaming — requires auth
GET  /a2a/:slug/tasks/:episode_id      Poll task state — requires auth
POST /a2a/:slug/tasks/:episode_id:cancel  Cancel in-flight — requires auth
```

Platform-level catalogue (optional, Phase 4):
```
GET  /.well-known/agent-directory.json  List of A2A-enabled agent slugs + card URLs
```

The AgentCard's `supportedInterfaces[0].url` for supply_chain_oracle would be:
`https://api.agent-bestiary.world/a2a/supply_chain_oracle`

---

## 3. Auth model — external principals

### 3.1 What already exists

`fermi_auth` already has a complete API key system:
- `create_api_key(pool, user_id, name, scopes)` → `(plaintext_key, ApiKey)`
- `validate_api_key(pool, key)` → `AuthPrincipal::ApiKey`
- Key format: `ferm_` + 64 hex chars. Prefix-indexed, Argon2-hashed.
- `ApiKey.scopes: Vec<String>` — already supports scope-gating.

### 3.2 New scopes for A2A

Add two scope values:

| Scope | Meaning |
|---|---|
| `a2a:invoke:*` | Can invoke any A2A-enabled agent on the platform |
| `a2a:invoke:<slug>` | Can invoke one specific agent (e.g. `a2a:invoke:supply_chain_oracle`) |

Scope validation happens at the A2A middleware layer before the request
reaches the handler. An API key without the relevant scope gets 403.

### 3.3 How external callers get keys

Two paths:

**Self-serve (Phase 2):** An ABW user (or the agent owner) generates an
A2A API key via the dashboard or via the `stripe_billing` agent's
`generate_client_api_key` tool. The key is issued under their user account.
They distribute it to their external framework's configuration.

**Agent-owner-gated (Phase 3):** An agent owner enables external access on
their agent and generates per-caller keys with a billing cap. The agent
owner's Stripe Connect account receives the royalty.

### 3.4 Auth header

Standard Bearer:
```
Authorization: Bearer ferm_<64 hex chars>
```
The existing `validate_api_key` middleware handles this. No new auth code
for Phase 1.

---

## 4. Agent Card mapping — ABW → A2A

The A2A AgentCard schema (v1.0) requires:
`name`, `description`, `supportedInterfaces`, `version`, `capabilities`,
`defaultInputModes`, `defaultOutputModes`, `skills`.

Mapping from the existing ABW agent card:

```
ABW field                            → A2A field
─────────────────────────────────────────────────────────────
agent_id                             → name (display; use metadata.description for desc)
metadata.description                 → description
version                              → version
metadata.tags                        → skills[0].tags
capabilities.mcp_tools               → skills (one skill per distinct use-case)
accepts                              → defaultInputModes (see §4.1)
produces                             → defaultOutputModes (see §4.1)
capabilities.output_contract         → skills[i].outputModes
metadata.sample_queries              → skills[i].examples
─ (ABW URL structure)                → supportedInterfaces[0].url
─ (always "HTTP+JSON")               → supportedInterfaces[0].protocolBinding
─ (always "1.0")                     → supportedInterfaces[0].protocolVersion
```

### 4.1 inputModes / outputModes from schema IDs

ABW's `accepts` and `produces` are schema IDs like `["scro/bom-query/1"]`.
A2A expects MIME types like `["application/json"]` or `["text/plain"]`.

Mapping rules:
- If any `accepts` entry is a schema ID (contains `/` and not a MIME type)
  → `defaultInputModes: ["application/json"]`
- If `accepts` is empty or contains free-text labels like `"query"`
  → `defaultInputModes: ["text/plain"]`
- If `produces` contains schema IDs
  → `defaultOutputModes: ["application/json"]`
- Otherwise
  → `defaultOutputModes: ["text/plain"]`

### 4.2 Skills

One skill per agent (simple starting point):

```json
{
  "id": "<agent_id>",
  "name": "<agent_id with underscores → spaces, title-cased>",
  "description": "<metadata.description>",
  "tags": "<metadata.tags>",
  "examples": "<metadata.sample_queries[0..3]>",
  "inputModes": "<same as defaultInputModes>",
  "outputModes": "<same as defaultOutputModes>"
}
```

Agents with multiple distinct use-cases can declare multiple skills in
Phase 3 via a new `a2a_skills` field on the agent card.

### 4.3 Capabilities

```json
"capabilities": {
  "streaming": true,
  "pushNotifications": false
}
```

Streaming is true because ABW already has `execute_agent_stream`. Push
notifications are Phase 4.

### 4.4 Security schemes

```json
"securitySchemes": {
  "bearerApiKey": {
    "httpAuthSecurityScheme": {
      "scheme": "Bearer",
      "bearerFormat": "ferm_<hex64>",
      "description": "ABW API key. Generate at https://agent-bestiary.world/settings/api-keys"
    }
  }
},
"securityRequirements": [{ "schemes": { "bearerApiKey": {} } }]
```

### 4.5 Example compiled AgentCard

For `supply_chain_oracle`:

```json
{
  "name": "supply_chain_oracle",
  "description": "Typed BoM pricing resolver and supply chain risk assessor...",
  "version": "2.0.0",
  "supportedInterfaces": [{
    "url": "https://api.agent-bestiary.world/a2a/supply_chain_oracle",
    "protocolBinding": "HTTP+JSON",
    "protocolVersion": "1.0"
  }],
  "capabilities": { "streaming": true, "pushNotifications": false },
  "securitySchemes": { "bearerApiKey": { "httpAuthSecurityScheme": { "scheme": "Bearer" } } },
  "securityRequirements": [{ "schemes": { "bearerApiKey": {} } }],
  "defaultInputModes": ["application/json"],
  "defaultOutputModes": ["application/json"],
  "skills": [{
    "id": "supply_chain_oracle",
    "name": "Supply Chain Oracle",
    "description": "Price a Bill of Materials and flag supply chain risks...",
    "tags": ["supply-chain", "pricing", "bom", "risk"],
    "examples": [
      "{\"task\":\"resolve_bom\",\"bom_items\":[{\"name\":\"Ashwagandha\",\"qty\":0.05,\"unit\":\"kg\",\"role\":\"substrate\"}]}"
    ],
    "inputModes": ["application/json"],
    "outputModes": ["application/json"]
  }]
}
```

---

## 5. Task API — Episode facade

### 5.1 Episode → Task state mapping

| ABW Episode state | A2A TaskState |
|---|---|
| Row reserved, not started | `TASK_STATE_SUBMITTED` |
| Execution in progress | `TASK_STATE_WORKING` |
| Completed successfully | `TASK_STATE_COMPLETED` |
| Failed (error) | `TASK_STATE_FAILED` |
| Cancelled | `TASK_STATE_CANCELED` |

`task.id` = `episode_id` (UUID). Callers can poll using the episode ID.

### 5.2 `POST /a2a/:slug/message:send` (sync, blocking)

Default behaviour (`returnImmediately: false`): run the agent synchronously
and return a completed Task.

**Request:**
```json
{
  "message": {
    "messageId": "<uuid>",
    "role": "ROLE_USER",
    "parts": [
      { "data": { "task": "resolve_bom", "bom_items": [...] } }
    ]
  }
}
```

**Extraction:** `parts[0].data` (if present, typed) OR `parts[0].text`
(free text) is used as the agent query. This maps cleanly to ABW's
existing text query model.

**Response (blocking):**
```json
{
  "task": {
    "id": "<episode_id>",
    "contextId": "<workspace_id or request-scoped uuid>",
    "status": { "state": "TASK_STATE_COMPLETED" },
    "artifacts": [{
      "artifactId": "<uuid>",
      "name": "agent_response",
      "parts": [{ "data": { /* agent's JSON output */ } }]
    }]
  }
}
```

If the agent returns prose (no JSON), `parts[0].text` is used instead of
`parts[0].data`.

**Non-blocking (`returnImmediately: true`):**
Returns `Task { status: TASK_STATE_SUBMITTED }` immediately. Caller polls
via `GET /a2a/:slug/tasks/:episode_id`.

### 5.3 `POST /a2a/:slug/message:stream`

SSE stream. Each event is a `StreamResponse` JSON object on a `data:` line:

```
data: {"task": {"id": "...", "status": {"state": "TASK_STATE_WORKING"}}}
data: {"artifactUpdate": {"taskId": "...", "artifact": {"parts": [{"data": {...}}]}}}
data: {"statusUpdate": {"taskId": "...", "status": {"state": "TASK_STATE_COMPLETED"}}}
```

Implementation: wrap the existing `execute_agent_stream_handler` SSE
output, translating token chunks into `artifactUpdate` events and the
final status into `statusUpdate`.

### 5.4 `GET /a2a/:slug/tasks/:episode_id`

Poll an episode's state. Returns a `Task` object. For completed episodes,
includes the artifact (agent's `raw_response`). For in-progress, returns
`TASK_STATE_WORKING` with no artifact.

### 5.5 Error mapping

| HTTP | A2A error |
|---|---|
| 401 | Missing/invalid API key |
| 403 | Valid key but wrong scope |
| 404 (agent) | Agent not found or not A2A-enabled |
| 404 (task) | `TaskNotFoundError` |
| 402 | Insufficient credits — body includes Stripe checkout URL |
| 429 | Rate limited |
| 500 | `InvalidAgentResponseError` |

---

## 6. Billing flow

### 6.1 What already exists

- `credit_charge(pool, user_id, amount, ...)` — debit credits from an account
- `agent_episode_payouts` — royalty mechanism
- `stripe_billing` agent — models Checkout, Connect, metered billing
- `gas.rs` — existing gas fee calculation
- `ApiKey.user_id` — links API key to a billing account

### 6.2 The A2A billing path

```
External caller (API key ferm_xyz)
  → validate_api_key → AuthPrincipal::ApiKey { user_id: "u_ext_123" }
  → check credits: credit_get_balance(pool, "u_ext_123")
    if < min_credits → 402 + Stripe checkout URL for credit top-up
  → execute agent (existing pipeline — unchanged)
  → debit credits: credit_charge(pool, "u_ext_123", execution_cost)
  → record royalty: agent_episode_payouts (agent owner earns)
  → platform fee: 10% of execution_cost (existing gas model)
```

The execution_cost is the same as for internal callers: LLM token cost +
gas fee. The external caller pays in credits loaded via Stripe.

### 6.3 Credit top-up for external callers

When an external caller has insufficient credits, the 402 response includes:
```json
{
  "error": {
    "code": 402,
    "message": "Insufficient credits. Top up your balance to continue.",
    "details": [{
      "@type": "type.googleapis.com/google.rpc.ErrorInfo",
      "reason": "INSUFFICIENT_CREDITS",
      "domain": "agent-bestiary.world",
      "metadata": {
        "current_balance": "0.00",
        "topup_url": "https://agent-bestiary.world/credits/topup?ref=a2a"
      }
    }]
  }
}
```

External callers can also pre-authorize via Stripe's metered billing
(managed by the `stripe_billing` agent).

### 6.4 Agent-owner pricing config (Phase 3)

New optional field on agent cards:

```json
"a2a": {
  "enabled": true,
  "pricing": {
    "model": "per_execution",
    "credits_per_call": 10,
    "description": "10 credits per BOM pricing call"
  }
}
```

`credits_per_call` is additional to the base LLM token cost — the owner's
surcharge. This is what flows to `agent_episode_payouts` as the royalty.
A `credits_per_call` of 0 means the agent is free (only LLM cost charged).

---

## 7. What does NOT change

- `execute_agent_handler`, `execute_agent_stream_handler` — unchanged
- `Pulse::grade`, `episode_boundary` — unchanged
- `envelope::build`, grounding gates — unchanged
- `output_contract`, TYPED_TIER_EXEMPT, contract_sketch — unchanged
- Internal `execute_agent` MCP tool — unchanged
- Workspace message handler — unchanged
- The agent card JSON format — one new optional `a2a` field in Phase 3

The A2A layer is a translation boundary. It reads the agent card to build
the AgentCard JSON and wraps the execute path to produce Task responses.
All verification still runs.

---

## 8. Which agents are A2A-enabled

Phase 1–2: All published curated agents with `tier: "curated"` and
`visibility: "public"` are A2A-enabled by default. The agent-card.json
endpoint returns 404 for non-public or non-curated agents.

Phase 3: Agent owners can opt out via `a2a: { "enabled": false }`.

---

## 9. Implementation phases

### Phase 1 — Discovery (no billing, read-only)

**Deliverable:** `GET /a2a/:slug/agent-card.json` works for any public
curated agent. No auth required (it's a public discovery endpoint).

**Implementation:**
1. New handler `handlers::a2a::agent_card_handler`
2. Mapping function `AgentCard → A2A AgentCard JSON` (§4)
3. Route: `.route("/a2a/:slug/agent-card.json", get(...))`
4. Returns 404 for non-existent or non-public agents

**Test:** fetch `GET /a2a/supply_chain_oracle/agent-card.json` → valid A2A
AgentCard with correct skills, inputModes, outputModes, securitySchemes.

**No Stripe, no billing, no execution.** Pure discovery.

### Phase 2 — Sync execution with API key billing

**Deliverable:** `POST /a2a/:slug/message:send` works. API key auth.
Credits debited. Episode recorded. Task returned.

**Implementation:**
1. New handler `handlers::a2a::send_message_handler`
2. A2A auth middleware: validates Bearer token → `AuthPrincipal::ApiKey`
3. Scope check: `a2a:invoke:*` or `a2a:invoke:<slug>`
4. Credit check before execution; 402 if insufficient
5. Wrap existing `execute_agent_handler` logic; return Task JSON
6. `GET /a2a/:slug/tasks/:episode_id` → poll task state
7. New API key scope values in `fermi_auth`
8. `POST /api/me/api-keys` extension: accept `a2a:invoke:*` scope

**Test:** external caller with API key invokes supply_chain_oracle, gets
a COMPLETED Task with the priced BOM as an Artifact.

### Phase 3 — Streaming + agent-owner pricing

**Deliverable:** `POST /a2a/:slug/message:stream` SSE works. Agent-owner
`a2a.pricing` config on card. `credits_per_call` royalty flows to owner.

**Implementation:**
1. New handler `handlers::a2a::stream_message_handler`
2. SSE wrapping of `execute_agent_stream_handler` output
3. `a2a.enabled` + `a2a.pricing` fields on agent card (optional, Rust
   `serde(default)` — zero migration impact)
4. Royalty accounting in the A2A execute path via `agent_episode_payouts`
5. `stripe_billing`'s `generate_client_api_key` tool wired to produce
   keys with A2A scopes

### Phase 4 — Discovery at scale + push notifications

**Deliverable:** Platform-level agent directory. Optional webhook support.

**Implementation:**
1. `GET /.well-known/agent-directory.json` — list of A2A-enabled agents
   with their card URLs (links to Phase 1 endpoints)
2. Push notification config CRUD (`POST /a2a/:slug/tasks/:id/pushNotificationConfigs`)
3. Webhook delivery on task completion
4. xaman_ek updated to know about A2A endpoints per agent

---

## 10. New routes summary

```
# Phase 1
GET  /a2a/:slug/agent-card.json

# Phase 2
POST /a2a/:slug/message:send
GET  /a2a/:slug/tasks/:episode_id
POST /a2a/:slug/tasks/:episode_id:cancel

# Phase 3
POST /a2a/:slug/message:stream

# Phase 4
GET  /.well-known/agent-directory.json
POST /a2a/:slug/tasks/:episode_id/pushNotificationConfigs
GET  /a2a/:slug/tasks/:episode_id/pushNotificationConfigs/:config_id
DELETE /a2a/:slug/tasks/:episode_id/pushNotificationConfigs/:config_id
```

---

## 11. New DB requirements

None for Phase 1. Phase 2 requires:
- API key scopes already stored as `text[]` — add `a2a:invoke:*` to the
  allowed vocabulary in `fermi_auth`
- Optionally: `a2a_enabled` column on `agents` table (or read from card
  JSON — prefer card JSON to avoid a migration for Phase 2)

Phase 3:
- `a2a_pricing` JSONB on `agents` table (or read from card JSON)

Phase 4:
- `a2a_push_configs` table for webhook configurations

---

## 12. xaman_ek update

After Phase 1 ships, xaman_ek's card needs to know:
- Every curated public agent has an A2A endpoint at `/a2a/:slug/`
- The A2A-provider design doc is at `docs/DESIGN_a2a_provider.md`
- The URL pattern for agent discovery
