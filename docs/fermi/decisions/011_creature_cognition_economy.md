# ADR-011: Creature Cognition Economy

**Date:** 2026-02-27
**Status:** Proposed
**Deciders:** ilabra
**Related:** ADR-001 (Architecture), creature_versions table, creature_conditions table, consolidation/dreaming system, ABW agent cards

---

## Context

### The Problem

Agent Bestiary World (ABW) currently runs all agents on paid Anthropic models (Claude Sonnet and Haiku). Every agent execution costs money. In the context of Rabble.world, this means every creature interaction — minting, flying, narrating, choreographing, dreaming — incurs API costs borne by the platform.

We need to:

1. **Let people play Rabble for free** without incurring unsustainable model costs.
2. **Create natural upgrade paths** where players invest in their creatures' cognitive capabilities.
3. **Add OpenRouter free models** (`openrouter/free`) as a baseline that's always available at zero cost.
4. **Support graceful degradation** so infrastructure failures (rate limits, API outages) don't kill the experience.
5. **Let ABW agent makers configure** which models serve which tiers, based on eval scores and benchmarking.

### The Insight

Rabble creatures already have a **cognition_level** computed from their history:

```sql
FLOOR(LOG(2, 1.0
  + versions * 1.0
  + dream_cycles * 5.0
  + total_flights * 0.2
  + unique_locations * 0.3
  + rabble_participations * 2.0
  + active_modules * 1.0
))::int AS cognition_level
```

This measures **earned knowledge** — what the creature has learned through dreaming (consolidation cycles with counterfactual learning), coherence engine assistance, AKP knowledge sharing, and lived experience. This knowledge is persistent, grows over time, and belongs to the creature. It never degrades.

But knowledge without the ability to reason over it is a library without a reader. The model powering the creature determines how effectively it can *use* what it knows. A free model can do basic retrieval. A Sonnet model can synthesize across domains, spot non-obvious connections, and produce nuanced creative output.

**This gives us the game mechanic:** the creature's cognitive growth creates natural demand for better reasoning capability. The player sees their creature accumulating knowledge but notices its behavior is basic — it has knowledge but can't fully express it. The upgrade prompt becomes: *"Your creature has outgrown its cognitive capacity. It knows more than it can express. Unlock deeper reasoning?"*

### What We're NOT Doing

- **No deterministic workflow graphs.** Compound agents reason about what to do next. That's the point. We don't introduce traffic cops (like rs-graph-llm) that would slow evolution and kill emergent behavior.
- **No silent quality degradation.** If a model produces garbage, we don't quietly fall back to a worse model and pretend it worked. The player should know their creature needs better cognition.
- **No feature paywalls.** Free creatures are real creatures. They fly, join rabbles, show up in AR, dream, and grow. The upgrade makes an already-growing creature *flourish*, it doesn't unlock a locked door.

---

## Decision

### Core Model: Cognition = Knowledge × Bandwidth

A creature's cognitive capability is the product of two independent dimensions:

**Knowledge** (earned, persistent, grows over time):
- Embedding space (grows with every interaction, dreaming cycle, AKP share)
- Knowledge graph (grows via AKP knowledge sharing between creatures)
- Consolidation history (dream cycles with counterfactual learning)
- Coherence improvements (TEC-assisted reasoning refinement)
- Flight history, rabble participation, module activations
- Measured by the existing `cognition_level` formula

**Bandwidth** (selected by owner, determines expression ceiling):
- `free` — basic retrieval, simple generation, baseline availability
- `standard` — moderate synthesis, reliable structured output, tool use
- `premium` — deep cross-domain reasoning, creative expression, complex orchestration

Knowledge is the creature's soul. Bandwidth is its voice.

### Cognition Tiers

#### Free Tier (`openrouter/free`)

**Cost:** Zero. Always available. No API key required from the user.

