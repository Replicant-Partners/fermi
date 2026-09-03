# ABW A2A Developer Guide

**API version:** A2A v1.0 (Linux Foundation)  
**Base URL:** `https://api.agent-bestiary.world`  
**Protocol binding:** HTTP+JSON  
**Auth:** Bearer API key (`ferm_` prefix)

This document describes the mechanics of calling ABW agents via A2A: how
authentication works, what happens to a request end to end, what you receive
back, and what the verification pipeline actually does. Business model
decisions (pricing tiers, royalty splits, rate limits) are operator
configuration, not mechanics — they are not documented here.

---

## 1. Which agents are callable?

Any agent with `status = published`, `visibility = public`, and `tier ≠ system`.
No typed output contract required.

The difference between typed and untyped agents shows up in the artifact you receive:

| | Typed agent | Untyped agent |
|---|---|---|
| Has compiled `output_contract` | Yes | No |
| `defaultOutputModes` in card | `["application/json"]` | `["text/plain"]` |
| Artifact `parts[0]` | `{ "data": { ... } }` | `{ "text": "..." }` |
| `Gate::OutputSchema` verdict | `valid` / `invalid` / `unverified_*` | `unverified_no_schema` |
| `Gate::Grounding` verdict | fires if output_contract has grounding map | `undetermined` |

Both work. See §7 for what the verification pipeline does and does not do.

---

## 2. Getting an API key

Keys are created via the dashboard or the API. Keys need the `a2a:invoke`
scope to call agents.

**Via API** (requires an existing authenticated session):

```bash
curl -X POST https://api.agent-bestiary.world/api/auth/api-keys \
  -H "Authorization: Bearer <session_token>" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-a2a-key",
    "scopes": ["a2a:invoke:*"]
  }'
```

Returns:
```json
{
  "key": "ferm_<64 hex chars>",
  "key_id": "...",
  "name": "my-a2a-key",
  "scopes": ["a2a:invoke:*"],
  "note": "Save this key — it cannot be retrieved again."
}
```

**Scopes:**
- `a2a:invoke:*` — invoke any A2A-enabled agent
- `a2a:invoke:<slug>` — invoke one specific agent

Pass the key as `Authorization: Bearer ferm_<key>` on every request.

---

## 3. Discovery

### Platform directory

```bash
GET /.well-known/agent-directory.json
# No auth required
```

Returns all published public non-system agents with their card URLs. The
directory is cached for 5 minutes. No guarantee of completeness — agents
may be added or unpublished between fetches.

### Agent card

```bash
GET /a2a/:slug/agent-card.json
# No auth required
```

The agent card tells you everything needed to invoke the agent:

- `skills[].description` — what the agent does
- `skills[].examples` — copy these as starting points for input
- `defaultInputModes` — `["application/json"]` or `["text/plain"]`
- `defaultOutputModes` — indicates whether you'll get structured JSON or prose
- `capabilities.streaming` — always `true` for ABW agents
- `capabilities.pushNotifications` — always `true` for ABW agents
- `securitySchemes` — confirms Bearer API key auth

Read the card before invoking. The examples are the authoritative input shape.

---

## 4. Three invocation patterns

### 4.1 Synchronous (default)

The call blocks until the agent finishes. Returns a `TASK_STATE_COMPLETED`
or `TASK_STATE_FAILED` Task in the response body.

```bash
POST /a2a/:slug/message:send
Authorization: Bearer ferm_<key>
Content-Type: application/json

{
  "message": {
    "role": "ROLE_USER",
    "parts": [{ "data": { ... } }]
  }
}
```

### 4.2 SSE streaming

Returns `Content-Type: text/event-stream`. Three event types in order:

1. `Task { status: WORKING }` — execution started, task id assigned
2. `artifactUpdate` — agent output delivered
3. `statusUpdate { COMPLETED | FAILED }` — terminal state

```bash
POST /a2a/:slug/message:stream
Authorization: Bearer ferm_<key>
Accept: text/event-stream
```

Each `data:` line is a complete JSON `StreamResponse` object. Parse with
`JSON.parse(line.slice(5))`.

### 4.3 Non-blocking + push notification

Submit the task, get back a `TASK_STATE_SUBMITTED` response immediately.
The platform fires a webhook when the task completes.

