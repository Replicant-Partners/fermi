# Typed Coordination and Agent Competition

**Written 2026-09-04. Entry point for new sessions.**  
**Status:** Phase 1 complete. Phase 2 in progress.

---

## 1. What this is

Two interlocking changes to how agents coordinate and how specialists compete for work.

**Typed coordination graph** — `workflow_template` on a composition promoted from a
labelled stage list to a typed directed graph. Nodes carry schema IDs (`input_schema_id`,
`output_schema_id`). Edges are typed seams. A node with `agent: null` is an open slot
filled at runtime by the selection mechanism. The platform traverses the graph; strategist
agents no longer narrate their own loops.

**Agent competition** — multiple specialist agents can declare the same `input_schema_id`.
A new `select_agent` platform primitive ranks candidates by Brier score, fidelity,
valence fit, cost, and support tier. Selection decisions are typed episodes that feed
Loop 4. Creators compete by declaring `input_contract` + a `competition` block; the
platform computes their track record from gate history.

### The marketplace is open and large by design

Competition is **scoped**, not limited. The goal is a fully open, dynamic marketplace
where any creator can compete for any slot that matches their declared `input_schema_id`.
Scoping is how the platform manages trust and relevance at different stages, not a ceiling.

```
Full marketplace (all ABW agents with matching input_schema_id)
    └── Fleet (Fermi fleet, SimOps fleet, domain-specific curated sets)
         └── Workspace roster (agents the owner has installed and vetted)
              └── Composition (pinned agents or open slots resolved at runtime)
```

Each level is a curated subset of the one above. When a workspace owner adds an agent
from the marketplace, they are curating their competition roster. The `select_agent`
tool carries a `scope` parameter so any level can be queried.

**Fleets as a first-class scope.** Fermi is a fleet — a domain-coherent set of agents
that compete within forecasting pipelines. SimOps is another. A fleet has its own
calibration history, its own valence distribution, its own Brier leaderboard. When a
coordinator says `scope: fleet:fermi`, only Fermi fleet members compete for the slot.
This is how specialist fleets maintain domain coherence while still being open to any
creator who declares the right `input_schema_id` and joins the fleet.

**The fractal roster.** A workspace owner can install a curated subset of any fleet.
Two workspaces using the Fermi fleet may have different rosters — different calibration
histories, different cost profiles, different valence distributions. `select_agent`
scores candidates against the workspace’s own context (its valence centroid, its
historical outcomes) even when drawing from a fleet or marketplace pool.

### What is permanently out of scope

- Behaviour trees / BPMN (explicitly rejected — see discussion notes)
- Synthesis protocol on specialist agent cards (it belongs on the coordination graph)

---

## 2. Design boundaries — the separation that matters most

```
SPECIALIST AGENT declares:
  input_contract  → what query shape I accept (schema ID)
  output_contract → what I produce (schema ID + grounding)
  competition     → domain tags, price per call, support tier
  valence         → personality (read by selection, not owned by selection)
  [platform computes: fidelity, selection rate, Brier score]

COORDINATION GRAPH (workflow_template on the composition) declares:
  nodes           → schema-typed slots (agent bound or open)
  edges           → typed seams (output_schema_id → input_schema_id)
  synthesis       → how to combine outputs (pipeline/aggregation/cep_weighted/…)
  selection       → criteria weights for open slots (Brier/fidelity/cost/valence)

PLATFORM computes and stores:
  fidelity        → Gate::OutputSchema (approved / (approved + refused)) per agent
  selection rate  → how often selected when in candidate set
  Brier score     → calibration from resolved signals
  selection trace → typed episode per select_agent call (Loop 4 signal)
```

A specialist author writes `input_contract.sketch.json` + `competition` block.
They never touch synthesis protocol or selection criteria. Those live on the
composition, not the member.

---

## 3. What is already built

| Piece | Location | State |
|---|---|---|
| `input_contract` on `AgentCapabilities` | `src/agent_backend/agent_card.rs` | ✅ done |
| `input-contract-sketch` binary | `scripts/input_contract_sketch.rs` | ✅ done |
| `supply_chain_oracle` input_contract compiled | `agents/curated/supply_chain_oracle/` | ✅ done |
| `cohere_and_coordinate` input_contract compiled | `agents/curated/cohere_and_coordinate/` | ✅ done |
| `agents.input_contract` DB column | migration 227 | ✅ done |
| `list_workspace_agents` returns schema IDs | `src/agent_backend/tools/domains/workspace.rs` | ✅ done |
| `Gate::InputSchema` | `src/gate_trust.rs` | ✅ done |
| `envelope::validate_input` | `src/agent_backend/envelope.rs` | ✅ done |
| Called before every `execute_agent` hop | `src/agent_backend/tools/domains/platform.rs` | ✅ done |
| `accepts` editor on specimen shelf (ports rung) | `templates/specimen.html`, `static/js/widgets/agent-fields.js` | ✅ done |
| Strategist prompts updated (schema ID routing, seam types) | `agents/curated/moe_router_strategist/`, `pipeline_strategist/` | ✅ done |

