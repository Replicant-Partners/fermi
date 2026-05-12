# Composition as a First-Class Element

**Companion to** `docs/AGENT_MODEL.md` (what an agent is) and
`docs/VERTICAL_HARNESS_SPLIT.md` (substrate vs vertical layering).

AGENT_MODEL.md treated composition as "a workspace with member
agents" — accurate but thin. This doc upgrades composition to a
first-class creation surface, peer to agent creation. It also names
the **coordination strategist** as a class of agents and introduces
the **dual RSI loops** (cascade + tune-the-team) that distinguish
compositions from single agents.

**Status**: design draft. Locks the conceptual model so the
composition-creation UX, the strategist agent class, and the
composition-level improvement loop have a shared frame.

---

## 1. What a composition *is*

A composition is a **goal-bearing assemblage of agents**. It exists
to accomplish something — that something is its **mission**. It has
member agents (the experts), a **coordination strategist** (an
agent that embodies one strategy for getting work done across the
members), success signals it can be evaluated against, and an
**improvement loop** so the team gets better over time.

The composition is the conceptual primary. The **workspace** is its
operational surface — a `teams` row plus the git repo, chat,
shared memory, and gas wallet that let the composition run. One
workspace = one composition. Per AGENT_MODEL §4.

> **Compositions ≠ workflows.** A workflow is a static stage
> diagram. A composition can use a workflow (a strategist might
> impose one) but it can also run dynamically — debate, voting,
> hierarchical delegation, MoE routing. The strategist decides.

---

## 2. The conversational creation arc

Composition creation parallels agent creation. xamanEK is the
guide for both. Each step has an authoritative template in
`compositions/templates/` (parallels `agents/templates/`).

| Step | Question xamanEK asks | What gets captured | Source material |
|---|---|---|---|
| 1 | "What do you want to accomplish?" | `teams.mission` (free text) | `compositions/templates/DESIGN_CHECKLIST.md` |
| 2 | "What kinds of expertise will you need?" | Member agents (workspace_agents rows) | `compositions/templates/PROMPT_ENGINEERING_GUIDE.md` + the agent catalogue |
| 3 | "How should they work together?" | `teams.coordination_strategist_id` (pointer to a strategist agent) | Tag-filtered catalogue (`tag = coordination_strategy`) |
| 4 | "How will you know it's working?" | Success signals — for MVP, free text; later, structured criteria | (deferred — extracted from real examples) |
| 5 | "How should the team learn?" | Surfaced from the strategist's own declaration of supported RSI modes — user picks among those | Strategist card metadata |

xamanEK reads the templates as canonical guide material the way it
will (eventually) for agent creation. The first version of the
flow is a structured form that xamanEK augments with recommendations;
fully conversational xamanEK comes after.

---

## 3. Strategist — a class of agents

Coordination strategies are **a class of agents tagged
`coordination_strategy`**. They live in `agents/curated/` like any
other agent. They get assigned to a composition at create time and
participate in the workspace runtime.

**Why agents, not a separate trait/registry crate** (the path
considered and rejected): strategists need to learn. If they're agents,
they automatically get:

- Ontology evolution (consolidation)
- Persona versioning
- Eval signals + drift detection
- Dyad state with the human admin
- HITL audit trail

A pure-Rust trait registry can't learn. An agent-tagged-as-strategist
gets all of the above for free. The "registry" is just
`SELECT agent_id FROM agents WHERE 'coordination_strategy' = ANY(tags)`.

### 3.1 The strategist's card declares

In addition to the standard agent card fields, a strategist's card
declares (in `metadata.strategy` — free-form JSON for now; we
formalise as a struct once we have ≥3 strategist implementations):

```json
"strategy": {
  "rsi_modes": ["cascade", "tune_team"],
  "member_count_min": 2,
  "member_count_max": null,
  "member_role_requirements": [],
  "intended_for": "Discourse coherence — multi-party explanatory reasoning"
}
```

- `rsi_modes` — which composition-level improvement loops this
  strategist supports (see §4). Surfaced at create time so the user
  picks among only what's actually available.
- `member_count_min/max` — bounds on team size. Some strategies
  (vote, debate) require ≥2; pipeline can be any length.
- `member_role_requirements` — typed slots if the strategy needs
  specific roles (e.g., debate needs ≥2 with opposing positions).
  Empty list = any members work.
- `intended_for` — short prose so xamanEK's recommendation logic
  can match user task descriptions to strategies.