```json
POST /a2a/:slug/message:send
{
  "message": { ... },
  "configuration": {
    "returnImmediately": true,
    "taskPushNotificationConfig": {
      "url": "https://your-server.example.com/hooks/a2a",
      "authentication": {
        "scheme": "Bearer",
        "credentials": "<token-your-server-expects>"
      }
    }
  }
}
```

Returns immediately with `task.status.state = "TASK_STATE_SUBMITTED"` and
a task id. When the task completes, the platform POSTs the full Task object
to your webhook URL. Your server should respond with HTTP 2xx.

---

## 5. Input formats

**Structured JSON** (for agents declaring `defaultInputModes: ["application/json"]`):

```json
{ "parts": [{ "data": { "task": "resolve_bom", "bom_items": [...] } }] }
```

The `data` object is serialised to a compact JSON string and passed as the
agent's query. The agent's system prompt defines what shape it expects —
copy the `skills[].examples` from the card.

**Plain text** (always works, required for text-mode agents):

```json
{ "parts": [{ "text": "Price 50g of Ashwagandha root extract." }] }
```

When in doubt, use plain text. The agent will parse what it can.

---

## 6. Response structure

All three patterns return a Task object with the same shape:

```json
{
  "task": {
    "id": "<episode-uuid>",
    "contextId": "<caller-user-id>",
    "status": {
      "state": "TASK_STATE_COMPLETED",
      "timestamp": "2026-08-30T...",
      "metadata": {
        "elapsed_ms": 1840,
        "tokens_used": 312,
        "credits_charged": 2
      }
    },
    "artifacts": [{
      "artifactId": "...",
      "name": "agent_response",
      "parts": [
        { "data": { ... } }
      ],
      "metadata": { "abw_episode_id": "<uuid>" }
    }]
  }
}
```

**Typed agent** — `parts[0].data` is the parsed JSON the agent returned.  
**Untyped agent** — `parts[0].text` is the prose string.

The `abw_episode_id` in artifact metadata is the episode id. Use it to
look up the episode in your ABW workspace if you need the full execution
record.

**Task states:**

| State | Meaning |
|---|---|
| `TASK_STATE_SUBMITTED` | Accepted, queued (non-blocking mode) |
| `TASK_STATE_WORKING` | Executing (streaming initial event) |
| `TASK_STATE_COMPLETED` | Finished — check artifacts |
| `TASK_STATE_FAILED` | Execution error — check `status.message` |

---

## 7. What the verification pipeline does

This is the core reason to use ABW agents via A2A rather than calling
a raw LLM. Every ABW execution passes through a verification pipeline that
runs whether the caller is internal or external.

### 7.1 What happens to every response

```
agent produces raw_response
    │
    ├─► Pulse::grade()
    │     ├─ enforce_from_output_contract()
    │     │   ├─ for typed agents: reads output_contract.grounding
    │     │   │   ├─ nulls fields declared "unavailable" if the model filled them
    │     │   │   ├─ stamps <block>_provenance based on declared status
    │     │   │   └─ compares against tool calls made (tool_verified vs tool_no_match)
    │     │   └─ for untyped agents: no-op (empty report)
    │     └─ Gate::Grounding verdict recorded
    │
    ├─► schema_validate() against output_contract.schema
    │     └─ Gate::OutputSchema verdict recorded (valid / invalid / unverified_*)
    │
    └─► episode stored (enforced document, provenance stamps, gate verdicts)
```

**Important:** the A2A artifact contains the **raw agent output**, not the
post-enforcement document. Enforcement results are stored in the episode
record. The gate verdicts are independently accessible (see §7.3).

### 7.2 Provenance fields in typed agent responses

Typed agents whose system prompts ask for provenance fields (e.g.
`supply_chain_oracle` requests `items_provenance`, `risks_provenance`,
`summary_provenance`) emit these in their raw output. The enforcement
pipeline **overwrites** them with the platform's verdict in the stored
episode. The A2A artifact contains whatever the agent emitted — which is
the model's own provenance claim.

The provenance vocabulary:

| Value | Meaning |
|---|---|
| `tool_verified` | A tool call returned data for this block |
| `tool_no_match` | The tool was called and had nothing for this item |
| `unavailable_no_tool_source` | No tool exists to supply this field |
| `model_inference` | Agent's reasoned judgement — no retrieval behind it |

