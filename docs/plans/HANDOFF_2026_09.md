# Handoff — September 2026

**Last updated:** 2026-09-05  
**Status:** Active development. This file is the entry point for any new session.

---

## What has been built (complete and tested)

### A2A typed interface stack
- `AgentCapabilities.input_contract` (Phase A) — `input-contract-sketch` binary, 11 agents compiled
- `list_workspace_agents` returns `input_schema_id` + `output_schema_id` (Phase B, migration 227)
- `Gate::InputSchema` + `envelope::validate_input` — advisory hop validation (Phase C)
- Strategist prompts updated: schema ID routing, seam types (Phase D)
- Fleet tags: `fleet:fermi` (6 agents), `fleet:weather` (4 agents)

### Typed coordination and competition
- `competition` block on agent cards (migration 228) — domains, price, support tier
- `select_agent` workspace tool — ranks candidates by Brier, fidelity, cost, valence fit
  - Fleet scope SQL implemented (`fleet:<id>` tag query)
  - Workspace-level selection weights (migration 231, `teams.selection_weights`)
  - Loop 4B: workspace weights nudged ±0.02 from fidelity outcomes during dreaming
- `select_agent_decisions` trace table (migration 229) — records every selection call
- `Gate::OutputSchema` promoted to `Retention::Recorded` (migration 230)
- `/api/agents/:id/fidelity` endpoint — live from gate_decisions ledger
- `/api/agents/:id/competition-stats` endpoint — from select_agent_decisions

### Typed coordination graph executor
- `WorkflowTemplate` extended: `nodes`, `edges`, `synthesis`, `selection`
- `coordination_graph.rs` — level-based executor supporting three topologies:
  - **Pipeline**: sequential, each node feeds the next
  - **MoE fan-out**: all nodes at level 0 receive the same input, outputs synthesised
  - **Hybrid**: mixed DAG, arbitrary fan-out/fan-in
- Synthesis protocols: `selection`, `aggregation`, `cep_weighted`, `max_risk`, `pipeline`
- `execute_coordination_graph` workspace tool — auto-reads from composition strategist card
- `moe_router_strategist` and `pipeline_strategist` output contracts: `abw/coordination-trace/1`

### Specimen shelf
- `accepts` editor (ports rung)
- Competition credential block (live fidelity + selection rate via fetch on drawer open)
- Typed chip state: schema IDs render in aqua monospace, distinct from bare labels
- Brain section: provider-aware sampling params (llm_provider dropdown, temperature slider,
  provider-conditional max_tokens/top_k/top_p/extended_thinking/penalties)

### Contract builder
- Tab 3 ("Who uses it") redesigned: consumers → calibration signal → prompt needs → synthesis
- Synthesis protocol moved to collapsible "coordinator agents only" section
- Prompt snippet: minimal one-line copy + full block in `<details>`

### Composition editor
- `▷ Graph` tab on workspace page — visible only when workspace has a coordination strategist
- Form-based node/edge editor (Phase 1): add/remove/configure typed nodes, draw edges
- Synthesis protocol + candidate scope pickers
- Saves via PUT to strategist agent's `workflow_template`

---

## Open — ordered by value

### 1. Composition editor — Phase 2: visual graph
**What:** Canvas-based drag-and-drop graph visualization to replace the current form list.
**Why:** The form-based Phase 1 editor is functional but not legible for complex compositions.
A visual graph makes topology obvious — fan-out vs. pipeline vs. hybrid at a glance.
**Design:** SVG/canvas with node boxes, directional edges. Node click → opens config panel.
Drag to reorder. No external graph library (too heavy) — lightweight custom SVG.
**Files to create:** `static/js/widgets/composition-graph.js`, `static/css/composition-graph.css`
**Integration:** Replace `renderCgEditor()` in workspace.html with a canvas renderer that
maintains the same data model (cgNodes, cgEdges, cgSynthesis, cgScope).

