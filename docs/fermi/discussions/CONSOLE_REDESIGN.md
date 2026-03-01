# Fermi Console Redesign — FPL-Native Visual Editor

**Date:** 2026-03-02
**Status:** Design proposal
**Context:** The console has 8,500 lines of UI code that can't do what a 20-line FPL file in Zed already does. The root cause: the console was built as a form that generates fragments, not as a visual editor for the FPL program. This document defines the correct architecture.

---

## The Principle

**The FPL program is the source of truth.** The console is a visual editor for that program. Every FPL construct has a visual counterpart. The UI reads from the AST and writes to the AST. The AST can always be serialized back to valid FPL text.

This is how Zed works with FPL — the text IS the program. The console replaces the text with visual widgets, but the underlying data model is identical.

```
┌─────────────────────────────────────────────────────────┐
│                    FPL Source Text                       │
│  (can be viewed/edited directly via Ctrl+E)             │
└──────────────────────┬──────────────────────────────────┘
                       │ parse
                       ▼
┌─────────────────────────────────────────────────────────┐
│                    FPL AST (Program)                     │
│  QuestionStmt + DriverStmt[] + EvidenceStmt[]           │
│  + AgentStmt[] + ModelStmt + SimulateStmt               │
└──────────┬───────────────────────────────┬──────────────┘
           │ read                          │ write
           ▼                               ▼
┌─────────────────────┐     ┌─────────────────────────────┐
│   Visual Cockpit    │     │   Local Executor            │
│   (GPUI widgets)    │     │   (Monte Carlo, no server)  │
└─────────────────────┘     └─────────────────────────────┘
           │                               │
           │ agent execution               │ results
           ▼                               ▼
┌─────────────────────────────────────────────────────────┐
│                    ABW API                               │
│  Agent execution, persistence, leaderboard, credits     │
└─────────────────────────────────────────────────────────┘
```

---

## FPL AST → UI Mapping

Every node in `fermi/src/ast.rs` maps to a visual element:

### Program (root)

The `Program` struct holds `Vec<Statement>`. The cockpit is a visual representation of this vector. The six-zone layout maps to statement types:

| AST Node | Zone | Visual Element |
|----------|------|----------------|
| `QuestionStmt` | Question Hub (top) | Question text + domain + target date + base rate |
| `DriverStmt[]` | Driver Map (center) | Driver cards with editable distributions |
| `EvidenceStmt[]` | Evidence Landscape (left) | Evidence items with sentiment + relevance |
| `AgentStmt[]` | Agent Fleet (right) | Agent cards with status + schedule + results |
| `ModelStmt` | Driver Map (bottom) | Model expression editor |
| `SimulateStmt` | Driver Map (results) | Simulation config + results display |
| `BaseRate` | Outside View (left-top) | Reference class + historical frequency |

### QuestionStmt

```rust
pub struct QuestionStmt {
    pub text: String,                    // → Question text input (large, top)
    pub base_rate: Option<BaseRate>,     // → Outside View zone
    pub target_date: Option<String>,     // → Target date input
    pub resolution_criteria: Option<String>, // → Resolution criteria input
}
```

**UI:** The Question Hub at the top. All fields are editable TextInput entities. Changing any field updates the AST immediately.

### BaseRate (Outside View)

```rust
pub struct BaseRate {
    pub reference_class: String,         // → Reference class display/edit
    pub historical_frequency: f64,       // → Base rate percentage display
    pub sample_size: Option<usize>,      // → Sample size display
    pub source: String,                  // → Source attribution
    pub reasoning: Option<String>,       // → Reasoning text
    pub generated_by: GeneratedBy,       // → "agent" or "human" badge
}
```

**UI:** The Outside View zone. When an agent generates a base rate, it populates these fields and shows `generated_by: agent(macro_forecaster)`. The user can override any field, which changes `generated_by` to `human`.

### DriverStmt

