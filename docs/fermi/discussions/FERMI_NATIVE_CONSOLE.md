# Fermi Native Console — Design Exploration

**Date:** 2026-02-27
**Status:** Exploration / Discussion
**Context:** The web notebook approach is choking. Zed + FPL + MCP already works for authoring. The question is: what does a discrete, standalone product look like?

---

## The Vision

An MMOG-style command center for competitive forecasting. Bloomberg Terminal meets Eve Online's market interface meets a research lab.

- Your **agents** are your fleet
- Your **forecasts** are your positions
- Your **Brier score** is your leaderboard rank
- Your **portfolio** is your track record

The player manages research agents, composes forecasts, publishes predictions, and watches their calibration improve over time — all from a native Rust desktop application with a dark, information-dense UI.

This is NOT a code editor. It's NOT a notebook. It's a **console** — purpose-built for the forecasting workflow.

---

## Why Not the Web Notebook

The current web notebook approach has several problems:

1. **Fragile frontend.** Flutter web compiled to JS, served from Vercel, talking to a Rust API. Multiple failure modes, slow iteration, build pipeline complexity.
2. **Wrong metaphor.** A notebook implies sequential cells, linear execution, data science workflow. Forecasting is more like portfolio management — you have many active positions, you monitor them, you update them, you track resolution.
3. **No offline capability.** Parsing, simulation, and report generation don't need a server. 10,000 Monte Carlo iterations take milliseconds in Rust. Why round-trip to an API?
4. **No native feel.** Web apps feel like web apps. A forecasting console should feel like a Bloomberg terminal — fast, dense, keyboard-driven, always-on.

## Why Not Just Zed

Zed + MCP + FPL already works for authoring forecasts. But:

1. **Zed is a code editor.** The UX is optimized for editing text files, not managing a portfolio of active forecasts.
2. **No persistent dashboard.** You can't see your Brier score, your active forecasts, your agent fleet status, your leaderboard position at a glance.
3. **No social layer.** Competitive forecasting needs leaderboards, shared forecasts, tournament brackets. Zed doesn't do that.
4. **FPL files are the engine, not the product.** Players shouldn't need to know FPL exists. The console generates and manages FPL under the hood.

Zed remains the power-user escape hatch — you can always drop into Zed to hand-edit FPL files, run the MCP tools directly, etc. But the console is the primary interface.

---

## FPL vs YAML: Resolution

In the context of a native console, **FPL earns its existence** more than in a web notebook:

| Dimension | FPL advantage | YAML advantage |
|---|---|---|
| Model expressions | Native parseable syntax: `model: a * b * (if c then 1.3 else 1.0)` | Opaque string field, needs separate expression parser anyway |
| Tree-sitter integration | Already built, works in Zed for power users | Every editor highlights YAML already |
| Tooling cost (sunk) | Parser, AST, grammar already exist | Would need to rebuild validation on serde structs |
| Console UX | Console generates FPL programmatically — user never sees it | Console could generate YAML equally well |
| Readability | Purpose-built, reads like a document | Familiar but verbose |