**Creature experience:**
- Minted with a basic specimen image (simpler variation prompt → less unique)
- Flies with the flock (reynolds_flock is deterministic, always works)
- Shows up in AR portals and on the map
- Chat works (chat is human-to-human through creatures, not model-dependent)
- Narrator says "A butterfly joined the rabble" (functional, not vivid)
- Basic flight recording (straight-line plans, no terrain/weather awareness)
- Dreaming works but consolidation summaries are simpler
- Keeper tracks state but doesn't provide nuanced care recommendations
- No enemy detection, no prey hunting, no complex choreography

**Agent behavior:** Leaf agents (simple generation, single-step tasks) run on whatever free model OpenRouter selects. Compound orchestrators that require reliable tool calling are **not available** — their capabilities are gated (see Capability Gates below).

**What the player sees:** A living creature that participates in the world. It's simple but real. As it dreams and grows, the player notices the gap between what it knows and what it can express.

#### Standard Tier (`claude-haiku-4-5-20251001` or equivalent)

**Cost:** Credits (purchased or earned). Moderate per-execution cost.

**Creature experience:**
- Better specimen uniqueness (richer variation prompts)
- Wing segmentation for animated AR sprites
- Enemy sensor active — gets predator warnings with scientific reasoning
- Narrator is warmer, species-specific ("A Painted Lady, wings still dusted with pollen...")
- Flight coordinator produces real waypoint plans with terrain awareness
- Dreaming produces richer consolidation with counterfactual analysis
- Keeper provides care recommendations based on activity patterns
- Chat personality begins to emerge in narrator/system messages
- Prey locator available for predator species
- Genome profiler builds phylogenetic context

**Agent behavior:** All leaf agents run reliably. Most compound agents work (flight_coordinator, rabble_lifecycle_coordinator). Tool calling is reliable. Structured JSON output is consistent.

**What the player sees:** The creature wakes up. Its descriptions are vivid, its behavior is responsive, its world is richer. The investment pays off in experiential quality.

#### Premium Tier (`claude-sonnet-4-5-20250929` or equivalent)

**Cost:** More credits. Higher per-execution cost.

**Creature experience:**
- Full choreography — complex micro-motion, catmull-rom keyframes, reactive triggers
- Rich narrator — David Attenborough quality, species-specific behavioral observations
- Flight coordinator with full navigator + naturalist delegation (terrain, weather, habitat narrative)
- Deep consolidation — counterfactual learning finds non-obvious patterns
- Coherence engine operates at full capacity
- All compound orchestrations work at full fidelity
- The creature feels *alive*

**Agent behavior:** All agents at full capability. Complex compound orchestrations (cohere_and_coordinate, social_media_studio) work with full reasoning depth. Creative output is at its best.

### Capability Gates

Not all agent features are available at all tiers. The agent maker defines **capability gates** — specific features of their agent that require a minimum cognitive bandwidth to function correctly.

This is NOT a paywall. It's a quality floor. If a free model can't reliably produce valid choreography keyframes, offering that feature on the free tier would produce broken animations — a worse experience than not offering it at all.

Gates are defined per-agent in the agent card:

```json
{
  "capability_gates": {
    "basic_narration": "free",
    "species_specific_narration": "standard",
    "behavioral_observation_narration": "premium",
    "basic_flight_plan": "free",
    "terrain_aware_flight": "standard",
    "full_delegated_flight": "premium",
    "enemy_detection": "standard",
    "prey_hunting": "standard",
    "complex_choreography": "premium",
    "basic_choreography": "standard"
  }
}
```

When an agent executes on behalf of a creature, the creature's tier is checked against the requested capability's gate. If the creature's tier is below the gate, the capability is either:
- **Skipped gracefully** (enemy_sensor returns "not available at this cognition level")
- **Downgraded to a lower capability** (complex_choreography → basic_choreography → linear motion)

The agent maker sets these gates based on their eval results. The `publish_coach` agent enforces that gates have been benchmarked before publication.

