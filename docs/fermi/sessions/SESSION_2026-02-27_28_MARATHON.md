# Session Context — 2026-02-27/28 Marathon Session

**Duration:** ~12 hours across two days (+ follow-up sessions)
**Commits:** 21 total (18 fermi, 3 rabble)
**Status:** Research Cockpit fully interactive with native menus, window management, mouse interaction on all controls

---

## What Was Built This Session

### 1. Tethering Self-Healing (fermi `5fd5454`)

**Files:** `src/handlers/creatures/state.rs`, `src/handlers/creatures/tethering.rs`

- Device flights inherit `rabble_id` from `creature_state` when self-healing creates a new flight
- `perch_handler` and `host_rabble_handler` preserve device flights instead of ending them
- Active device flights get linked to new rabble's `swarm_id`

### 2. Brilliant Frame AR Prototype (rabble `8e61d9d`)

**Files:** `lib/services/frame_service.dart`, `lib/screens/frame_ar_screen.dart`, `docs/FRAME_AR_DESIGN.md`

- `FrameService` — BLE connection to Brilliant Frame glasses, render loop (~8fps), IMU head-tracking, chat bubble management
- `FrameArScreen` — phone control panel with mirror preview of what Frame OLED displays
- Creatures + chat bubbles rendered to 640×400 OLED through transparent lenses
- Design doc with 5-phase roadmap (scaffold → sprites → nav → interaction → ARCore)
- Key insight: same ABW agents, different render target. Frame today, ARCore tomorrow.
- Dependencies added: `frame_sdk: ^0.0.7`, `flutter_blue_plus: ^1.31.0`

### 3. Creature Cognition Economy — ADR-011 (fermi `dfaf52a`)

**File:** `docs/fermi/decisions/011_creature_cognition_economy.md`

- **Core model:** Cognition = Knowledge × Bandwidth
- Knowledge is earned (dreaming, AKP, coherence engine) — persistent, grows over time
- Bandwidth is selected (free/standard/premium model tier) — determines expression ceiling
- Three tiers: free (`openrouter/free`), standard (Haiku), premium (Sonnet)
- Capability gates per agent, set by agent maker based on eval scores
- Graceful degradation on infrastructure failures only, never on quality failures
- Creature's `cognition_tier` flows through execution context — no traffic cops, no deterministic graphs
- Rabble-level tier from anchor creature (social incentive to upgrade)
- 6-phase implementation plan
- **Rejected:** rs-graph-llm / deterministic workflow graphs — kills emergent behavior

### 4. Tethered Rabble Movement Fix (fermi `51e8683`, rabble `2717db2`)

**Root cause found:** Two bugs working together:

1. **Backend:** `host_rabble_handler` created a duplicate perch flight (`data_source='app'`) alongside the existing device flight when a tethered creature hosted a rabble. Telemetry updated the device flight but map could read the stale perch flight.
2. **Frontend:** `RabbleChatScreen` never re-fetched swarm position. Backend broadcast `rabble_moved` SSE events but nobody listened.

**Fix:**
- **Backend:** `host_rabble_handler` now detects active device flight and reuses it as the rabble flight — no duplicate
- **Frontend:** New `RabbleStreamService` (SSE client for `/api/rabble/:id/stream`) handles `rabble_moved` events, updates mutable swarm center coordinates that flow to `MiniMap`

**Files changed:**
- `fermi/src/handlers/creatures/state.rs` — reuse device flight, clean up flight loop
- `rabble/lib/services/rabble_stream_service.dart` — new SSE client (366 lines)
- `rabble/lib/screens/rabble_chat.dart` — wire SSE stream, mutable swarm position

### 5. Fermi Native Console — Design + GPUI Scaffold (fermi `25f72c3`, `6f9b73d`)

**Files:** `docs/fermi/discussions/FERMI_NATIVE_CONSOLE.md`, `crates/fermi-console/`