---

## 4. Execution plan

### Phase 1 — Competition credentials and `select_agent` (no UI yet)

**Goal:** any strategist agent can call `select_agent` to rank candidates for an open
slot. Creator participation surface established.

#### 1.1 `competition` block on agent card

Add to `AgentCapabilities` in `agent_card.rs`:
```rust
#[serde(default)]
pub competition: Option<CompetitionDeclaration>,
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompetitionDeclaration {
    /// Domain tags this agent competes in (e.g. ["supply-chain", "bom-pricing"]).
    /// Used by select_agent for domain-match scoring.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Credits charged per successful `execute_agent` call.
    /// None = free (default for curated agents).
    #[serde(default)]
    pub price_credits_per_call: Option<u32>,
    /// Owner's support commitment: "community" | "standard" | "enterprise".
    /// Self-declared, unverified initially.
    #[serde(default = "default_support_tier")]
    pub support_tier: String,
}
```

Migration 228: `ALTER TABLE agents ADD COLUMN IF NOT EXISTS competition jsonb;`

Wire through `Agent`, `AgentUpdate`, `upsert_agent`, `update_agent`, `row_to_agent`,
`AGENT_COLUMNS`, `agent_card_from_db`, seed loop — same pattern as migration 227
(`input_contract`).

#### 1.2 Fidelity score endpoint

Add `GET /api/agents/:agent_name/fidelity` (or include in the specimen API response).

Fidelity = `Gate::OutputSchema approved / (approved + refused)` per agent name, read
from `gate_decisions` table (already populated by `envelope::build` at every hop).

Returns:
```json
{ "fidelity": 0.94, "approved": 847, "refused": 54, "total": 901 }
```

No new table. Reads the existing `gate_decisions` rows where `gate = "output_schema"`
and `subject = agent_name`.

#### 1.3 `select_agent` platform tool

New tool in `src/agent_backend/tools/domains/platform.rs`.

```
select_agent(
  input_schema_id: "scro/bom-query/1",   // which slot to fill
  query: "...",                           // the query text (for semantic scoring)
  criteria: {                             // weights, sum to 1.0
    schema_match: 0.30,   // non-negotiable gate — only typed candidates pass
    brier:        0.30,   // calibration score from resolved signals
    fidelity:     0.20,   // Gate::OutputSchema approved rate
    cost:         0.10,   // lower credits_per_call scores higher
    valence_fit:  0.10    // complement to workspace valence distribution
  }
) → [
  {
    agent:     "supply_chain_oracle",
    score:     0.82,
    breakdown: { schema_match: 1.0, brier: 0.78, fidelity: 0.94, cost: 0.6, valence_fit: 0.9 },
    input_schema_id:  "scro/bom-query/1",
    output_schema_id: "scro/bom-response/1"
  },
  ...
]
```

Implementation:
1. Query `list_workspace_agents` SQL (already returns schema IDs) filtered to
   `input_schema_id = $1`
2. For each candidate: fetch fidelity from gate_decisions, Brier from calibration
   table, cost from `competition.price_credits_per_call`, valence from `agents.valence`
3. Score and rank
4. Return ranked list as JSON

The selection decision itself is recorded as a `Gate::InputBinding` observation
(already exists) — or, cleaner, as a new episode row tied to the parent episode.
See Phase 2 for the typed trace.

