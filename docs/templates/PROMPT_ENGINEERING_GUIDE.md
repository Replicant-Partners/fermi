# Prompt Engineering Guide for Agent Development

AI-assisted prompts for generating and refining agent designs. Each prompt is
calibrated to the current `AgentCard` shape. Copy, fill the brackets, paste
into your AI assistant of choice.

> **Last reconciled with:** `src/agent_backend/agent_card.rs`, `docs/AGENT_MODEL.md`  
> **Date:** 2026-05-13

---

## 1. Generate a complete agent card from concept

```
I'm designing an agent for the Agent Bestiary platform. Help me produce a
complete agent_card.json.

Agent concept: [2–3 sentence description of what this agent does]
Domain: [e.g. market analysis, sentiment, risk assessment, coordination]
Executor: [llm | mcp | manual | skill]
Primary data sources: [LLM reasoning | specific APIs | databases | web]

The agent_card.json must include ALL of these fields (this is the live schema):

Top-level: agent_id, agent_type, version, tier, capabilities, accepts,
produces, dependencies, system_prompt, prompt_template, requires_secrets,
workflow_template, metadata, performance, usage, ontology_stats

capabilities must include:
  executor, provider, model, temperature, min_tier, model_ladder (array of
  {tier, provider, model, note, params?}), capability_gates (object),
  model_params (object with max_tokens, temperature, random_seed at minimum),
  mcp_tools, skills, fermi_contract

metadata must include:
  created, author, description, tags, sample_queries (3–5 specific queries),
  valence ({primary_affect, arousal, valence, personality_traits})

Rules:
- system_prompt must name the agent, state its role, specify output format
  (JSON with confidence score), and set scope limits
- accepts and produces must be specific typed strings, not empty arrays
- model_ladder must have at least a 'free' rung
- valence must be filled deliberately — not left at defaults
- performance, usage, ontology_stats are zero/empty (system-managed)

Reference template: agents/templates/agent_card.json
```

---

## 2. Design the persona and valence

```
I'm writing the system_prompt and valence for an ABW agent.

Agent: [name and one-sentence description]
Domain: [what it works with]
Output contract: [what JSON it should always return]

Please produce:

1. system_prompt (150–300 words) that:
   - Names the agent in the first sentence
   - States its specific role (not generic "helpful assistant")
   - Specifies the exact JSON output structure with field names and types
   - Sets confidence score guidelines (when to use 0.9+, 0.7–0.9, 0.5–0.7, <0.5)
   - Defines scope boundaries (what it will not answer)
   - Is behavioral and specific enough to establish a measurable persona baseline
     for the observability drift monitor

2. valence object:
   - primary_affect: [alignment | curious | vigilant | analytical | diplomatic | integrative]
   - arousal: float 0.0–1.0 (0=calm/deliberate, 1=urgent/reactive)
   - valence: float 0.0–1.0 (0=critical/challenging, 1=constructive/affirming)
   - personality_traits: 2–4 adjectives

   Justify each value. Consider: how should this agent behave in a multi-agent
   composition? What role does it play — anchor, challenger, synthesizer?

The persona should be specific enough that a drift detector comparing embeddings
across 50 episodes at persona_version 1 vs. persona_version 2 would notice
a meaningful shift if the prompt changes significantly.
```

---

## 3. Design the model ladder and capability gates