**Decision:** Keep FPL as the underlying format. The console generates and manages FPL files programmatically. Power users can edit FPL directly in Zed. The console doesn't expose FPL syntax to casual users — it presents a form-based UI for defining drivers, evidence, and models, and serializes to FPL behind the scenes.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Fermi Console (native Rust desktop app)                │
│                                                         │
│  ┌─────────────┐ ┌──────────────┐ ┌──────────────────┐  │
│  │ Dashboard   │ │ Portfolio    │ │ Agent Fleet      │  │
│  │ - Brier     │ │ - Active     │ │ - Status         │  │
│  │ - Streak    │ │ - Resolved   │ │ - Performance    │  │
│  │ - Rank      │ │ - Drafts     │ │ - Config         │  │
│  │ - Feed      │ │ - Shared     │ │ - Execution log  │  │
│  └─────────────┘ └──────────────┘ └──────────────────┘  │
│                                                         │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ Forecast Composer                                   │ │
│  │ - Question builder                                  │ │
│  │ - Driver editor (distributions, binary, discrete)   │ │
│  │ - Evidence panel (attach sources, relevance scores) │ │
│  │ - Model expression builder (visual + text)          │ │
│  │ - Simulation runner (local, instant)                │ │
│  │ - Results: histogram, tornado, sensitivity, sankey  │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                         │
│  ┌──────────────────┐ ┌────────────────────────────────┐ │
│  │ Leaderboard      │ │ Tournament / Challenge         │ │
│  │ - Global rank    │ │ - Active tournaments           │ │
│  │ - By domain      │ │ - My entries                   │ │
│  │ - Friends        │ │ - Results                      │ │
│  └──────────────────┘ └────────────────────────────────┘ │
└────────────────────────┬────────────────────────────────┘
                         │
          ┌──────────────┴──────────────┐
          │ What runs locally           │
          │ - FPL parsing & validation  │
          │ - Monte Carlo simulation    │
          │ - Report/chart generation   │
          │ - Agent card browsing       │
          │ - Portfolio state (SQLite)   │
          │ - Offline drafts            │
          └──────────────┬──────────────┘
                         │ HTTPS (only when needed)
          ┌──────────────┴──────────────┐
          │ What needs the server       │
          │ - Agent execution (LLM)     │
          │ - Brier scoring & resolution│
          │ - Public forecast registry  │
          │ - Leaderboard computation   │
          │ - Tournament management     │
          │ - Forecast sharing/collab   │
          │ - Authentication            │
          └─────────────────────────────┘
