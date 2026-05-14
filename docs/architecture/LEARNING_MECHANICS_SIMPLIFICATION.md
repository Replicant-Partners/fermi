# Learning Mechanics Simplification Plan

**Status:** Defined, not yet started  
**Date:** 2026-05-13  
**Related:** `docs/architecture/COMPOSITION_FEEDBACK_LOOP_PLAN.md`,  
             `docs/papers/coherence_improvement_loop.md`

---

## Problem statement

The current agent catalogue contains several coordination and meta agents whose
responsibilities overlap, are stranded (not wired to anything), or duplicate
functionality already in the Rust consolidation pipeline. The result is that
the learning mechanics are spread across multiple agents and unclear to
understand as a system.

The goal of this plan is to make the learning mechanics as simple and coherent
as possible: **one owner per loop, no duplicated responsibilities, clear
boundaries.**

---

## The three learning loops — as they should be

```
LOOP 1 — Individual agent learning
  Owner: Rust consolidation pipeline (ConsolidationWorker)
  Trigger: user clicks "Consolidate Now", or scheduled
  Input: unconsolidated episodes in agent's episodic memory
  Process: DBSCAN cluster → LLM extract rules → ontology snapshot → dream_synopsis
  Output: updated ontology, new rules/entities/facts, dream_synopsis on snapshot
  No agent involved. Pure infrastructure.

LOOP 2 — Workspace / composition learning
  Owner: cohere_and_coordinate (the workspace strategist)
  Trigger: user clicks shelf buttons, or post-session dreaming trigger
  Input: workspace conversation, TEC scores, agent valence distribution,
         intention signals from member agents
  Process: coherence assessment → homophily detection → cascade RSI (write
           context episodes to member agents) → tune-team RSI (propose
           composition_versions) → write own session-observation episode
  Output: coordination brief in chat, cascade episodes in member memory,
          optional composition_versions proposal pending HITL
  cohere_and_coordinate's own dreaming cycle (Loop 1) consolidates its
  session-observation episodes into team-effectiveness rules in its ontology.

LOOP 3 — Behavioral correction
  Owner: observability stack + HITL
  Trigger: anomaly_events detected by ObservabilityWorker
  Input: eval signals, timeline entries, anomaly events
  Process: anomaly → HITL queue → reviewer acts → coherence gate → two-write
  Output: synthetic corrected episode, episode_correction record, optional
          persona_version bump
  No coordination agents involved. Pure observability + human review.
```

These three loops are independent. They share episodic memory as the medium
(episodes flow into Loop 1; Loop 2 writes episodes that Loop 1 consolidates;
Loop 3 writes synthetic correction episodes that Loop 1 consolidates). But
their ownership and trigger paths do not overlap.

---

## Current state vs. target state

### Agents being retired

#### `dream_narrator`
**What it does:** Turns consolidation synopses into narrative.  
**Why retire:** The consolidation pipeline's `generate_dream_synopsis()` function
in `agent-bestiary/consolidate/src/main.rs:448` already generates a 2–3 paragraph
first-person narrative and stores it on `ontology_snapshots.dream_synopsis`. The
`dream_narrator` agent duplicates this exactly. The pipeline writes the narrative
natively; the agent adds nothing.  
**Migration:** Remove from catalogue. The `dream_synopsis` field on ontology
snapshots is already populated by the pipeline. The agent_detail page's
"Dream Notes" display reads from `ontology_snapshots.dream_synopsis` directly —
no change needed there.

#### `coherence_consultant`
**What it does:** Takes TEC scores and generates workspace recommendations or
dream notes (2cr / 5cr tiers).  
**Why retire:** `cohere_and_coordinate` should issue recommendations directly
from its own LLM call, not via a sub-agent hop. The workspace handler currently
calls `coherence_consultant` as a sub-call inside the evaluate_coherence
handler (`src/handlers/workspace.rs:1865`). This is an unnecessary indirection.
`cohere_and_coordinate` has the same model access and a richer workspace context.  
**Migration:** Move the recommendation logic into `cohere_and_coordinate`'s
system prompt (it is already partly there). Remove the sub-call in the workspace
handler. The three shelf tiers (Index / Recommendations / Dream Notes) remain
exactly as they are — only the backend changes from calling `coherence_consultant`
to calling `cohere_and_coordinate` directly.

