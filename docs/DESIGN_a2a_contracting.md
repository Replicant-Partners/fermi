# A2A Contracting in ABW

> **Naming note:** This document uses "A2A" to mean two distinct things that
> must not be conflated:
>
> - **A2A standard** (Agent2Agent Protocol v1.0, Linux Foundation) — an *external
>   federation protocol* for cross-platform agent communication.
> - **ABW A2A contracting** — an *internal typed-RPC contract system* within the
>   ABW platform. What this document designs.
>
> See §11 for the full conformance assessment and the decision between them.

**Status:** Design reference — living document  
**Date:** 2026-08-30  
**Covers:** How agent-to-agent typed contracts work, where enforcement happens,
what gaps remain, and how to extend the pattern to new agent pairs.

---

## 1. One sentence

Every agent declares what it produces (`output_contract`). Every execution path
enforces that declaration. No per-agent code change is needed for new typed agents.

That is the goal. This document records how close we are, where the gaps are,
and what it takes to close each one.

---

## 2. The contract surface — on the agent card

```json
capabilities: {
  "output_contract": {
    "domain":          "supply-chain",
    "produces_schema": "scro/bom_response",
    "schema":          { /* full JSON Schema — compiled, not hand-written */ },
    "grounding": {
      "items":   { "status": "sourced",     "tool": "web_search", "why": "…" },
      "risks":   { "status": "inferred",    "from": "items + domain knowledge", "why": "…" },
      "summary": { "status": "inferred",    "from": "items (arithmetic) + oracle synthesis", "why": "…" }
    },
    "calibration": { "signal": "market_price_check", "comparison": "predicted_vs_spot" },
    "synthesis":   "pipeline"
  }
}

accepts:  ["scro/bom-query/1"]       // schema ID, not a free-text label
produces: ["scro/bom_response"]      // matches output_contract.produces_schema
```

The author writes a **sketch** (`output_contract.sketch.json` beside the card).
The compiler (`cargo run --bin contract-sketch -- <agent_id>`) derives the full
`schema` and `grounding` from it. Nothing else requires a code edit.

### 2.1 Sketch → compiled contract — the only author step

```bash
# Write agents/curated/<id>/output_contract.sketch.json
# Compile:
cargo run --bin contract-sketch -- <id>
# Splice result into card (see docs/DESIGN_typed_output_contracts.md §10):
python3 scripts/splice_contract.py <id>
# Remove from TYPED_TIER_EXEMPT (one line, shrink-only list):
# src/workflows/agent_contract.rs — lower BASELINE in the same commit
```

No Rust edit for the verification logic. The SoC is clean for the enforcement
path: a new agent with a compiled sketch gets all four gates automatically.

---

## 3. The gate stack

```
Gate::Admission    ── at publish
  card_contract::validate reads output_contract from the card JSON.
  Refuses if output_contract is absent or malformed.
  TYPED_TIER_EXEMPT: 79 agents grandfathered (shrink-only; new agents are
  refused at publish if they have no contract).

Gate::InputBinding ── per execution, advisory
  port_trust::bind_input reads agent.accepts labels.
  Currently a HEURISTIC (is_text_input) — explicitly temporary.
  Success condition: when accepts entries are schema IDs, the heuristic
  is deleted and bind_input validates structure against the schema.
  Counts mismatch rate so the moment to make it fatal is visible.

Gate::Grounding    ── per execution, enforcing
  enforce_from_output_contract reads output_contract.grounding from the card.
  Nulls unavailable blocks. Stamps _provenance on inferred/sourced blocks.
  Falls back to FIELD_CONTRACTS for agents not yet migrated.
  Recorded in gate_decisions (Retention::Recorded — durable).

Gate::OutputSchema ── at delegation hop, enforcing
  envelope::build reads output_contract.schema from the card.
  Validates the response against the declared JSON Schema.
  Counted (Retention::Counted — not yet durable per-episode).
```

---

## 4. Enforcement paths — current state

There are two execution paths. They now both use `enforce_from_output_contract`
for grounding but via different call sites.

### 4.1 Delegation hop (agent → agent via execute_agent MCP tool)

```
execute_execute_agent (tools_legacy.rs)
  └─ envelope::build(agent_name, output_contract, output, episode_id)
       ├─ enforce_from_output_contract(agent_slug, output_contract, doc)
       │    ├─ reads output_contract.grounding  ← general path ✓
       │    └─ fallback: enforce(agent_slug, doc) → FIELD_CONTRACTS
       └─ schema_validate against output_contract.schema → Gate::OutputSchema
```

