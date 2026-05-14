# Agent Design Checklist

Use this checklist to plan your agent before writing a single line of JSON. Answer every question — the answers map directly to fields in `agent_card.json`.

> **Ground truth:** `src/agent_backend/agent_card.rs`  
> **Design rationale:** `docs/AGENT_MODEL.md`  
> **Last reconciled:** 2026-05-13

---

## Step 1 — Purpose and scope

### What does your agent do?

- [ ] **One-sentence description** (becomes `metadata.description`):  
  _"My agent ________________________________________"_

- [ ] **`agent_type`** — pick the closest domain tag:  
  `research` | `creative` | `coherence` | `social` | `coordination` | `composition`

- [ ] **`accepts`** — what typed inputs does it consume?  
  _Examples: `workspace-state`, `evidence-set`, `forecast-question`, `review-text`, `query-text`_

- [ ] **`produces`** — what typed outputs does it emit?  
  _Examples: `evidence-summary`, `forecast-adjustment`, `coordination-plan`, `sentiment-score`_

> `accepts` and `produces` are the machine-readable contract that the composition
> planner, eval framework, and xamanEK use to reason about this agent. Write them
> before the system prompt — they should determine the prompt, not the other way around.

- [ ] **`sample_queries`** — 3–5 canonical questions this agent answers well.  
  These become the default eval test cases. Be specific, not generic.

---

## Step 2 — Persona and valence

The system prompt is the agent's voice and decision policy. It is also the
target of the recursive improvement loop — everything the observability stack
measures is ultimately measured against what the prompt declares the agent to be.

- [ ] **`system_prompt`** drafted:
  - [ ] Names the agent and states its role clearly in the first sentence
  - [ ] Specifies output format (JSON fields, confidence score, evidence citation)
  - [ ] Defines scope — what it will and won't answer
  - [ ] Is behavioral, not generic ("You are helpful" is not a persona)

- [ ] **`valence`** — the affective signature (shapes collaboration in compositions):

  | Field | Your choice | Guidance |
  |---|---|---|
  | `primary_affect` | _______ | `analytical` · `curious` · `vigilant` · `diplomatic` · `alignment` · `integrative` |
  | `arousal` | 0.___ | 0.0 calm/deliberate → 1.0 urgent/reactive |
  | `valence` | 0.___ | 0.0 critical/challenging → 1.0 constructive/affirming |
  | `personality_traits` | _______ | 2–4 adjectives; e.g. `["precise", "evidence-driven"]` |

  > Valence is not decoration. In multi-agent compositions, valence diversity
  > produces better collective outputs than a team of identical personalities.
  > Design your agent's affective signature deliberately.

---

## Step 3 — Execution strategy

### Executor and model

- [ ] **`capabilities.executor`:**
  - [ ] `llm` — LLM analyzes and generates (recommended starting point)
  - [ ] `mcp` — calls external tools (requires MCP server config)
  - [ ] `manual` — human-in-the-loop
  - [ ] `skill` — multi-step workflow

- [ ] **`capabilities.provider`:** `anthropic` | `mistral` | `openrouter` | `qwen` | `glm`

- [ ] **`capabilities.model`** (default/fallback): _________

### Cognition economy (ADR-011)

The model ladder lets the same agent serve different user tiers with different
models — same prompt and persona, different cognitive bandwidth.

- [ ] **`capabilities.min_tier`**: `free` | `standard` | `premium`  
  _The lowest tier this agent will accept. Below this it fails gracefully._

- [ ] **`capabilities.model_ladder`** — one rung per tier you want to support:

  | Tier | Provider | Model | Notes |
  |---|---|---|---|
  | `free` | | | Fast, cheap baseline |
  | `standard` | | | Balanced |
  | `premium` | | | Frontier — only if the task warrants it |

  _Remove rungs you don't need. At minimum include a `free` rung._

- [ ] **`capabilities.capability_gates`** — any features that should only activate
  at a given tier?  
  _Example: `{ "deep_reasoning": "premium", "extended_context": "standard" }`_  
  _Leave empty `{}` if all capabilities are available at all tiers._

### Sampling parameters

- [ ] **`capabilities.model_params`** configured:

  | Parameter | Value | Guidance |
  |---|---|---|
  | `max_tokens` | _____ | Set explicitly; don't rely on provider defaults |
  | `temperature` | _____ | Prefer this over the legacy top-level `temperature` field |
  | `top_p` | _____ | Optional; `0.95` is a safe default |
  | `extended_thinking` | false | Only for Anthropic; forces `temperature=1.0` |
  | `random_seed` | _____ | Set for reproducible eval runs |

  > Temperature is a **collaboration knob**, not a creativity dial:
  > - `0.0–0.3` — rigid, deterministic; good for stable agent-to-agent interfaces
  > - `0.4–0.7` — analysis sweet spot
  > - `0.7+` — generative, exploratory; use when surfacing options for humans

