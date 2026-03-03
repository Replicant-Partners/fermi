# Fermi Console MVP Architecture

**Date:** 2026-03-03
**Status:** Design document for MVP alignment
**Context:** The console has working data flows but needs architectural clarity before further development. This document maps the design vision to the codebase and defines the MVP roadmap.

---

## Core Principle

The Fermi Console is a purpose-built dashboard that hides FPL complexity while delivering the same intelligence as the Zed+MCP extension. The FPL program is the forecast. The console is the visual editor. Power users can always fall back to Zed for direct FPL editing.

---

## The Fermi Agent

**Fermi** is the meta-forecasting agent — omnipresent in the UX. It is:
- An absolute expert in Tetlock methodology and its application
- Cognizant of all agents registered to the `fermi-orchestra` tag
- A subset of Xaman Ek's ABW meta-agent knowledge, specialized for forecasting
- The orchestrator that suggests which specialized agent to assign to which driver
- The advisor that helps craft compelling prompts for each agent+driver combination

**Implementation:** Fermi is an MCP-style agent that:
- Runs locally (same as the current macro_forecaster execution path)
- Has access to the agent registry (knows all fermi-orchestra agents, their taxonomies, skills)
- Understands the FPL program structure (can read the current AST)
- Suggests decompositions, agent assignments, distribution types, query formulations

**Where Fermi appears in the UX:**
- Assistant panel messages (contextual guidance per FPL node)
- Agent picker suggestions ("For this driver, I recommend sentiment_analyzer because...")
- Query formulation help ("Based on this driver's rationale, here's a good query for market_research...")
- Validation warnings ("Your p5 > p50 — distribution is backwards")
- Post-simulation interpretation ("Your inside view diverges 15pp from base rate — here's why...")

---

## Three Artifacts

The Composer produces three views of the same underlying data:

### 1. FPL Program (source of truth)
- The AST (`Program` struct in `src/ast.rs`)
- Serialized to `.fpl` text via `generate_fpl_text()`
- Versioned (each Ctrl+S creates a snapshot)
- Stored in `forecasts/<name>.fpl`

### 2. Evidence Wiki (research log)
- Generated from the AST + agent evidence via `generate_evidence_wiki()`
- Organized by driver headings
- Each agent entry is an evidence log with attribution
- Human edits to evidence content (not structure) are possible
- Stored in `forecasts/<name>.evidence.md`
- History-flow visualization potential (evolving knowledge over versions)

### 3. Forecast Index (live visualization)
- Simulation results + version history
- Inside view (our probability) vs outside view (base rate) over time
- Histogram of simulation distribution
- Evidence treemap (drivers sized by impact, colored by evidence quality)
- Rendered via plotters to pixel buffers displayed in GPUI

---

## Right Panel: Three Tabs

```
┌──────┬──────┬──────┐
│ Edit │ FPL  │ Wiki │
├──────┴──────┴──────┤
│                    │
│  [active tab       │
│   content]         │
│                    │
│  + Assistant       │
│    messages        │
│    (always visible │
│     below tabs)    │
│                    │
└────────────────────┘
```

### Edit Tab
- When a driver is focused: driver editor fields + per-driver evidence
- When agent picker is open: agent cards with query input + schedule buttons
- Default: "Click a driver to edit"

### FPL Tab
- Live-generated FPL source from the AST
- Read-only in console (edit in Zed for power users)
- Updates on every change

### Wiki Tab
- Full evidence wiki rendered as structured content
- Driver headings (expandable)
- Evidence items with source, relevance, key findings
- Agent attribution (which agent produced this evidence)
- Human-editable content (notes, annotations)
- Links to sources (hyperlinks with previews, future: PDF attachments)

---

## Agent Assignment Flow

```
User clicks "+ agent" on a driver
        ↓
Right panel switches to Edit tab → Agent Picker
        ↓
Fermi suggests: "For 'market_share' driver, I recommend
market_research because it specializes in competitive dynamics"
        ↓
User sees fermi-orchestra agents with:
  - Name, skills, model, execution stats
  - Fermi's recommendation highlighted
        ↓
User types custom query (or accepts Fermi's suggested query)
        ↓
User picks schedule: Run once / Daily / Weekly
        ↓
Agent fires immediately with driver-specific query
        ↓
Evidence flows back → linked to agent → linked to driver
        ↓
Wiki updates, Forecast Index recalculates
```

---

## Evidence Model

```
Evidence belongs to Agent → Agent bound to Driver → Driver in Program

EvidenceStmt {
    id: unique
    source: "Agent: market_research (Claude API)"  ← attribution
    summary: "..."
    key_findings: [...]
    relevance: 0.0-1.0
    date: "2026-03-03"
}

AgentStmt {
    name: "market_research"
    query: "Research competitive dynamics for market_share driver..."
    schedule: Weekly
    driver_refs: ["market_share"]  ← binding
}

DriverStmt {
    name: "market_share"
    evidence_refs: ["market_research_0"]  ← links
}
```