#### `performance_coach`
**What it does:** Reads stats from the agent_detail page and generates coaching
text.  
**Why retire:** The observability stack (Phase 3–4) now surfaces every signal
`performance_coach` reads — eval dimension scores, trend charts, anomaly events,
dyad state — with richer context and a dedicated observatory UI. The "Ask
Performance Coach" button on agent_detail (`agent_detail.html:3737`) is a
pre-observability workaround.  
**Migration:** Replace the "Ask Performance Coach" section in the Activity tab
with an "Observatory" link that deep-links to `/observatory?agent=<id>`. If
a conversational interface to observability data is wanted, route it to
`observability_coordinator` via the workspace chat pattern, not a standalone
button.

#### `coherence_evaluator`
**What it does:** Wraps the TEC settling engine and returns coherence scores.  
**Why demote:** It is infrastructure, not an agent. It has no persona, no dreaming
budget, no valence, no episodic memory. It is correctly invoked as a service
by the workspace handler (`src/handlers/workspace.rs:1081`) — that invocation
is fine and stays. What changes is that it disappears from the agent catalogue
so users don't see it as a hire-able agent.  
**Migration:** Keep the code and all service invocations unchanged. Remove the
entry from the catalogue by moving it to a `system` tier with `visibility:
private`, or simply marking it `status: archived`. Do not delete the card —
the service invocations reference it by name.

### Agents being absorbed

#### `intention_coordinator`
**What it does:** Prospective conflict detection — collects agent intentions
before action, detects duplication/contradiction/dependency/budget conflicts,
emits signals (`CLEAR`, `OVERLAP_WARNING`, `CONFLICT_ALERT`, etc.).  
**Why absorb:** This is exactly the Stage 0 that should run inside
`cohere_and_coordinate` before the TEC settling pass. The intention map
(`_coordination/intention_map.json`) is a workspace file already writable by
`cohere_and_coordinate` via its `write_workspace_file` tool. The conflict
signals map directly to TEC relation types (`IntentionAligns` → coherence+,
`IntentionConflicts` → incoherence+) that enrich the coherence graph.  
**Migration:** Extract the intention coordination logic from the
`intention_coordinator` system prompt and merge it as **Stage 0 — Pre-flight**
in `cohere_and_coordinate`'s workflow:
```
Stage 0 — Pre-flight (runs when agents are about to act):
  Read _coordination/intention_map.json
  Check declared intentions for conflict/overlap/dependency
  Emit CLEAR | OVERLAP_WARNING | CONFLICT_ALERT | DEPENDENCY_WAIT | BUDGET_GATE
  Write updated intention map
  Feed IntentionAligns / IntentionConflicts into coherence graph

Stage 1 — Assess (existing)
Stage 2 — Diagnose (existing)
Stage 3 — Coordinate (existing)
Stage 4 — Tension Audit (new, from COMPOSITION_FEEDBACK_LOOP_PLAN.md)
```
The `intention_coordinator` card is then archived (not deleted — it may be
useful as a standalone agent for non-workspace contexts).

### Agents staying unchanged

| Agent | Reason |
|---|---|
| `cohere_and_coordinate` | Expanded (absorbs intention coordination, direct LLM calls) |
| `observability_coordinator` | Correct owner of Loop 3 surface |
| `eval_runner` | Correct composition member |
| `anomaly_triager` | Correct composition member |
| `dyad_observer` | Correct composition member |
| `xaman_ek` | Cross-surface navigator, unique role |
| All domain agents | Correct, independent, no overlap |