- MMOG-style forecasting command center design doc
- GPUI v0.2.2 (Zed's GPU-accelerated UI framework) chosen over egui/Clay/Tauri
- Ayu Mirage theme fully defined as Rust constants
- Sidebar navigation with 5 panels (Dashboard, Portfolio, Agent Fleet, Composer, Leaderboard)
- Keyboard shortcuts (⌘1-5 panel switching, ⌘Q quit)
- Dashboard with stat cards (Brier score, active forecasts, agent fleet, rank)
- Rust toolchain bumped to `stable` for GPUI compatibility

**Linux build deps required:**
```bash
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev libfreetype-dev
```

### 6. Fermi Forecasting System — Sprint 1 Backend (fermi `32a275a`)

**Files:** `migrations/094_fermi_forecasting.sql`, `src/handlers/forecasts.rs`, `src/handlers/mod.rs`, `src/api_server.rs`

**Migration 094 creates:**
- `fermi_notebooks` — optional authoring container (with `fpl_source` field)
- `fermi_portfolios` — named collections with domain tagging
- `fermi_forecasts` — first-class standalone forecasts (notebook_id optional)
- `fermi_forecast_updates` — probability revision history with agent attribution
- `fermi_portfolio_forecasts` — many-to-many membership
- `fermi_leaderboard` — materialized view ranked by avg Brier score
- `resolve_forecast()` SQL function — atomic Brier computation
- `compute_brier_score()` SQL function
- `refresh_fermi_leaderboard()` SQL function

**22 API endpoints:**
- Forecast CRUD: `POST/GET/PUT/DELETE /api/forecasts`, `GET /api/forecasts/:id`
- Resolution: `POST /api/forecasts/:id/resolve` (computes Brier score)
- Void: `POST /api/forecasts/:id/void`
- Probability updates: `POST /api/forecasts/:id/update-probability`
- Portfolio CRUD: `POST/GET /api/portfolios`
- Portfolio stats: `GET /api/portfolios/:id/stats` (Brier aggregation + calibration curve)
- Portfolio membership: `POST/DELETE /api/portfolios/:id/forecasts`
- Leaderboard: `GET /api/leaderboard`
- Personal stats: `GET /api/forecasts/my-stats`
- Public discovery: `GET /api/forecasts/public` (no auth required)

### 7. Console API Client (fermi `a5c2be3`)

**File:** `crates/fermi-console/src/api/client.rs` (970 lines)

- Full typed HTTP client for ABW API
- Runtime-updatable API key via `Arc<RwLock<ApiConfig>>`
- All 22 forecast/portfolio/leaderboard endpoints wrapped
- Agent execution and listing
- Team queries
- Query builders (`ForecastQuery`, `LeaderboardQuery`)
- Error mapping (401→NotAuthenticated, 429→RateLimited, etc.)
- Unit tests

### 8. Console Live API Integration + Portfolio Panel (fermi `a11bede`)

**File:** `crates/fermi-console/src/main.rs` (major rewrite)

- `ApiClient` wired into `FermiConsole` as shared `Arc`
- Auto-connects via `FERMI_API_KEY` or `ABW_API_KEY` env var
- Dashboard shows real stats from `/api/forecasts/my-stats`
- Activity feed built from resolved forecasts with Brier-colored indicators
- Portfolio panel with three sections: Active, Drafts, Resolved
- Each forecast shows probability badge, question, domain, Brier score, target date, status, outcome
- Sidebar nav items clickable (mouse_down handler)
- Sidebar shows connection status + user display name
- Data auto-refreshes when switching panels
- GPUI `AsyncFnOnce` spawn pattern working with `WeakEntity`

### 9. Research Cockpit Design (fermi `af808ca`, `9eb51d3`)

**File:** `docs/fermi/discussions/RESEARCH_COCKPIT.md` (598+ lines)

Replaces the linear form-based composer with a spatial OODA-loop research workspace:

- **Six zones:** Question Hub, Outside View, Evidence Landscape, Driver Map, Agent Fleet, Timeline
- **Outside View first** — Tetlock base rate anchoring before inside view
- **Probability flanked** by outside view (base rate) and inside view (model mean) with divergence always visible
- **Evidence as clustered map** — bullish/bearish/neutral, gaps, contradictions
- **Drivers as dependency graph** — ghost nodes for agent suggestions
- **Agent Fleet as living dashboard** — running/completed/idle with findings
- **Timeline as audit trail** — probability evolution history
- **Divergence warning** at >20pp from base rate
- **Question → auto-orchestration** — agents fire on question entry
- **FPL as internal representation** — cockpit is a visual FPL editor

### 10. Research Cockpit Implementation (fermi `a8e8f3b`)

**New files:**
- `crates/fermi-console/src/cockpit.rs` (1305 lines) — six-zone layout with full data structures
- `crates/fermi-console/src/text_input.rs` (700+ lines) — reusable editable text field entity

**Cockpit features:**
- Question Hub with editable text field (large mode), domain, target date, resolution criteria
- Live probability display flanked by outside view and inside view with divergence
- Outside View panel: reference class, historical frequency, sample size, source, reasoning, divergence warning
- Evidence Landscape: sentiment-colored items, relevance scores, evidence gaps with suggested agents
- Driver Map: continuous/binary drivers, ghost nodes for agent suggestions, model expression, simulation results
- Agent Fleet: running/completed/idle agents with findings, cost, model used
- Timeline: horizontal event strip with probability trace

**TextInput entity:**
- `EntityInputHandler` implementation for GPUI text input protocol
- Typing, backspace, delete, cursor movement, selection, copy/cut/paste, home/end
- Focus ring, placeholder text, on_change/on_submit callbacks
- Label support, large mode for question hub
- Ayu Mirage themed

### 11. Agent Orchestration on Question Submit (fermi `77b7027`)

**Files:** `crates/fermi-console/src/cockpit.rs` (major additions), `crates/fermi-console/src/main.rs`

When user types question + ⌘Enter:
1. **Outside view search fires** — base rate lookup via macro_forecaster
2. **Probability anchors to base rate** (35% default, Tetlock discipline)
3. **Inside view agents fire in parallel** — macro_forecaster (detailed), market_research, sentiment_analyzer
4. **Immediate visual feedback** — agents shown as "running", evidence gaps populated, timeline updated
5. **API calls fire** via `tokio::spawn` to ABW backend
6. `populate_from_agent_result()` ready to process responses
7. Simple sentiment detection heuristic for evidence classification
8. Domain detection from question keywords

---

## Current State of Each Codebase

### fermi (Rust backend + console)

```
fermi/
├── src/                          # API server
│   ├── handlers/
│   │   ├── forecasts.rs          # NEW: 22 forecast/portfolio/leaderboard endpoints
│   │   ├── creatures/state.rs    # MODIFIED: tether rabble movement fix
│   │   └── creatures/tethering.rs # MODIFIED: self-healing device flights
│   └── api_server.rs             # MODIFIED: routes + migration 094 registered
├── migrations/
│   └── 094_fermi_forecasting.sql # NEW: forecasting tables + Brier scoring
├── crates/
│   └── fermi-console/            # NEW: native GPUI desktop app
│       ├── src/
│       │   ├── main.rs           # App shell, dashboard, portfolio, navigation
│       │   ├── api/client.rs     # Typed API client for ABW
│       │   ├── cockpit.rs        # Research Cockpit (6-zone OODA workspace)
│       │   ├── composer.rs       # Legacy linear composer (superseded by cockpit)
│       │   └── text_input.rs     # Reusable editable text field entity
│       ├── Cargo.toml            # GPUI + rusqlite + reqwest + fermi deps
│       └── README.md             # Build deps for Linux/macOS/Windows
├── docs/fermi/
│   ├── decisions/
│   │   └── 011_creature_cognition_economy.md  # ADR: model ladder + tiers
│   └── discussions/
│       ├── FERMI_NATIVE_CONSOLE.md            # Console design exploration
│       └── RESEARCH_COCKPIT.md                # OODA loop UX design
└── rust-toolchain.toml           # MODIFIED: bumped to "stable"
```

### rabble (Flutter mobile app)

```
rabble/
├── lib/
│   ├── services/
│   │   ├── frame_service.dart           # NEW: Brilliant Frame BLE integration
│   │   └── rabble_stream_service.dart   # NEW: SSE client for rabble events
│   ├── screens/
│   │   ├── frame_ar_screen.dart         # NEW: Frame AR glasses experience
│   │   └── rabble_chat.dart             # MODIFIED: SSE rabble_moved handling
│   └── ...
├── docs/
│   └── FRAME_AR_DESIGN.md               # NEW: Frame AR design + roadmap
└── pubspec.yaml                         # MODIFIED: frame_sdk + flutter_blue_plus
```

---

## What Needs to Happen Next (Priority Order)

### 1. ~~Channel Integration for Agent Results~~ ✅ DONE (fermi `957d41a`)

**Completed:** Option A — CockpitState is now a GPUI Entity.

- `CockpitState` implements `Render`, owned as `Entity<CockpitState>` by `FermiConsole`
- `orchestrate_question()` uses `cx.spawn()` instead of `tokio::spawn`
- Each agent gets its own spawned task with a `WeakEntity` handle
- On completion: `this.update(cx, |cockpit, cx| { ... })` pushes results back to UI thread
- On failure: `mark_agent_failed()` updates fleet panel with error details
- `check_orchestration_complete()` finalizes probability adjustment when all agents done
- `populate_from_agent_result()` now also extracts drivers and updates base rate / reference class / sample size from agent metadata
- `agent_result_to_json()` bridges typed `AgentExecutionResult` → `JsonValue`
- Orchestration status bar shows live progress (`N/M complete`)
- Agent fleet panel shows error details for failed agents

**Flow:**
```
User types question + ⌘Enter
→ FermiConsole.on_trigger_question_orchestration()
→ cockpit.update(cx, |c, cx| c.orchestrate_question(..., cx))
→ cx.spawn() fires N agent API calls in parallel
→ Each completes → this.update(cx, |c, cx| c.populate_from_agent_result())
→ cx.notify() → GPUI re-renders → zones update live
```

### 2. ~~Editable Driver Parameters~~ ✅ DONE (fermi `d646d86`)

**Completed:** Click driver node → inline editor expands with full parameter editing.

- `toggle_driver_edit()` expands/collapses driver nodes in the Driver Map
- Continuous drivers: p5/p50/p95 values with visual range bar, distribution type, unit
- Binary drivers: probability + impact multiplier display
- `accept_driver()` confirms suggested (ghost) drivers from agents
- `remove_driver()` deletes a driver and recomputes model expression
- `update_continuous_driver()` / `update_binary_driver()` for parameter changes
- `render_driver_editor()`, `render_param_value()`, `render_range_bar()` visual components
- Accept/remove buttons on each driver node

### 3. ~~⌘R Local Simulation~~ ✅ DONE (fermi `d646d86`)

**Completed:** Full FPL generation → parse → execute → display pipeline.

- `generate_fpl()` builds FPL source from cockpit state (accepted drivers only)
- `auto_model_expression()` generates model from driver names (continuous multiply, binary if-then)
- `run_simulation()` parses FPL → runs `fermi::executor::Executor` (10k iterations <100ms)
- Results displayed in Driver Map zone: mean, median, p5, p95, σ, iteration count, timing
- `effective_fpl()` supports manual FPL override via `fpl_source_override`
- ⌘E toggles FPL source view with `cached_fpl_source` for display
- Timeline event recorded on simulation complete

### 4. ~~⌘P Publish Flow~~ ✅ DONE (fermi `d646d86`)

**Completed:** Async publish via `cx.spawn()` with full cockpit state serialization.

- `publish_forecast()` collects all cockpit state into `CreateForecastRequest`
- Includes: probability, drivers JSON, evidence JSON, sim results, FPL source, domain, target date, resolution criteria
- Async POST via `cx.spawn()` + `WeakEntity` callback (same pattern as agent orchestration)
- Success: `forecast_id` stored, status → "active", timeline event "Forecast published"
- Failure: error displayed in `publish_status` (red text in Question Hub)
- Status indicator in Question Hub (green = published, red = error, gold = in progress)
- Agents used list included in the request

### 5. ~~Probability Slider~~ ✅ DONE (fermi `d646d86`, `2351b26`)

**Completed:** Interactive slider bar in Question Hub with mouse handlers and nudge buttons.

- `render_probability_slider_interactive()` — 200px bar with filled portion + thumb indicator
- 5%–95% range labels, fill color changes to gold on divergence warning
- `set_probability()` clamps to [0.05, 0.95]
- `commit_probability_change()` records timeline event on drag end
- `on_mouse_down` / `on_mouse_up` handlers for drag interaction
- Nudge buttons: -5, -1, +1, +5 percentage point adjustments via `cx.listener()`
- Divergence indicator updates in real time (pp from base rate)
- Keyboard hints bar added: ⌘Enter research · ⌘R simulate · ⌘P publish · ⌘E toggle FPL

### 6. Native Menus + Window Management ✅ DONE (fermi `2351b26`)

**Completed:** Full native application menu bar and window controls.

**Menu bar** (`cx.set_menus()`):
- Fermi Console: About, New Forecast ⌘N, Quit ⌘Q
- File: New Forecast ⌘N, Publish Forecast ⌘P
- View: Dashboard ⌘1, Portfolio ⌘2, Agent Fleet ⌘3, Composer ⌘4, Leaderboard ⌘5, Toggle FPL Source ⌘E
- Forecast: Research Question ⌘Enter, Run Simulation ⌘R, Publish ⌘P, Reset Cockpit
- Window: Minimize ⌘M, Zoom, Toggle Fullscreen ^⌘F

**Window controls:**
- `MinimizeWindow` (⌘M) → `window.minimize_window()`
- `ZoomWindow` → `window.zoom_window()` (maximize/restore)
- `ToggleFullscreen` (^⌘F) → `window.toggle_fullscreen()`
- `ResetCockpit` → creates fresh `CockpitState` Entity
- Traffic light buttons in sidebar header (close/minimize/zoom) with macOS-style colored dots
- Fullscreen toggle button (⛶) in sidebar header

**Interactive mouse handlers:**
- Driver nodes: click to `toggle_driver_edit()` via `cx.listener()`
- Accept button: click to `accept_driver()` + `auto_model_expression()`
- Remove button: click to `remove_driver()` when editing
- All interactive elements use `ElementId` for GPUI's stateful element tracking

---

## Key Technical Decisions Made

1. **GPUI over egui/Clay** — Zed's framework, GPU-accelerated, CRDT collaboration potential
2. **FPL stays** — the cockpit is a visual FPL editor, not a replacement
3. **Outside view first** — Tetlock discipline: anchor to base rate before inside view
4. **No deterministic workflow graphs** — agents reason about what to do next, no traffic cops
5. **Creature cognition tiers** — model quality is a game mechanic, not a settings page
6. **SSE for real-time** — rabble_moved events, not polling
7. **Standalone forecasts** — `notebook_id` is optional, forecasts are first-class
8. **Materialized view for leaderboard** — refreshed on resolution, live fallback query

---

## Environment Setup for Next Session

```bash
# Fermi console
cd /home/ilabra/fermi
export FERMI_API_KEY="your-api-key"  # or ABW_API_KEY
cargo run -p fermi-console

# To test with the API server running locally:
# Terminal 1: cargo run --bin api-server
# Terminal 2: FERMI_API_KEY=... cargo run -p fermi-console
```

**Linux deps (already installed):**
```bash
sudo apt-get install -y libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libfontconfig1-dev libfreetype-dev
```

---

## Commit Log

### fermi (18 commits)
```
2351b26 feat: Native menus, window controls, interactive probability slider + driver clicks
a6740c0 docs: update session notes — all 5 priority items DONE
d646d86 feat: Editable drivers, ⌘R simulation, ⌘P publish, probability slider
7a4e1df docs: update session notes — channel integration marked DONE
957d41a feat: Entity channel integration — agent results flow back to Research Cockpit UI
77b7027 feat: Agent orchestration on question submit — cockpit comes alive
a8e8f3b feat: Research Cockpit — six-zone OODA loop workspace with editable text input
9eb51d3 docs: Add Outside View (Tetlock base rate) as first-class zone
af808ca docs: Research Cockpit — OODA loop UX design for forecast composer
813327c feat: Fermi Console Forecast Composer (legacy, superseded by cockpit)
a11bede feat: Fermi Console Sprint 2 — live API integration + portfolio panel
a5c2be3 feat: Fermi Console API client — Sprint 2 foundation
6f9b73d docs: Fermi Console README with build deps
32a275a feat: Fermi forecasting system — Sprint 1 backend complete
25f72c3 feat: Fermi Console Phase 0 — GPUI scaffold with Ayu Mirage dashboard
51e8683 fix: tethered creature rabble movement — reuse device flight
dfaf52a docs: ADR-011 Creature Cognition Economy
5fd5454 build: self-healing tether — preserve device flights, inherit rabble_id
```

### rabble (3 commits)
```
2717db2 feat: rabble SSE stream — real-time rabble_moved events update map position
8e61d9d feat: Brilliant Frame AR prototype — creatures + chat bubbles on glasses
```

---

## Architecture Diagrams

### Fermi Console Architecture
```
┌─────────────────────────────────────────────────────┐
│  Fermi Console (GPUI native app)                    │
│                                                     │
│  ┌───────────┐ ┌────────────┐ ┌──────────────────┐  │
│  │ Dashboard │ │ Portfolio  │ │ Research Cockpit │  │
│  │ (real API)│ │ (real API) │ │ (6-zone OODA)    │  │
│  └───────────┘ └────────────┘ └──────────────────┘  │
│                                                     │
│  Local: FPL parse + Monte Carlo sim + chart render  │
└────────────────────────┬────────────────────────────┘
                         │ HTTPS (API key auth)
┌────────────────────────▼────────────────────────────┐
│  ABW API (agent-bestiary.world)                     │
│  ├── 22 forecast/portfolio/leaderboard endpoints    │
│  ├── Agent execution (53 agents, multi-model)       │
│  ├── Brier scoring + materialized leaderboard       │
│  └── Teams + sharing                                │
└─────────────────────────────────────────────────────┘
```

### Research Cockpit Layout
```
┌──────────────────────────────────────────────────────┐
│  QUESTION HUB: "Will AMD reach $200 by 2026-12-31?" │
│  outside 35% ◄── [65%] ──► inside $187M             │
│                 divergence: +30pp                    │
├──────────┬──────────────┬──────────────┬─────────────┤
│ OUTSIDE  │  EVIDENCE    │  DRIVER MAP  │ AGENT FLEET │
│ VIEW     │  LANDSCAPE   │              │             │
│          │              │  drivers +   │  ● running  │
│ base rate│  ● bullish   │  model expr  │  ✓ done     │
│ ref class│  ● bearish   │  + sim       │  ○ idle     │
│ diverge  │  ◌ gaps      │  results     │  [assign]   │
├──────────┴──────────────┴──────────────┴─────────────┤
│  TIMELINE: ──●────●──────●───────●────── now         │
└──────────────────────────────────────────────────────┘
```

### Cognition Economy Model
```
CREATURE COGNITION = KNOWLEDGE × BANDWIDTH

Knowledge (earned, persistent):          Bandwidth (selected, upgradeable):
├── Embedding space                      ├── free (openrouter/free)
├── Knowledge graph (AKP)                ├── standard (Haiku)
├── Dream cycles (consolidation)         └── premium (Sonnet)
└── Coherence improvements

Agent capability gates (set by agent maker, based on evals):
├── Agent X feature A → requires "standard" minimum
├── Agent X feature B → available at "free"
└── Agent X feature C → requires "premium"
```