```rust
pub struct DriverStmt {
    pub name: String,                    // → Driver name (editable)
    pub display_name: Option<String>,    // → Human-readable label
    pub description: Option<String>,     // → Description text
    pub driver_type: DriverType,         // → Continuous/Binary badge
    pub distribution: Option<Distribution>, // → Distribution editor (p5/p50/p95 etc.)
    pub probability: Option<f64>,        // → Binary probability slider
    pub impact_multiplier: Option<f64>,  // → Binary impact field
    pub unit: Option<String>,            // → Unit label
    pub rationale: Option<String>,       // → Rationale text area
    pub constraints: Vec<Constraint>,    // → Constraint rules display
    pub evidence_refs: Vec<String>,      // → Links to evidence items
}
```

**UI:** Each driver is a card in the Driver Map zone. The card shows:
- Header: name, type badge, summary line
- Expanded editor: all fields as TextInput entities
- Distribution visualization: range bar for continuous, probability bar for binary
- Linked evidence: references to EvidenceStmt items
- Linked agents: which agents are assigned to this driver (from AgentStmt.driver_refs)

**Critical:** Drivers are NOT created by the UI and then agents assigned. Drivers can be:
1. **Suggested by agents** — agent returns evidence + suggests a driver → appears as ghost node
2. **Created manually** — user clicks "+ Driver" → blank card opens
3. **Populated from FPL** — loading an existing .fpl file populates all drivers

### AgentStmt — THE KEY MISSING PIECE

```rust
pub struct AgentStmt {
    pub name: String,                    // → Agent ID (from ABW catalogue)
    pub agent_type: Option<String>,      // → Type badge (research, sentiment, etc.)
    pub query: String,                   // → The research query (editable)
    pub executor: Option<ExecutorType>,  // → LLM/MCP/Manual/Skill
    pub schedule: Option<Schedule>,      // → Schedule selector
    pub driver_refs: Vec<String>,        // → Which drivers this agent feeds
    pub depends_on: Vec<String>,         // → Agent dependency chain
    pub confidence_threshold: Option<f64>, // → Minimum confidence to accept
}
```

**UI:** Each agent is a row in the Agent Fleet zone. But agents are NOT just status indicators — they are **active research assignments**:

- **Query:** The specific research question the agent is investigating (editable)
- **Schedule:** When the agent runs — `once`, `daily`, `weekly(monday, 09:00)`, `monthly`
- **Driver refs:** Which drivers this agent's output feeds into (drag-to-link or dropdown)
- **Depends on:** Other agents that must complete first (dependency chain)
- **Confidence threshold:** Minimum confidence to auto-accept results (0.0-1.0 slider)
- **Status:** idle → running → completed/failed
- **Results:** Evidence items produced, confidence score, execution time, credits charged

**The agent-driver binding is the core workflow:**

```
┌──────────────┐     assigns to      ┌──────────────┐
│ Agent:       │ ──────────────────→  │ Driver:      │
│ research_    │     driver_refs:     │ market_tam   │
│ analyst      │     [market_tam]     │              │
│              │                      │ dist: tri(   │
│ query: "sat  │     produces         │   2B, 5B, 7B │
│ connectivity │ ──────────────────→  │ )            │
│ TAM 2026"    │     evidence         │              │
│              │                      │ evidence:    │
│ schedule:    │                      │ [morgan_     │
│ weekly       │                      │  stanley]    │
└──────────────┘                      └──────────────┘
```

When an agent completes:
1. Its evidence is added to the linked driver's evidence list
2. If the agent suggests distribution changes, those appear as pending updates on the driver
3. The user can accept or reject the updates
4. Accepted updates modify the driver's distribution parameters
5. The model auto-recalculates

### EvidenceStmt

```rust
pub struct EvidenceStmt {
    pub id: String,                      // → Unique ID
    pub source: String,                  // → Source name
    pub summary: Option<String>,         // → Summary text
    pub url: Option<String>,             // → Link
    pub relevance: Option<f64>,          // → Relevance score (0-1)
    pub date: Option<String>,            // → Date
    pub strength: Option<f64>,           // → Evidence strength
    pub key_findings: Vec<String>,       // → Bullet points
}
```