---

## Step 4 — Identity contract

These fields let every other system surface reason about this agent without
parsing its system prompt. Write them as if you were documenting an API.

- [ ] **`accepts`** lists the types of input this agent can meaningfully process
- [ ] **`produces`** lists the types of output a caller can rely on receiving
- [ ] **`dependencies.required`** lists agent IDs that must exist for this agent to work
- [ ] **`dependencies.optional`** lists agent IDs that enhance but aren't required

---

## Step 5 — Secrets and tools

- [ ] **`requires_secrets`** — does this agent need credentials?  
  For each: name (env var), label (display name), description (what it is / where to get it), is_required

- [ ] **`capabilities.mcp_tools`** — if executor is `mcp`, what tools does it call?  
  For each tool: name, description, input_schema

---

## Step 6 — Ontology design

Agents learn through ADM (Active Dreaming Memory). The ontology defines what
they accumulate and how it evolves.

- [ ] **Core entities** (aim for 5–10; add more as needed):

  | Entity | Type | Why it matters |
  |---|---|---|
  | _______ | Company / Person / Product / Event / Concept / ... | |
  | _______ | | |
  | _______ | | |

- [ ] **Core relationships** (how entities connect):

  | From | Relationship | To | Cardinality |
  |---|---|---|---|
  | _______ | _______ | _______ | `\|\|--o{` one-to-many |
  | _______ | _______ | _______ | `}o--\|\|` many-to-one |
  | _______ | _______ | _______ | `}o--o{` many-to-many |

- [ ] **`ontology.mermaid` created** — validate at https://mermaid.live/

- [ ] **Evolution strategy**: what triggers new entities or relationships?  
  _The dreaming worker extracts these from episode clusters; design entities that
  will naturally appear in your agent's query/response transcripts._

---

## Step 7 — Compound agent (skip for atomic agents)

Only fill this section if `agent_type` is `composition` or a domain compound type.

- [ ] **`dependencies.required`** lists all member agent IDs
- [ ] **`workflow_template`** designed:
  - [ ] `mermaid` stage flow diagram drawn
  - [ ] Each `stage` has: name, assigned agent, accepts, produces
  - [ ] `description` explains what the compound agent orchestrates
- [ ] Does this composition need a **coordination strategist**?  
  _See `docs/COMPOSITION_AS_FIRST_CLASS.md` for the strategist pattern._

---

## Step 8 — Observability readiness

The observability stack starts collecting signals from the first eval run.
Design for it from the start.

- [ ] **Eval test cases**: are the `sample_queries` specific enough to use as
  automated eval test cases? (They will be by default.)

- [ ] **Drift baseline**: the system uses the system_prompt + embedding of early
  episodes to establish a persona baseline. Does your prompt have enough
  specificity that a meaningful baseline can be established?

- [ ] **Dyad identity**: if this agent will have repeated interactions with the
  same users, the social tracker will build `rapport`/`trust`/`reciprocity`
  metrics per dyad. Is the agent's persona consistent enough to make those
  metrics meaningful?

- [ ] **Capability gate for drift threshold**: if this agent is expected to
  evolve rapidly (e.g. a learning agent undergoing frequent HITL interventions),
  set a looser drift threshold:  
  `capability_gates: { "drift_threshold": 0.35 }`  
  Default is `0.20`.

---

## Step 9 — Final pre-build checks

- [ ] `agent_card.json` valid JSON (check at https://jsonlint.com/)
- [ ] All ALL_CAPS placeholders replaced
- [ ] All comment lines (`//`) removed
- [ ] `system_prompt` is specific and behavioral
- [ ] `accepts` / `produces` accurately describe the I/O contract
- [ ] `valence` filled in deliberately (not left at defaults)
- [ ] `model_ladder` has at least a `free` rung
- [ ] `sample_queries` has at least 3 specific, diverse queries
- [ ] `ontology.mermaid` created and validated
- [ ] `README.md` written (agent description, queries, limitations, performance targets)

---

## Resources

- **Agent model and design rationale:** `docs/AGENT_MODEL.md`
- **Composition and strategist patterns:** `docs/COMPOSITION_AS_FIRST_CLASS.md`
- **Observability stack:** `docs/architecture/OBSERVABILITY_ARCHITECTURE_SPEC.md`
- **Worked examples:** `agents/templates/examples/`
- **Getting started tutorial:** `agents/templates/GETTING_STARTED.md`
- **Mermaid ER syntax:** https://mermaid.js.org/syntax/entityRelationshipDiagram.html
