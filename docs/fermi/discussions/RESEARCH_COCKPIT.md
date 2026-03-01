# Research Cockpit — OODA Loop UX for Forecasting

**Date:** 2026-02-28
**Status:** Design exploration
**Supersedes:** Linear form-based composer (Sprint 2 composer.rs)
**Context:** The forecast composer should not be a boring form. It should be a living research workspace where the user is at the center of a cyclical intelligence process, orchestrating agents and curating a dynamic evidence landscape.

---

## The Problem with Forms

The Sprint 2 composer is a linear form: question → drivers → model → simulate → publish. This is wrong for several reasons:

1. **Forecasting is not linear.** You don't define drivers then never touch them again. You discover them, revise them, discover more, throw some away, split others. The process is cyclical.
2. **The agents are invisible.** The form treats agents as a button you press after filling in fields. But agents should be *actively working* from the moment you type a question — researching, suggesting, challenging.
3. **Evidence is flat.** A list of evidence items doesn't show you what you know, what you don't know, where the contradictions are, or where the gaps are. Evidence has structure — it clusters, it conflicts, it has varying relevance and freshness.
4. **The user is a typist, not a commander.** A form makes you feel like you're filling in a tax return. The UX should make you feel like you're running an intelligence operation.

## The OODA Loop

The forecast lifecycle is an OODA loop (Boyd's Observe-Orient-Decide-Act cycle):

```
        ┌──────────────────────────────────────────┐
        │                                          │
        ▼                                          │
   ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐
   │ OBSERVE │───▶│ ORIENT  │───▶│ DECIDE  │───▶│  ACT    │
   │         │    │         │    │         │    │         │
   │ Agents  │    │ Evidence│    │ Set     │    │ Publish │
   │ gather  │    │ map,    │    │ proba-  │    │ Update  │
   │ evidence│    │ gaps,   │    │ bility, │    │ Assign  │
   │         │    │ contra- │    │ adjust  │    │ more    │
   │         │    │ dictions│    │ drivers │    │ agents  │
   └─────────┘    └─────────┘    └─────────┘    └─────────┘
        ▲                                          │
        │                                          │
        └──────────────────────────────────────────┘
```

Each cycle refines the forecast. The UX should make each phase visible and fluid.

---

## The Research Cockpit Layout

The cockpit is **not** a scrolling form. It's a **spatial workspace** with the question at the center and information radiating outward.

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │              "Will AMD reach $200 by 2026-12-31?"        │    │
│  │                                                          │    │
│  │                    ┌──────────┐                          │    │
│  │                    │   65%    │  ← live probability      │    │
│  │                    │ ▲ +3%   │     (shifts as evidence   │    │
│  │                    └──────────┘      arrives)             │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌─────────────┐  ┌──────────────────────┐  ┌───────────────┐   │
│  │             │  │                      │  │               │   │
│  │  EVIDENCE   │  │    DRIVER MAP        │  │  AGENT FLEET  │   │
│  │  LANDSCAPE  │  │                      │  │               │   │
│  │             │  │  ┌───┐ ┌───┐ ┌───┐   │  │  ● macro_fc   │   │
│  │  ○ Gartner  │  │  │TAM│─│SHR│─│GRW│   │  │    running…   │   │
│  │  ○ TipRanks │  │  └───┘ └───┘ └───┘   │  │  ● sentiment  │   │
│  │  ● NVIDIA   │  │    │     │     │      │  │    3 findings │   │
│  │  ◌ [gap]    │  │    ▼     ▼     ▼      │  │  ○ monte_c    │   │
│  │  ◌ [gap]    │  │  ┌─────────────────┐  │  │    idle       │   │
│  │             │  │  │ MODEL: TAM×SHR× │  │  │               │   │
│  │  clusters:  │  │  │ GRW×(if CTR…)   │  │  │  [+ assign]   │   │
│  │  ■ bullish  │  │  └─────────────────┘  │  │               │   │
│  │  ■ bearish  │  │                      │  │  cost: 3cr     │   │
│  │  ■ neutral  │  │  sim: 10k iter      │  │  this session   │   │
│  │             │  │  mean: $187 p95:$240 │  │               │   │
│  └─────────────┘  └──────────────────────┘  └───────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐    │
│  │  TIMELINE  ──●────●──────●───────●────────●──── now      │    │
│  │              │    │      │       │        │              │    │
│  │           created  ev1   ev2   prob↑    agent3           │    │
│  │           65%     67%    63%    68%      65%             │    │
│  └──────────────────────────────────────────────────────────┘    │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### The Five Zones

#### 1. Question Hub (top center)

The question is always visible. Below it, the **live probability** — a large, prominent number that shifts as evidence arrives. The probability has a delta indicator showing recent movement and direction. This is the heartbeat of the forecast.

The question field is editable. When you change it (or first enter it), it triggers the **question orchestration** — agents fan out to research.

#### 2. Evidence Landscape (left panel)

Not a flat list. A **clustered, visual map** of evidence:

- **Filled circles** (●) = evidence you have, with source and relevance
- **Empty circles** (◌) = evidence gaps the agents identified ("No data on China market share")
- **Clusters** = evidence grouped by sentiment/direction (bullish/bearish/neutral) or by topic
- **Contradictions** highlighted — two pieces of evidence that point in opposite directions get a visual link with a warning
- **Freshness** — older evidence fades, recent evidence is bright
- **Relevance** — more relevant evidence is larger/brighter

Clicking an evidence item shows its full text, source, date, and which agent found it. You can dismiss evidence (mark as not relevant), or promote it (increase weight).

The evidence landscape is the **Orient** phase — it shows you the shape of what you know.

#### 3. Driver Map (center panel)

Drivers are not a list — they're a **dependency graph**. Each driver is a node. Connections show how they combine in the model expression. The model expression is the graph's structure, not a text field.

- **Continuous drivers** show their distribution as a mini sparkline inside the node
- **Binary drivers** show their probability as a filled arc
- **The model node** at the bottom shows how everything combines
- **Simulation results** appear below the model node — mean, percentiles, histogram

You can:
- Drag to rearrange the graph
- Click a driver to edit its distribution parameters (inline, not a modal)
- Click the model node to edit the expression
- ⌘R runs simulation and the results animate in

Drivers can be **suggested by agents** — they appear as ghost nodes with a "+" button to accept them. The agent explains why it thinks this driver matters.

#### 4. Agent Fleet (right panel)

Your active research agents and their status:

- **Running** (●) — currently executing, with a progress indicator
- **Completed** — shows finding count, click to see results
- **Idle** (○) — available to assign
- **[+ assign]** — button to assign a new agent to this forecast

Each agent shows:
- What it found (evidence count, key finding summary)
- Cost (credits consumed)
- Model used (tier indicator)

You can assign agents from here: "Run macro_forecaster on this question" or "Run sentiment_analyzer on the evidence gaps." The agent results flow back into the Evidence Landscape and may suggest new Drivers.

#### 5. Timeline (bottom strip)

A horizontal timeline showing the forecast's history:

- When it was created
- Each evidence addition (with the probability at that point)
- Each probability update (with reason)
- Each agent execution
- The current state

This is the **audit trail** — it shows intellectual honesty. You can see how your thinking evolved. Clicking any point on the timeline shows the forecast state at that moment.

---

## Interaction Flows

### Flow 1: New Forecast (Cold Start)

```
1. User types question: "Will AMD reach $200 by 2026-12-31?"
2. Question Hub shows the question, probability starts at 50% (uninformed prior)
3. ORCHESTRATION FIRES automatically:
   a. macro_forecaster → researches AMD market dynamics
   b. market_research → finds analyst consensus, competitor data
   c. monte_carlo_sim → suggests drivers and distributions
4. As agents complete (seconds to minutes):
   - Evidence Landscape populates with findings (animated, items fade in)
   - Driver Map populates with suggested drivers (ghost nodes appear)
   - Agent Fleet shows progress and results
5. User reviews:
   - Accepts/rejects suggested drivers (click ghost nodes)
   - Adjusts distributions (click driver nodes, drag sliders)
   - Dismisses irrelevant evidence
   - Notes evidence gaps
6. User adjusts probability based on the evidence landscape
7. ⌘R runs simulation → results appear in Driver Map
8. User iterates: assigns more agents to fill gaps, adjusts, re-simulates
9. ⌘Enter publishes → forecast enters Brier tracking
```

### Flow 2: Probability Update (Warm Cycle)

```
1. User opens existing forecast from Portfolio
2. Cockpit loads with current state: evidence, drivers, probability, timeline
3. User assigns agent: "Run sentiment_analyzer on recent AMD news"
4. Agent returns → new evidence appears in landscape
5. Evidence shifts the cluster balance (more bullish evidence)
6. User adjusts probability: 65% → 68%
7. Timeline records the update with reason: "New bullish sentiment from Q4 earnings"
8. Probability delta shows ▲ +3%
```

### Flow 3: Evidence Gap Discovery

```
1. Cockpit shows evidence landscape with a gap: "No data on China server market"
2. User clicks the gap → option to assign an agent
3. "Run market_research with query: AMD China server market share"
4. Agent returns → gap fills with new evidence
5. Evidence may suggest a new driver (e.g., "china_exposure")
6. Driver appears as ghost node → user accepts → model updates
7. Re-simulate → probability shifts
```

---

## Agent Orchestration on Question Entry

When a question is entered or changed, the system fires a **question analysis orchestration**:

```
Question: "Will AMD reach $200 by 2026-12-31?"
                    │
                    ▼
    ┌───────────────────────────────┐
    │  Question Analyzer (local)    │
    │  - Extract entity: AMD        │
    │  - Extract metric: stock $200 │
    │  - Extract timeframe: 2026    │
    │  - Classify domain: tech/fin  │
    │  - Suggest reference class    │
    └───────────┬───────────────────┘
                │
        ┌───────┼───────────┐
        ▼       ▼           ▼
   ┌─────────┐ ┌──────────┐ ┌──────────────┐
   │ macro_  │ │ market_  │ │ sentiment_   │
   │ fore-   │ │ research │ │ analyzer     │
   │ caster  │ │          │ │              │
   │         │ │ "AMD     │ │ "AMD stock   │
   │ "AMD    │ │ market   │ │ sentiment    │
   │ revenue │ │ share,   │ │ from news    │
   │ and     │ │ TAM,     │ │ and social"  │
   │ growth" │ │ competi- │ │              │
   │         │ │ tors"    │ │              │
   └────┬────┘ └────┬─────┘ └──────┬───────┘
        │           │              │
        ▼           ▼              ▼
   ┌─────────────────────────────────────┐
   │  Results Aggregator                  │
   │                                      │
   │  → 3-5 suggested drivers            │
   │  → 5-10 evidence items              │
   │  → 1-2 evidence gaps identified     │
   │  → base rate from reference class    │
   │  → initial probability estimate      │
   │  → suggested model expression        │
   └──────────────────────────────────────┘
```

The orchestration is **non-blocking** — the cockpit is usable immediately. Results stream in as agents complete. The user sees the workspace come alive as intelligence arrives.

---

## Evidence Landscape Visualization

The evidence landscape is the most novel UI element. It's not a list — it's a **force-directed graph** where:

- **Nodes** = evidence items
- **Size** = relevance score (0-1)
- **Color** = sentiment direction
  - Green = supports the forecast (bullish)
  - Red = contradicts the forecast (bearish)  
  - Grey = neutral/contextual
- **Brightness** = freshness (recent = bright, old = dim)
- **Edges** = evidence items that reference each other or share sources
- **Clusters** = evidence that naturally groups by topic or direction
- **Gap nodes** = dashed circles representing identified evidence gaps

The landscape answers the question: **"What does the evidence look like?"** at a glance. A forecast with mostly green, large, bright nodes is well-supported. One with a mix of red and green has tension. One with many gap nodes needs more research.

### Evidence Gap Detection

Gaps are identified by agents or by pattern analysis:

- **Topic gaps**: "You have evidence on AMD's data center business but nothing on gaming"
- **Temporal gaps**: "Your most recent evidence is 3 months old"
- **Perspective gaps**: "All evidence is from sell-side analysts — no independent research"
- **Contradiction gaps**: "Two sources disagree on market share — need a tiebreaker"

Gaps are actionable — clicking a gap suggests an agent to assign.

---

## Driver Map as Dependency Graph

The driver map replaces the flat driver list with a visual graph:

```
    ┌─────────────┐     ┌─────────────┐
    │  market_size │     │ growth_rate │
    │  ▁▂▃▅▇▅▃▂▁  │     │  ▁▂▄▇▄▂▁   │
    │  tri(500,    │     │  N(0.25,    │
    │   1200,2500) │     │    0.05)    │
    └──────┬──────┘     └──────┬──────┘
           │                   │
           │    ┌──────────┐   │
           │    │ mkt_share│   │
           │    │  ▁▃▇▃▁   │   │
           │    │ tri(0.15, │   │
           │    │  0.22,    │   │
           │    │  0.35)    │   │
           │    └────┬─────┘   │
           │         │         │
           ▼         ▼         ▼
    ┌──────────────────────────────┐
    │         × multiply          │
    │                              │
    │  ┌────────────────────┐      │
    │  │  major_contract    │      │
    │  │  binary: 65%       │      │
    │  │  if true: ×1.3     │      │
    │  └────────────────────┘      │
    │                              │
    │  model: mkt_size × mkt_share │
    │  × (1+growth) × (if ctr…)   │
    └──────────────┬───────────────┘
                   │
                   ▼
    ┌──────────────────────────────┐
    │  SIMULATION RESULTS          │
    │  mean: $187M  median: $165M  │
    │  p5: $82M     p95: $340M     │
    │  ▁▂▃▅▇▇▅▃▂▁▁                │
    └──────────────────────────────┘
```

Each driver node shows:
- Name
- Mini distribution sparkline
- Key parameters
- Click to edit inline (sliders for distribution params)

Ghost nodes (agent-suggested drivers) appear with dashed borders and a "+" to accept.

---

## Agent Fleet as Living Dashboard

The agent fleet panel shows agents as living entities, not a static list:

```
┌─ Agent Fleet ──────────────────────────┐
│                                        │
│  ● macro_forecaster          running   │
│    ▓▓▓▓▓▓▓▓░░░░ 65%                   │
│    "Analyzing AMD revenue trends…"     │
│    Sonnet · est. 8s · 0.3cr            │
│                                        │
│  ✓ sentiment_analyzer        done      │
│    3 findings · 2 bullish · 1 neutral  │
│    Haiku · 1.2s · 0.1cr               │
│    [view findings] [run again]         │
│                                        │
│  ✓ market_research           done      │
│    5 findings · TAM data + competitors │
│    Sonnet · 4.3s · 0.3cr              │
│    [view findings] [run again]         │
│                                        │
│  ○ monte_carlo_sim           idle      │
│    [assign to this forecast]           │
│                                        │
│  ○ entity_investigator       idle      │
│    [assign to this forecast]           │
│                                        │
│  ─────────────────────────────         │
│  Session cost: 0.7cr                   │
│  Agents used: 2/14 available           │
│                                        │
│  [+ Assign agent…]                     │
│  [Auto-research (3cr)]                 │
└────────────────────────────────────────┘
```

The "Auto-research" button fires the full question orchestration — all relevant agents fan out simultaneously. The user can also assign individual agents for targeted research.

---

## Probability as Living Indicator

The probability is not a text field you type into. It's a **living indicator** that:

1. **Starts at 50%** (uninformed prior) or at the base rate if one is found
2. **Shifts as evidence arrives** — each piece of evidence nudges it (the system suggests, user confirms)
3. **Can be manually adjusted** — user drags a slider or types a number
4. **Shows its history** — sparkline of probability over time
5. **Shows confidence** — wider confidence interval = more uncertainty
6. **Shows the delta** — "▲ +3% since last session"

Every probability change is recorded with a reason (automatic or user-provided), creating the revision history that feeds calibration analysis.

---

## Implementation Architecture

### GPUI Entity Model

```
FermiConsole (root)
  └── ResearchCockpit (entity, one per active forecast)
        ├── QuestionHub (entity)
        │     ├── question text (editable)
        │     ├── probability indicator (interactive)
        │     └── base rate display
        │
        ├── EvidenceLandscape (entity)
        │     ├── evidence nodes (force-directed layout)
        │     ├── gap nodes
        │     ├── cluster computation
        │     └── contradiction detection
        │
        ├── DriverMap (entity)
        │     ├── driver nodes (editable inline)
        │     ├── model expression (graph structure)
        │     ├── ghost nodes (agent suggestions)
        │     └── simulation results display
        │
        ├── AgentFleet (entity)
        │     ├── active agents (with progress)
        │     ├── completed agents (with results)
        │     ├── available agents
        │     └── orchestration trigger
        │
        └── Timeline (entity)
              ├── event markers
              ├── probability trace
              └── click-to-restore
```

### Data Flow

```
Question entered
    │
    ├──▶ Local: extract entities, classify domain
    │
    ├──▶ API: fire agent orchestration
    │         POST /api/agents/macro_forecaster/execute
    │         POST /api/agents/market_research/execute
    │         POST /api/agents/sentiment_analyzer/execute
    │
    │    (results stream back via SSE or polling)
    │
    ├──▶ Evidence Landscape: new nodes appear
    │
    ├──▶ Driver Map: ghost nodes suggested
    │
    ├──▶ Probability: initial estimate from agents
    │
    └──▶ Timeline: "created" event recorded

User adjusts driver
    │
    ├──▶ Driver Map: node updates
    ├──▶ FPL regenerated
    └──▶ Simulation auto-runs (debounced)
              │
              └──▶ Results update in Driver Map

User assigns agent
    │
    ├──▶ Agent Fleet: agent starts running
    ├──▶ API: POST /api/agents/:id/execute
    │
    │    (agent completes)
    │
    ├──▶ Evidence Landscape: new evidence nodes
    ├──▶ Driver Map: possible new ghost nodes
    ├──▶ Probability: suggested adjustment
    └──▶ Timeline: agent execution event

User publishes (⌘Enter)
    │
    ├──▶ API: POST /api/forecasts (with all state)
    ├──▶ Forecast enters Brier tracking
    ├──▶ Portfolio updates
    └──▶ Timeline: "published" event
```

### FPL as Internal Representation

FPL remains the underlying format. The cockpit generates FPL from its state, and the fermi executor runs simulations on it. But the user never needs to see FPL unless they toggle "source view" (⌘E).

The cockpit state serializes to/from FPL:
- Question → `question "..."`
- Drivers → `driver name type { ... }`
- Evidence → `evidence id { ... }`
- Agents → `agent name { ... }`
- Model → `model: expression`
- Simulate → `simulate N iterations`

This means:
- Power users can edit FPL directly in Zed and the cockpit reflects changes
- The cockpit can load any existing FPL file
- FPL files are the portable, version-controllable format
- The cockpit is a visual editor for FPL, not a replacement

---

## What Makes This Different

### vs. Metaculus / Manifold / Polymarket

Those platforms are **prediction markets** — you bet on outcomes. The research process is invisible. You see a question and a number. There's no evidence landscape, no driver model, no agent support, no OODA loop.

Fermi Console makes the **research process** the product. The prediction is the output, but the journey — the evidence gathering, the driver modeling, the agent orchestration, the probability revision — that's what you're paying for and what makes you a better forecaster.

### vs. Jupyter / Observable / Hex

Those are **notebooks** — linear, code-centric, data-science-oriented. They're great for analysis but terrible for the cyclical, evidence-driven, agent-assisted forecasting workflow. You don't write code to forecast — you orchestrate intelligence.

### vs. Bloomberg Terminal

Bloomberg is **information display** — it shows you data but doesn't help you reason about it. Fermi Console is **intelligence synthesis** — it helps you combine evidence, model uncertainty, and track your calibration. Bloomberg tells you what happened. Fermi Console helps you figure out what will happen.

---

## Implementation Phases

### Phase 1: Cockpit Shell

- [ ] Spatial layout with five zones (question hub, evidence, drivers, agents, timeline)
- [ ] Question hub with editable text and live probability display
- [ ] Evidence landscape as a simple list (upgrade to force-directed later)
- [ ] Driver map as a list with inline editing (upgrade to graph later)
- [ ] Agent fleet panel with assign/execute capability
- [ ] Timeline as a horizontal strip with event markers

### Phase 2: Agent Integration

- [ ] Question → auto-orchestration (fire agents on question entry)
- [ ] Agent results → evidence landscape population
- [ ] Agent results → driver suggestions (ghost nodes)
- [ ] Agent results → probability suggestion
- [ ] Manual agent assignment from fleet panel
- [ ] SSE streaming for agent progress

### Phase 3: Visual Upgrades

- [ ] Evidence landscape as force-directed graph (using lyon for 2D geometry)
- [ ] Driver map as dependency graph with sparklines
- [ ] Probability as animated indicator with history sparkline
- [ ] Timeline with click-to-restore
- [ ] Evidence gap visualization
- [ ] Contradiction highlighting

### Phase 4: Intelligence Features

- [ ] Evidence gap detection (automatic)
- [ ] Contradiction detection (automatic)
- [ ] Calibration feedback ("Your forecasts in this domain tend to be overconfident by 8%")
- [ ] Base rate lookup from reference classes
- [ ] Cross-forecast evidence sharing (evidence from one forecast relevant to another)

### Phase 5: Collaboration (CRDT)

- [ ] Shared forecasts — multiple users see the same cockpit
- [ ] Real-time evidence additions from team members
- [ ] Probability consensus (each team member sets their own, see the distribution)
- [ ] Agent execution visible to all team members
- [ ] Conflict resolution for driver edits

---

## Open Questions

1. **Force-directed layout in GPUI?** GPUI doesn't have a built-in graph layout engine. Options: compute layout in Rust (e.g., fdg-sim crate), render with GPUI's custom Element API. Or start with a simpler grid layout and upgrade later.

2. **Agent streaming?** Currently agents return results in one shot. For the cockpit to feel alive, we need streaming — partial results appearing as the agent works. This requires SSE from the agent execution endpoint.

3. **Probability suggestion algorithm?** When evidence arrives, how do we suggest a probability adjustment? Options: simple heuristic (bullish evidence → nudge up), Bayesian update from evidence strength, or ask an agent to synthesize.

4. **Evidence clustering?** How do we automatically cluster evidence? Options: embedding similarity (requires embedding computation), topic extraction (LLM), or manual tagging.

5. **Performance with many nodes?** A forecast with 20 evidence items, 8 drivers, and 5 agents has ~33 interactive nodes. GPUI should handle this fine, but the force-directed layout computation needs to be efficient.

---

## References

- Boyd's OODA Loop: https://en.wikipedia.org/wiki/OODA_loop
- Tetlock's Superforecasting methodology (base rates, reference classes, probability updates)
- GPUI custom elements: `crates/gpui/src/element.rs` in Zed source
- Force-directed graph layout: `fdg-sim` crate on crates.io
- Existing composer: `crates/fermi-console/src/composer.rs`
- ABW agent execution: `src/handlers/agents.rs` → `execute_agent_handler`
- Forecast API: `src/handlers/forecasts.rs` (Sprint 1)

---

## Revision History

- **2026-02-28:** Initial design exploration