**UI:** Evidence items in the Evidence Landscape zone. Each item shows:
- Source + date
- Summary text
- Relevance score (visual bar)
- Key findings (expandable list)
- Sentiment classification (bullish/bearish/neutral — derived from key_findings)
- Which driver it's linked to
- Which agent produced it (or "manual" if user-entered)

### ModelStmt

```rust
pub struct ModelStmt {
    pub expression: Expression,          // → Model expression editor
}
```

**UI:** The model expression at the bottom of the Driver Map zone. Shows as:
- Formatted expression text: `revenue = market_tam * market_share * arpu`
- Auto-generated from drivers if not manually set
- Editable as text (power users)
- Visual representation: which drivers feed into the model

### SimulateStmt

```rust
pub struct SimulateStmt {
    pub iterations: usize,               // → Iteration count selector
}
```

**UI:** Simulation controls + results in the Driver Map zone:
- Iteration count (default 10,000)
- Run button (Ctrl+R)
- Results: mean, median, p5, p95, std_dev
- Histogram visualization
- Sensitivity analysis (which driver contributes most variance)

---

## The Forecast Lifecycle (What the Console Must Support)

### Phase 1: Question Definition
User types a question. The console creates a `QuestionStmt` in the AST.

### Phase 2: Agent Dispatch (Automatic)
On question submit, the console:
1. Creates `AgentStmt` nodes for default research agents (macro_forecaster, market_research, sentiment_analyzer)
2. Each agent has a query derived from the question
3. Each agent has `driver_refs: []` initially (unbound)
4. Agents execute via `POST /api/agents/{id}/execute`
5. Results stream back via Entity channel integration (already built)

### Phase 3: Evidence + Driver Population
As agents complete:
1. Evidence items are added to the AST as `EvidenceStmt` nodes
2. If the agent suggests drivers (from its reasoning), `DriverStmt` nodes are created as `suggested: true`
3. The Outside View populates from the macro_forecaster's base rate output
4. The user sees evidence appearing in the Evidence Landscape and ghost drivers in the Driver Map

### Phase 4: Driver Refinement (User + Agents)
The user:
1. Accepts/rejects suggested drivers
2. Edits driver parameters (distribution, probability, unit, rationale)
3. Assigns agents to specific drivers: "I want sentiment_tracker to monitor market_share weekly"
4. Adds manual evidence
5. Adds constraints between drivers

The agents:
1. Run on their schedules (or on-demand)
2. Produce new evidence that feeds into their assigned drivers
3. Suggest parameter updates based on new evidence
4. The user accepts or rejects updates

### Phase 5: Model + Simulation
1. Model expression auto-generates from accepted drivers (or user edits manually)
2. User runs simulation (Ctrl+R) — local Monte Carlo, instant results
3. Results show in the Driver Map zone: histogram, percentiles, sensitivity
4. User adjusts probability based on results + outside view

### Phase 6: Publish + Version
1. User publishes forecast (Ctrl+P) — POST to ABW API
2. Forecast gets a version number (v1)
3. Subsequent updates create new versions (v2, v3, ...)
4. Each version records: probability change, evidence added, drivers modified, agent results
5. Version history is visible in the Timeline zone

### Phase 7: Ongoing Monitoring
1. Scheduled agents continue running (daily, weekly, monthly)
2. New evidence streams in
3. User gets notifications when agents find significant changes
4. User can update probability and create new versions
5. External signals (if configured) provide market data

### Phase 8: Resolution
1. When the target date arrives, the forecast resolves
2. Brier score is calculated
3. Retrospective analysis: which drivers were wrong, what biases appeared
4. Learning feeds into the user's calibration profile

---

## What Needs to Change in the Codebase

### 1. Central AST State (NEW)

The cockpit currently stores drivers, evidence, agents as separate `Vec`s. It should store a single `Program` (the FPL AST) and derive everything from it.