This is where the companion → oracle call lands. Fully typed and enforced.

### 4.2 Direct execution (person or script via /api/agents/:id/execute)

```
execute_agent_handler (handlers/execution.rs)
  └─ Pulse::grade(&agent_id, raw)           ← GAP: no output_contract passed
       └─ grounding_trust::enforce(agent_slug, doc)  ← reads FIELD_CONTRACTS only
```

`Pulse::grade` does not yet receive `output_contract`. Agents with a compiled
`output_contract.grounding` but no `FIELD_CONTRACTS` entry (i.e. all newly
typed agents including the oracle) get `Gate::Grounding = Undetermined` on the
direct execution path.

**The fix** (see §7.1): add `output_contract: Option<&Value>` to `Pulse::grade`
and call `enforce_from_output_contract` instead of `enforce`. Four callers:

| Caller | Has output_contract? | Pass |
|---|---|---|
| `execute_agent_handler` | Yes — card loaded via resolve_agent_card | `agent_card.capabilities.output_contract.as_ref()` |
| `execute_agent_stream_handler` | Yes | same |
| `post_workspace_message_handler` | No — only db_agent row | `None` → fallback to FIELD_CONTRACTS |
| `persist_opened` | No | `None` → fallback |

This is a targeted change (one function signature, four call sites) not a large
refactor.

### 4.3 Which path does the SimOps companion → oracle call travel?

The companion emits `invoke_agent` action blocks. The kask client dispatches
these via the `execute_agent` MCP tool, which goes through **4.1** (delegation
hop). `Gate::OutputSchema` and `Gate::Grounding` both fire on every oracle
response. The direct-execute gap (4.2) does not affect A2A calls between agents.

---

## 5. The declaration census — `GET /api/declarations`

The census counts what each agent has declared. The `field_contract` rung is
currently read from `FIELD_CONTRACTS` (the Rust const) via `has_field_contract`.

```sql
-- CENSUS_SQL (declaration_ladder.rs) — current
SELECT a.agent_name,
       ... AS ports,
       a.output_contract ? 'produces_schema' AS output_type,
       jsonb_typeof(a.output_contract -> 'schema') = 'object' AS output_schema
  FROM agents a WHERE ...
-- `field_contract` NOT in SQL — computed by has_field_contract() from Rust const
```

**The gap:** agents with compiled `output_contract.grounding` (oracle, equity_analyst, weather
members) are not counted in `field_contract` even though enforcement is now working for them.

**The fix** (see §7.2): add a SQL column to CENSUS_SQL and update `has_field_contract`:

```sql
jsonb_typeof(a.output_contract -> 'grounding') = 'object' AS field_contract
```

```rust
pub fn has_field_contract(agent_name: &str) -> bool {
    grounding_trust::contracts_for(agent_name).next().is_some()
}
// Replace with:
pub fn has_grounding_contract(agent_name: &str, output_contract: Option<&Value>) -> bool {
    grounding_trust::contracts_for(agent_name).next().is_some()
        || output_contract
            .and_then(|oc| oc.get("grounding"))
            .and_then(|g| g.as_object())
            .map(|g| !g.is_empty())
            .unwrap_or(false)
}
```

---

## 6. A2A contract pattern — how to declare a new agent pair

A2A contracting is expressed entirely through the **callee's card**. There is no
bilateral contract file. Any caller that reads the callee's card knows both what
to send and what to expect back.

### 6.1 The callee declares

```json
// agents/curated/<callee>/agent_card.json
{
  "accepts": ["<namespace>/<request-type>/<version>"],
  "produces": ["<namespace>/<response-type>"],
  "capabilities": {
    "input_contract": {
      "accepts_schema": "<namespace>/<request-type>/<version>",
      "note": "...",
      "required_fields": ["task", "..."],
      "task_discriminator": "task"
    },
    "output_contract": { /* compiled from sketch */ }
  }
}
```

`input_contract` is not yet enforced — it is the declarative statement of what
callers must send. `port_trust::bind_input` currently uses a heuristic over
`accepts` labels; when labels become schema IDs, `bind_input` validates against
the declared schema and the heuristic is deleted.

### 6.2 The caller references

The caller (compound agent, strategist, or met agent) reads the callee's card
via `list_workspace_agents` and finds the callee's `input_contract.accepts_schema`.
It constructs a query conforming to that schema, sends it via `execute_agent`,
and receives a response that has been validated against `output_contract.schema`.