---

## Changes required

### 1. `cohere_and_coordinate` card update

**System prompt additions:**
- Stage 0 (Pre-flight): intention map read/write, conflict detection signals,
  TEC relation feeding
- Stage 4 (Tension audit): valence homophily detection, anti-convergence
  alerts, `propose_composition_change` tool call when warranted
- Direct LLM recommendation logic (removes sub-call to `coherence_consultant`)

**Card metadata:**
- Add `"tune_team"` to `rsi_modes`
- Add `propose_composition_change` to `mcp_tools` (from COMPOSITION_FEEDBACK_LOOP_PLAN.md)
- Update `dependencies.optional` to remove `intention_coordinator` (absorbed)
  and `coherence_consultant` (absorbed)

**File:** `agents/curated/cohere_and_coordinate/agent_card.json`

### 2. Workspace handler update

**`src/handlers/workspace.rs`:** Remove the `coherence_consultant` sub-call
in `evaluate_coherence_handler`. The Recommendations and Dream Notes tiers
call `cohere_and_coordinate` directly with the TEC scores and conversation
as context.

### 3. Agent detail page update

**`templates/agent_detail.html`:** Replace the "Performance Coach" section
in the Activity tab with an Observatory link/button routing to
`/observatory?agent=<agent_id>`.

### 4. Catalogue hygiene

Mark these agents with `status: archived` or `tier: system, visibility: private`:
- `dream_narrator`
- `coherence_consultant`
- `performance_coach`
- `coherence_evaluator` (demote to infrastructure, keep callable)

The `intention_coordinator` card: keep as `status: published` for standalone
use, but remove from workspace auto-attachment. Users who want it as an
independent agent can still hire it.

---

## What the learning mechanics look like after this plan

```
Agent Bestiary
│
├── Individual learning (per agent)
│   └── ConsolidationWorker (Rust, automatic)
│       ├── DBSCAN cluster episodes
│       ├── LLM extract rules → ontology
│       ├── generate_dream_synopsis() → ontology_snapshots.dream_synopsis
│       └── Decrement dreaming_budget_credits
│
├── Workspace learning (per composition)
│   └── cohere_and_coordinate (strategist, shelf-wired)
│       ├── Stage 0: Pre-flight (intention coordination)
│       ├── Stage 1–3: Assess → Diagnose → Coordinate (TEC)
│       ├── Stage 4: Tension audit (valence homophily, composition proposals)
│       └── Writes cascade episodes to member agent memory
│           (Loop 1 consolidates these on next member dreaming cycle)
│
└── Behavioral correction (per agent, human-mediated)
    └── ObservabilityWorker + HITL
        ├── Anomaly detection → HITL queue
        ├── Coherence gate → two-write memory
        └── Synthetic correction episodes
            (Loop 1 consolidates these on next agent dreaming cycle)
```

No agent duplication. No stranded agents in the coordination layer.
One owner per loop. Episodes are the shared medium.

---

## Implementation order

| # | What | Size |
|---|---|---|
| 1 | Archive `dream_narrator`, `coherence_consultant`, `performance_coach` cards | XS |
| 2 | Demote `coherence_evaluator` to system/private | XS |
| 3 | Remove `coherence_consultant` sub-call from workspace handler | S |
| 4 | Replace "Performance Coach" button with Observatory link on agent_detail | S |
| 5 | Merge intention coordination into `cohere_and_coordinate` Stage 0 | M |
| 6 | Add Stage 4 tension audit to `cohere_and_coordinate` (from composition loop plan) | M |
| 7 | Remove direct recommendation LLM call in `cohere_and_coordinate` (uses own model now) | S |

Steps 1–4 are safe and immediate — they reduce surface area without changing any
loop behaviour.  
Steps 5–7 are the intelligence work — they should happen after the composition
feedback loop storage layer (Phase 1–2 of COMPOSITION_FEEDBACK_LOOP_PLAN.md)
is in place.
