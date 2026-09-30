# SimOps companion fleet awareness

**Status:** Complete — `simops-orchestra` tag on all 11 specialists; `list_workspace_agents`
extended with `tags`; companion's optional deps completed; role-based routing
in companion's prompt; `list_workspace_agents` added to companion's `mcp_tools`.
**Date:** 2026-09-08
**Related:** `docs/architecture/FERMI_ORCHESTRA_AWARENESS.md` — the fermi pattern this
             generalises from,
             `docs/architecture/META_AGENT_FLEET_AWARENESS.md` — xaman_ek fleet awareness
             (platform-wide),
             `apps/kask_simops.json` — the app manifest that governs auto_hire,
             `src/agent_backend/tools/domains/workspace.rs` — `list_workspace_agents` implementation,
             `src/handlers/orchestras.rs` — platform-level orchestra injection (fermi/xaman_ek, not simops)

---

## The problem

`simops_companion` is a domain-constrained MoE strategist for SimOps. It
decomposes process simulation tasks and dispatches them to a fleet of
specialists (cascade, predictor, optimizer, narrator, and others), then
synthesises their outputs into a coherent process decision or explanation.

Doing that job requires the companion to know its specialist fleet: which agents
exist, what role each plays, and whether a given workspace has substituted or
extended the standard set.

The original implementation hard-coded agent IDs directly in the companion's
`## When to do what` section — `invoke_agent on simops_cascade`, `invoke_agent
on simops_narrator`, etc. This produced three defects:

1. **New specialists required a prompt edit.** The `kask_simops` auto_hire
   manifest is the admission mechanism, but the companion's prompt listed agents
   by ID. Admitting a new specialist to the manifest had no effect on routing
   until someone also edited the prose.
2. **Cross-workspace variation was unhandled.** A workspace that substitutes
   `simops_narrator_local` (a local model, zero API cost) for `simops_narrator`
   would not be seen by the companion — it was wired to call the standard ID.
   The substitution existed in the workspace but the companion could not reach it.
3. **Extended workspaces were invisible.** Kask-extended workspaces add
   specialists beyond the base 12 (product_scout, regulatory_scanner, etc.). A
   companion with a hardcoded list cannot discover them.

The design below eliminates all three.

---

## Structural difference from fermi

`simops_companion` and `fermi` are both domain-constrained MoE orchestrators,
but they differ in every dimension of fleet governance:

| Dimension | fermi | simops_companion |
|---|---|---|
| Specialist pool scope | Platform-level (all users share) | Workspace-level (per workspace) |
| Governance mechanism | `orchestra_members` grant table | `kask_simops` auto_hire manifest |
| Fleet awareness tier | Prompt-tier injection at execute time (`DomainRoster`) | Tool-tier discovery at runtime (`list_workspace_agents`) |
| Routing style | One specialist per driver (pick-one) | Task-dispatched (cascade for mass balance, SCOracle for pricing, etc.) |
| Calibration signal | Brier score (forecast accuracy) | SOSA observation accuracy (yield vs prediction) |
| Prompt injection | Yes — `inject_orchestra_context` builds `DomainRoster` | No — companion calls `list_workspace_agents` when needed |

**Why tool-tier for simops, not prompt-tier:** the workspace agent set is the
governing fact, and it varies per workspace and can change during a session
(extended agents added on demand). A prompt injection baked in at execute time
would be stale the moment the workspace changes. `list_workspace_agents`
returns live state; one call per session is cheap; the companion already
operates turn-by-turn.

The workspace IS the orchestra. Tool-based discovery is structurally more
accurate than prompt injection for workspace orchestras.

---

## What already existed

| piece | description |
|---|---|
| `simops_companion` (type: strategist) | Orchestrates SimOps by emitting a typed action grammar (`invoke_agent`, `edit_process`, `fork_variation`, etc.) that the kask client executes. |
| `kask_simops` app manifest | Defines `auto_hire` of 12 agents: simops_advisor, simops_cascade, simops_narrator, simops_predictor, simops_optimizer, simops_companion, simops_dynamics_runner, sidestream_miner, comparator, supply_chain_oracle, sensor_advisor, energy_advisor. |
| `list_workspace_agents` platform tool | Returns all agents hired into the current workspace with name, type, description, accepts, produces, schema IDs. |
| `workspace_agents` table | Per-workspace hired agent set. This IS the SimOps orchestra governance; admission = auto_hire + the app manifest. |

---

## Design decisions

### 1. Identification — `simops-orchestra` tag

Each SimOps specialist gets a `simops-orchestra` tag added to `metadata.tags`.
This is the `fermi-orchestra` analog: it marks an agent as belonging to the
SimOps specialist fleet rather than being a general workspace agent that happens
to be hired.