### 2. Fan-out parallel execution
**What:** The graph executor currently runs level-nodes sequentially. For MoE, all member
nodes should dispatch in parallel (same input, outputs collected when all complete).
**Why:** True MoE parallelism — 3 analysts running simultaneously vs. serially.
**Design:** Use `tokio::task::JoinSet` or equivalent. Challenge: `execute_node` takes
`&ToolContext` which is not `'static`. Options:
  - Wrap ToolContext in `Arc<ToolContext>` and clone for each task
  - OR: keep sequential but flag nodes as "can parallelize" and batch
**Files:** `src/agent_backend/coordination_graph.rs` — modify the level execution loop
**Constraint:** The current sequential implementation is correct and preserves output order.
Parallelism is a performance optimization only — semantics are identical.

### 3. Marketplace scope in `select_agent`
**What:** `scope: {level: "marketplace"}` currently stubs to workspace scope.
Full implementation queries all public ABW agents with matching `input_schema_id`.
**Why:** The whole point of the competition design is an open dynamic market.
**Design:**
```sql
SELECT a.agent_id, a.agent_name, a.agent_type, a.description,
       a.input_contract->>'accepts_schema'   AS input_schema_id,
       a.output_contract->>'produces_schema' AS output_schema_id,
       a.competition, a.valence
FROM agents a
WHERE a.input_contract->>'accepts_schema' = $1
  AND a.visibility = 'public'
```
**Complexity:** Trust signals matter more at marketplace scope. `fidelity` (from gate_decisions)
and `competition.support_tier` are the key trust anchors. Rate limiting and billing for
cross-owner agent calls are needed before production use.
**Files:** `src/agent_backend/tools/domains/workspace.rs` — `execute_select_agent`

### 4. A2A external provider (`DESIGN_a2a_provider.md`)
**What:** ABW agents callable from external systems via the A2A protocol.
**Endpoints to build:**
- `GET /.well-known/agent.json` — Agent Card for an ABW agent
- `POST /a2a/:slug/message:send` — sync execution, returns Task
- `POST /a2a/:slug/message:stream` — SSE streaming execution
- `GET /a2a/:slug/tasks/:episode_id` — task status
**Auth:** New API key scope `a2a:invoke`. External callers get API keys; billing via credits.
**Files to create:**
- `src/a2a_card.rs` (Agent Card mapping: ABW → A2A) — stub exists
- `src/a2a_task.rs` (Episode → Task state mapping) — stub exists
- Routes in `src/api_server.rs`
**Design doc:** `docs/DESIGN_a2a_provider.md`

### 5. Loop 4A — composition evolution
**What:** Changing team *membership* based on performance signals (Shapley attribution,
session history, valence homophily). Currently only Loop 4B (routing accuracy) is implemented.
**Why:** The full Loop 4 closes both routing AND team structure.
**Design:**
  - Read `forecast_agent_credit` (Shapley scores) for workspace composition members
  - Read session patterns from strategist's consolidated memory
  - Generate `composition_versions` proposals when evidence is sufficient (≥10 sessions)
  - Owner reviews + accepts/rejects via `/api/workspaces/:id/composition/versions`
**Already exists:** `composition_versions` table (migration 113), proposal/review API

### 6. `check_contract_builder.js` failures (contract builder tools tab)
**What:** 12 pre-existing failures in the tools/grounding affordances (response-shape picker,
reverse-lookup hints, tool-switch path). Not caused by our changes.
**How to run properly:**
```bash
# With Railway DATABASE_URL in scope:
DATABASE_URL=$RAILWAY_DB cargo test contract_builder_headless -- --nocapture
# OR: hit the live API endpoint manually:
curl https://agent-bestiary.world/api/contracts/tools | jq . > /tmp/shapes.json
node scripts/check_contract_builder.js /tmp/shapes.json
```
**Blocker:** needs a running database. The Railway deployment has one.
The `shapes.json` file is generated by `tests/contract_builder_headless.rs` — running that
test against Railway produces the file and runs the check automatically.

