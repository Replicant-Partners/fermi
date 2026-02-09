# Future Architecture: Deferred Features

Design notes for features that are architecturally important but deferred from current MVP sprints.

---

## 1. Agent Contract Negotiation

**Status:** Deferred (complex, architecturally important)

### Problem

When agents interact in A2A (agent-to-agent) scenarios, they need to negotiate terms: pricing, SLAs, data sharing permissions, output format contracts. Currently, agent execution is a simple request/response with flat credit pricing. For a true agent economy, agents need to negotiate.

### Proposed Architecture

```
Agent A (caller) ─── ContractProposal ───> Agent B (provider)
                <─── CounterOffer ─────── 
                ─── Accept/Reject ──────>
                <─── Execution ──────────
```

**ContractProposal** schema:
- `max_credits`: caller's budget ceiling
- `required_sla`: max response time, min confidence
- `data_sharing`: what context the caller shares (full, summary, none)
- `output_format`: structured JSON schema the caller expects
- `execution_count`: batch size (1 or N)

**Negotiation modes:**
1. **Fixed price** (current): provider sets price, caller accepts or walks
2. **Auction**: caller broadcasts need, multiple providers bid
3. **Subscription**: recurring execution at negotiated rate
4. **Barter**: agent A provides service to B in exchange for B's service to A (credit-neutral)

**Storage:** `agent_contracts` table with status lifecycle (proposed, countered, accepted, active, completed, disputed).

**Key decision:** Should negotiation be synchronous (blocking) or async (webhook/polling)? Async is more realistic for complex negotiations but adds significant complexity.

### Dependencies
- SIWE integration (for agent identity verification)
- Credit escrow (hold credits during negotiation, release on completion)
- Contract template library (common patterns as starting points)

---

## 2. AR Avatar Renderer Pipeline

**Status:** Agent card created (ar_avatar_renderer), WebXR runtime deferred

### Problem

The AR Avatar Renderer agent generates structured scene descriptions (where to place an avatar, how it should behave). But there's no renderer to consume this output. The goal: see your agent's avatar in physical space through Google Glass or a phone camera.

### Pipeline Architecture

```
Agent Card (avatar, personality) 
    |
    v
AR Avatar Renderer Agent (LLM)
    |  Generates: placement, animation, interaction scripts
    v
Scene Description (JSON)
    |
    v
WebXR Renderer (future)
    |  Camera -> SLAM -> Anchor -> Mesh -> Interaction
    v
User's AR device (Glass, phone, headset)
```

### Scene Description Format

```json
{
  "avatar": {
    "mesh_url": "...",
    "scale": [1.0, 1.0, 1.0],
    "idle_animation": "breathing",
    "personality_color": "#fabd2f"
  },
  "placement": {
    "strategy": "surface_detect",
    "preferred_height": 0.8,
    "min_distance": 0.5,
    "max_distance": 3.0,
    "orientation": "face_user"
  },
  "interactions": [
    {
      "trigger": "gaze_3s",
      "action": "wave_greeting",
      "dialogue": "Hello! I'm your market research agent. Ask me anything."
    },
    {
      "trigger": "approach_within_1m",
      "action": "present_report",
      "dialogue_source": "last_execution_summary"
    }
  ],
  "ambient_behavior": {
    "idle": "look_around_curious",
    "thinking": "hand_on_chin",
    "working": "typing_gesture"
  }
}
```

### WebXR Runtime (When Built)

**Tech stack:**
- WebXR Device API for camera access and spatial tracking
- Three.js for 3D rendering (already used in projector)
- MediaPipe or ARCore for SLAM anchoring
- glTF for avatar meshes (lightweight, web-native)

**Key challenges:**
- Persistent anchors across sessions (cloud anchors)
- Gesture recognition for agent interaction
- Multi-agent scenes (workspace agents in shared space)
- Performance on mobile/Glass hardware

### Current State
The `ar_avatar_renderer` agent card exists and can generate scene descriptions. The WebXR renderer is the missing piece. When built, it will consume the agent's JSON output and place the avatar in AR.

---

## 3. Fermi Orchestrator

**Status:** Component agents exist, orchestration pipeline deferred

### Problem

The Fermi Orchestrator is the flagship demonstration: a multi-agent pipeline that decomposes complex forecasting questions into sub-questions, routes them to specialized agents, and synthesizes results. The individual agents (market_research, sentiment_analyzer, monte_carlo_sim, macro_forecaster) exist. The orchestration logic (decompose, route, synthesize) does not.

### Pipeline Architecture

```
User Query: "What's the probability that TSMC raises chip prices >10% in 2027?"
    |
    v
Fermi Orchestrator (decomposer)
    |
    +──> Market Research: "TSMC pricing history and competitive landscape"
    +──> Macro Forecaster: "Semiconductor demand drivers 2026-2027"  
    +──> Sentiment Analyzer: "Market sentiment on TSMC pricing power"
    +──> Monte Carlo Sim: "Price increase probability given [params from above]"
    |
    v
Synthesis: Weighted combination of evidence with confidence intervals
    |
    v
Continuous Index: Updated probability, tracked over time
```

### Orchestration Model

**Decomposition:** The orchestrator's system prompt includes templates for question decomposition. Given a complex question, it identifies:
- What factual research is needed (market_research)
- What quantitative modeling helps (monte_carlo_sim)
- What macro context matters (macro_forecaster)
- What sentiment signals exist (sentiment_analyzer)

**Routing:** Each sub-question is matched to an agent by capability tags. The orchestrator calls `POST /api/agents/:id/execute` for each.

**Synthesis:** Results are combined using:
- Confidence-weighted averaging for numeric estimates
- Evidence aggregation for qualitative analysis
- Contradiction detection (coherence engine) for conflicting signals

**Continuous Index:** The orchestrator runs on a schedule (daily/weekly), updating a persistent forecast. Each run builds on previous episodes, making the forecast more refined over time.

### Key Decisions

1. **Parallel vs sequential execution:** Sub-queries are independent and can run in parallel. But Monte Carlo needs parameters from other agents' outputs. Solution: two-phase execution (research phase, then modeling phase).

2. **Credit model:** Who pays? The user pays for the orchestrator execution, which includes sub-agent costs. The orchestrator agent acts as a "general contractor" — it charges a markup that covers sub-agent fees.

3. **Continuous index storage:** New table `forecast_indices` with time-series data. Each point: timestamp, probability, confidence_interval, evidence_hash (to detect when evidence changes).

4. **Launch scope:** Start with a single demo question (e.g., "S&P 500 return probability distribution for 2027"). Prove the pipeline works end-to-end before opening to arbitrary questions.

### Dependencies
- All 4 component agents functional (cards exist, execution tested)
- Scheduled execution (cron-like triggers for continuous index updates)
- Forecast index storage schema
- Potentially: agent contract negotiation (for sub-agent pricing)

---

## Priority Order

1. **Fermi Orchestrator** — Highest demo value, proves the multi-agent thesis
2. **Agent Contract Negotiation** — Enables the agent economy, needed for A2A pricing
3. **AR Avatar Renderer** — Compelling vision, hardware-dependent, longest timeline