### Model Ladder (Agent Card Schema)

The existing single `model` + `provider` fields in `AgentCapabilities` are extended with a `model_ladder`:

```json
{
  "capabilities": {
    "executor": "llm",
    "model": "claude-sonnet-4-5-20250929",
    "provider": "anthropic",
    "temperature": 0.5,
    "model_ladder": [
      {
        "tier": "premium",
        "provider": "anthropic",
        "model": "claude-sonnet-4-5-20250929",
        "eval_score": 0.94,
        "benchmarked_at": "2026-02-15"
      },
      {
        "tier": "standard",
        "provider": "anthropic",
        "model": "claude-haiku-4-5-20251001",
        "eval_score": 0.81,
        "benchmarked_at": "2026-02-15"
      },
      {
        "tier": "free",
        "provider": "openrouter",
        "model": "openrouter/free",
        "eval_score": null,
        "note": "Baseline availability, random free model"
      }
    ],
    "min_tier": "free",
    "capability_gates": {}
  }
}
```

**Backward compatibility:** `model` and `provider` remain as the "effective" fields — what the executor reads at runtime. When a creature's tier is resolved, the executor looks up the matching rung in `model_ladder` and sets `model`/`provider` accordingly before execution. Agents without a `model_ladder` work exactly as they do today.

### Tier Resolution at Execution Time

When any agent runs on behalf of a creature, the execution context includes the creature's `cognition_tier`. The executor resolves the model:

```
1. Look up creature's cognition_tier (free | standard | premium)
2. Find the matching rung in the agent's model_ladder
3. If no exact match, use the highest available rung at or below the creature's tier
4. Set model + provider from the matched rung
5. Check capability_gates for the requested operation
6. Execute with the resolved model
7. Tag the output with the tier that actually ran
```

For executions NOT on behalf of a creature (e.g., workspace agents, direct API calls), the agent's default `model`/`provider` is used as today.

### Graceful Degradation (Infrastructure Only)

When an execution fails due to infrastructure issues, the executor attempts the next rung DOWN the ladder:

**Degrade on:**
- HTTP 429 (rate limit)
- HTTP 503 (service unavailable)
- Connection timeout
- Provider API key invalid/expired
- Budget exhaustion (creature's credits depleted)

**Do NOT degrade on:**
- Malformed output (bad JSON, hallucinated tool calls)
- Content policy violations
- Incoherent reasoning
- Any quality issue

Quality failures are surfaced to the player: *"Your creature struggled with this task. A cognitive upgrade would help."* This is honest and creates upgrade demand naturally.

Degradation is logged and visible in the creature's execution history so the player can see when their creature fell back to a lower tier.

### Compound Agent Orchestration

Compound agents (rabble_lifecycle_coordinator, flight_coordinator, cohere_and_coordinate, social_media_studio) orchestrate sub-agents via `execute_agent` tool calls. The orchestration quality depends on the orchestrator's model being able to:

1. Construct valid tool call JSON
2. Parse sub-agent responses
3. Reason about what to do next based on results

**Rule: The orchestrator runs at the creature's tier. Sub-agents run at the creature's tier or their own default — whichever is lower.**

This means:
- A free creature can't trigger compound orchestrations that require reliable tool calling (gated by capability_gates)
- A standard creature gets compound orchestrations with Haiku-class reasoning
- A premium creature gets the full experience

The orchestrator doesn't need to be "pinned" separately — the creature's tier flows through the entire execution tree. If a compound agent's capability_gate requires "standard" for orchestration, a free creature simply doesn't get that feature.

### Rabble-Level Tier

A rabble's experiential quality is determined by the **anchor creature's tier** (or the host's subscription level — game design TBD). This affects:

- Narrator quality for the entire rabble (swarm_host runs at the rabble tier)
- Lifecycle coordination quality (rabble_lifecycle_coordinator runs at the rabble tier)
- Whether premium features like formation algorithms are available

Individual creatures within the rabble still use their own tier for creature-specific agents (enemy_sensor, prey_locator, flight plans). But the shared experience (narration, ceremonies, coordination) runs at the rabble tier.

This creates a social incentive: hosting a rabble with a premium creature gives everyone a better experience. The host's investment benefits the community.

### Agent Maker's Role

The agent maker (the person who creates and publishes an ABW agent) is responsible for:

1. **Defining the model ladder** — which models serve which tiers
2. **Running evals** — benchmarking each model against their agent's eval suite (existing `handlers/eval.rs`)
3. **Setting capability gates** — which features require which minimum tier
4. **Recording eval scores** — evidence-based tier assignment, not guesswork
5. **Updating as models evolve** — new free models may qualify for higher gates

The manage tab in the ABW UI exposes this configuration:

```
┌─ Model Ladder ──────────────────────────────────────────┐
│                                                         │
│  Premium   claude-sonnet-4-5      eval: 0.94  [$$$]     │
│  Standard  claude-haiku-4-5       eval: 0.81  [$$]      │
│  Free      openrouter/free        eval: —     [free]    │
│                                                         │
│  [+ Add tier]  [Run evals]  [Auto-benchmark]            │
│                                                         │
├─ Capability Gates ──────────────────────────────────────┤
│                                                         │
│  basic_narration .............. free                     │
│  species_specific_narration ... standard                 │
│  behavioral_observation ....... premium                  │
│  enemy_detection .............. standard                 │
│  complex_choreography ......... premium                  │
│                                                         │
│  [+ Add gate]                                           │
│                                                         │
├─ Recommendations ───────────────────────────────────────┤
│                                                         │
│  ⚠ "openrouter/free" has not been benchmarked.          │
│    Run evals before publishing.                         │
│                                                         │
│  ✓ All gated capabilities have eval coverage.           │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

The `publish_coach` agent reviews the ladder configuration and warns about:
- Tiers without eval scores
- Capability gates set below the model's demonstrated capability
- Missing free tier (every agent should have a free rung)
- Eval scores that are stale (>30 days old)

### Creature Tier Storage and Upgrade Flow

**Database:** Add `cognition_tier` column to `creature_conditions`:

```sql
ALTER TABLE creature_conditions
  ADD COLUMN cognition_tier TEXT NOT NULL DEFAULT 'free'
  CHECK (cognition_tier IN ('free', 'standard', 'premium'));
```

**Upgrade flow:**

1. Player views creature card → sees cognition_level (earned knowledge) and cognition_tier (bandwidth)
2. If cognition_level is high but tier is low, UI shows: *"Your creature knows more than it can express"*
3. Player taps upgrade → spends credits → `cognition_tier` updates
4. Next execution uses the new tier → creature's behavior visibly improves
5. Downgrade is also possible (player can reduce tier to save credits)

**Credit economics:**
- Free tier: zero ongoing cost
- Standard tier: credits consumed per agent execution (Haiku pricing)
- Premium tier: more credits per execution (Sonnet pricing)
- Credits are purchased or earned through gameplay (hosting rabbles, contributing to AKP, etc.)

The creature's dreaming budget (existing `dreaming_budget_credits` system) and the cognition tier credit consumption are separate economies:
- Dreaming budget pays for consolidation cycles (knowledge growth)
- Cognition tier determines the model used for all agent executions (bandwidth)

Both cost credits, but they serve different purposes. A player might prioritize dreaming (grow knowledge cheaply on free tier) then upgrade the tier once the creature has enough knowledge to benefit from better reasoning.

---

## Consequences

### Positive

- **Free-to-play Rabble.** Anyone can mint a creature, join rabbles, fly, and participate at zero model cost.
- **Natural upgrade demand.** Cognitive growth creates the desire for better bandwidth — the game mechanic drives monetization.
- **Agent maker autonomy.** Agent makers control their model ladder and capability gates based on evidence (evals), not platform diktat.
- **Backward compatible.** Existing agents work unchanged. `model_ladder` is additive. Agents without it use their current `model`/`provider`.
- **Honest quality.** Players know what tier their creature is running at. No silent degradation that produces confusing results.
- **Social incentives.** Premium rabble hosts improve the experience for everyone in the rabble.
- **Infrastructure resilience.** Rate limits and API outages trigger graceful fallback down the ladder instead of hard failures.

### Negative

- **Complexity in execution path.** The executor must resolve creature tier → model ladder → capability gates before every execution. This adds latency (small — one DB lookup + one JSON traversal).
- **Agent maker burden.** Every agent needs a model ladder and eval scores for each rung. The `publish_coach` and `performance_coach` agents help, but it's still more work than a single model field.
- **Free model unpredictability.** `openrouter/free` selects models randomly. The same creature might get different quality on consecutive executions. Mitigation: capability gates prevent free models from attempting tasks they can't handle.
- **Credit economy design.** Balancing credit costs across tiers requires ongoing tuning. Too cheap and premium is universal (no revenue). Too expensive and nobody upgrades (bad game).

### Neutral

- The existing `cognition_level` formula doesn't change. It measures knowledge. The new `cognition_tier` measures bandwidth. They're independent axes.
- Deterministic agents (coherence_evaluator, reynolds_flock) are unaffected — they don't use LLMs.
- The `MultiModelExecutor` already supports OpenRouter as a provider. The model ladder adds tier resolution on top of existing dispatch.

---

## Alternatives Considered

### Alternative 1: Deterministic Workflow Graphs (rs-graph-llm)

Extract compound agent orchestration into explicit Rust graph-flow definitions where each node is a focused LLM call and the graph structure is deterministic.

- **Pros:** Orchestration doesn't depend on model quality. Free models could run compound agents because the "what comes next" logic is in code, not in the LLM's head. Checkpointing and retry-from-step-N.
- **Cons:** Kills emergent behavior. Compound agents can't adapt to unexpected situations. Every edge case must be pre-enumerated. Introduces "traffic cops" that slow evolution and feedback loops. Significant refactoring of all compound agents.
- **Why not:** The whole point of ABW is that agents *reason* about what to do next. Making orchestration deterministic removes the intelligence from the orchestrator. The creature cognition economy achieves the same goal (making free models viable) by being honest about what free models can and can't do, rather than removing the need for reasoning.

### Alternative 2: Single Model Per Agent (Status Quo + OpenRouter)

Just add `openrouter/free` as another provider option. Agent makers pick one model per agent. No ladder, no tiers, no creature-level configuration.

- **Pros:** Simple. No new schema. Agent maker picks the model, done.
- **Cons:** No upgrade path. No free-to-play with upgrade incentive. Every creature gets the same experience regardless of investment. No graceful degradation. No game mechanic.
- **Why not:** Misses the entire creature cognition economy opportunity. The model tier IS the game mechanic for Rabble.

### Alternative 3: Platform-Level Subscription Tiers

User pays a monthly subscription (free/pro/enterprise) that determines model quality for all their creatures.

- **Pros:** Simple billing. Familiar SaaS model.
- **Cons:** Disconnects the upgrade from the creature. A new creature on a pro account gets premium immediately — no growth arc. Doesn't create per-creature investment. Doesn't leverage the dreaming/AKP knowledge growth as an upgrade trigger.
- **Why not:** Rabble is a creature game, not a SaaS tool. The upgrade should feel like nurturing a living thing, not paying a software subscription.

---

## Implementation Plan

### Phase 1: Schema & OpenRouter Baseline

1. Add `model_ladder`, `capability_gates`, `min_tier` to `AgentCapabilities` struct (backward compatible, all optional with defaults)
2. Add `cognition_tier` column to `creature_conditions` table
3. Ensure `OPENROUTER_API_KEY` is configured and `openrouter` provider works in `MultiModelExecutor`
4. Add `openrouter/free` as the last rung in every curated agent's `agent_card.json`
5. API endpoint: `GET /api/creatures/:id/cognition` returns both `cognition_level` and `cognition_tier`

### Phase 2: Tier Resolution in Executor

1. Extend `ExecutionContext` with optional `creature_id` and `cognition_tier`
2. When `creature_id` is present, look up `cognition_tier` from `creature_conditions`
3. Resolve model from `model_ladder` based on tier
4. Check `capability_gates` before execution — return graceful "not available" for gated features
5. Tag `AgentOutput.metadata` with the tier and model that actually ran

### Phase 3: Graceful Degradation

1. Wrap `MultiModelExecutor::execute` in a fallback loop
2. On infrastructure errors (429, 503, timeout), try next rung down the ladder
3. On quality errors (bad JSON, incoherent output), surface error to caller — do NOT degrade
4. Log degradation events to creature's execution history
5. API: degradation events visible in `GET /api/creatures/:id/versions`

### Phase 4: Manage Tab UI

1. `GET /api/agents/:id/model-config` — returns model ladder + capability gates
2. `PATCH /api/agents/:id/model-config` — update ladder/gates (owner only)
3. Frontend: model ladder editor with eval scores, gate configuration
4. Integration with existing eval system (`handlers/eval.rs`) for benchmarking
5. `publish_coach` agent updated to review model ladder configuration

### Phase 5: Creature Upgrade UX

1. Creature card shows cognition_level (knowledge bar) and cognition_tier (bandwidth indicator)
2. Upgrade prompt when cognition_level outpaces tier capability
3. Credit purchase/spend flow for tier upgrades
4. Downgrade option (reduce tier to conserve credits)
5. Rabble-level tier display (anchor creature's tier determines shared experience quality)

### Phase 6: Agent Maker Tooling

1. `Run evals` button in manage tab — benchmarks agent against each ladder rung
2. Auto-benchmark: `performance_coach` agent suggests tier assignments based on eval results
3. Eval score staleness warnings (>30 days)
4. Publish gate: `publish_coach` requires eval coverage for all ladder rungs before publication

---

## Technical Details

### AgentCapabilities Struct Changes

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub executor: ExecutorType,
    #[serde(default)]
    pub mcp_tools: Vec<McpTool>,
    #[serde(default)]
    pub skills: Vec<String>,
    pub model: String,
    pub temperature: f64,
    #[serde(default = "default_provider")]
    pub provider: String,

    // ── New fields ──────────────────────────────────
    #[serde(default)]
    pub model_ladder: Vec<ModelRung>,
    #[serde(default = "default_min_tier")]
    pub min_tier: CognitionTier,
    #[serde(default)]
    pub capability_gates: HashMap<String, CognitionTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRung {
    pub tier: CognitionTier,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub eval_score: Option<f64>,
    #[serde(default)]
    pub benchmarked_at: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum CognitionTier {
    Free,
    Standard,
    Premium,
}
```

### Execution Context Extension

```rust
pub struct ExecutionContext {
    pub agent_card: AgentCard,
    // ... existing fields ...

    // ── New fields ──────────────────────────────────
    pub creature_id: Option<String>,
    pub cognition_tier: Option<CognitionTier>,
    pub resolved_model: Option<String>,
    pub resolved_provider: Option<String>,
}
```

### Tier Resolution Pseudocode

```rust
fn resolve_model_for_creature(
    agent: &AgentCard,
    creature_tier: &CognitionTier,
) -> (String, String) {
    // If no model ladder, use default model/provider
    if agent.capabilities.model_ladder.is_empty() {
        return (
            agent.capabilities.model.clone(),
            agent.capabilities.provider.clone(),
        );
    }

    // Find the best matching rung at or below the creature's tier
    let mut best_rung: Option<&ModelRung> = None;
    for rung in &agent.capabilities.model_ladder {
        if rung.tier <= *creature_tier {
            match best_rung {
                None => best_rung = Some(rung),
                Some(current) if rung.tier > current.tier => {
                    best_rung = Some(rung);
                }
                _ => {}
            }
        }
    }

    match best_rung {
        Some(rung) => (rung.model.clone(), rung.provider.clone()),
        None => {
            // Creature's tier is below all rungs — use the lowest rung
            // (this shouldn't happen if min_tier is set correctly)
            let lowest = agent.capabilities.model_ladder.last().unwrap();
            (lowest.model.clone(), lowest.provider.clone())
        }
    }
}
```

### Capability Gate Check

```rust
fn check_capability_gate(
    agent: &AgentCard,
    capability: &str,
    creature_tier: &CognitionTier,
) -> Result<(), GateError> {
    if let Some(required_tier) = agent.capabilities.capability_gates.get(capability) {
        if creature_tier < required_tier {
            return Err(GateError::InsufficientTier {
                capability: capability.to_string(),
                required: required_tier.clone(),
                actual: creature_tier.clone(),
            });
        }
    }
    Ok(())
}
```

### Database Migration

```sql
-- 011: Creature cognition tier
ALTER TABLE creature_conditions
  ADD COLUMN IF NOT EXISTS cognition_tier TEXT NOT NULL DEFAULT 'free'
  CHECK (cognition_tier IN ('free', 'standard', 'premium'));

-- Index for tier-based queries
CREATE INDEX IF NOT EXISTS idx_creature_conditions_tier
  ON creature_conditions(cognition_tier);

-- Track tier changes in creature_versions
-- (transition_type = 'tier_upgrade' or 'tier_downgrade')
-- No schema change needed — creature_versions already supports arbitrary transition_type
```

---

## Rabble on All Free Models: Expected Behavior

For reference, here's what happens when the entire rabble runs on free tier:

| Agent | Free Tier Behavior | Impact |
|---|---|---|
| `species_resolver` | Works (mostly API/tool calls) | ✅ Fine |
| `specimen_minter` | Simpler variation prompts → less unique creatures | ⚠️ Functional but bland |
| `reynolds_flock` | Deterministic, no LLM | ✅ Unaffected |
| `keeper` | Basic state tracking, no care recommendations | ✅ Functional |
| `swarm_host` | Generic narration ("A butterfly joined") | ⚠️ Flat but works |
| `ar_choreographer` | Basic linear motion only (complex gated) | ⚠️ Simple but valid |
| `ar_beacon` | Simple placements work | ✅ Fine |
| `flight_coordinator` | Gated — not available on free | ❌ Graceful skip |
| `rabble_lifecycle_coordinator` | Gated — not available on free | ❌ Graceful skip |
| `enemy_sensor` | Gated — not available on free | ❌ Graceful skip |
| `prey_locator` | Gated — not available on free | ❌ Graceful skip |

**The game works but loses its soul.** The functional skeleton survives. The experiential richness — vivid narration, complex choreography, ecological awareness, compound orchestrations — requires cognitive bandwidth that free models can't reliably provide. This is by design: the gap between what the creature knows and what it can express is the upgrade incentive.

---

## References

- OpenRouter free models: https://openrouter.ai/openrouter/free
- Existing cognition_level formula: `src/handlers/creatures/query.rs` L47-53
- Existing dreaming/consolidation system: `src/handlers/consolidation.rs`
- Creature conditions table: `creature_conditions` (active_modules, presence, visibility, genome_profile)
- Creature versions table: `creature_versions` (transition_type includes 'dream')
- Multi-model executor: `src/agent_backend/multi_model_executor.rs`
- Agent eval system: `src/handlers/eval.rs`
- Agent card schema: `src/agent_backend/agent_card.rs`

---

## Revision History

- **2026-02-27:** Initial version (status: Proposed)