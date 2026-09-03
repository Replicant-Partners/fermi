# A2A Protocol Design for ABW

**Status:** Design Proposal — 2026-08-29  
**Author:** Ivan Labra  
**Scope:** General agent-to-agent invocation protocol for all ABW agents and strategist agents

---

## 1. The problem stated precisely

Every agent-to-agent invocation in ABW goes through `execute_agent`. The callee's *output* is
validated at the delegation hop by `envelope::build` → `schema_validate::validate`, using the
callee's `output_contract`. This is the one typed seam in the system.

The callee's *input* is never validated. There is no `input_contract` field in the codebase.
`accepts` and `produces` on agent cards are `Vec<String>` labels — parsed by `AgentCard` and
stored, but never validated against actual content.

`list_workspace_agents` returns only `name`, `type`, `description`. Strategist agents (moe_router,
pipeline, cohere_and_coordinate) describe using `accepts`, `produces`, `skills` from this tool in
their system prompts, but the tool does not return those fields. Routing classification happens
on `description` text, not on typed interface declarations.

The result: A2A is half-typed. Outputs are validated; inputs are not. Strategists route by
description text, not by declared schema. The contracts exist only as documentation conventions
on two sides of a call without any enforcement point connecting them.

---

## 2. What already exists (the foundation to build on)

```
agent_card.json
├── accepts: Vec<String>          ← labels; read by strategists via description heuristics
├── produces: Vec<String>         ← labels; being migrated to schema IDs (e.g. "scro/bom-response/1")
├── capabilities
│   ├── output_contract           ← compiled JSON Schema + grounding; validated by envelope.rs ✓
│   ├── mcp_tools                 ← what this agent can call
│   └── (no input_contract)       ← THE GAP
├── dependencies.required/optional ← composition membership
└── workflow_template              ← per-stage accepts/produces for pipeline_strategist
```

```
src/agent_backend/envelope.rs
  envelope::build(agent_name, output_contract, output, episode_id) → validates response
  ← called in execute_execute_agent AFTER the call completes
  ← not called for the INPUT (what is sent to the callee)
```

```
src/agent_backend/tools_legacy.rs
  execute_list_workspace_agents()
    SELECT agent_name, agent_type, description    ← ONLY these three fields
    FROM workspace_agents JOIN agents
    ← accepts, produces, capabilities NOT returned
```

---

## 3. The correct A2A pattern — callee-owned, not bilateral

The wrong pattern I explored first: a bilateral contract file in `ontologies/contracts/`
mapping one caller to one callee. This creates N×M files as the fleet grows and is the
opposite of discoverable — a new caller can't find the oracle's interface without knowing
to look for a specific file.

The right pattern: **the callee owns both halves of its interface**.

```
supply_chain_oracle/agent_card.json
├── capabilities
│   ├── input_contract            ← NEW: what callers must send
│   │   ├── accepts_schema: "scro/bom-query/1"
│   │   ├── schema: { JSON Schema }
│   │   └── task_discriminator: "task"   (optional: field that routes variants)
│   └── output_contract           ← EXISTING: what the oracle returns
│       ├── produces_schema: "scro/bom-response/1"
│       └── schema: { JSON Schema }
├── accepts: ["scro/bom-query/1"]   ← becomes a schema ID reference, not a label
└── produces: ["scro/bom-response/1"]  ← already moving this way
```

Any caller that wants to invoke the oracle calls `list_workspace_agents`, receives the oracle's
`input_contract.accepts_schema`, and knows exactly what to send. No bilateral file needed.

---

## 4. The three changes that make A2A general

### 4.1 `input_contract` on `AgentCapabilities`

Symmetric to `output_contract`. Lives in `capabilities`, compiled from a sketch (same toolchain).

```rust
// In src/agent_backend/agent_card.rs — AgentCapabilities
pub input_contract: Option<serde_json::Value>,
```

Sketch format (same author surface as `output_contract.sketch.json`):

```json
{
  "accepts_schema": "scro/bom-query/1",
  "title": "BOM pricing request",
  "blocks": [
    {
      "name": "task",
      "note": "Discriminator — always 'resolve_bom'"
    },
    {
      "name": "bom_items",
      "note": "Array of items to price. Each item must have name, qty, unit, role."
    }
  ]
}
```

The compiled `input_contract` has:
- `accepts_schema` — the schema ID
- `schema` — the full JSON Schema
- `required` — which fields are required

### 4.2 `list_workspace_agents` returns typed capability info

The SQL query and response need to include `accepts`, `produces`, and (optionally)
`input_contract.accepts_schema` from the agent card JSONB column:

```sql
SELECT a.agent_name,
       a.agent_type,
       a.description,
       a.card_json->'accepts'                              AS accepts,
       a.card_json->'produces'                             AS produces,
       a.card_json->'capabilities'->'input_contract'->>'accepts_schema'  AS input_schema_id,
       a.card_json->'capabilities'->'output_contract'->>'produces_schema' AS output_schema_id
FROM workspace_agents wa
JOIN agents a ON wa.agent_id = a.id
WHERE wa.workspace_id = $1
```

Response shape:
```json
[
  {
    "name": "supply_chain_oracle",
    "type": "research",
    "description": "...",
    "accepts": ["scro/bom-query/1"],
    "produces": ["scro/bom-response/1"],
    "input_schema_id": "scro/bom-query/1",
    "output_schema_id": "scro/bom-response/1"
  }
]
```

### 4.3 Envelope validates input (not just output)

Currently `envelope::build` is called AFTER execution, validating the output.
A symmetric `envelope::validate_input` should be called BEFORE execution:

```rust
// New function in envelope.rs
pub fn validate_input(
    agent_name: &str,
    input_contract: Option<&Value>,
    query: &str,
) -> InputValidationReport

// Called in execute_execute_agent BEFORE dispatching the call
let input_report = envelope::validate_input(
    agent_name,
    declared_input_contract.as_ref(),
    &query,
);
```

Like the output validation, an invalid input does not halt execution (backwards compat) but
is recorded in the episode and surfaced via `Gate::InputSchema` (a new gate variant).

---

## 5. How strategist agents interact with this

With `list_workspace_agents` returning typed capability info, each strategist pattern becomes:

### MoE Router

```
Stage 0 — Classify:
  1. list_workspace_agents → get input_schema_id, output_schema_id, calibration per member
  2. For members whose input_schema_id matches the query's structure: these are candidates
  3. Among candidates: prefer by calibration score (existing logic, unchanged)
  4. Route query formatted as the member's input_schema_id
```

The router no longer routes by description heuristics alone. It has a typed signal: does
the query's shape match the member's declared `accepts` schema?

### Pipeline Strategist

```
SEAM CHECK (already exists but label-only):
  Current: does upstream produces[label] == downstream accepts[label]? (string compare)
  With A2A: does upstream produces_schema_id == downstream accepts_schema_id?
            AND does upstream output_contract.schema satisfy downstream input_contract.schema?

This makes pipeline_strategist's seam validation REAL — a schema compatibility check,
not a label coincidence.
```

### App-scoped met agents (e.g. SimOps Companion)

The companion is a domain-constrained MoE. Its routing is baked into its system prompt
(not dynamic like moe_router). With `input_contract` on each member:

```
- oracle's input_contract.schema lives ON the oracle card
- companion system prompt references the canonical shape
- kask validates invoke_agent actions against oracle's input_contract
- No bilateral contract file needed
```

The companion's system prompt should say:
"When invoking supply_chain_oracle, send a query conforming to the oracle's declared
input_contract (accepts_schema: 'scro/bom-query/1')."

The kask client can look this up from the oracle card at runtime.

---

## 6. The ontologies directory — correct scope

```
fermi/ontologies/
  samples/
    market_research_ontology.json    ← agent KG snapshot (entities/relationships from episodes)
    sentiment_analyzer_ontology.json ← agent KG snapshot
```

This directory is for **per-agent knowledge graphs** that emerge from episodic memory via the
ADM pipeline (episodes → consolidation → KG). Every agent can have one. They grow over time
as the agent accumulates experience. They are NOT for A2A contracts.

The file I created (`ontologies/contracts/simops_companion__supply_chain_oracle.json`) is
misplaced. The contents should be distributed across:
- `input_contract` on the oracle's card (the request schema)
- `output_contract` on the oracle's card (the response schema — already there via sketch)

The `ontologies/contracts/` directory should be deleted.

---

## 7. Agent roles in the A2A topology

Every agent in ABW can be described by its A2A role:

```
┌─────────────────────────────────────────────────┐
│  Role          │ Calls others │ Called by others │
│────────────────┼──────────────┼─────────────────│
│  met_agent     │ Yes          │ User (direct)    │
│  strategist    │ Yes (route)  │ met_agent / user │
│  specialist    │ Rarely       │ strategist / met │
│  compound      │ Yes (fixed)  │ user / other     │
└─────────────────────────────────────────────────┘
```