```

---

## UI Framework Options

For a Rust-native desktop app with an information-dense, dark-themed UI:

| Framework | Pros | Cons | Fit |
|---|---|---|---|
| **Clay** (C, Rust bindings) | 16.7k stars, microsecond layout, zero deps, renderer-agnostic, WASM support | C library with Rust bindings (FFI), no built-in widgets, need to build everything | ⭐⭐⭐ Best for custom MMOG-style UI |
| **egui** | Pure Rust, immediate mode, great for tools/dashboards, easy to start | Immediate mode can be limiting for complex layouts, not as polished for "product" UIs | ⭐⭐⭐ Fastest to prototype |
| **Iced** | Elm-architecture, pure Rust, good widget library, native feel | Slower development pace, less mature ecosystem | ⭐⭐ Good but slower to build with |
| **Tauri** | Web frontend (React/Svelte) + Rust backend, cross-platform | Still a web app in a wrapper — defeats the purpose | ⭐ Defeats the purpose |
| **GPUI** (Zed's framework) | What Zed itself uses, Rust-native, high performance | Not publicly stable, tightly coupled to Zed, limited docs | ⭐⭐ Interesting but risky |
| **Slint** | Declarative UI, Rust-native, good for embedded/desktop | Licensing complexity, smaller community | ⭐⭐ Worth evaluating |

**Initial recommendation:** Start with **egui** for rapid prototyping. It's pure Rust, immediate mode (fast iteration), has built-in support for plots/charts (via egui_plot), and can produce the information-dense dark UI you want. If the UI needs to become more polished/custom later, consider migrating to Clay + a custom renderer.

**Alternative path:** If the MMOG aesthetic is paramount from day one, Clay with a wgpu renderer gives you complete control over every pixel, but at the cost of building every widget from scratch.

---

## Console Panels (Detail)

### 1. Dashboard

The home screen. At-a-glance view of your forecasting life.

- **Brier Score** — current calibration score, trend sparkline, percentile rank
- **Active Forecasts** — count, nearest resolution dates, alerts
- **Agent Fleet** — agents online/offline, recent executions, cost burn rate
- **Feed** — recent activity (forecasts resolved, agents executed, leaderboard changes)
- **Streak** — consecutive days with forecast activity (gamification)

### 2. Portfolio

All your forecasts, organized and filterable.

- **Active** — open forecasts awaiting resolution
- **Resolved** — past forecasts with Brier scores, color-coded by calibration
- **Drafts** — work-in-progress forecasts (local, not published)
- **Shared** — forecasts you've published or shared with others
- **By Domain** — filter by topic (tech, economics, geopolitics, etc.)

Each forecast card shows: question, current probability, confidence interval, resolution date, contributing agents, Brier score (if resolved).

### 3. Agent Fleet

Your research agents and their status.

- **Agent Cards** — browse all available agents, see capabilities, performance stats
- **My Agents** — agents you've hired/configured, their execution history
- **Execution Log** — recent agent runs with model used, tokens consumed, cost, output quality
- **Configuration** — model ladder, capability gates, cognition tier (ties into ADR-011)
- **Performance** — per-agent accuracy, Brier impact, cost efficiency

### 4. Forecast Composer

The main authoring experience. NOT a text editor — a structured form.

- **Question Builder** — natural language question, resolution criteria, target date, domain
- **Driver Editor** — add continuous/binary/discrete drivers with visual distribution pickers (drag sliders for triangular p5/p50/p95, etc.)
- **Evidence Panel** — attach evidence with source, summary, relevance score, date
- **Agent Assignment** — assign research agents to drivers, schedule execution
- **Model Builder** — visual expression builder (drag drivers into a formula) OR text mode for power users
- **Simulation** — run Monte Carlo locally, see results instantly
- **Results** — histogram, tornado chart, sensitivity analysis, sankey flow, all rendered natively

Under the hood, the composer generates an FPL file. Power users can toggle "source view" to see/edit the FPL directly.

### 5. Leaderboard

Competitive forecasting rankings.

- **Global** — all-time Brier score ranking
- **By Domain** — rankings within specific topics
- **Friends** — compare with people you follow
- **Seasonal** — monthly/quarterly rankings with resets
- **Calibration Curve** — visual showing how well-calibrated you are across confidence levels

### 6. Tournaments

Structured forecasting competitions.

- **Active Tournaments** — join, submit forecasts, track standings
- **My Entries** — forecasts submitted to tournaments
- **Results** — past tournament outcomes, prizes, rankings
- **Create** — host your own tournament (premium feature)

---

## Local vs Server Split

### Runs Locally (instant, offline-capable)

| Capability | Implementation |
|---|---|
| FPL parsing & validation | Existing `parser.rs` + `ast.rs` |
| Monte Carlo simulation | Existing `executor.rs` — 10k iterations in <100ms |
| Chart generation | egui_plot or custom rendering |
| Report generation | Existing Mermaid + markdown pipeline |
| Agent card browsing | Bundled agent cards (sync from server periodically) |
| Portfolio state | Local SQLite database |
| Offline drafts | FPL files on disk + SQLite metadata |
| Sensitivity analysis | Local computation |

### Requires Server (ABW API)

| Capability | Endpoint |
|---|---|
| Agent execution | `POST /api/agents/:id/execute` (LLM calls) |
| Forecast publishing | `POST /api/forecasts` (public registry) |
| Brier scoring | `POST /api/forecasts/:id/resolve` (resolution oracle) |
| Leaderboard | `GET /api/leaderboard` |
| Tournament management | `/api/tournaments/*` |
| Forecast sharing | `/api/forecasts/:id/share` |
| Authentication | `/api/auth/*` |
| Agent card sync | `GET /api/agents` (periodic sync to local cache) |

---

## Data Model (Local SQLite)

```sql
-- Local portfolio database
CREATE TABLE forecasts (
    id TEXT PRIMARY KEY,
    question TEXT NOT NULL,
    fpl_source TEXT,              -- the FPL file content
    current_probability REAL,
    confidence_low REAL,
    confidence_high REAL,
    resolution_date TEXT,
    domain TEXT,
    status TEXT DEFAULT 'draft',  -- draft | active | resolved
    brier_score REAL,             -- NULL until resolved
    actual_outcome INTEGER,       -- NULL until resolved
    server_id TEXT,               -- NULL if not published
    created_at TEXT,
    updated_at TEXT
);

CREATE TABLE simulation_results (
    id TEXT PRIMARY KEY,
    forecast_id TEXT REFERENCES forecasts(id),
    iterations INTEGER,
    mean REAL,
    median REAL,
    p5 REAL,
    p25 REAL,
    p75 REAL,
    p95 REAL,
    std_dev REAL,
    run_at TEXT
);

CREATE TABLE agent_executions (
    id TEXT PRIMARY KEY,
    forecast_id TEXT REFERENCES forecasts(id),
    agent_id TEXT,
    model_used TEXT,
    tier TEXT,
    tokens_used INTEGER,
    cost_usd REAL,
    execution_time_ms INTEGER,
    output_summary TEXT,
    executed_at TEXT
);

CREATE TABLE agent_cache (
    agent_id TEXT PRIMARY KEY,
    card_json TEXT,
    synced_at TEXT
);

CREATE TABLE user_profile (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

---

## Keyboard-Driven UX

The console should be keyboard-first, like a Bloomberg terminal or Vim.

| Key | Action |
|---|---|
| `Ctrl+N` | New forecast |
| `Ctrl+P` | Open portfolio |
| `Ctrl+A` | Agent fleet |
| `Ctrl+L` | Leaderboard |
| `Ctrl+R` | Run simulation (in composer) |
| `Ctrl+Enter` | Publish forecast |
| `Tab` | Cycle panels |
| `/` | Command palette (fuzzy search everything) |
| `Ctrl+S` | Save draft |
| `Ctrl+E` | Toggle FPL source view |
| `?` | Help / keyboard shortcuts |

---

## Relationship to Existing Infrastructure

```
Fermi Console (new)          Zed + MCP (existing)
     │                            │
     │  Both talk to:             │
     │                            │
     └──────────┬─────────────────┘
                │
     ┌──────────▼──────────┐
     │   ABW API Server    │
     │  (agent-bestiary.   │
     │   world)            │
     │                     │
     │  - Agent execution  │
     │  - Brier scoring    │
     │  - Leaderboards     │
     │  - Tournaments      │
     │  - Public forecasts │
     └─────────────────────┘
```

The console and Zed are complementary:
- **Console** = the product UX for forecasters (portfolio, leaderboard, tournaments)
- **Zed** = the power-user/developer UX for agent makers and FPL authors
- **Both** share the same API, same agent system, same FPL format

---

## Implementation Phases

### Phase 0: Spike (1-2 days)
- [ ] egui "hello world" with Ayu Mirage theme
- [ ] Render a single FPL file's simulation results as a histogram
- [ ] Prove the local parse → simulate → render loop works in <100ms

### Phase 1: Core Console (1-2 weeks)
- [ ] Dashboard panel with mock data
- [ ] Portfolio panel with local SQLite
- [ ] Forecast composer with driver editor + simulation
- [ ] FPL generation from composer form
- [ ] Chart rendering (histogram, tornado)

### Phase 2: Server Integration (1 week)
- [ ] Authentication (API key or OAuth)
- [ ] Agent execution via API
- [ ] Forecast publishing
- [ ] Brier score retrieval
- [ ] Agent card sync

### Phase 3: Social Layer (1 week)
- [ ] Leaderboard panel
- [ ] Shared forecasts
- [ ] Calibration curve visualization
- [ ] Friend/follow system

### Phase 4: Tournaments (future)
- [ ] Tournament browsing and entry
- [ ] Tournament-specific leaderboards
- [ ] Tournament creation (premium)

### Phase 5: Polish (ongoing)
- [ ] Keyboard shortcuts throughout
- [ ] Command palette
- [ ] Animations and transitions
- [ ] Custom themes
- [ ] Export/import portfolios

---

## Open Questions

1. **egui vs Clay?** egui is faster to prototype but Clay gives more visual control. Start with egui, migrate if needed?
2. **Local-first sync model?** How do local drafts sync with the server? CRDTs? Last-write-wins? Manual publish?
3. **FPL visibility?** Should the console ever show raw FPL, or is it always behind the form UI? (Recommendation: toggle-able source view for power users)
4. **Monetization?** Free tier with local-only simulation, paid tier for agent execution + publishing + tournaments?
5. **Cross-platform?** egui compiles to Windows/Mac/Linux. Do we care about mobile? (Probably not — forecasting is a desktop activity)
6. **Branding?** Is this "Fermi Console"? "Fermi Terminal"? "Fermi Station"? Something else entirely?

---

## References

- Clay UI library: https://github.com/nicbarker/clay (16.7k stars, C with Rust bindings)
- egui: https://github.com/emilk/egui (pure Rust, immediate mode)
- Existing FPL parser: `src/parser.rs`, `src/ast.rs`
- Existing executor: `src/executor.rs`
- Existing MCP server: `src/bin/agent-mcp-server.rs`
- Existing notebook API: `src/handlers/notebooks.rs`
- Ayu Mirage theme colors: defined in `docs/fermi/guides/fpl-reference.md`
- ADR-011 (Creature Cognition Economy): model ladder and agent tier system applies to agent execution costs in the console

---

## Revision History

- **2026-02-27:** Initial exploration document