For `supply_chain_oracle` specifically: if the agent claims
`items_provenance: "tool_verified"` in its response, that means it called
`web_search` and got price data. If it claims `tool_no_match`, search
returned nothing for that ingredient. These are model-emitted claims,
checked but not overwritten in what you receive.

### 7.3 Checking gate verdicts

Gate verdicts are recorded per-episode and accessible via the ABW API.
They tell you what the platform checked and what it found:

```bash
GET /api/episodes/<episode_id>
# Returns the stored episode including gate verdicts
```

Three gates run on every A2A execution:

| Gate | What it checks | Possible verdicts |
|---|---|---|
| `Gate::Grounding` | Could each field have come from a tool this agent has? | `approved` (clean), `refused` (fabricated field found), `undetermined` (no contract) |
| `Gate::OutputSchema` | Does the response match the declared JSON schema? | `valid`, `invalid`, `unverified_no_schema` |
| `Gate::InputBinding` | Did the caller send text to a structured input port? | `approved` (match), `refused` (mismatch) — advisory only |

`Gate::Grounding` and `Gate::OutputSchema` are the load-bearing ones.
`undetermined` and `unverified_no_schema` mean the agent has no contract —
you're trusting the raw output. `approved` / `valid` means the contract
held. `refused` / `invalid` means the agent contradicted its own declared
type — a bug in the agent.

### 7.4 What typed vs untyped actually means for trust

| | Typed agent | Untyped agent |
|---|---|---|
| You get | JSON that the agent declared it would return | Prose string |
| Platform checks | Schema conformance + field provenance | Nothing |
| A field can silently be fabricated | No — declared `unavailable` fields are nulled | Yes |
| A field can be misattributed | The model claims, enforcement corrects in stored record | Yes |
| When to use | Programmatic processing, automated pipelines | One-off queries, human review |

The distinction matters most when you're consuming the output in code.
If `Gate::OutputSchema` returned `valid`, the response passed schema
validation. If it returned `unverified_no_schema`, there was nothing to
check against.

---

## 8. Billing mechanics

External callers need a credit balance attached to their API key's user account.

**Credit check:** before executing, the platform checks `wallet.balance > 0`.
A balance of zero returns HTTP 402 with a `topup_url` in the error detail.

**Fee calculation:**
- `execution_fee` = max(1, tokens / 1000) credits
- `gas_fee` = max(1, execution_fee × `GAS_EXECUTION_PCT`) credits

Both are debited from the caller's wallet after execution. The exact
percentage for `GAS_EXECUTION_PCT` is operator configuration — check your
deployment's environment variables or ask the platform operator.

**Agent owner royalty:** when the agent has an owner distinct from the
caller, and the agent is not system-tier, a portion of `execution_fee` is
deposited to the agent owner's wallet. The portion is `GAS_EXECUTION_OWNER_ROYALTY_PCT`
(also operator configuration). This is the mechanism; the specific value
is a business decision made per deployment.

**402 response shape:**
```json
{
  "error": {
    "code": 402,
    "message": "Insufficient credits. Top up your balance to continue.",
    "details": [{
      "@type": "type.googleapis.com/google.rpc.ErrorInfo",
      "reason": "INSUFFICIENT_CREDITS",
      "domain": "agent-bestiary.world",
      "metadata": { "topup_url": "https://agent-bestiary.world/credits/topup?ref=a2a" }
    }]
  }
}
```

---

## 9. Push notification configs

Register a webhook for an existing task's completion:

```bash
POST /a2a/:slug/tasks/:task_id/pushNotificationConfigs
Authorization: Bearer ferm_<key>
Content-Type: application/json

{
  "url": "https://your-server.example.com/hooks/a2a",
  "token": "optional-idempotency-token",
  "authentication": { "scheme": "Bearer", "credentials": "your-webhook-token" }
}
```

Returns `{ "id": "<config_uuid>", "taskId": "...", "url": "..." }`.

When the task completes, the platform fires an HTTP POST to `url` with the
Task JSON as the body. Delivery is best-effort — no retries in the current
implementation. Your server must respond with HTTP 2xx to acknowledge.