---

## What Exists Today (codebase map)

### Working
- `cockpit.rs` (3400 lines): Composer with FPL AST, agent dispatch, driver editing
- `charts.rs` (190 lines): Plotters rendering (histogram, index chart, treemap)
- `main.rs` (2800 lines): App shell, panels, auth, portfolio, leaderboard
- `text_input.rs` (736 lines): Editable text fields
- `api/client.rs` (970 lines): ABW API client
- `src/ast.rs`: FPL AST with Program helpers
- `src/agent_backend/`: AgentRegistry, LLMExecutor, ToolAwareExecutor
- `agents/curated/`: 53 agent cards (4 fermi-orchestra)

### Data flows working
- Question → macro_forecaster → base rate + drivers + evidence ✓
- Driver editing → AST update → FPL generation ✓
- Agent assignment with custom query + schedule ✓
- Simulation → Forecast Index display ✓
- Save FPL + evidence wiki to disk ✓
- Local forecasts in Portfolio ✓
- Plotters histogram + index chart ✓

### Not yet built
- Tabbed right panel (Edit/FPL/Wiki)
- Fermi meta-agent (query suggestion, agent recommendation)
- Evidence wiki rendered in UI (only saved to disk)
- Human evidence editing
- Evidence hyperlinks/previews
- Treemap visualization in UI
- Version diff/comparison
- Agent scheduling execution (schedules stored but not executed)

---

## MVP Roadmap

### Sprint 1: Tabbed Right Panel (current)
- [ ] Three tabs: Edit, FPL, Wiki
- [ ] Wiki tab renders evidence organized by driver
- [ ] FPL tab shows live source (replaces Ctrl+E toggle)
- [ ] Assistant messages always visible below tabs

### Sprint 2: Fermi Meta-Agent
- [ ] Create `fermi` agent card in agents/curated/
- [ ] Fermi knows all fermi-orchestra agents (reads registry)
- [ ] Fermi suggests agent assignments based on driver nature
- [ ] Fermi helps formulate queries for agent+driver combinations
- [ ] Fermi provides Tetlock methodology guidance

### Sprint 3: Evidence Richness
- [ ] Human evidence entry (add notes, links per driver)
- [ ] Evidence hyperlinks with previews
- [ ] Evidence quality indicators (confidence bars)
- [ ] Evidence history (what changed between versions)

### Sprint 4: Live Index Dashboard
- [ ] Treemap visualization in Forecast Index section
- [ ] Version timeline with diff capability
- [ ] Portfolio dashboard with all forecasts' indices
- [ ] Sensitivity analysis (which driver matters most)

### Sprint 5: Agent Scheduling
- [ ] Execute scheduled agents (daily/weekly cron)
- [ ] Background agent execution with notifications
- [ ] Auto-update evidence wiki when agents complete
- [ ] Trigger-based execution (divergence threshold → re-research)

### Sprint 6: Polish & Ship
- [ ] Theme refinement (colors, typography, spacing)
- [ ] Keyboard navigation improvements
- [ ] Error handling and edge cases
- [ ] Performance optimization
- [ ] Release build + distribution

---

## Code Quality Principles

- **Lean and clean**: No duplicate data structures. AST is source of truth.
- **Modular**: charts.rs, cockpit.rs, main.rs have clear responsibilities.
- **No Python patches**: All code changes in Rust via proper tooling.
- **Test before commit**: cargo build must pass before every commit.
- **Small commits**: Each commit does one thing and explains why.

---

## Key Architectural Decisions

1. **Local agent execution** (not ABW API) — same as MCP server
2. **FPL AST as source of truth** — UI reads from and writes to the AST
3. **Plotters for visualization** — renders to pixel buffers for GPUI
4. **Evidence wiki as companion artifact** — grows alongside FPL
5. **Fermi meta-agent for orchestration** — not hardcoded agent selection
6. **Governance model** — user explicitly assigns agents, controls costs
7. **Version snapshots** — each save creates an immutable record

---

## Open Questions (Resolved)

1. **Fermi agent cost model**: TBD but will follow ABW economics — similar credit model to other agents.
2. **BYOA (Bring Your Own Agent)**: Publish to ABW with a special flag to register with fermi-orchestra. Discovery via Xaman Ek.
3. **Collaboration**: Defer for now. Start with git version-based merge. CRDT is aspirational but version-based is simpler and works.
4. **Mobile/Rabble**: Separate product. No intertwining. Forecasting is desktop-only via the console.
5. **Creature integration**: Deferred. Rabble is a separate world. Maybe a future experiment.
