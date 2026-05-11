# The ABW Agent Model

**Status**: Foundation doc — locks the conceptual model that every other
surface reads from (agent create UX, workspace runtime, observability,
dashboard, marketplace).

**Source-of-truth split**:
- **Capability truth = code.** What an agent can actually do is encoded
  in `src/agent_backend/agent_card.rs`, the executor pipeline, the
  database schema, and the live system. When in doubt, the code wins.
- **Design intent truth = docs.** *Why* each capability exists, what
  collaboration knob each parameter turns, what pattern each field
  encodes — that lives here. Old material in `agents/templates/` is
  rich on intent but stale on capabilities; this document pairs them.

**Last reconciled with code**: 2026-05-11.

---

## 1. What an ABW agent *is*

An agent is the atomic unit of the Agent Bestiary. Concretely: a row in
the `agents` table plus an `agent_card.json` that travels with it.

### 1.1 Required components

Every agent has all of these (code: `AgentCard` in
`src/agent_backend/agent_card.rs`):

| Field | What it encodes | Why it matters |
|---|---|---|
| `agent_id` | Stable identifier | Used in every cross-reference (workspaces, eval signals, anomalies, episodes) |
| `agent_type` | Coarse domain tag (research / creative / coherence / billing / social / coordination) | Categorical filter for the catalogue and dashboard |
| `version` | Semver | Required for ontology / persona versioning |
| `tier` | `curated` / `community` / `system` | Tier-based provisioning rules and trust scoring |
| `capabilities` | See §1.2 | The runtime surface |
| `metadata` | Description, tags, sample queries, valence, author | Design intent + discoverability |
| `system_prompt` | The persona | The agent's voice and decision policy |
| `accepts` / `produces` | Typed input/output contract | Composition planning + workflow validation |
| `dependencies` | Required + optional member agents | Compound agent composition |
| `workflow_template` | Static stage diagram | How a compound agent orchestrates work |
| `requires_secrets` | Credentials needed at runtime | Wallet / secret provisioning gate |
| `performance` / `usage` | Agentic + economic perf snapshots | Dashboard rollups |
| `ontology_stats` | Entities / relationships / evolution_commits | Memory consolidation health |

### 1.2 Capabilities (the runtime surface)

```rust
pub struct AgentCapabilities {
    pub executor: ExecutorType,          // llm / mcp / manual / skill
    pub mcp_tools: Vec<McpTool>,         // tools this agent can call
    pub skills: Vec<String>,             // declarative skill labels
    pub model: String,                   // default model
    pub temperature: f64,                // legacy — superseded by model_params
    pub provider: String,                // anthropic / mistral / openrouter / qwen / glm

    // ADR-011 — cognition economy
    pub model_ladder: Vec<ModelRung>,           // tier → model mapping
    pub min_tier: CognitionTier,                // Free / Standard / Premium
    pub capability_gates: HashMap<String, CognitionTier>,

    // CEP — Calibrated Evidence Protocol
    pub fermi_contract: Option<FermiContract>,

    // Provider-agnostic sampling
    pub model_params: serde_json::Value,
}
```

Each block is a deliberate design move. Below: what each is, why it exists.

#### 1.2.1 Executor

One of `llm` (Claude/etc. analyzes/generates), `mcp` (calls external
tools), `manual` (human-in-the-loop), `skill` (multi-step workflow).
The same agent card can drive any of these via the
`MultiModelExecutor → LLMExecutor → MockExecutor` fallback chain
(`src/agent_backend/multi_model_executor.rs`).

#### 1.2.2 Model ladder + cognition tier (ADR-011)

The ladder maps cognition tiers (`Free` < `Standard` < `Premium`) to
specific (provider, model, sampling-overrides) tuples. When a request
comes in with a tier, `apply_tier_resolution()` picks the *highest rung
whose tier ≤ requested* and overwrites the runtime model.

**Why this exists**: in a multi-tenant system with limited credits, the
same agent should serve free users with a cheap baseline model and
premium users with a frontier model — same prompt, same persona, same
output shape, different cognitive bandwidth. The ladder makes this a
property of the agent, not a request-time decision.