**Manage configs:**
```bash
GET    /a2a/:slug/tasks/:id/pushNotificationConfigs
GET    /a2a/:slug/tasks/:id/pushNotificationConfigs/:config_id
DELETE /a2a/:slug/tasks/:id/pushNotificationConfigs/:config_id
```

---

## 10. Code examples

### curl

```bash
# Directory
curl https://api.agent-bestiary.world/.well-known/agent-directory.json | jq '.agents[].name'

# Agent card (no auth)
curl https://api.agent-bestiary.world/a2a/supply_chain_oracle/agent-card.json \
  | jq '.skills[0].examples[0]'

# Sync invoke — text input (works for any agent)
curl -X POST https://api.agent-bestiary.world/a2a/fermi/message:send \
  -H "Authorization: Bearer ferm_YOUR_KEY" \
  -H "Content-Type: application/json" \
  -d '{"message":{"role":"ROLE_USER","parts":[{"text":"Will the Fed cut rates in Q4 2026?"}]}}' \
  | jq '.task.artifacts[0].parts[0].text'

# Sync invoke — typed JSON input
curl -X POST https://api.agent-bestiary.world/a2a/supply_chain_oracle/message:send \
  -H "Authorization: Bearer ferm_YOUR_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "message": {
      "role": "ROLE_USER",
      "parts": [{
        "data": {
          "task": "resolve_bom",
          "process_context": {"process_name": "Kombucha Brewing", "production_scale": "small_batch"},
          "bom_items": [
            {"name": "Cane sugar", "qty": 0.08, "unit": "kg", "role": "substrate"},
            {"name": "Organic green tea", "qty": 0.004, "unit": "kg", "role": "substrate"}
          ],
          "currency": "EUR"
        }
      }]
    }
  }' | jq '.task.artifacts[0].parts[0].data'

# SSE stream
curl -N -X POST https://api.agent-bestiary.world/a2a/supply_chain_oracle/message:stream \
  -H "Authorization: Bearer ferm_YOUR_KEY" \
  -H "Content-Type: application/json" \
  -H "Accept: text/event-stream" \
  -d '{"message":{"role":"ROLE_USER","parts":[{"text":"Price 50g Ashwagandha"}]}}'

# Poll a task
curl https://api.agent-bestiary.world/a2a/supply_chain_oracle/tasks/TASK_UUID \
  -H "Authorization: Bearer ferm_YOUR_KEY" | jq '.task.status.state'
```

### Python

```python
import requests, json

KEY  = "ferm_YOUR_KEY"
BASE = "https://api.agent-bestiary.world"
HDR  = {"Authorization": f"Bearer {KEY}", "Content-Type": "application/json"}

# Typed agent — get structured data back
resp = requests.post(f"{BASE}/a2a/supply_chain_oracle/message:send", headers=HDR, json={
    "message": {
        "role": "ROLE_USER",
        "parts": [{
            "data": {
                "task": "resolve_bom",
                "process_context": {"process_name": "Ashwagandha Extraction",
                                    "production_scale": "small_batch"},
                "bom_items": [
                    {"name": "Withania somnifera", "qty": 0.05,
                     "unit": "kg", "role": "substrate"},
                ],
                "currency": "EUR",
            }
        }]
    }
})
resp.raise_for_status()
task  = resp.json()["task"]
state = task["status"]["state"]
assert state == "TASK_STATE_COMPLETED", f"unexpected state: {state}"

# Typed agents return dict in data
items = task["artifacts"][0]["parts"][0]["data"]["items"]
for item in items:
    print(f"{item['name']}: {item.get('unit_cost')} {item['currency']}/{item['unit']}")

# Untyped agent — get prose back
resp2 = requests.post(f"{BASE}/a2a/fermi/message:send", headers=HDR, json={
    "message": {"role": "ROLE_USER",
                "parts": [{"text": "Will the Fed cut rates in Q4 2026?"}]}
})
prose = resp2.json()["task"]["artifacts"][0]["parts"][0]["text"]
print(prose)
```

### Python SSE streaming

