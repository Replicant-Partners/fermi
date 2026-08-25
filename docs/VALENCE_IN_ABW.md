# Valence in the ABW — From Concept to Implementation

> **Audience:** anyone reasoning about ABW agents — designers, platform engineers, governance reviewers.
> **Cross-refs:** `docs/AGENT_MODEL.md` §1.3 · `docs/papers/abw_as_allosteric_substrate.md` §4 ·
> `docs/architecture/FEEDBACK_LOOPS.md` Loops 3–4 · `src/agent_backend/agent_card.rs` (`AgentValence`).

---

## Page 1 — Conceptual

### What "valence" means in the ABW

In the Agent Bestiary Workspace (ABW), an **agent** is not just a capability — it is
a *collaborator*. Two agents can have identical tool access, identical model
ladders, and identical I/O contracts, yet behave very differently in a workspace
because they argue, hedge, escalate, and concede differently. ABW captures that
behavioural difference in a first-class, machine-readable field on every agent
card: `metadata.valence`.

Valence is the agent's **affective signature** — its emotional register and
collaboration style, encoded as four numbers and labels:

| Field | Range | Meaning |
|---|---|---|
| `primary_affect` | enum | `alignment` · `curious` · `vigilant` · `analytical` · `diplomatic` · `integrative` · `guidance` |
| `arousal` | 0.0 – 1.0 | 0.0 calm/deliberate → 1.0 urgent/reactive |
| `valence` | 0.0 – 1.0 | 0.0 critical/challenging → 1.0 constructive/affirming |
| `personality_traits` | string[] | Adjectives shaping collaboration style |

The naming convention is borrowed directly from affect theory (the
arousal–valence circumplex), but ABW does not use it metaphorically: these
fields are read by code paths that change runtime behaviour. The crucial
design commitment, repeated across `docs/AGENT_MODEL.md` and the templates,
is that **valence is not a system-prompt decoration**. The system prompt
expresses persona in prose; valence projects that persona into a structured
form that every other surface in the platform can reason about.

### Why ABW treats valence as first-class

The original draft model stored persona only inside the system prompt. That was
adequate for single-agent invocations but broke down once the platform moved to
**compositions** (workspaces with multiple cooperating agents) and **dual RSI
loops** (per-agent learning and per-team learning). Three forces pushed valence
to the surface:

1. **Diversity matters as much as skill.** A workspace stocked with three
   "analytical, low-arousal" agents converges fast and confidently — and is
   often confidently wrong. A workspace mixing analytical, diplomatic, and
   integrative valences surfaces objections, reframings, and syntheses that
   homogeneous teams suppress. This is the "valence diversity matters as much
   as skill diversity" claim that recurs through `docs/AGENT_MODEL.md` §1.3
   and `agents/templates/README.md`.

2. **Composition needs a substrate to mutate.** The outer RSI loop
   (`Composition Dreaming`, `tune_team`) needs to *propose changes to a team*
   on the basis of accumulated session evidence. Without a structured persona
   field, the only handle the strategist has on "the team is too uniform" is
   prose comparison. Valence gives it numbers it can spread, cluster, and
   regress against coherence outcomes.

3. **Allosteric framing.** `docs/papers/abw_as_allosteric_substrate.md` reads
   ABW as a CAS substrate where each agent is a subunit and each composition
   is an oligomeric assembly. In that frame, valence is the *subunit-interface
   specificity parameter* — the thing that distinguishes a heterooligomer
   (qualitatively new collective behaviour) from a homooligomer (more of the
   same). The cooperativity hypothesis the paper proposes (heterogeneous
   compositions outperform homogeneous ones, with a Hill-coefficient analog)
   is operationally a claim about valence diversity.

### How valence shows up at the workspace level

A workspace's valence distribution is the multiset of its members' `(arousal,
valence)` points plus their `primary_affect` and trait tags. Two derived
quantities matter operationally:

- **Arousal spread** = max(arousal) − min(arousal) across members.
- **Valence spread** = max(valence) − min(valence) across members.

When either spread collapses below a threshold (currently `0.25`, defined in
`composition_dream_handler` and `docs/architecture/FEEDBACK_LOOPS.md` Loop 3),
the strategist agent (`cohere_and_coordinate`) flags **valence homophily** —
the team has drifted toward an echo chamber and is at risk of suppressing
productive incoherence. Homophily is not a failure on its own; it is a signal
that, *combined* with chronic coherence patterns, justifies a composition
proposal.

The same field also feeds:

- **Composition design** (`xaman_ek` in design mode) — when a user asks for a
  team for a task, the navigator picks members whose valences span the
  required affective range, not just the required skill range.
- **Marketplace / matchmaking** — valence is one of the social-layer signals
  for "would these two agents actually work well together for this owner?"
- **HITL review framing** — when an agent is flagged for persona drift, the
  reviewer sees the valence delta alongside the embedding-mean delta, so
  "drifted toward more critical / more aroused" is legible at a glance.

---

## Page 2 — Technical

### Schema and storage

Valence is defined in Rust at `src/agent_backend/agent_card.rs:494-501`:

```rust
pub struct AgentValence {
    pub primary_affect: String,
    pub arousal: f64,
    pub valence: f64,
    pub personality_traits: Vec<String>,
}
```

It lives inside `AgentMetadata` as `valence: Option<AgentValence>` (line 513),
serialised to JSON and stored in two places that must stay in sync:

- **Filesystem** — `agents/<tier>/<name>/agent_card.json` under
  `metadata.valence`. Authoring source of truth for curated and template
  agents.