When the companion calls `list_workspace_agents` and finds `simops-orchestra` in
an agent's tags, it knows that agent is a dispatch target. Agents without the
tag (a user's personal productivity agent, say) are correctly excluded from
routing consideration.

Role is derived from existing tags on each specialist card:

| tag(s) | role |
|---|---|
| `cascade` | Deterministic mass balance, energy balance, carbon accounting, LCC |
| `predictor` | Yield regression, SOSA learning, R² tracking |
| `optimizer` | What-if solver, actuation planning |
| `narrator` | Plain-language explanation of cascade output |
| `dynamics` | ODE time-series, coupled biology models |
| `advisor` | Conversational process design wizard |
| `sidestream` | Sidestream identification and value mining |
| `pricing` or `bom` | Ingredient costs, bill of materials (supply_chain_oracle) |
| `comparator` | Side-by-side variation comparison |
| `energy-balance` | Energy density, LCA, carbon intensity per stage |
| `sensor-design` | SOSA-compatible sensor and instrumentation design |

**Why derivation, not a new field:** adding a `simops_role` field to agent cards
would be a third representation of information already encoded in tags. It would
need to stay in sync with tags while serving a different consumer. Derivation
keeps the card as the single source; routing policy lives in the companion's
prompt, not the registry.

**Why not hardcoded IDs in routing rules:** role-based routing (`find the
cascade specialist → invoke`) means that when two agents share a role (e.g.,
`simops_narrator` and `simops_narrator_local`), the companion selects by model
tier or user preference rather than failing. It also means new specialists appear
automatically once tagged and hired.

### 2. Discovery — tool-tier, not prompt-tier

`list_workspace_agents` is added to `simops_companion`'s `mcp_tools`. The
companion calls it at the start of a session (or when a routing decision requires
it) to discover the live specialist set.

This is correct because:
- The workspace agent set is dynamic — kask-extended workspaces add specialists
  beyond the base 12.
- The tool returns live state; a prompt injection computed at execute time would
  be stale the moment the workspace changes.
- The companion operates turn-by-turn; one discovery call per session adds no
  perceptible latency.

The companion does not need a full enumeration of every tool call every
specialist can accept. It needs names, roles (from tags), and type. The
`list_workspace_agents` response provides all three.

### 3. `list_workspace_agents` extended to include `tags`

The SQL query in `src/agent_backend/tools/domains/workspace.rs` gains `a.tags`
in its SELECT clause. No migration is needed — `tags` is already an `agents`
table column. The companion can now:

1. Filter the response to agents carrying `simops-orchestra`.
2. Read role tags to determine dispatch target.
3. Prefer agents by model tier or other workspace-specific preference.

The extended field is additive and backward-compatible: callers that do not read
`tags` are unaffected.

### 4. Routing — role-based, not ID-hardcoded

The companion's `## When to do what` section is rewritten from hardcoded agent
ID instructions to role-based routing:

```
1. Call list_workspace_agents.
2. Filter to agents tagged simops-orchestra.
3. Derive role from remaining tags (cascade, predictor, narrator, …).
4. Dispatch to the role that matches the task.
   When two agents share a role, prefer the one whose model tier matches
   the user's tier or stated preference.
```

This is analogous to fermi's `routing_decision` pattern, with one structural
difference: simops_companion's routing IS the action it emits
(`invoke_agent` or `invoke_member` with a typed query). The action grammar
already makes routing visible. A separate `routing_decision` output field would
duplicate it and is deliberately omitted.

### 5. Optional deps completed

The companion's optional dependencies were updated to include all 12 auto_hire
agents. Four were missing from the original declaration:
`sensor_advisor`, `simops_advisor`, `simops_dynamics_runner`, `energy_advisor`.
Optional deps are now complete and match the `kask_simops` manifest exactly.

### 6. No `routing_decision` output field (deliberate)

Unlike fermi, `simops_companion` does not emit a separate `routing_decision`
field. The reason is architectural: fermi emits a structured forecast document
where the routing metadata is distinct from the result; `simops_companion` emits
a typed action (`invoke_agent` with a named target and a typed query) that IS
the routing decision. Making routing transparent is the action grammar's job.
A parallel `routing_decision` field would be redundant.

### 7. Calibration — Loop 5 for SimOps (roadmap)

The analog of Brier score for SimOps is SOSA observation accuracy: how well the
predictor's yield forecast matched the actual measured yield recorded via
`simops_write_observation`. `get_agent_calibration` works for agents that have
resolved `eval_signals`; SimOps agents accumulate these as `sosa_observation`
results are scored against actuals.

The companion does not currently call `get_agent_calibration`. When two
specialists share a role, it selects by model tier. Calibration-based selection
is roadmap (see below).

---

## What changed

| file | change |
|---|---|
| `src/agent_backend/tools/domains/workspace.rs` | `list_workspace_agents` SQL gains `a.tags` in SELECT clause. |
| `agents/curated/simops_advisor/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_cascade/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_narrator/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_predictor/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_optimizer/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_dynamics_runner/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/sidestream_miner/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/comparator/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/supply_chain_oracle/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/sensor_advisor/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/energy_advisor/agent_card.json` | Added `simops-orchestra` tag to `metadata.tags`. |
| `agents/curated/simops_companion/agent_card.json` | Fixed optional deps (4 missing agents added); added `list_workspace_agents` to `mcp_tools`; rewrote `## When to do what` and `## What you are NOT` to be role-based not ID-hardcoded. |

---

## What this enables

**New specialists appear automatically.** Any agent added to the `kask_simops`
`auto_hire` manifest and tagged `simops-orchestra` is discovered by the companion
on the next `list_workspace_agents` call — no prompt edit required.

**Extended workspaces work correctly.** A kask-extended workspace that adds
`product_scout`, `regulatory_scanner`, or `valuechain_mapper` and tags them
`simops-orchestra` will have those specialists discovered by the companion. The
companion sees what is actually hired, not a hardcoded catalogue.

**Workspace substitution works correctly.** A workspace that replaces
`simops_narrator` with `simops_narrator_local` (local model, no API cost) is
handled: both agents carry the `narrator` role tag; the companion discovers what
is hired and prefers accordingly. The hardcoded ID path would have silently
called the non-existent standard agent.

**Role competition is resolvable.** When two agents share a role tag, the
companion has a principled selection path: model tier first, calibration score
when available (roadmap). Previously, competition was invisible and selection was
arbitrary.

---

## The broader pattern

`simops_companion` and `fermi` are both domain-constrained MoE orchestrators
that need fleet awareness, but they are instances of a forked pattern:

| | fermi | simops_companion | xaman_ek |
|---|---|---|---|
| Scope | Platform-level specialist pool | Workspace-level specialist pool | Whole platform fleet |
| Fleet size | 12 specialists (governed) | 12 base + N extended (per workspace) | 100+ agents |
| Roster in prompt | Yes — `DomainRoster` injection at execute time | No — tool call at runtime | No — O(structure) digest |
| Agent names in routing | Yes — `execute_agent` requires an ID | Yes — `invoke_agent` requires an ID | No — tools answer per-agent questions |
| Calibration in routing | Yes — Brier-based, `routing_decision` field | Roadmap — SOSA accuracy | Not yet |
| Competition signal | `routing_decision` output field | Action grammar (routing IS the action) | Roadmap |

The key fork between fermi and `simops_companion` is the governance scope:
fermi's orchestra is platform-level and stable, so a prompt injection built
from a DB view at execute time is accurate and cheap. The SimOps orchestra is
workspace-level and variable, so tool-tier discovery at runtime is the correct
mechanism. The pattern is not preference — it follows from where truth lives.

---

## Roadmap

### Near-term (this sprint)
- [x] `simops-orchestra` tag on all 11 specialists
- [x] `list_workspace_agents` in companion's `mcp_tools`
- [x] Extended `list_workspace_agents` to include `tags`
- [x] Role-based routing in companion's prompt
- [x] Optional deps completed

### Medium-term
- [ ] **`simops_contract` field on specialist cards** — a proper typed capability
      declaration analogous to `fermi_contract`: `role`, `task`,
      `calibration_signal`. Requires a DB migration to add the column to
      `agents`. Enables structured routing queries (`list_workspace_agents?contract=simops_contract`)
      rather than tag-parsing. Tag-parsing is correct for now; the contract field
      is the clean long-term form.
- [ ] **Companion calibration routing** — call `get_agent_calibration` when two
      agents share the same role tag (e.g., `simops_narrator` vs
      `simops_narrator_local`). Pick the better-calibrated agent for the
      workspace. Currently falls back to model tier.
- [ ] **Kask-extended auto_hire cohort** — define which extended agents
      (`product_scout`, `regulatory_scanner`, `valuechain_mapper`,
      `marketing_composer`) are admitted in extended workspaces, assign their
      role tags, and document the admission path from the manifest.
- [ ] **Workspace-scoped prompt injection** — if long sessions accumulate many
      tool calls and the discovery overhead becomes measurable, add a
      workspace-aware injection path to `inject_orchestra_context`. Currently not
      needed; the companion's per-session `list_workspace_agents` call is cheap.

### Long-term
- [ ] **SOSA calibration scores in `list_workspace_agents`** — surface
      `eval_signals` with `dimension = 'sosa_accuracy'` (yield prediction
      accuracy) per specialist in the tool response. The companion can then prefer
      the better-calibrated predictor/cascade pair without a separate
      `get_agent_calibration` call per candidate.
- [ ] **Workspace health digest** — analogous to the fleet digest injected into
      `xaman_ek`, but workspace-scoped: a compact summary of what is active, what
      is calibrated, and what SOSA contracts are bound. Injected into the
      companion's system prompt at execute time for long-running sessions where
      the specialist set is known to be stable.
- [ ] **Brier-based admission gate for workspace specialists** — optional minimum
      SOSA accuracy threshold for specialist role assignment (e.g., a predictor
      below a floor accuracy is demoted to `uncalibrated` status in the routing
      decision). Requires a policy decision before implementation.