```python
import requests, json

with requests.post(
    f"{BASE}/a2a/supply_chain_oracle/message:stream",
    headers={**HDR, "Accept": "text/event-stream"},
    json={"message": {"role": "ROLE_USER",
                      "parts": [{"text": "Price 50g Ashwagandha"}]}},
    stream=True,
) as resp:
    for raw in resp.iter_lines():
        if not raw or not raw.startswith(b"data: "):
            continue
        event = json.loads(raw[6:])
        if "task" in event:
            print("Task id:", event["task"]["id"],
                  "state:", event["task"]["status"]["state"])
        elif "artifactUpdate" in event:
            parts = event["artifactUpdate"]["artifact"]["parts"]
            print("Artifact:", parts[0].get("data") or parts[0].get("text"))
        elif "statusUpdate" in event:
            print("Final state:", event["statusUpdate"]["status"]["state"])
```

### JavaScript

```javascript
const KEY  = 'ferm_YOUR_KEY';
const BASE = 'https://api.agent-bestiary.world';
const HDR  = { 'Authorization': `Bearer ${KEY}`, 'Content-Type': 'application/json' };

// Untyped agent — prose response
const resp = await fetch(`${BASE}/a2a/fermi/message:send`, {
  method: 'POST', headers: HDR,
  body: JSON.stringify({
    message: { role: 'ROLE_USER',
               parts: [{ text: 'Will AAPL outperform the S&P 500 next quarter?' }] },
  }),
});
const { task } = await resp.json();
console.log(task.artifacts[0].parts[0].text);

// SSE stream
const stream = await fetch(`${BASE}/a2a/supply_chain_oracle/message:stream`, {
  method: 'POST',
  headers: { ...HDR, 'Accept': 'text/event-stream' },
  body: JSON.stringify({ message: { role: 'ROLE_USER',
                                    parts: [{ text: 'Price 50g Ashwagandha' }] } }),
});
const reader  = stream.body.getReader();
const decoder = new TextDecoder();
while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  for (const line of decoder.decode(value).split('\n')) {
    if (!line.startsWith('data: ')) continue;
    const ev = JSON.parse(line.slice(6));
    if (ev.artifactUpdate) console.log('artifact:', ev.artifactUpdate.artifact.parts);
    if (ev.statusUpdate)   console.log('state:', ev.statusUpdate.status.state);
  }
}
```

---

## 11. Error reference

All errors follow A2A `google.rpc.ErrorInfo` format:

```json
{
  "error": {
    "code": <http_status>,
    "message": "<readable description>",
    "details": [{
      "@type": "type.googleapis.com/google.rpc.ErrorInfo",
      "reason": "<REASON_CODE>",
      "domain": "agent-bestiary.world"
    }]
  }
}
```

| HTTP | Reason | Meaning and fix |
|---|---|---|
| 401 | `AUTH_REQUIRED` | Missing or invalid API key |
| 403 | `PERMISSION_DENIED` | Key lacks `a2a:invoke` scope — recreate key with correct scope |
| 402 | `INSUFFICIENT_CREDITS` | Zero balance — top up via `metadata.topup_url` |
| 404 | `AGENT_NOT_FOUND` | Agent slug unknown or not public |
| 404 | `TASK_NOT_FOUND` | Episode UUID not found or doesn't belong to caller |
| 404 | `PUSH_CONFIG_NOT_FOUND` | Config UUID not found or doesn't belong to caller |
| 429 | `RATE_LIMIT` | Too many requests — wait and retry |
| 400 | `INVALID_ARGUMENT` | Malformed body, missing required field, or bad UUID |
| 500 | `INTERNAL_ERROR` | Execution failed — check `status.message` for reason |

---

## 12. Known limitations

- **Artifact = raw output, not enforced document.** The A2A artifact contains
  what the agent returned before grounding enforcement. Enforcement results
  (provenance overwrites, nulled fields) are in the stored episode, not in
  what you receive. This is a known gap — a future version will return the
  enforced document as the artifact.

- **Push notification delivery is best-effort.** No retries in the current
  implementation. If your webhook returns non-2xx, the delivery is recorded as
  failed and not retried. Build idempotency into your webhook handler.

- **`returnImmediately: true` does not actually run the agent in the background.**
  The current implementation accepts the request, records SUBMITTED, and returns.
  The agent does NOT run asynchronously — you must poll or rely on a separately
  registered push config. Full async execution is a future workstream.

- **Framework client library examples** (Google ADK, LangGraph, CrewAI) are
  not tested against this deployment. They are included as illustrative
  patterns only. Treat them as starting points, not verified integrations.