### 3.2 The catalogue grows over time

First strategist: `cohere_and_coordinate` (coherence strategy —
existing). Subsequent strategists, in rough authoring order:

1. `coherence_strategist` (rename of `cohere_and_coordinate` to
   match the pattern — alias preserved)
2. `debate_strategist` — opposing-position assignment + judge
3. `pipeline_strategist` — sequential workflow execution
4. `vote_strategist` — N-of-M consensus
5. `moe_router_strategist` — input classifier picks one member
6. `reflexion_strategist` — produce, critique, iterate

Each is just an agent card + system prompt + tool set. None of these
require new Rust code — they're authored content.

---

## 4. The dual RSI loops

Single agents have one improvement loop: dreaming
(consolidation → ontology evolution → persona refinement).
Compositions have TWO peer loops operating at different abstraction
levels.

| Loop | What improves | Where state lives | Trigger | Strategist must declare |
|---|---|---|---|---|
| **Cascade RSI** | Each member agent's *role-in-the-composition* awareness | Member's `agent_timeline_entries` augmented with composition context; relational dyad_state | Composition dreaming → cascade to members | `"cascade"` in `rsi_modes` |
| **Tune-the-team RSI** | The composition itself — strategist swap, member changes, weight retuning | New: `composition_versions` table (mirrors `agent_versions`) | Composition dreaming → strategist proposes structural deltas → HITL gates | `"tune_team"` in `rsi_modes` |

Both fire from the same trigger ("composition dreaming") but write
to different layers. A strategist can support both, one, or neither.
xamanEK surfaces whatever the chosen strategist offers — the user
doesn't pick blind.

Concrete examples:

- `coherence_strategist` supports **cascade** (members become aware of how their utterances contribute to workspace TEC scores) but probably not **tune_team** (the strategy is the strategy — swapping it changes the composition's identity)
- `pipeline_strategist` supports neither today — pipelines are static. Future versions could support cascade.
- `moe_router_strategist` naturally supports **tune_team** — the router learns over time which member is best for which input class. Could also support cascade if members learn from their own routing decisions.
- `debate_strategist` supports **cascade** (members learn their adopted positions) and arguably **tune_team** (which members get picked as debaters could evolve).

This is exactly the **MoE improvement loop** named in the design
session — the composition learns to be a better team.

---

## 5. Workspaces as the operational surface (elevated)

AGENT_MODEL §4 named this but underweighted it. Re-stating with the
right emphasis:

- The **composition** is what the user creates and reasons about
- The **workspace** is what runs — the chat, the git repo, the
  shared memory, the gas wallet
- The workspace's UI is composition-shaped: header shows mission +
  strategist, shelves show coordination shelf alongside coherence
  and observability, members are listed as participants not just
  hired agents

This means the workspace detail page becomes the *primary
composition surface*. The current dashboard's "Compositions" block
is just a directory of those surfaces.

---

## 6. Data model

Minimum additions to support all of the above:

```sql
-- §1 — composition identity
ALTER TABLE teams
    ADD COLUMN IF NOT EXISTS mission TEXT,
    ADD COLUMN IF NOT EXISTS coordination_strategist_id UUID,
    ADD COLUMN IF NOT EXISTS strategist_assigned_at TIMESTAMPTZ;

-- §4 — tune-the-team RSI history
CREATE TABLE IF NOT EXISTS composition_versions (
    composition_version_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES teams(id),
    version_number INT NOT NULL,
    mission TEXT,
    coordination_strategist_id UUID,
    member_agent_ids UUID[],
    member_weights JSONB,
    diff_summary TEXT,           -- why this version
    proposed_by TEXT,            -- 'user' or strategist agent_id
    accepted_by TEXT,            -- user_id of the human who approved
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

That's it for MVP. Per-strategist strategy metadata lives in the
agent card JSON (no DB column needed).

**Backfill on first deploy:** existing workspaces get `mission =
NULL` and `coordination_strategist_id = NULL`. They keep working —
nothing in the runtime currently uses these fields. As users
encounter their existing workspaces in the new UX, they're prompted
to fill in mission + pick a strategist.

---

## 7. Templates — `compositions/templates/`

Mirrors `agents/templates/`. Contents:

- `README.md` — what an ABW composition is
- `DESIGN_CHECKLIST.md` — the 5 questions of §2 with worked-example
  answers
- `PROMPT_ENGINEERING_GUIDE.md` — xamanEK prompts for each step
- `composition_card.json` (or the equivalent — likely just a
  documented JSON shape for create-payload, since compositions
  don't have a free-standing "card" the way agents do; they're
  workspace + strategist references)
- `examples/`
  - `observability/` — the existing observability composition
    (observability_coordinator + 3 specialists) as a worked example
  - Later: a research-team example, a debate-panel example, etc.

The templates are the canonical guide material xamanEK reads from
during composition creation. Like agents/templates/, they document
design intent (why coordination strategies, why dual RSI, why
declared mission) alongside the mechanical shape.

---

## 8. Non-goals (deliberate punts)

- **Structured success criteria.** §2 step 4 captures success as
  free text for MVP. We extract structure once we see real examples
  rather than designing speculatively.
- **Automatic composition dreaming on a schedule.** Triggers are
  explicit-user-invocation only for MVP. Time-based triggers (e.g.
  "every 100 episodes, run cascade RSI") come later.
- **Composition marketplace.** Forks/royalties for compositions
  (analogous to fork pricing for agents) are out of scope. Reusable
  compositions live in `compositions/templates/examples/` for now.
- **Cross-composition orchestration.** A composition that *contains*
  other compositions (recursive MoE) is interesting but speculative.
  Defer.
- **xamanEK fully conversational.** First version is a smart form
  with xamanEK recommendations. Full conversational creation comes
  after we have signal on what users actually create.

---

## 9. Open questions remaining

After this doc, three things are still open:

1. **Strategist invocation cadence.** Is the strategist called on every
   workspace turn, on `@`-mention only, on coordination events
   (member conflict, eval anomaly), or only when the user asks? My
   lean: on `@`-mention and on coordination events (anomaly_triager
   surfaces a conflict → strategist gets asked to mediate). Pure
   per-turn invocation is too expensive.
2. **Member weighting.** Some strategies (vote, MoE routing) need
   per-member weights. Where do they live — on `workspace_agents`
   as a column, or on `composition_versions.member_weights` as a
   JSON map? Per-version is cleaner (weights ARE the team's tunable
   state under tune-the-team RSI).
3. **Strategist replacement.** Can the user swap the strategist
   mid-composition? Yes (it's just an UPDATE) but it produces a new
   `composition_versions` row and may invalidate cached coordination
   state. Need to define what carries over.

---

## 10. Implementation order

Minimum thing that proves the workflow. Each step is one
commit/PR.

**Step 1 — Migration + tag the existing strategist.**
- `teams.mission`, `teams.coordination_strategist_id`,
  `teams.strategist_assigned_at`
- `composition_versions` table
- Tag `cohere_and_coordinate` with `coordination_strategy`
- Stamp its card metadata with `strategy.rsi_modes = ["cascade"]`
  and `strategy.intended_for = "Discourse coherence ..."`

**Step 2 — Composition creation surface.**
- Update the workspace-create UX in `dashboard.html` to a guided
  form with mission textarea + strategist dropdown + member picker
- Backend: accept `mission` and `coordination_strategist_id` in
  the create payload
- Workspace header shows mission + assigned strategist

**Step 3 — `compositions/templates/`.**
- Author the directory: README, DESIGN_CHECKLIST,
  PROMPT_ENGINEERING_GUIDE
- Move the observability composition's design notes into
  `compositions/templates/examples/observability/`

**Step 4 — "Ask strategist" shelf.**
- Workspace right panel gets a "Strategy" shelf alongside Coherence
  and Observability
- Buttons: "Diagnose coordination state" (free), "Propose
  tune-the-team delta" (priced — strategist runs analysis)

**Step 5 — Composition dreaming trigger.**
- Manual button on workspace: "Run composition dreaming"
- Fires the strategist with longitudinal context
- Cascade-RSI strategist writes context into member episodes
- Tune-team-RSI strategist proposes a `composition_versions` row
  marked `proposed_by = <strategist_id>`, status pending HITL
- User approves → new version becomes active

**Step 6 — xamanEK upgrade.**
- xamanEK gains `recommend_strategist(task)` and
  `recommend_members(task, strategist)` tools
- Augments the form-driven flow with conversational suggestions
- Eventually replaces the form for users who prefer chat

Steps 1–2 are the MVP — they prove the workflow end-to-end with
the existing strategist. Steps 3–4 add the surface depth. Steps
5–6 close the RSI loop and the conversational ergonomics.