### 7. More input_contract sketches
**Current:** 11 agents typed (fermi fleet, weather fleet, SimOps companion, adaptogen curator)
**Remaining value:** ~70 curated agents still have no typed input contracts.
High-priority additions:
- `debate_strategist` → `abw/debate-request/1`
- `vote_strategist` → `abw/vote-request/1`
- Fermi orchestra members (fixture_context_agent, football_institution_agent)
- SimOps fleet agents (simops_cascade, simops_predictor, simops_optimizer)

---

## Key invariants to preserve

1. **`check_specimen_shelf.js`** must pass (`node scripts/check_specimen_shelf.js`)
2. **`check_agent_fields.js`** must pass (`node scripts/check_agent_fields.js`)
3. **`cargo test --lib -p fermi -- agent_backend::agent_card::tests`** must all pass (14 tests)
4. **`cargo run --bin input-contract-sketch -- --all --check`** must pass for all 11 agents
5. **`cargo run --bin contract-sketch -- --all --check`** must pass for all 14 output sketches

---

## Architecture diagram

```
SPECIALIST AGENT declares:
  input_contract    → what query shape I accept (schema ID, compiled from .sketch.json)
  output_contract   → what I produce (schema ID + grounding, compiled from .sketch.json)
  competition       → domain tags, price per call, support tier
  valence           → personality (affects valence_fit scoring in select_agent)
  [platform computes: fidelity (gate_decisions), selection_rate (select_agent_decisions)]

COORDINATION GRAPH (teams.workflow_template or strategist agent's workflow_template):
  nodes             → schema-typed slots (agent bound or open → select_agent fills)
  edges             → typed seams (output_schema_id → input_schema_id)
  synthesis         → selection | aggregation | pipeline | cep_weighted | max_risk
  selection         → scope (workspace | fleet | marketplace) + criteria weights

EXECUTION:
  execute_coordination_graph(template, entry_input, ctx)
    → assign_levels() → BFS topology detection
    → execute each level (sequential; Phase 2: parallel for fan-out)
    → synthesise_outputs() at fan-in points
    → return CoordinationTrace (typed, includes topology + gate verdicts)

LOOP 4B (during agent dreaming):
  consolidate_selection_performance()
    → reads select_agent_decisions WHERE selected = agent_name (last 30 days)
    → reads gate_decisions WHERE gate = 'output_schema' AND subject = agent_name
    → stores SemanticRule with fidelity + selection count
    → nudges teams.selection_weights ±0.02 based on fidelity
```

---

## File map — where key things live

```
src/agent_backend/coordination_graph.rs  Graph executor (topology detection, synthesis)
src/agent_backend/tools/domains/workspace.rs  select_agent, execute_coordination_graph
src/agent_backend/agent_card.rs          AgentCapabilities + CoordinationGraph structs
src/gate_trust.rs                        Gate::InputSchema, Gate::OutputSchema (Recorded)
src/agent_backend/envelope.rs            validate_input(), build()
src/handlers/agents.rs                   fidelity + competition-stats endpoints
agent-bestiary/memory/src/consolidation.rs  Loop 4B (consolidate_selection_performance)
templates/workspace.html                 Composition graph editor (▷ Graph tab)
templates/specimen.html                  Competition block, typed chips, brain section
static/js/widgets/agent-fields.js        competition group, provider-aware sampling params
static/js/widgets/contract-builder.js    Tab 3 redesign
scripts/input_contract_sketch.rs         Input contract sketch compiler
agents/curated/*/input_contract.sketch.json  11 compiled input contracts
migrations/
  227_agent_input_contract.sql           input_contract column
  228_agent_competition.sql              competition column
  229_select_agent_decisions.sql         selection trace table
  230_gate_input_schema_and_output_schema_recorded.sql  Gate promotion + CHECK widening
  231_workspace_selection_weights.sql    teams.selection_weights
docs/plans/TYPED_COORDINATION_AND_COMPETITION.md  Full design spec (may be outdated)
docs/DESIGN_a2a_provider.md             A2A external provider design (NOT YET BUILT)
docs/DESIGN_a2a_contracting.md          A2A contracting design (partially built)
docs/DESIGN_a2a_protocol.md             A2A protocol design (built)
```