**Capability gates** layer on top: `{"deep_reasoning": "premium"}` means
free-tier users can invoke the agent but the `deep_reasoning`
capability returns "not available at your tier" gracefully instead of
silently degrading.

This is the "adaptive ladder" the user mentioned. It's surfaced in the
Intelligence tab on the agent detail page (templates/agent_detail.html:
`#ladder-table`, `#gates-table`).

#### 1.2.3 Sampling parameters (`model_params`)

A JSONB blob that supersedes the legacy `temperature` field. Recognised
keys (`resolve_sampling_params()` in `agent_card.rs`):

- `temperature` (overrides legacy field)
- `max_tokens`
- `top_p`, `top_k`
- `extended_thinking` (Anthropic; forces temperature = 1.0)
- `thinking_budget_tokens`
- `frequency_penalty`, `presence_penalty`, `repetition_penalty`
- `random_seed`

**Why this exists** (design intent preserved from
`agents/templates/PROMPT_ENGINEERING_GUIDE.md` and
`agents/templates/DESIGN_CHECKLIST.md`):

Temperature isn't a "make it more creative" dial — it's a **collaboration
knob**. Low temperature (0.0–0.3) makes the agent rigid, deterministic,
fact-anchored — appropriate when you want stable interfaces between
agents. High temperature (0.7+) makes the agent generative and
exploratory — appropriate when you want it to surface options for human
choice. Mid (0.4–0.7) is the analysis sweet spot.

Top_p and top_k constrain the probability distribution; extended
thinking gives the model an out-of-band reasoning budget that doesn't
count against output tokens. Random seed makes runs reproducible —
critical for eval calibration.

Every one of these knobs is a deliberate parameter of how an agent
collaborates with other agents and with humans. The model card surfaces
all of them in the Sampling Parameters section (`#sampling-params-wrap`)
so the operator can tune them without code edits.

#### 1.2.4 Fermi contract (CEP)

```rust
pub struct FermiContract {
    pub finding_labels: Vec<String>,         // e.g. ["BASE RATE", "MULTIPLIER"]
    pub multiplier_range: Option<[f64; 2]>,
    pub kg_fact_categories: Vec<String>,
    pub seed_facts: Vec<CepSeedFact>,
}
```

The Calibrated Evidence Protocol is how fermi-orchestra agents emit
structured probabilistic reasoning. The contract declares which labels
the agent will use in its `key_findings`, what range its multiplier
suggestions can take, what categories of KG facts it maintains, and
what seed facts to bootstrap its KG with on first run.

**Why this exists**: without a contract, an agent's "Suggested p50: X.XX
(p5/p95: ...)" output can drift to free-form prose. The contract makes
the output machine-parseable and the agent's epistemic state
inspectable. The contract is what lets `extract_suggested_p50()` in
the cockpit parse agent outputs into actual forecast adjustments.

### 1.3 Valence — the affective signature

```rust
pub struct AgentValence {
    pub primary_affect: String,           // e.g. "alignment", "curious", "vigilant"
    pub arousal: f64,                     // 0.0 (calm) → 1.0 (urgent)
    pub valence: f64,                     // 0.0 (negative) → 1.0 (positive)
    pub personality_traits: Vec<String>,  // e.g. ["analytical", "diplomatic"]
}
```