No bilateral contract file. No per-agent code change. The contract is the card.

### 6.3 What list_workspace_agents should return

Currently `list_workspace_agents` returns only `name`, `type`, `description`.
For typed routing, it should also return (see §7.3):

```json
{
  "name": "supply_chain_oracle",
  "type": "research",
  "description": "...",
  "accepts": ["scro/bom-query/1"],
  "produces": ["scro/bom_response"],
  "input_schema_id": "scro/bom-query/1",
  "output_schema_id": "scro/bom_response"
}
```

Strategist agents (`moe_router_strategist`, `pipeline_strategist`) currently route
by reading `description` text because `accepts`/`produces` are not returned. With
typed schema IDs returned from `list_workspace_agents`, routing can match on structure.

### 6.4 Extending to new A2A transactions

**To add a new typed A2A pair:**

1. **Callee**: write `output_contract.sketch.json` → compile → splice into card
2. **Callee**: set `accepts: ["namespace/type/1"]` on card
3. **Callee**: add `input_contract` stub (schema will be enforced when §7.4 is done)
4. **Caller**: update system prompt to reference the callee's schema ID when invoking
5. **Done** — all four gates work automatically on the next call

No Rust changes. No bilateral contract files. No FIELD_CONTRACTS entries.

---

## 7. Gap register — ordered by blocking impact

### 7.1 `Pulse::grade` does not pass `output_contract` ✅ FIXED

`Pulse::grade` now accepts `output_contract: Option<&Value>`. The two main
execution handlers (`execute_agent_handler`, `execute_agent_stream_handler`)
pass the card's output_contract; the workspace-message handler and `persist_opened`
pass `None` and fall back to FIELD_CONTRACTS gracefully.

**File:** `src/episode_boundary.rs` + `src/handlers/execution.rs`,
`src/handlers/execution_stream.rs`, `src/handlers/workspace/messages.rs`

### 7.2 `field_contract` census rung counts FIELD_CONTRACTS only ✅ FIXED

CENSUS_SQL now includes a `field_contract` column reading
`output_contract.grounding` from the JSONB column — covering both the new
compiled-contract path and the legacy FIELD_CONTRACTS path.
`has_grounding_contract(name, output_contract)` covers both at runtime.

**File:** `src/declaration_ladder.rs`

### 7.3 `list_workspace_agents` returns only name/type/description ✅ FIXED

`execute_list_workspace_agents` now returns `accepts[]`, `produces[]`,
`input_schema_id`, and `output_schema_id` per agent. Strategist agents reading
this tool now have typed interface information for routing decisions.

**File:** `src/agent_backend/tools_legacy.rs`

### 7.4 `input_contract` is declared but not enforced (MEDIUM — by design)

**Impact:** What callers send is not validated. `Gate::InputBinding` is advisory
and uses the `is_text_input` heuristic over label strings.

**Status:** Deliberately deferred. `port_trust::is_text_input` is explicitly
labelled temporary in the module docs. The success condition is: when `accepts`
entries are schema IDs, `bind_input` validates against the schema, and `is_text_input`
is deleted. Requires: a type registry or a naming convention that distinguishes
text-shaped types from structured types.

**Fix:** Add `fermi/free_text_query` as a registered text type. Agents that accept
free text declare `accepts: ["fermi/free_text_query"]`. Structured agents declare
their schema IDs. `bind_input` looks up the type to determine shape. No Rust
constant per agent.

**File:** `src/port_trust.rs` (update `bind_input`), `src/grounding_trust.rs`
(type registry or convention), agent cards

### 7.5 `pipeline_strategist` seam check is label-only (LOW)