```rust
pub struct CockpitState {
    // THE source of truth — the FPL program being edited
    pub program: Program,
    
    // Derived views (rebuilt when program changes)
    pub question: QuestionView,      // from program.question()
    pub drivers: Vec<DriverView>,    // from program.drivers()
    pub evidence: Vec<EvidenceView>, // from program.evidence()
    pub agents: Vec<AgentView>,      // from program.agents()
    pub model: ModelView,            // from program.model()
    pub sim_config: SimView,         // from program.simulate()
    
    // Runtime state (not in AST)
    pub agent_executions: HashMap<String, AgentExecution>,
    pub sim_results: Option<SimResults>,
    pub predicted_probability: f64,
    pub editing_driver: Option<usize>,
    
    // UI entities
    pub text_inputs: TextInputPool,  // shared TextInput entities
    pub api: Arc<ApiClient>,
}
```

### 2. AST ↔ UI Sync

Every UI edit writes to the AST. Every AST change triggers a UI rebuild.

```rust
impl CockpitState {
    /// Modify the AST and rebuild derived views
    fn update_program(&mut self, f: impl FnOnce(&mut Program)) {
        f(&mut self.program);
        self.rebuild_views();
        self.regenerate_fpl_cache();
    }
    
    /// Rebuild all derived views from the AST
    fn rebuild_views(&mut self) {
        self.question = QuestionView::from_ast(&self.program);
        self.drivers = self.program.drivers().map(DriverView::from_ast).collect();
        self.evidence = self.program.evidence().map(EvidenceView::from_ast).collect();
        self.agents = self.program.agents().map(AgentView::from_ast).collect();
        self.model = ModelView::from_ast(&self.program);
        self.sim_config = SimView::from_ast(&self.program);
    }
}
```

### 3. Agent-Driver Binding (NEW)

Agents must be assignable to drivers. The UI needs:
- Drag agent → drop on driver (or dropdown selector)
- Visual link lines between agents and their target drivers
- Agent schedule selector (once, daily, weekly, monthly)
- Confidence threshold slider

When an agent completes, its results flow to the bound driver:
```rust
fn on_agent_complete(&mut self, agent_name: &str, output: &AgentOutput) {
    // Find which drivers this agent feeds
    let driver_refs = self.program.agent(agent_name)
        .map(|a| a.driver_refs.clone())
        .unwrap_or_default();
    
    // Add evidence to each linked driver
    for evidence in &output.evidence {
        let evidence_stmt = EvidenceStmt::from_agent_output(evidence);
        self.update_program(|p| p.add_evidence(evidence_stmt.clone()));
        
        // Link evidence to driver
        for driver_name in &driver_refs {
            self.update_program(|p| {
                if let Some(driver) = p.driver_mut(driver_name) {
                    driver.evidence_refs.push(evidence_stmt.id.clone());
                }
            });
        }
    }
    
    // If agent suggests distribution updates, create pending update
    if let Some(suggested_dist) = extract_distribution_suggestion(&output) {
        for driver_name in &driver_refs {
            self.pending_updates.push(PendingDriverUpdate {
                driver_name: driver_name.clone(),
                agent_name: agent_name.to_string(),
                suggested_distribution: suggested_dist.clone(),
                confidence: output.confidence,
            });
        }
    }
}
```

### 4. Versioning (NEW)

Each publish creates a version snapshot:
```rust
pub struct ForecastVersion {
    pub version: u32,
    pub timestamp: DateTime<Utc>,
    pub program: Program,           // full AST snapshot
    pub probability: f64,
    pub change_summary: String,     // "Updated market_tam p50 from 5B to 6B based on Morgan Stanley report"
    pub trigger: VersionTrigger,    // Manual, AgentUpdate, ScheduledReview
}

pub enum VersionTrigger {
    Manual,                         // User clicked Publish
    AgentUpdate(String),            // Agent produced significant new evidence
    ScheduledReview,                // Weekly review cycle
}
```

### 5. FPL Round-Trip (CRITICAL)

The console must be able to:
1. **Parse** an existing .fpl file into the AST → populate the cockpit
2. **Generate** valid FPL text from the AST → viewable via Ctrl+E
3. **Round-trip** without loss: parse → display → edit → generate → parse = same program

This means the `generate_fpl` function must produce ALL FPL constructs, not just the subset currently supported (question + drivers + model + simulate).