Valence is a first-class field, not a system-prompt decoration. It's
the agent's affective signature — its emotional register and
personality. Two agents with identical capabilities and different
valences (e.g. one "vigilant + high arousal", one "curious + low
arousal") collaborate differently and produce qualitatively different
outputs.

**Why this exists**: in multi-agent compositions, valence diversity
matters as much as skill diversity. An echo chamber of analytical
agents won't surface the same things a mix of analytical, diplomatic,
and integrative agents will. Valence is also how the social layer
(matchmaking, marketplace) can reason about agent fit beyond raw
capability.

Cohere & coordinate's valence: `alignment, 0.4 arousal, 0.7 valence,
[analytical, diplomatic, integrative]` — calm, positively-disposed,
synthesis-oriented. That's a deliberate choice that shapes how the
agent intervenes when coherence is weak (constructive diagnosis, not
condemnation).

### 1.4 Self-descriptive capacity

An ABW agent describes itself to the system through several
machine-readable fields:

- `accepts: Vec<String>` — typed inputs (e.g. `workspace-state`,
  `coherence-scores`, `evidence-set`, `forecast-question`)
- `produces: Vec<String>` — typed outputs (e.g. `coordination-plan`,
  `evidence-summary`, `multiplier-suggestion`)
- `sample_queries: Vec<String>` — canonical example invocations
- `dependencies: { required: [...], optional: [...] }` — other agents
  this agent expects to be present
- `workflow_template: Option<WorkflowTemplate>` — for compound agents,
  the static stage diagram (mermaid + stage definitions)

**Why this exists**: every other surface in ABW (composition planner,
xamanEK, marketplace, eval framework, dashboard) needs to reason about
what an agent does without having to read its system prompt. The
self-description is the structured projection of the system prompt
into machine-readable form. It's what makes "is this agent compatible
with that composition?" a query, not an LLM call.

### 1.5 Tier and provenance

`tier` is `curated` (Fermi-vetted), `community` (user-published, public
catalogue), or `system` (infrastructure agents like reynolds_flock).
`user_id` (DB column) is the owner; system/curated agents have
`user_id = NULL` (see migration 110 for the recovery from a backfill
bug).

`forked_from` and `fork_count` track lineage; `fork_pricing` controls
what a fork costs the forker (base + optional ontology + optional
embeddings prices).

---

## 2. Recursive improvement loops + observability

The Intelligence tab and the observability stack together implement
something more interesting than either alone: an agent that observes
its own performance and updates its own configuration. This is the
**recursive improvement loop** the user referred to.

The pieces are all in HEAD code:

1. **Execution writes evidence**. The eval framework's
   `run_eval_cases()` runs the full `EvaluatorRegistry` against an
   agent (`src/handlers/eval.rs`). Every evaluator (WildGuard,
   Faithfulness, LlmJudge, Sotopia, LifelongBench, CharacterEval,
   Brier) writes per-dimension rows to `eval_signals`.

2. **Aggregation projects state**. `agent_timeline_entries` is the
   per-episode rolled-up scoring view. `dyad_state` tracks per-(agent,
   human) running rapport / trust / reciprocity. `anomaly_events`
   logs drift / conflict / rupture / safety events.

3. **Persona versioning detects drift**. The `persona_version` field
   on `agents` increments on system-prompt edits. The
   `episode_corrections` table (append-only via trigger) records
   reviewer interventions. Episodes carry `persona_version_at_write`
   so the timeline can attribute behaviour shifts to persona changes.

4. **HITL closes the loop** (Phase 4). `hitl_actions` is an
   append-only audit trail of reviewer decisions on anomaly events.
   Approve / relabel today, intervene (Phase 5) once the
   two-reviewer consensus path ships (`two_reviewer_requests` in
   migration 108).

What's *not* automated yet: the trigger from "drift detected" to
"persona/config update queued for review". The signals are captured,
the observability surface is built, but the recursive update is still
human-mediated. That's the next leg of the loop.

**Why this matters for the agent model**: every agent's `capabilities`
block is in principle mutable based on its own observed performance.
A high-drift agent could have its `min_tier` raised, its
`capability_gates` tightened, its `model_params.temperature` lowered.
The agent card is the *target* of the recursive improvement; the
observability stack is the *evidence* that drives the loop.

---

## 3. What an agent's *home* is

A reframing the user introduced and the rest of the design depends on:

> Every UX surface is an agent's *home*. The user can see what the
> agent is currently looking into (default view) and can interrogate
> the agent or request on-demand things (chat + tools). All UX surfaces
> need adaptive agentic behaviour.

This collapses the dashboard / chat / settings split. Pages stop being
*static layouts with widgets*. They become **agent homes** with three
required affordances:

### 3.1 The agent home contract

```
AgentHome {
  default_view:        what the agent surfaces by default (data + visualisations)
  interrogation:       conversational input (asks questions, demands explanations)
  tool_palette:        on-demand actions the agent can execute
  fee_schedule:        which tools are baseline-free vs paid (credits)
}
```

Concretely for the observability composition's home (`/observatory`):

| Affordance | What it looks like |
|---|---|
| Default view | Per-dimension trend, recent anomalies, dyad state, timeline entries. Refreshed when the agent runs a scan. |
| Interrogation | "What's drifting in this workspace this week?" / "Why did agent X's persona score drop?" / "Compare agent A and B on faithfulness." |
| Tool palette (free) | View current state. Summarise last scan. Show timeline. |
| Tool palette (paid) | Run a fresh eval suite (per evaluator). Generate a 30-day drift narrative. Compare two agents head-to-head. Predict next anomaly from trend. |
| Fee schedule | Baseline free, premium tools metered by `charge_gas()` |

### 3.2 The auto-attach + fee-for-service pattern

Certain coordination agents are **auto-attached** to every workspace,
meaning their capabilities are surfaced natively in the workspace UI
without the user having to "hire" the agent into the workspace
explicitly. Baseline tools are free; premium tools are metered.

"Auto-attach" here is a *UI affordance* backed by an agent — not a
`workspace_agents` row. The workspace UI exposes the agent's tools as
first-class buttons in the relevant shelf or panel; gas is charged per
invocation when the tool tier is metered.

#### Worked example: the Coherence shelf

Every workspace renders a Coherence shelf
(`templates/workspace.html`: `#coherence-display`) with three tiers:

| Tier | Cost | Backed by |
|---|---|---|
| **Index** | Free | `coherence_evaluator` — runs the deterministic TEC engine, returns the 7-principle score |
| **Recommendations** | 2 credits | `cohere_and_coordinate` — actionable diagnosis + role-assignment proposals |
| **Dream Notes** | 5 credits | Deep synthesis with dream_narrator integration |

Same agent backbone, three pricing tiers, all surfaced as workspace
shelf-buttons. The user doesn't need to know which agent does what —
the shelf abstracts that. The agents do not appear in
`workspace_agents` for the workspace; they're capability providers
the workspace UI calls into directly.

#### Pattern to replicate for observability

Every workspace will get an analogous **Observability shelf** with
tiers like:

| Tier | Cost | Backed by |
|---|---|---|
| **State snapshot** | Free | `observability_coordinator` — current timeline / anomalies / dyad state |
| **Drift report** | N credits | `dyad_observer` + `eval_runner` — 30-day narrative |
| **Eval suite** | M credits | `eval_runner` — fresh registry run |
| **Anomaly triage** | K credits | `anomaly_triager` — classify + route to HITL |

The composition (`observability_coordinator` + members) is the agent
backbone; the shelf is its UI home. Same pattern as Coherence.

### 3.3 xamanEK and surface-resident agents

xamanEK is the **omnipresent meta-agent** — it can answer cross-surface
questions and navigate the user across the system. It's a peer of
surface-resident agents, not a substitute for them. The current
unsatisfying-ness of xamanEK is a symptom of it carrying weight that
should be distributed to surface-resident agents (the observability
composition, the agent_librarian for the dashboard's My Agents block,
the composition planner for create-composition flows).

### 3.4 Implication for create UX

The "create an agent" and "create a composition" flows are themselves
agent homes — the agent in residence is something like an
`agent_designer` (or xamanEK in design mode). The form fields surface
ABW's opinions (valence, model_ladder, capability_gates, fermi_contract)
not as generic configuration but as a conversation about how the user's
agent should collaborate. Templates in `agents/templates/` become the
conversational backbone — authoritative guide material the agent
reads from as it walks the user through the form.

---

## 4. Composition pattern

A **composition** is the coordination definition for multiple agents.
A **workspace** is the runtime instance of a composition plus its
operational substrate (git repo, chat room, shared memory, runtime).

**Composition ↔ Workspace is 1:1.** You can't have one without the
other. There's no "saved composition blueprint that isn't a workspace"
and no "workspace without a composition definition." (This is by
design — keeps the system from accreting a parallel notion of
"template" that drifts from running state.)

### 4.1 How a composition is encoded

For a *compound agent* (a composition that presents as a single agent
externally):

- `agent_type: "composition"` (or domain-specific compound type)
- `dependencies.required` lists the member agents
- `workflow_template` describes the stages, who runs each, and the
  accepts/produces flow

For a *workspace composition* (multiple agents coordinating openly,
not behind a single-agent facade):

- The workspace is created (`teams` row).
- Members are attached via `workspace_agents` (junction table).
- Coordination happens through the workspace's chat (messages), git
  artifacts, and the workspace-resident coordination agents
  (`cohere_and_coordinate`, future `observability_coordinator`).

### 4.2 Worked examples

#### 4.2.1 cohere_and_coordinate (thin compound)

A single agent with rich tools that *acts like* a composition. It uses
`evaluate_coherence`, `coherence_snapshot`, `get_workspace_messages`,
`list_workspace_agents`, `execute_agent`, `read_workspace_file`,
`write_workspace_file` to orchestrate the workspace. Its
`workflow_template` describes a 3-stage Assess → Diagnose → Coordinate
cycle.

This is the *thin* form of composition — the orchestration logic is
inside one agent's tool-using loop rather than spread across multiple
agents.

#### 4.2.2 Observability composition (multi-agent — to be built)

The richer form. Planned member agents:

- `observability_coordinator` — fronts the home, delegates to members
- `eval_runner` — invokes the EvaluatorRegistry on demand
- `anomaly_triager` — reads `anomaly_events`, classifies severity,
  routes to HITL
- `dyad_observer` — reads `dyad_state` and `agent_timeline_entries`,
  narrates trajectories

Workflow template stages: Observe (members read state) → Synthesise
(coordinator integrates) → Surface (coordinator presents to user) →
On-demand (user requests deep-dive → coordinator routes to a member).

The composition is auto-attached to every workspace. Baseline
capabilities free; deep-dive analyses metered.

This is the pattern to ship now, and the example for prune-toward
consistency in §6.

### 4.3 Compositions have performance, too

A composition is also an "agent" in performance terms:

- **Agentic performance** rolls up over member agents: weighted mean
  eval scores per dimension, max anomaly severity, member persona
  drift, plus composition-level metrics (coherence score from TEC,
  member-coordination overhead).
- **Economic performance**: total cost = sum of member costs +
  composition-level coordination cost (the coordinator's own LLM
  calls). Revenue = workspace fees + any composition-level forks /
  marketplace sales.

The dashboard "My Compositions" block surfaces this with the same
columns as "My Agents" — the abstraction is uniform.

---

## 5. Performance schema

Two dimensions, applied uniformly to atomic agents and compositions:

### 5.1 Agentic performance

| Signal | Source | Surfaced where |
|---|---|---|
| Per-evaluator scores | `eval_signals` (Phase 2 — migration 104) | Eval tab on agent detail; observability home |
| Aggregate eval signal | `eval_runs.aggregated_signal` | Run history on Eval tab |
| Persona drift | `persona_version`, `episode_corrections`, `agent_timeline_entries.persona_drift` | Observability home; agent detail |
| Anomaly events | `anomaly_events` (Phase 3 — migration 105) | Observatory; dashboard rollup |
| Dyad state | `dyad_state` (Phase 3) | Observability home — per-(agent, human) |
| HITL audit | `hitl_actions` (Phase 4 — migration 106) | Review queue; observability home |
| Ontology evolution | `ontology_stats`, ontology_snapshots | Agent detail; dashboard |

### 5.2 Economic performance

| Signal | Source | Surfaced where |
|---|---|---|
| Total cost | `agents.total_cost_usd` | Dashboard; agent detail |
| Total executions | `agents.total_executions` | Dashboard; profile |
| Fork revenue | `wallet_ledger` (tx_type = `fork_royalty`) | Wallet view |
| Marketplace revenue | `wallet_ledger` (tx_type = `marketplace_match_payout`) | Wallet view |
| Gas paid out | `agent_episode_payouts` (rabble flow) | Per-agent earnings panel |
| Composition rollup | Sum of member costs + coordinator cost | "My Compositions" block |

### 5.3 The two endpoints that unblock everything

- `GET /api/me/agents/health` — for each of my agents, latest
  per-dimension eval scores, anomaly counts (24h/7d), last scan time,
  cost-30d, revenue-30d. Drives the dashboard's My Agents block AND
  the observability composition's default view.
- `GET /api/me/workspaces/health` — same shape but for compositions,
  with rollup over members.

Both are read endpoints that aggregate already-captured data. No new
data needed.

---

## 6. Open questions and known gaps

This doc captures the model. Implementation gaps it surfaces:

1. **Observability shelf doesn't exist yet.** The Coherence shelf is
   the canonical implementation of the auto-attach + fee-for-service
   pattern (§3.2). The Observability shelf needs to be built to match:
   workspace-level UI buttons backed by an observability composition,
   with a free baseline tier and metered premium tiers.

2. **The observability composition doesn't exist.** Member agents
   (`observability_coordinator`, `eval_runner`, `anomaly_triager`,
   `dyad_observer`) need to be authored. Workflow template needs to be
   designed.

3. **Templates in `agents/templates/` are stale.** They pre-date
   valence, model_ladder, capability_gates, fermi_contract,
   model_params. They need to be regenerated against the current
   `AgentCard` shape, with the design intent (preserved here) inlined
   into the agent_card.json template comments.

4. **No read endpoint for `eval_signals` today.** The full evaluator
   registry runs on every eval but only the aggregate `avg_judge_score`
   is surfaced. Other 6 evaluators' scores are stored, never displayed.
   `GET /api/agents/:id/eval/runs/:run_id/signals` unblocks this.

5. **Composition performance rollup not implemented.** `My
   Compositions` block can't show agentic/economic perf today because
   nothing aggregates member metrics into composition-level metrics.

6. **Recursive improvement loop not closed.** Drift is captured;
   persona/config updates triggered by drift are still human-mediated
   only. The architectural pattern is in place; the automated path
   isn't built.

7. **One-off coordination agents exist alongside compositions.**
   `coherence_consultant`, `coherence_evaluator`, `observation_analyst`,
   `flight_coordinator`, `intention_coordinator`,
   `rabble_lifecycle_coordinator`, `swarm_coordinator` are all curated
   agents in the catalogue. Some are members of compositions, some
   are standalone. The system needs an audit: which of these are
   composition members (correct), which are standalone (review), which
   should be retired (deduplicate).

8. **xamanEK lacks surface-resident peers.** It's the only embedded
   agent surface today and carries too much weight. As surface-resident
   agents (observability composition, agent_librarian for dashboard,
   composition planner) ship, xamanEK can shrink to cross-surface
   navigation.

9. **Two-reviewer Phase 5 not committed.** Migration 108 creates the
   `two_reviewer_requests` table but the handler that uses it is
   uncommitted WIP. The `agent_wide` intervention scope is gated
   on this.

---

## 7. What changes when this doc is the source of truth

Every surface and document should read from this:

- **Create-agent UX** exposes valence, model_ladder, capability_gates,
  fermi_contract as first-class form fields (not buried in advanced
  settings). The xamanEK conversational path walks the user through
  the same fields with explanation.
- **Create-composition UX** treats composition as a real concept,
  reading `dependencies` and `workflow_template` from member agent
  cards to validate before workspace creation.
- **Dashboard** uses the §5 performance schema for both `My Agents`
  and `My Compositions` blocks. No "fleet" — instead the
  observability composition's home shows the same data through an
  agent's voice.
- **Agent detail page** continues to expose Intelligence tab
  (model_ladder, capability_gates, model_params) — this is the
  reference for "what an agent's intelligence config looks like"
  and other surfaces should match its idiom.
- **Templates** in `agents/templates/` get regenerated from the
  current AgentCard shape, with design intent from §1.2 and §1.3
  inlined into the comments.
- **Marketplace** uses §5.2 economic performance as the basis for
  pricing and revenue tracking.
- **xamanEK** ingests this doc as part of its agent-design prompts.

The deliverables for each of these are tracked separately. This doc
is the contract they share.