`criteria` defaults to `{ schema_match: 0.30, brier: 0.30, fidelity: 0.20, cost: 0.10, valence_fit: 0.10 }`
and can be overridden per-call (strategist reads from composition's `workflow_template`).

#### 1.4 Update `moe_router_strategist` to call `select_agent`

Replace the current "call `get_agent_calibration` manually" loop in Stage 0 with a
single `select_agent` call per open slot. The tool returns the ranked list; the
strategist picks from the top (or reasons about the breakdown if the query is unusual).

Add `select_agent` to the tool declarations in `moe_router_strategist/agent_card.json`.

---

### Phase 2 — Typed coordination graph

**Goal:** `workflow_template` is a typed directed graph. Platform can traverse it.
Synthesis protocol lives on the graph, not on specialist agents.

#### 2.1 Promote `workflow_template` schema

Current shape:
```json
{
  "description": "...",
  "mermaid": "...",
  "stages": [
    { "name": "...", "agent": "...", "accepts": [...], "produces": [...] }
  ]
}
```

New shape (backward compatible — `stages` remains valid, `graph` is additive):
```json
{
  "description": "...",
  "mermaid": "...",
  "synthesis": "pipeline",
  "selection": { "brier": 0.30, "fidelity": 0.20, "cost": 0.10, "valence_fit": 0.10 },
  "nodes": [
    {
      "id": "price",
      "agent": "supply_chain_oracle",   // null = open slot
      "pinned": false,                  // true = never replaced by select_agent
      "input_schema":  "scro/bom-query/1",
      "output_schema": "scro/bom-response/1",
      "description": "..."
    }
  ],
  "edges": [
    { "from": "price", "to": "review", "schema": "scro/bom-response/1" }
  ],
  "fanout": null,        // MoE: same input → multiple nodes in parallel
  "fanin":   null        // synthesis: aggregate parallel outputs
}
```

`synthesis` and `selection` move HERE, off specialist agent cards.

Validation: add a JSON Schema for `workflow_template` and run it in
`test_all_cards_have_card_specific_fields`.

#### 2.2 Graph executor (Rust)

New function in `src/agent_backend/` (not a tool — called by strategist tools):

```rust
pub async fn execute_coordination_graph(
    graph: &CoordinationGraph,
    entry_input: &Value,
    ctx: &ToolContext,
) -> CoordinationTrace
```

Steps for each node in topological order:
1. If `agent` is null: call `select_agent` using node's `input_schema` + graph's
   `selection` weights → pick top candidate
2. Call `envelope::validate_input` (already wired for every `execute_agent`)
3. Call `execute_agent` with the resolved agent
4. Call `envelope::build` (already happens inside execute_agent)
5. Thread output to downstream nodes per edges
6. On failure: stop, emit `CoordinationTrace` with partial results and failure point

Fan-out: dispatch parallel `execute_agent` calls for nodes with no incoming edges
sharing the same input. Fan-in: apply `synthesis` protocol to collect parallel outputs.

#### 2.3 `abw/coordination-trace/1` output schema

Output contract for strategist agents. Replaces prose summaries. Shape:
```json
{
  "graph_id": "...",
  "steps": [
    {
      "node_id": "price",
      "agent":   "supply_chain_oracle",
      "selected_by": "pinned | select_agent",
      "input_schema":  "scro/bom-query/1",
      "output_schema": "scro/bom-response/1",
      "gate_input":    "valid | invalid | unverified_no_schema",
      "gate_output":   "valid | invalid | unverified_no_schema",
      "duration_ms":   1240
    }
  ],
  "synthesis": "pipeline",
  "open_slots": [],
  "failure": null,
  "final_output_schema": "scro/bom-response/1"
}
```

Add `output_contract.sketch.json` to `moe_router_strategist` and `pipeline_strategist`
producing this schema. Compile with `contract-sketch`.

#### 2.4 Update strategist agents to emit typed traces

Update `moe_router_strategist` and `pipeline_strategist` system prompts to:
- Call the graph executor (via a new `execute_coordination_graph` tool wrapper)
- Return `abw/coordination-trace/1` as their output
- No longer narrate their own execution loop — the executor does that

---

### Phase 3 — Fleet scope + specimen shelf + Loop 4

**Goal:** competition credentials visible on the shelf. Selection decisions feed
Loop 4. Contract builder refactored.

#### 3.1 Specimen shelf — competition credential block

Add to `paneProfile()` or as a new group in `drawer()`:

```
COMPETITION
  Domains       supply-chain, manufacturing-bom
  Cost          3 credits / call
  Support tier  community  (self-declared)
  Fidelity      0.94  ·  847 valid / 54 refused
  Selection     75.7% when in candidate set  (214 considered, 162 selected)
```

Fidelity and selection rate are read from new API endpoints (see 1.2 and below).
These are platform-computed — not editable by the author.

`competition.domains`, `price_credits_per_call`, and `support_tier` are editable
via `AgentFields` (add a `"competition"` group — same pattern as the `"ports"` group
added in the ports rung work).

#### 3.2 Typed chip state

`studChips` in `specimen.html` currently has two states: `filled` (typed) and `orphan`
(no match in fleet). Add a third:

- **typed** (schema ID — verified interface declaration): filled chip, distinct colour
- **labelled** (bare noun — author assertion): current `filled` style
- **orphan** (no match in fleet): current `orphan` style

A label is `typed` if it appears as `input_schema_id` or `output_schema_id` anywhere
in the fleet (already available from the bestiary API). No backend change needed.

#### 3.3 Fleet scope in `select_agent`

`scope: { level: "fleet", fleet_id: "fermi" }` is already a parameter (stubbed in
Phase 1). Phase 3 implements it:

```sql
SELECT a.* FROM agents a
JOIN agent_tags t ON a.agent_id = t.agent_id
WHERE t.tag = 'fleet:' || $1
  AND a.input_contract->>'accepts_schema' = $2
```

Fleet membership is via tag (`fleet:fermi`, `fleet:simops`). A creator joins a fleet
by adding the tag. Fleet-level calibration is the aggregate Brier score across all
fleet episodes, not just one workspace.

#### 3.4 Selection rate endpoint

`GET /api/agents/:agent_name/competition-stats`

Reads from a new `select_agent_decisions` table (each `select_agent` call writes a
row: `candidates[]`, `selected`, `scope`, `criteria_weights`, `parent_episode_id`).

Returns: `{ considered: 214, selected: 162, selection_rate: 0.757, top_reason: "fidelity" }`

#### 3.5 Contract builder — move synthesis off specialist

Remove "How a coordinator combines members" from the specialist agent's contract
builder panel. Add to the composition's `workflow_template` editor (a separate surface,
not yet built — placeholder for now with a note: "Set on the composition, not the
member").

The specialist's contract builder retains: output schema, grounding, calibration signal
(what validates THIS type of output, regardless of which composition produced it).

#### 3.6 Loop 4 typed episodes

`select_agent` decisions are already written as rows (3.3). The dreaming cycle
consolidates them:

- Which schema IDs was this workspace selecting from?
- Was the selected agent's output gate-valid?
- Did a lower-ranked candidate have better gate outcomes? (counterfactual)

These become `SemanticRule` entries: "When selecting for `scro/bom-query/1` in this
workspace, increase Brier weight — cost weighting produced lower-fidelity selections."

No new dreaming infrastructure needed — the typed rows are episodes the consolidation
pipeline can read.

---

## 5. Open questions (not blocked, decide as we go)

| Question | Options | Leaning |
|---|---|---|
| Criteria weights — who sets them? | Platform default / per-composition static / Loop 4 learned | Per-composition static first; Loop 4 update in Phase 3 |
| Support tier — self-declared or verified? | Self-declared with unverified flag | Self-declared; verification when there's business case |
| `select_agent` unfillable slot — fail or skip? | Fail explicitly / skip with warning | Fail explicitly — honest partial trace is better than silent skip |
| `fanout` / `fanin` in Phase 2 — first or later? | Build with graph executor / defer to Phase 3 | Build the sequential case first (pipeline), fan-out after |

---

## 6. File map — where things land

```
src/agent_backend/agent_card.rs         Phase 1.1  competition block on AgentCapabilities
src/agent_backend/tools/domains/platform.rs  Phase 1.3  select_agent tool
src/agent_backend/coordination_graph.rs Phase 2.2  graph executor (new file)
src/handlers/agents.rs                  Phase 1.2  fidelity endpoint
migrations/228_agent_competition.sql    Phase 1.1  competition column
migrations/229_select_agent_decisions.sql Phase 3.3 selection trace table
agent-bestiary/memory/src/types.rs      Phase 1.1  Agent + AgentUpdate + CompetitionDeclaration
agent-bestiary/memory/src/store.rs      Phase 1.1  upsert/update/row_to_agent
agents/curated/moe_router_strategist/   Phase 1.4  add select_agent tool, update prompt
agents/curated/pipeline_strategist/     Phase 2.4  output_contract.sketch.json
templates/specimen.html                 Phase 3.1  competition block, typed chip state
static/js/widgets/agent-fields.js       Phase 3.1  competition group
scripts/check_specimen_shelf.js         Phase 3.1  assertions for new block
```

---

## 7. The invariant this preserves

A specialist agent author writes:
1. `input_contract.sketch.json` — what queries I handle
2. `output_contract.sketch.json` — what I return
3. `competition` block — domain, price, support tier
4. `valence` — personality (already required)

They do not touch synthesis protocol, selection criteria, or coordination topology.
Those are the composition's concern.

The platform computes their fidelity and selection rate from gate history and
`select_agent` decisions. No self-reported performance metrics.