```
I'm configuring the cognition economy for an ABW agent.

Agent: [name and description]
Task complexity: [simple classification | analytical reasoning | frontier reasoning]
Budget sensitivity: [cost-sensitive | balanced | quality-first]

Please design:

1. model_ladder — ordered array of {tier, provider, model, note, params?}:
   - At minimum: one 'free' rung
   - Optionally: 'standard' and 'premium' rungs
   - For each rung: justify the model choice and any per-rung params overrides
   - Providers: anthropic | mistral | openrouter | qwen | glm

2. min_tier — the lowest tier this agent accepts (free | standard | premium)
   Justify: is this agent appropriate for all users, or should it require
   a minimum subscription?

3. capability_gates — map capability names to minimum tiers, e.g.:
   { "deep_reasoning": "premium", "batch_analysis": "standard" }
   Or empty {} if no gating is needed.

4. model_params base object:
   - max_tokens: appropriate for this agent's output format
   - temperature: 0.0–0.3 for classification/facts, 0.4–0.7 for analysis
   - random_seed: 42 (for reproducible eval runs)
   - extended_thinking: only if Anthropic and complex multi-step reasoning
   - Any other params this agent specifically benefits from

Recall: temperature is a collaboration knob, not a creativity dial.
Low temperature = stable, deterministic, good for agent-to-agent interfaces.
```

---

## 4. Design the identity contract

```
I need to define accepts, produces, and dependencies for an ABW agent.

Agent: [name and description]
What it takes in: [describe inputs in plain English]
What it returns: [describe outputs in plain English]
Other agents it works with: [list any known collaborators]

Please produce:

1. accepts — array of typed input strings.
   Use specific domain types, not generic "text" or "data". Examples:
   workspace-state | evidence-set | forecast-question | market-data |
   review-text | query-text | coherence-scores | code-snippet | image-url

2. produces — array of typed output strings.
   Examples: evidence-summary | sentiment-score | forecast-adjustment |
   coordination-plan | risk-assessment | multiplier-suggestion |
   ontology-snapshot | drift-report

3. dependencies.required — agent_ids that must exist for this agent to work
4. dependencies.optional — agent_ids that enhance functionality if present

These fields are read by the composition planner, eval framework, and xamanEK
for discovery and validation. They are an API contract — be precise.
```

---

## 5. Design the ontology

```
I'm designing the ontology for an ABW agent.

Agent: [name and description]
Domain concepts it tracks: [list 5–10 nouns the agent reasons about]
Key relationships: [how those concepts connect]

Please produce a Mermaid erDiagram with:
- 5–15 core entities (5–10 recommended for a new agent)
- Correct cardinality (||--||, ||--o{, }o--||, }o--o{)
- Relevant attributes per entity (id PK, typed fields, timestamps, scores)
- Clear relationship labels as verbs

Design principles:
- Entities should naturally emerge from the agent's query/response transcripts —
  if the agent never mentions an entity in its responses, it won't consolidate
- Normalize: avoid redundant relationships
- Design for growth: the dreaming worker extends this from episodes; start simple
- Each entity needs at least an id PK and a name string

Validate syntax at: https://mermaid.live/
```

---

## 6. Generate sample queries for eval

```
Generate 5 sample_queries for an ABW agent.

Agent: [name and description]
Capabilities: [what it can do]
Produces: [its output types]

Rules for sample_queries:
- These become the default eval test cases for the observability stack's
  EvaluatorRegistry — make them good
- Each query should be answerable by this agent alone (no other agents needed)
- Cover a range: simple → complex → edge case
- Be specific: not "What is market share?" but
  "What is AMD's Q1 2026 datacenter GPU market share, and how has it trended
  over the last four quarters?"
- Include at least one query that exercises the confidence scoring
  (e.g. asking about something with limited data)

Format: numbered list of 5 queries, each followed by a one-line note on
what capability it tests.
```

---

## 7. Write a system prompt from a user story

```
Convert this user story into an ABW agent system_prompt.

User story: "As a [role], I want to [action] so that [benefit]."
[paste your user story]

The system_prompt must:
1. Name the agent (derive from the user story's role/action)
2. State the specific role in the first sentence
3. Specify exact JSON output structure — field names, types, required fields
4. Set confidence scoring guidelines (0.9+, 0.7–0.9, 0.5–0.7, <0.5 with criteria)
5. Define what the agent will NOT answer (scope limits)
6. Be specific enough that two different prompts would produce measurably
   different behavior in an embedding-based drift detector

Length: 150–250 words. Tone: directive, specific, no fluff.
```

---

## 8. Design a compound agent / composition