**Impact:** Pipeline seam validation is a string comparison ("does upstream
`produces` label equal downstream `accepts` label?"). With schema IDs, this
becomes a real compatibility check.

**Fix:** When §7.3 is done (list_workspace_agents returns schema IDs), update
`pipeline_strategist` system prompt to route on schema ID match. No Rust change.

### 7.6 FIELD_CONTRACTS still requires Rust for SQL cross-checks (LOW — by design)

**Impact:** Per-field SQL cross-checks against platform DB tables (genome_profiler
taxonomy vs creature_conditions, etc.) require Rust entries in FIELD_CONTRACTS.
This is genuinely platform-integration work — the SQL must know the DB schema.

**Status:** Deliberately kept in Rust. The three-tier distinction:
- Tier 1 (base enforcement): `output_contract.grounding` → card JSON, no Rust
- Tier 2 (tool-call verification): `output_contract.grounding` → card JSON, no Rust
- Tier 3 (DB cross-checks): `FIELD_CONTRACTS.cross_check_sql` → Rust, justified

FIELD_CONTRACTS is legacy for tiers 1 and 2 (will shrink to zero). It is
permanent for tier 3 cross-checks (a different concern).

---

## 8. SimOps rollout — first A2A pair

### 8.1 Current state of both agents

**`supply_chain_oracle` v2.0.0**
```
output_contract.schema:    compiled ✓  (scro/bom_response)
output_contract.grounding: compiled ✓  (items/risks/summary blocks)
TYPED_TIER_EXEMPT:         removed ✓  (BASELINE 80 → 79)
accepts:                   ["scro/bom-query/1"] ✓
produces:                  ["scro/bom_response"] ✓
input_contract:            declared (stub) ✓
system_prompt:             asks for provenance fields ✓
Gate::OutputSchema:        fires on every delegation hop ✓
Gate::Grounding (A2A):     fires via envelope::build ✓
Gate::Grounding (direct):  fires via Pulse::grade (output_contract passed) ✓
```

**`simops_companion` v2.1.0**
```
role:                      met_agent ✓
invoke_agent format:       fixed — uses "agent_id", typed BomQuery ✓
output_contract:           compiled ✓  (kask_simops/action_block, narrative)
TYPED_TIER_EXEMPT:         removed ✓  (BASELINE 79 → 78)
Gate::OutputSchema:        by design unverified_no_payload — see §8.4
Gate::Grounding:           by design no enforcement (narrative block) — see §8.4
```

### 8.2 What's working for A2A right now

When the companion invokes the oracle via `invoke_agent`:

```
companion sends:
  { type: "invoke_agent", agent_id: "supply_chain_oracle",
    query: { task: "resolve_bom", process_context: {...}, bom_items: [...], currency: "EUR" },
    render_as: "bom_table" }

kask client dispatches → execute_execute_agent MCP tool
→ envelope::build called with oracle's output_contract
→ enforce_from_output_contract stamps items_provenance, risks_provenance, summary_provenance
→ schema_validate checks response against scro/bom_response schema
→ Gate::OutputSchema records Approved/Refused/Undetermined
→ Gate::Grounding records verdict
```

### 8.3 The companion's coordinator contract

The companion's output format is `prose + __ACTION__...__END_ACTION__ delimited
blocks` parsed by the kask client. This is a different contract shape from
specialist agents (which produce a single top-level JSON document).

**Why the companion contract is narrative-only:**

The contract_sketch block model produces schemas where BLOCK NAME = TOP-LEVEL KEY
in the output document (e.g., `items`, `risks`, `summary` for the oracle). The
companion's action block JSON is `{ "type": "edit_process", "patch": {...} }` —
not a document with named top-level blocks.

The companion runs on the workspace-message path, NOT via delegation hop. This
means `envelope::build` is never called for the companion, and `Gate::OutputSchema`
is never asked. The `Pulse::grade` path handles grounding, but the companion has
no grounding to enforce (all output is inferred reasoning, not tool retrieval).

The authoritative action block contract is `apps/kask_simops.json` — six typed
action types with JSON Schema validation, enforced at the kask client layer. The
companion's `output_contract` satisfies the Admission gate and records the domain
and calibration signal; it does not duplicate the app schema.

**Comparison with `weather_oracle`:**

`weather_oracle` IS a coordinator with a standard output_contract: its output IS a
top-level JSON document (the forecast) with named blocks sourced from member agents
via `execute_agent`. The companion's output is not a top-level JSON document —
it is prose with embedded action blocks. These are different coordinator shapes.

A future `tests/simops_composition.rs` should assert:
- Oracle output conforms to scro/bom_response schema
- Companion delegation to oracle uses typed BomQuery
- Both gates report non-Undetermined on the A2A path

### 8.4 Gate summary for the companion

| Gate | Reading | Reason |
|---|---|---|
| `Admission` | Approved | Compiled output_contract |
| `InputBinding` | Declared (heuristic) | `accepts` contains "kask_simops/context_bundle" |
| `Grounding` | Undetermined | Narrative block, no enforcement |
| `OutputSchema` | Never asked | Companion runs on workspace-message path, not delegation hop |

### 8.5 Rollout sequence — all items completed

| Step | What | Gate unlocked | Status |
|---|---|---|---|
| ✓ | Oracle output_contract compiled | Gate::OutputSchema on delegation hop | Done |
| ✓ | Oracle removed from TYPED_TIER_EXEMPT | Gate::Admission enforced at publish | Done |
| ✓ | Companion invoke_agent format fixed | Typed BomQuery sent correctly | Done |
| ✓ | Pulse::grade receives output_contract | Gate::Grounding on direct /execute calls | Done |
| ✓ | Companion output_contract sketch compiled | Gate::Admission — companion off grandfathering | Done |
| ✓ | Census SQL updated | field_contract counts compiled contracts | Done |
| ✓ | list_workspace_agents returns typed info | Typed strategist routing enabled | Done |

---

## 9. Extending to more A2A pairs

The SimOps fleet has 12 agents. The oracle is the first pair; the same pattern
applies to all:

| Pair | Request type | Response type | Priority |
|---|---|---|---|
| companion → simops_cascade | `kask_simops/cascade_request` | `simops/cascade_result` | High |
| companion → comparator | `kask_simops/comparison_request` | `simops/comparison_result` | High |
| companion → sidestream_miner | `kask_simops/stage_description` | `simops/sidestream_analysis` | Medium |
| companion → sensor_advisor | `kask_simops/stage_description` | `sosa/sensor_proposal` | Medium |
| companion → energy_advisor | `kask_simops/stage_description` | `simops/energy_proposal` | Medium |

For each: write callee sketch → compile → remove from TYPED_TIER_EXEMPT. The
companion system prompt already has the right invocation format for cascade
(`invoke_agent on simops_cascade`); only the oracle has been typed so far.

**Cross-fleet typing** (beyond SimOps): the same mechanism applies to any agent
pair. The pattern scales because it lives on the card, not in code.

---

## 10. What to tell the UX team

The UX handoff document (`docs/UX_HANDOFF_trust_surfaces.md`) describes the gate
surfaces. Three things have changed since it was written:

**1. `grounding_enforced` means something broader now.**  
Previously: `true` only when an agent has a `FIELD_CONTRACTS` Rust entry.  
Now: `true` when an agent has a compiled `output_contract.grounding` (new path)
OR a `FIELD_CONTRACTS` entry (legacy path). More agents will show `grounding_enforced:
true` as their sketches are compiled without any Rust changes.

**2. `field_contract` count in `/api/declarations` is now accurate.**  
The census SQL was updated to read `output_contract.grounding` from the card JSONB,
covering both the new compiled-contract path and the legacy FIELD_CONTRACTS path.
The UX's `field_contract` count now reflects actual enforcement coverage.

**3. The artifact trace for oracle episodes now shows real grounding verdicts.**  
Oracle delegation hops have `grounding_enforced: true` and a `provenance.blocks`
array with actual verdicts (`items_provenance`, `risks_provenance`, `summary_provenance`).
Previously these were all Undetermined. The trace surface can render these.

**What they do NOT need to change:** Gate APIs, the trace schema, the gate decision
review flow. Those are unchanged. The new data appears in existing fields.

---

## 11. Relationship to the A2A standard protocol

### 11.1 What the A2A standard is

The Agent2Agent Protocol (A2A) v1.0 (Linux Foundation, Apache 2.0) is an
**external federation protocol** — how agent A on platform X communicates with
agent B on platform Y, without either knowing the other's internals.

Core primitives:
- `AgentCard` at `/.well-known/agent-card.json` (HTTP discoverable)
- **Tasks** with a lifecycle: `SUBMITTED → WORKING → COMPLETED/FAILED/CANCELED/INPUT_REQUIRED`
- **Messages** with typed `Parts` (text | raw bytes | URL | structured data)
- **Artifacts** — typed outputs of task execution
- JSON-RPC 2.0 / gRPC / HTTP+REST bindings
- Streaming (SSE), async (push notifications / webhooks)
- MCP for agent→tool communication; A2A for agent→agent communication across platforms

The A2A spec explicitly states: *"A2A does not specify how an agent talks to its
own sub-agents or how it invokes tools — use your framework's native primitives,
or MCP, for those."* What we built is exactly that: ABW's native primitives for
internal typed invocation.

### 11.2 Conformance gap

| A2A Requirement | ABW Status |
|---|---|
| `/.well-known/agent-card.json` endpoint | ❌ Not implemented |
| `AgentCard.skills` (id, name, description, tags, inputModes, outputModes) | ❌ ABW has `capabilities`/`mcp_tools`, different schema |
| `AgentCard.supportedInterfaces` (URL + protocolBinding + version) | ❌ Not in ABW card format |
| `AgentCard.defaultInputModes/outputModes` (MIME types) | ❌ ABW has `accepts`/`produces` as schema IDs |
| Task lifecycle (SUBMITTED/WORKING/COMPLETED/FAILED/INPUT_REQUIRED) | ❌ ABW has episodes with binary success/failure |
| `Message` with `Parts` (text/raw/url/data) | ❌ ABW uses plain text query strings |
| `Artifact` (typed task output with Parts) | ❌ ABW has raw_response; no Artifact model |
| SendMessage / GetTask / ListTasks / CancelTask / SubscribeToTask | ❌ ABW has `/execute` (single-shot) |
| SSE streaming (SendStreamingMessage) | ✅ ABW has `execute_agent_stream` |
| Push notifications (webhooks) | ❌ Not implemented |
| `A2A-Version` header | ❌ Not in ABW |
| JSON-RPC 2.0 or gRPC binding | ❌ ABW is REST-only |
| Security schemes declared in AgentCard | ⚠️ ABW has auth but not in A2A card format |

The ABW internal contracting mechanism maps conceptually to A2A but does not
conform to the wire protocol:

```
ABW internal contracting          A2A standard
(what this doc designs)           (cross-platform federation)

  output_contract                    AgentCard skills (inputModes/outputModes)
  execute_agent MCP tool             SendMessage API
  grounding enforcement              (A2A trusts agent outputs, no grounding layer)
  Gate::OutputSchema                 (no equivalent)
  Episode                            Task + lifecycle state machine
  workspace_id                       contextId
  SSE streaming              ≈       SendStreamingMessage
  api_server.rs REST API             JSON-RPC / gRPC / HTTP+REST binding
```

### 11.3 Two options

**Option A — Internal mechanism + A2A at the boundary (recommended)**

ABW keeps its internal contracting system (execute_agent, output_contract, gates)
for agent-to-agent calls within the platform. Additionally, expose each agent
externally via an A2A-conformant endpoint:

- Map agent cards to `/.well-known/agent-card.json` (skills from capabilities)
- Wrap the episode model in an A2A Task facade
- Expose `POST /message:send`, `GET /tasks/{id}` etc. per agent
- External callers (other platforms, ADK agents, etc.) can discover and call ABW agents via A2A
- Internal ABW calls continue through execute_agent with full grounding/gates

This is the standard "protocol translation layer" pattern. The oracle could be
both an internally-typed ABW agent AND an externally-discoverable A2A endpoint.

**Option B — Replace internal mechanism with A2A (not recommended)**

Replace execute_agent with A2A Task API. Map Episodes to A2A Tasks throughout.
This is a major refactor with unclear benefit for internal calls — the A2A
protocol has no grounding, no output_contract, no verification gates. ABW's
added verification layer would sit on top of A2A rather than replace it.

### 11.4 What an A2A boundary layer would look like for ABW

If Option A is pursued, the minimum A2A surface per agent:

```
GET  /.well-known/agent-card.json
  → map AgentCard.json to A2A schema
  → skills: one skill per capability/use-case, inputModes: ["text/plain", "application/json"]
  → supportedInterfaces: [{ url, protocolBinding: "HTTP+JSON", protocolVersion: "1.0" }]

POST /message:send
  → create an Episode from Message.parts[0].text (the query)
  → run agent via existing execute_agent pipeline
  → return Task with status TASK_STATE_COMPLETED + Artifact from raw_response

GET  /tasks/{episode_id}
  → map Episode to Task lifecycle
  → SUBMITTED (episode reserved) → WORKING (in-progress) → COMPLETED/FAILED
```

This facade requires:
- A route prefix per agent (or a shared A2A gateway that routes by agent_id)
- An Episode → Task mapping layer (thin)
- AgentCard → A2A AgentCard mapping (thin)
- Nothing in the core ABW execution path changes

### 11.5 Recommendation for this sprint

Focus remains on the internal contracting mechanism (what this doc covers).
A2A boundary layer is a separate workstream. The two are not in conflict:
internal contracts make ABW agents more verifiable; an A2A facade makes them
federatable. Both are desirable; do them in that order.