The **met agent** role (`simops_companion`, `wild_companion`) is the user-facing entry point
for an app. It is a domain-constrained MoE: it routes to specialists via its own action grammar
(not via a strategist agent). It IS the strategist for its app.

The **strategist agents** (`moe_router`, `pipeline`, `cohere_and_coordinate`, `vote`, `debate`)
are general-purpose routing/coordination agents used in open compositions (not app-specific).

Both patterns benefit from `input_contract` on specialists. The mechanism is the same —
what differs is where routing logic lives (in the met agent's system prompt vs. in a
strategist agent's dynamic classification).

---

## 8. The `accepts`/`produces` field migration

| Current state | Target state |
|---|---|
| `accepts: ["bom_items_json", ...]` — opaque labels | `accepts: ["scro/bom-query/1"]` — schema IDs |
| `produces: ["unit_costs", ...]` — opaque labels | `produces: ["scro/bom-response/1"]` — schema IDs |
| Strategist matches by label string equality | Strategist matches by schema ID + compatibility check |
| No validation at delegation hop | Input validated via `input_contract.schema` |

Migration path:
1. Agents get `input_contract` (sketch → compiled) alongside their `output_contract`
2. `accepts` field is updated to reference the input_contract's schema ID
3. `list_workspace_agents` is updated to return `accepts`/`produces` + schema IDs
4. Envelope extended for input validation (non-breaking — adds a report, doesn't halt)
5. Strategist agent prompts updated to use typed schema IDs (not description heuristics)

---

## 9. What changes for simops specifically

The SimOps A2A strategy document (`docs/SIMOPS_AGENT_STRATEGY.md`) should be updated to
reflect this general pattern:

1. The "A2A contract" between companion and oracle is expressed as:
   - `input_contract` on the oracle's card (`capabilities.input_contract`)
   - `output_contract` on the oracle's card (`capabilities.output_contract`) — already has sketch
   - NOT as a bilateral file in `ontologies/contracts/`

2. The companion references the oracle's schema ID in its system prompt and in the
   `invoke_agent` action documentation.

3. The `invoke_member` action type in `kask_simops.json` app schema could eventually
   grow an `agent_contracts` map, but this is downstream of the agent card changes.

---

## 10. Implementation ordering

```
Phase A — Agent card (no DB changes needed)
  A.1  Add input_contract field to AgentCapabilities (deserializes from card JSON)
  A.2  Add input_contract.sketch.json toolchain (alongside output_contract.sketch.json)
  A.3  Compile oracle input_contract from sketch → splice into card
  A.4  Delete ontologies/contracts/ (misplaced)

Phase B — Discovery (DB read change)
  B.1  Update execute_list_workspace_agents SQL to return accepts, produces, schema IDs
  B.2  Update strategist agent prompts to consume the new typed info

Phase C — Validation (new enforcement point)
  C.1  Add envelope::validate_input() symmetric to envelope::build()
  C.2  Call it in execute_execute_agent before dispatch
  C.3  Add Gate::InputSchema variant (lets the observation UI surface input violations)

Phase D — Strategist integration
  D.1  Update moe_router_strategist prompt: route by schema ID match, not description heuristic
  D.2  Update pipeline_strategist seam check: schema ID equality, not label equality
  D.3  Update cohere_and_coordinate: declare input_contract on the coordination type it handles
```

Phase A can happen independently and immediately. B, C, D depend on A.

---

## 11. Open questions

1. **Input contract sketches**: Should `input_contract` use the same sketch compiler as
   `output_contract`? Or a simpler format (input schemas don't need grounding maps — there's
   no provenance claim to make about WHERE the caller got the data from)?
   → Simpler format seems right. Input sketches need: schema fields and types. Not grounding.

2. **Variant schemas**: Some agents accept multiple query shapes (e.g. a `task` discriminator
   that routes to different logic). Should `input_contract` support a `oneOf` variant list?
   → Yes, via JSON Schema `oneOf` in the compiled `schema`. The sketch can have a `variants`
   block listing alternate task types.

3. **Backward compatibility**: Agents without `input_contract` should still be callable —
   input validation is a soft warning, not a hard block. Mirrors Gap #4 in the output
   contract story (enforcement gap).
   → Confirmed: `input_contract: None` means no input validation, same as `output_contract: None`.

4. **`ontologies/` directory governance**: Who owns what goes in `ontologies/`? Exclusively
   ADM consolidation pipeline output? Or can human-authored ontology seeds go here?
   → Needs a decision. Suggestion: `ontologies/` = machine-generated (consolidation output).
   Human-authored seed data goes in `agents/curated/<id>/seed_ontology.json` on the card.