- **Database** — the `agents.valence` JSONB column, added by migration
  `114_agent_valence_column.sql`. The migration's preamble states the design
  intent explicitly: valence was promoted from filesystem-only to a first-class
  DB column "so it can be read, written, and updated via the API without
  requiring a card file edit + redeploy". The column is JSONB so the shape can
  evolve without schema churn.

The DB and the card files are reconciled on load: the agent-card loader prefers
the file, then bridges any DB updates over the top so that owner edits made via
the dashboard (`templates/agent_detail.html` lines ~2453–2502, the *Valence*
edit panel) survive a process restart. See `src/api_server.rs:3506`
(*"Bridge valence from DB (may have been updated via API)"*).

### Read and write paths

- **Authoring (curated/community):** valence is filled in during agent design.
  `agents/templates/DESIGN_CHECKLIST.md` Step 2 and
  `agents/templates/GETTING_STARTED.md` Step 2 force the author to choose
  values deliberately — leaving valence at defaults is flagged as a pitfall in
  `agents/templates/README.md`.
- **API write:** `PATCH /api/agents/:id` accepts a `valence` field; the handler
  in `src/handlers/agents.rs:1974` adds `valence` to the SQL `SET` list when
  present, persisting through `agent-bestiary/memory/src/store.rs:683-768`.
- **API read:** every endpoint that returns an agent card (`GET /api/agents/:id`,
  composition listings, the workspace member listing) projects
  `metadata.valence` so callers never have to parse the system prompt.
- **UI write:** `templates/agent_detail.html` exposes two sliders (arousal,
  valence) plus free-text inputs for `primary_affect` and
  `personality_traits`, posting back the same JSON shape.

### Where the runtime actually consumes valence

Valence is structured persona, but it is not just metadata. Two runtime paths
read it:

**1. Composition Dreaming — `POST /api/workspaces/:id/composition/dream`**

Defined in `src/handlers/composition.rs:256-360`. The handler:

1. Loads each workspace member's `valence` from the `agents` table joined with
   `workspace_agents` (lines 263–286).
2. Embeds those member summaries into a `@cohere_and_coordinate` invocation
   prompt (lines 301–323) instructing the strategist to:
   - read its consolidated dreaming episodes for chronic coherence patterns,
   - **compute the team's valence distribution: arousal spread and valence
     spread, and flag homophily if spread < 0.25 on either axis**,
   - if a structural issue is detected, call `propose_composition_change` with
     evidence-grounded rationale,
   - if productive incoherence is being suppressed, issue an anti-convergence
     alert instead.
3. The strategist's reply, if it contains a proposal, becomes a row in
   `composition_versions` (status `pending`) for owner accept/reject.

This is Loop 4.A of the feedback architecture
(`docs/architecture/FEEDBACK_LOOPS.md` lines 158–185): the team's **shape**
mutates over time as a function of its own valence distribution and accumulated
coherence outcomes.

**2. Inner-loop coordination — `cohere_and_coordinate` Stage 4**

Inside a session, the strategist's tension-audit stage
(`docs/architecture/LEARNING_MECHANICS_SIMPLIFICATION.md` Stage 4) reads the
same valence distribution to choose between two interventions when coherence is
weak:

- If `Γ(C)` is low *and* valence spread is healthy → frame the issue as
  productive incoherence, write a synthesis-oriented coordination brief.
- If `Γ(C)` is low *and* valence spread is collapsed → escalate to
  homophily diagnosis; do not paper over the disagreement gap with prose.

### Validation and quality

- **Card load-time check:** the agent-card test suite (`agent_card.rs:758-761`)
  asserts `metadata.valence.is_some()` for every shipped card, so a
  curated/community agent missing valence fails CI rather than silently
  loading.
- **Range:** arousal and valence are validated as `f64` in `[0.0, 1.0]`. The UI
  sliders enforce the range client-side; the API handler does not currently
  range-check, which is acceptable because the only writers are owner-gated.
- **Drift observability:** persona drift is computed on embeddings of the
  system prompt; valence sits alongside that signal in the HITL reviewer view,
  so a drift event can be read as "embedding moved" *and* "valence moved from
  (0.4, 0.7) to (0.7, 0.3)" — i.e. the agent has become more aroused and more
  critical. That makes drift legible, not just numeric.

### Worked example

`xaman_ek` (the navigator agent) ships with:

```json
"valence": {
  "primary_affect": "guidance",
  "arousal": 0.4,
  "valence": 0.8,
  "personality_traits": ["omniscient", "approachable", "navigational"]
}
```

A workspace whose only members are `xaman_ek`, a hypothetical
`market_research_v2` (`alignment`, 0.45, 0.75), and `coherence_consultant`
(`integrative`, 0.4, 0.7) has arousal spread 0.05 and valence spread 0.10 —
both below 0.25. Composition Dreaming will flag this as homophily even though
the three agents nominally do different jobs. The proposal it writes will not
pick a replacement; per the prompt contract on
`composition.rs:314-315`, it names the **pattern** ("low arousal, high valence
across all three; the team will not surface adversarial readings of evidence")
and leaves the substitution choice to the owner. That is the design boundary:
valence makes structural diagnoses *legible*, but composition mutations remain
human-gated.

### Summary

Valence in the ABW is a four-field structured projection of agent persona,
stored once on disk and once in Postgres, written by both authors and the
dashboard, and read by the runtime to (a) detect homophily inside a workspace,
(b) frame coherence interventions in-session, and (c) drive the slow
team-shape RSI loop across sessions. Conceptually it is the affective
signature; technically it is `AgentValence` JSONB; operationally it is the
substrate that lets ABW reason about *who is in the room* and not only *what
they can do*.