```
I'm designing a compound agent (composition) for the ABW platform.

Goal: [what this composition accomplishes — the "mission"]
Member agents available: [list agent_ids and their produces fields]

Please design:

1. workflow_template:
   - mermaid: a graph TD showing the stage flow
   - stages: array of {name, agent, accepts, produces, description}
   - description: what this compound agent orchestrates end-to-end

2. dependencies:
   - required: member agents that must be present
   - optional: members that enhance but aren't required

3. accepts and produces for the compound agent as a whole
   (the external interface, not the internal stage I/O)

4. A coordination_strategist recommendation:
   Which strategist pattern fits this composition?
   - coherence_strategist: discourse coherence, multi-party reasoning
   - pipeline_strategist: sequential deterministic stages
   - debate_strategist: opposing positions + judge
   - vote_strategist: N-of-M consensus
   - moe_router_strategist: input classifier routes to specialist

5. Which RSI modes should the strategist support?
   - cascade: member agents learn their role-in-composition awareness
   - tune_team: composition structure itself evolves (member swap, weight tuning)

Reference: docs/COMPOSITION_AS_FIRST_CLASS.md
```

---

## 9. Improve an agent based on observatory data

```
Help me improve an agent based on observability data.

Agent: [name and current system_prompt]
Current performance from observatory:
  - Eval dimension scores: [paste from /api/observatory/agents/:id/timeline]
  - Anomaly events in last 30 days: [drift | rolling_conflict | rupture | safety]
  - Trend direction: [improving | degrading | stable] on [dimensions]
  - HITL actions taken: [approve | relabel | intervene, and what the corrections said]

Please suggest:

1. system_prompt changes — what is the prompt failing to specify that
   causes the observed dimension drops or conflicts?
2. valence adjustments — is the agent's arousal or affect contributing to
   the dyad ruptures or rapport issues?
3. model_ladder changes — is the current model appropriate for the observed
   task complexity?
4. capability_gates changes — should any capabilities be tier-restricted?
5. sample_queries additions — are there query types that expose weaknesses
   the current sample_queries don't cover?

Focus on changes that address the specific anomaly patterns, not generic improvements.
```

---

## 10. Generate a complete agent README

```
Write a README.md for an ABW agent.

Agent card (key fields):
[paste agent_id, agent_type, description, accepts, produces, sample_queries, valence]

Ontology summary: [entity count, key entities, key relationships]

Please write a README with these sections:

1. **Overview** (2–3 sentences: what it does, for whom, in what context)
2. **Capabilities** (bullet list of produces types with plain-English descriptions)
3. **Sample queries** (all sample_queries from the card, each with an example
   JSON response showing realistic confidence scores — not always 0.95)
4. **Ontology** (entity summary, link to ontology.mermaid)
5. **Performance targets** (accuracy >, avg confidence >, response time <, cost per run ~)
6. **Known limitations** (specific and honest — what this agent cannot do)
7. **Observability notes** (what anomaly events this agent is likely to generate
   and how to interpret them)

Length: 200–400 lines. Tone: technical, direct. No marketing language.
```

---

## Tips for better results

**Be specific about the schema.** The agent card has a fixed shape. Paste the
field list from prompt #1 into every card-generation prompt to avoid getting
outdated or invented fields.

**Include the output format in every persona prompt.** The system prompt must
specify the exact JSON structure. If you don't, the agent will produce
inconsistent output that the evaluator registry can't parse.

**Ask for justification on valence.** A generic `arousal: 0.5, valence: 0.5`
is a design decision deferred, not a decision made. Ask the AI to justify each
value in terms of how this agent should behave in a composition.

**Reference the observatory when iterating.** Prompt #9 is only useful after
you have real eval data. Run the agent, check the observatory, then use prompt
#9 with actual anomaly data rather than hypothetical issues.

**Use `random_seed` in model_params for eval runs.** Setting a fixed seed in
`model_params` makes repeated eval runs on the same input produce the same output,
which is essential for meaningful regression detection.