---

## Implementation Plan

### Sprint 1: AST-Centric Refactor (Foundation)
- [ ] Add `Program` field to `CockpitState`, replace separate Vecs
- [ ] Add helper methods to `Program`: `drivers()`, `agents()`, `evidence()`, `question()`, `model()`
- [ ] `update_program()` pattern for all mutations
- [ ] `rebuild_views()` to derive UI state from AST
- [ ] Round-trip: `generate_fpl()` produces complete FPL from AST
- [ ] Round-trip: `parse_fpl()` loads FPL text into cockpit
- [ ] Ctrl+E shows full generated FPL (already partially works)

### Sprint 2: Agent-Driver Binding
- [ ] Agent Fleet zone shows real agent assignments, not just status
- [ ] Each agent card shows: query, schedule, driver_refs, confidence_threshold
- [ ] Assign agent to driver (dropdown or drag)
- [ ] Agent schedule selector (once/daily/weekly/monthly)
- [ ] Agent execution flows results to bound drivers
- [ ] Pending updates: agent suggests distribution change → user accepts/rejects

### Sprint 3: Evidence Flow
- [ ] Evidence items linked to drivers (evidence_refs)
- [ ] Agent-produced evidence auto-links to bound driver
- [ ] Manual evidence entry with driver assignment
- [ ] Evidence sentiment derived from key_findings
- [ ] Evidence gaps: drivers without sufficient evidence highlighted

### Sprint 4: Versioning + Persistence
- [ ] Local SQLite storage for forecast drafts
- [ ] Version snapshots on publish
- [ ] Version history in Timeline zone
- [ ] Diff view between versions
- [ ] Load/save .fpl files from disk

### Sprint 5: Continuous Monitoring
- [ ] Agent scheduling (run agents on their configured schedules)
- [ ] Notification when agent finds significant new evidence
- [ ] Auto-suggest probability updates based on new evidence
- [ ] External signal integration (future)

---

## What to Keep from Current Code

The current 8,500 lines are not wasted. The following are solid and should be kept:

- **GPUI app shell** (main.rs): window management, menus, navigation, sidebar — all good
- **TextInput entity** (text_input.rs): works, tested, handles focus correctly
- **API client** (api/client.rs): typed HTTP client with all endpoints — solid
- **Theme** (main.rs theme module): Ayu Mirage colors — good
- **OAuth flow** (main.rs): localhost callback + fallback — works
- **Dashboard, Portfolio, Agent Fleet, Leaderboard panels** (main.rs): all functional
- **Render helpers**: zone cards, stat cards, forecast rows, leaderboard rows — reusable
- **Tokio runtime setup**: background runtime for reqwest — necessary

What needs to be **replaced**:

- **CockpitState**: rebuild around `Program` AST instead of separate Vecs
- **orchestrate_question**: should create AgentStmt nodes in the AST, not fire-and-forget
- **populate_from_agent_result**: should flow results to bound drivers via AST
- **generate_fpl**: must produce complete FPL, not fragments
- **Driver rendering**: must show agent bindings, evidence links, constraints
- **Agent rendering**: must show query, schedule, driver_refs, not just status

What should be **removed**:

- **populate_initial_scaffold**: scaffold drivers are a workaround for missing agent flow
- **Separate driver/evidence/agent Vecs**: replaced by Program AST
- **detect_sentiment heuristic**: sentiment should come from agent output, not keyword matching

---

## Success Criteria

The console is done when a non-programmer can:

1. Type a question and have agents automatically research it
2. See evidence streaming in, organized by driver
3. Accept or modify agent-suggested drivers and parameters
4. Assign additional agents to specific drivers with schedules
5. Run a simulation and understand the results visually
6. Set a probability informed by both outside view and inside view
7. Publish a forecast and track it over time
8. Receive updates when agents find new evidence
9. Version their forecast as their understanding evolves
10. See their Brier score improve as they get better at forecasting

And at any point, they can press Ctrl+E and see the complete, valid FPL program that represents their forecast — the same program that a power user would write by hand in Zed.