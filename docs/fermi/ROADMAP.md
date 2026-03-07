# Fermi Forecasting Platform — Master Roadmap

**Project:** Fermi — Probabilistic Forecasting IDE + Agent Platform  
**Version:** 0.9.0  
**Last Updated:** 2026-03-07  
**Current Focus:** Console Polish & Agent Presence  

---

## Vision

Fermi is a **forecasting command center** — a native desktop application where probabilistic reasoning is the primary activity. It combines:

- **FPL** (Forecasting Programming Language) as the computational backbone
- **AI agents** as tireless research assistants bound to forecast drivers
- **Tetlock methodology** (base rates, reference classes, calibration) as the intellectual framework
- **Visual composition** that hides FPL complexity while delivering its full power

The console is an MMOG-style cockpit for non-programmers. Power users can always fall back to Zed + MCP for direct FPL editing.

---

## Architecture Summary

```
┌─────────────────────────────────────────────────────┐
│  Fermi Console (GPUI native, Rust)                  │
│                                                     │
│  ┌───────────┐ ┌────────────┐ ┌──────────────────┐  │
│  │ Dashboard │ │ Portfolio  │ │ Research Cockpit │  │
│  │ (real API)│ │ (local+API)│ │ (6-zone OODA)    │  │
│  └───────────┘ └────────────┘ └──────────────────┘  │
│                                                     │
│  Local: FPL parse + Monte Carlo sim + chart render  │
│  Local: AgentRegistry + LLMExecutor (agent runs)    │
│  Persist: .fpl + .evidence.md + .state.json         │
└────────────────────────┬────────────────────────────┘
                         │ HTTPS (API key auth)
┌────────────────────────▼────────────────────────────┐
│  ABW API (agent-bestiary.world)                     │
│  ├── Auth (Google/GitHub OAuth)                     │
│  ├── Forecast CRUD + portfolio + leaderboard        │
│  ├── 53 agents (5 fermi-orchestra)                  │
│  ├── Brier scoring + materialized leaderboard       │
│  └── MCP server for Zed integration                 │
└─────────────────────────────────────────────────────┘
```

**Key architectural decisions:**
1. **FPL Program AST is the source of truth** — the UI reads/writes the AST, serializable to valid FPL text
2. **Local agent execution** — agents run locally via `AgentRegistry` + `LLMExecutor`, not via the ABW API
3. **ABW API for cloud features only** — auth, forecast CRUD, portfolio, leaderboard
4. **Three artifacts per forecast**: `.fpl` (program), `.evidence.md` (wiki), `.state.json` (versions, probability, confidence, agents, evidence)
5. **Deterministic probability normalization**: `P = base_rate × (sim_mean / baseline_mean)`
6. **Plotters for visualization** — renders to pixel buffers displayed in GPUI as images
7. **Governance model** — only Fermi fires on initial question; users explicitly assign agents to drivers

---

## Current State (as of 2026-03-07)

### What Exists and Works

| Component | Status | Lines | Tests |
|-----------|--------|-------|-------|
| FPL Core Engine (lexer, parser, semantic, executor) | ✅ Complete | ~4,950 | 59 |
| Active Dreaming Memory (Phases 0–5) | ✅ Complete | ~3,500 | 25 |
| ABW API (agents, forecasts, portfolio, leaderboard) | ✅ Complete | ~6,000 | — |
| MCP Server (Zed integration) | ✅ Complete | ~1,200 | — |
| Fermi Console (GPUI native app) | ✅ Functional | ~13,500 | — |
| Agent Bestiary (53 agents, 5 fermi-orchestra) | ✅ Complete | — | — |
| tree-sitter FPL grammar | ✅ Complete | ~400 | — |
| FPL Language Server | ✅ Basic | ~800 | — |

### Console Feature Matrix (98 commits, March 1–6)

| Feature | Status | Notes |
|---------|--------|-------|
| Research Cockpit (6-zone OODA layout) | ✅ | Question hub, outside view, evidence, drivers, agents, timeline |
| Question → Fermi orchestration | ✅ | Seeds decomposition, base rate, reference class, drivers |
| Agent → driver assignment with custom query | ✅ | Per-driver agent picker with schedule (once/daily/weekly) |
| Entity channel integration | ✅ | `cx.spawn()` + `WeakEntity` callback, live UI updates |
| Editable driver parameters | ✅ | p5/p50/p95, distribution type, binary probability, accept/remove |
| ⌘R local simulation | ✅ | FPL generation → parse → Executor (10K iterations <100ms) |
| ⌘P publish flow | ✅ | Async POST with full cockpit state serialization |
| Interactive probability slider | ✅ | Drag + nudge buttons, divergence warning at >20pp |
| Native menus + window management | ✅ | ⌘N/⌘R/⌘P/⌘E/⌘1-5, traffic lights, fullscreen |
| Tabbed right panel (Edit / FPL / Wiki) | ✅ | Sprint 1 complete |
| Fermi meta-agent | ✅ | Tetlock methodology, agent recommendations, distribution validation |
| Sensitivity analysis (Sobol indices) | ✅ | Driver ranking by influence |
| Inside view explanation | ✅ | Narrative interpretation with confidence + sensitivity data |
| Deterministic probability normalization | ✅ | `base_rate × (sim_mean / baseline_mean)` |
| Plotters: histogram, index chart, treemap | ✅ | Renders to pixel buffers for GPUI |
| Distribution sparklines on driver cards | ✅ | Inline mini-charts |
| Per-driver confidence dots | ✅ | Visual evidence quality |
| Portfolio with rich cards | ✅ | Probability, base rate comparison, confidence badge, mini sparklines |
| Full state persistence | ✅ | FPL + evidence.md + state.json, agent assignments survive reload |
| Version history with probability diffs | ✅ | In Wiki tab, shows evolution |
| Manual evidence entry | ✅ | Source + summary per driver |
| OAuth sign-in (Google/GitHub) | ✅ | Localhost callback flow |
| Agent Fleet panel | ✅ | fermi-orchestra agents + leaderboard |
| Forecast lifecycle (create/archive/delete) | ✅ | Import FPL (⌘O), Ctrl+S save |

### Known Issues (Critical)

| Issue | Impact | Root Cause | Status |
|-------|--------|------------|--------|
| Evidence sometimes lost on reload | High | Agent-attributed evidence not always written to state.json in certain flows | ✅ Fixed |
| Agent state indicators out of sync | Medium | `Completed`/`Idle` derived from evidence count, not actual execution state | ✅ Fixed — tracks `started_at`/`completed_at` |
| Inside view not always at top of Wiki | Low | Wiki generation ordering inconsistent across save paths | Open |
| Confidence is derived/hidden | Medium | Should be user-settable per driver, not just computed | ✅ Fixed — user-settable per driver |

---

## Forward Roadmap

### Phase 7: Agent Presence & Evidence Richness (Current — Weeks 1–2)

*Goal: Make agents feel alive and make evidence the centerpiece of the workflow.*

#### 7A: Agent Presence (pixel-agents inspiration)

- [x] Real-time agent state indicators (researching → found → idle → error) — colored borders, status icons
- [ ] Agent activity animation/pulse when working (subtle glow or spinner)
- [x] Agent "speech bubbles" showing latest finding — cached `latest_finding` field, falls back to evidence search
- [ ] Agent cards in fleet tab show current assignment and live status
- [x] Visual feedback that complex work is happening — elapsed time display (`12s…` while running, `8s` when done)
- [ ] Agent completion notification (toast or sound)
- [x] Retry/Re-run button on failed and completed agents
- [x] Error details shown inline for failed agents (truncated to 120 chars)
- [x] Credits charged display per agent (⚡ icon)
- [x] Enhanced status bar — running/completed/failed counts, gap count, colored indicators

#### 7B: Evidence Richness

- [ ] Distribution curve explanation per driver (how evidence justifies the shape)
- [x] Confidence per driver is **user-settable** — `editor_confidence` input (0–100%), `driver_confidence` HashMap, persisted in state.json
- [x] Computed vs user-set confidence shown side-by-side with delta on driver cards
- [x] Overall forecast confidence now averages per-driver user overrides
- [ ] Evidence hyperlinks with URL previews
- [ ] URL auto-summarization when adding evidence links
- [ ] Inside/outside view comparison sparklines in portfolio cards
- [x] Evidence gap highlighting — drivers with no agents get red border + warning badge, "Awaiting evidence" for assigned-but-empty
- [x] Running agents give driver card a gold border

#### 7C: Critical Bug Fixes

- [x] Fix evidence persistence on all reload paths (agent-attributed evidence) — fixed prior to this session
- [x] Fix agent state sync — `AgentExecution` now tracks `started_at`, `completed_at`, `latest_finding`
- [ ] Fix inside view ordering in Wiki tab
- [ ] Clean up FPL string sanitization edge cases

**Deliverable:** Agents feel present. Evidence is rich and trustworthy. No data loss.

---

### Phase 8: Agent Scheduling & Coordination (Weeks 3–4)

*Goal: Agents work autonomously on schedules, not just on-demand.*

- [ ] Execute scheduled agents (daily/weekly cron loop in background)
- [ ] Trigger-based execution (divergence threshold → re-research)
- [ ] Fermi suggests when to re-research based on evidence staleness
- [ ] Background agent execution with desktop notifications
- [ ] Auto-update evidence wiki when agents complete
- [ ] Agent cost tracking per forecast (total credits consumed)
- [ ] Batch agent execution (run all assigned agents at once)

**Deliverable:** Agents maintain forecasts autonomously. Evidence stays fresh.

---

### Phase 9: Version Management & Diff (Weeks 5–6)

*Goal: Full version control for forecasts with visual diffs.*

- [ ] Version diff view (what changed between snapshots — probability, drivers, evidence)
- [ ] Rollback to previous version with confirmation
- [ ] Version comparison side-by-side
- [ ] Git-based version control for FPL files (auto-commit on save)
- [ ] Version timeline is clickable (click to inspect, double-click to restore)
- [ ] Export version history as report (PDF/Markdown)

**Deliverable:** Full audit trail. Every probability change is explainable.

---

### Phase 10: Portfolio Management & Scoring (Weeks 7–8)

*Goal: Organize forecasts into portfolios with calibration tracking.*

- [ ] Group forecasts into named portfolios (drag-and-drop or tag-based)
- [ ] Compare forecasts within a portfolio (overlay distributions)
- [ ] Portfolio-level Brier scoring (aggregate calibration)
- [ ] Forecast lifecycle affordances: draft → active → mature → resolved → archived
- [ ] Resolution mechanism (enter actual outcome, compute Brier score)
- [ ] Calibration chart (predicted probability vs. actual frequency)
- [ ] Personal calibration stats (overconfidence bias, domain strengths)
- [ ] Portfolio dashboard with aggregate probability distributions

**Deliverable:** Users can track forecasting skill over time. Portfolios are first-class.

---

### Phase 11: Collaboration Foundation (Weeks 9–12)

*Goal: Shared forecasts and team forecasting.*

- [ ] Share forecast (get link, read-only view)
- [ ] Shared forecast editing (simple lock-based, not CRDT yet)
- [ ] Team probability consensus (each member sets their own, see distribution)
- [ ] Tournament creation and participation
- [ ] Tournament leaderboard with Brier scoring
- [ ] Real-time tournament updates (WebSocket)
- [ ] Forecast comments and annotations
- [ ] Activity feed (who changed what, when)

**Deliverable:** Multi-user forecasting. Competitive tournaments.

---

### Phase 12: Intelligence Features (Weeks 13–16)

*Goal: The system actively improves forecast quality.*

- [ ] Evidence gap detection (automatic — "No evidence for driver X in 14 days")
- [ ] Contradiction detection (conflicting evidence highlighted)
- [ ] Calibration feedback ("Your forecasts in this domain tend to be overconfident by 8%")
- [ ] Base rate divergence feedback ("When you diverge >20pp, you're right 40% of the time")
- [ ] Multiple reference classes (show 2–3 candidate base rates, let user choose)
- [ ] Cross-forecast evidence sharing (evidence from one forecast relevant to another)
- [ ] Divergence portfolio analysis ("Your best Brier scores come from forecasts within 15pp of base rate")
- [ ] Fermi proactive nudges ("Driver X hasn't been updated in 30 days — assign an agent?")

**Deliverable:** Forecasting skill improves through software feedback, not just practice.

---

### Phase 13: Polish, Performance & Distribution (Weeks 17–18)

*Goal: Ship-quality application.*

- [ ] Theme refinement (colors, typography, spacing, dark/light)
- [ ] Keyboard navigation improvements (Tab between zones, arrow keys in lists)
- [ ] Text wrapping fixes (GPUI `min_w(px(0.0))` pattern everywhere)
- [ ] Error handling and edge case hardening
- [ ] Performance optimization (large portfolios, many drivers)
- [ ] Startup time optimization (lazy loading, cached state)
- [ ] Release build + code signing (macOS, Linux)
- [ ] Auto-update mechanism
- [ ] Crash reporting (opt-in)
- [ ] Onboarding flow (first-run tutorial, sample forecast)

**Deliverable:** Installable, stable application ready for external users.

---

### Phase 14: Advanced Visualization (Weeks 19–20)

*Goal: Bloomberg-terminal-grade information density.*

- [ ] Force-directed evidence landscape (replace list with graph, using `fdg-sim` crate)
- [ ] Driver dependency graph with sparklines
- [ ] Animated probability indicator with history trace
- [ ] Evidence treemap with drill-down (click driver → see evidence items)
- [ ] Tornado chart for sensitivity analysis (horizontal bar chart)
- [ ] Fan chart (confidence bands over time)
- [ ] Mermaid ER diagram viewer for agent ontologies
- [ ] Exportable charts (PNG, SVG)

**Deliverable:** Information-dense visualizations that reward expert attention.

---

### Phase 15: Active Dreaming Memory Integration (Weeks 21–24)

*Goal: Agents learn from past forecasts and improve over time.*

The ADM system (Phases 0–5 complete) needs to be wired into the console workflow:

- [ ] Phase 6: Mermaid ontology generation from semantic memory
- [ ] Phase 7: Git integration (auto-commit ontology changes)
- [ ] Phase 8: Production deployment (consolidation worker on schedule)
- [ ] Agent memory cards (show what an agent has learned from past runs)
- [ ] Cross-forecast knowledge transfer (evidence from resolved forecasts informs new ones)
- [ ] Dream cycle visualization (what the agent consolidated overnight)
- [ ] Memory-informed agent suggestions ("Based on past runs, sentiment_analyzer works best for tech stocks")

**Deliverable:** Agents accumulate institutional knowledge. Forecasting improves with use.

---

### Phase 16: Mobile & Multi-Platform (Future — TBD)

*Goal: Forecasting on the go.*

- [ ] View forecasts (read-only) on mobile (Rabble integration or PWA)
- [ ] Agent management from mobile (trigger research, approve results)
- [ ] Tournament participation on mobile
- [ ] Push notifications (agent completed, divergence alert, tournament deadline)
- [ ] Light editing (probability slider, evidence notes)
- [ ] Responsive web dashboard (portfolio view, leaderboard)

**Deliverable:** Mobile companion for the desktop command center.

---

## Completed Phases (Archive)

### ✅ Phase 0: FPL Core Engine (v0.4.0, January 2026)

- Lexer (900 lines, 13 tests)
- Parser (850 lines, 8 tests)
- Semantic Analyzer (1,020 lines, 12 tests)
- Execution Engine (1,330 lines, 26 tests)
- **Total:** ~4,950 lines, 59/59 tests passing

### ✅ Phase 1: FPL Language Experience (v0.5.0, February 2026)

- FPL Language Server (tower-lsp, basic diagnostics)
- tree-sitter grammar for syntax highlighting
- Zed extension with inline diagnostics
- Execute command, basic results panel
- Hover information, autocompletion

### ✅ Phase 2: Agent Bestiary & Backend (v0.6.0, February 2026)

- 53 agents across 5 tiers (curated → system)
- AgentRegistry with tag-based discovery
- LLMExecutor (Claude Haiku) + ToolAwareExecutor
- MCP server for Zed integration
- ABW API: 122 routes, auth, forecasts, portfolio, leaderboard

### ✅ Phase 3: Active Dreaming Memory (v0.7.0, February 2026)

- Phase 0: Database + fermi-memory crate
- Phase 1: Vector search (pgvector) + DBSCAN clustering
- Phase 2: Distributed locking + job tracking
- Phase 3: Semantic memory (rules, entities, facts, bi-temporal)
- Phase 4: Consolidation workflow (automated 9-step process)
- Phase 5: Multi-provider LLM (Anthropic, Mistral, Qwen, OpenRouter)
- **Total:** ~3,500 lines, 25 tests, 12 PostgreSQL tables

### ✅ Phase 4: ABW API Sprint 1 (v0.8.0, February 2026)

- Forecast CRUD (create, read, update, list, archive, delete)
- Portfolio management (create, add/remove forecasts, list)
- Leaderboard (materialized view, Brier scoring)
- Teams and sharing
- OAuth authentication (Google, GitHub)

### ✅ Phase 5: Fermi Console — GPUI Scaffold (late February 2026)

- GPUI native application shell
- Dashboard with live API data
- Sidebar navigation (Dashboard, Portfolio, Fleet, Composer, Leaderboard)
- OAuth sign-in flow with localhost callback
- API client for ABW endpoints

### ✅ Phase 6: Research Cockpit — Full Build (v0.9.0, March 1–6, 2026)

*The marathon sprint: 98 commits, ~13.5K lines, 6 days.*

**Cockpit & Composition:**
- Six-zone OODA layout (question hub, outside view, evidence, drivers, agents, timeline)
- Question input → Fermi orchestrates outside view + inside view decomposition
- Outside View card (base rate, reference class, reasoning, update button) with probability multipliers
- Agents as visible entities on driver cards (status, findings count, speech bubbles)
- Agent picker with fermi-orchestra agents, custom queries, scheduling
- Manual evidence entry per driver
- Monte Carlo simulation with deterministic normalization
- Sensitivity analysis (Sobol indices)
- Inside view explanation with confidence scoring
- Forecast Index (mean, CI, divergence)

**Visualization:**
- Plotters histogram, index comparison chart, evidence treemap
- Distribution sparklines on driver cards
- Per-driver confidence dots
- Mini index sparklines in portfolio cards

**Right Panel (tabbed):**
- Edit tab: driver editor, evidence per driver, add evidence form
- FPL tab: live-generated FPL source
- Wiki tab: evidence organized by driver, version history with probability diffs

**Persistence:**
- Save: FPL + evidence.md + state.json (Ctrl+S)
- Load from Portfolio (click) — robust loading that survives bad FPL strings
- state.json stores: probability, confidence, versions, base rate, evidence, agents
- Agent assignments persist and restore with evidence linking
- FPL string sanitization for round-trip parsing

**Portfolio & Navigation:**
- Rich portfolio cards (probability, base rate comparison, confidence badge, mini sparklines)
- Load forecasts from disk with version/driver/evidence counts
- New forecast (Ctrl+N), Import FPL (Ctrl+O), Archive/Delete lifecycle
- OAuth sign-in (Google/GitHub)
- Agent Fleet tab with fermi-orchestra team + Leaderboard

**Agent System:**
- 5 fermi-orchestra agents: fermi, macro_forecaster, market_research, sentiment_analyzer, entity_investigator
- Fermi meta-agent seeds decomposition, recommends agents, validates distributions, interprets results
- Unique agent-driver bindings (same agent can research multiple drivers with different queries)
- Agent state sync on reload (Completed/Idle based on evidence count)

---

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Console UI | GPUI (Zed's GPU-accelerated framework) |
| Language | Rust (100% native) |
| Charts | plotters → pixel buffers → GPUI images |
| FPL Engine | Custom lexer/parser/executor, tree-sitter grammar |
| Agent Runtime | AgentRegistry + LLMExecutor (local) |
| LLM Providers | Anthropic (Claude), Mistral, Qwen, OpenRouter |
| API Backend | axum (Rust), PostgreSQL (Neon), pgvector |
| Auth | OAuth2 (Google, GitHub), API key |
| Memory | fermi-memory crate (episodic + semantic + consolidation) |
| Version Control | Git (manual, future: auto-commit) |
| MCP | Model Context Protocol server for Zed |

---

## Key Metrics

| Metric | Target | Current |
|--------|--------|---------|
| FPL tests passing | 59 | ✅ 59 |
| ADM tests passing | 25 | ✅ 25 |
| Console code | — | ~13,500 lines |
| Agents available | 53 | ✅ 53 (5 fermi-orchestra) |
| Simulation speed (10K iterations) | <100ms | ✅ <100ms |
| LSP diagnostics | <50ms | ✅ ~40ms |
| Saved forecasts (test) | — | 13 with evidence wikis |
| Commits (console sprint) | — | 98 |

---

## Versioning

| Version | Milestone | Status |
|---------|-----------|--------|
| v0.4.0 | FPL Core Engine | ✅ Complete |
| v0.5.0 | FPL Language Experience (LSP + Zed) | ✅ Complete |
| v0.6.0 | Agent Bestiary & Backend | ✅ Complete |
| v0.7.0 | Active Dreaming Memory (Phases 0–5) | ✅ Complete |
| v0.8.0 | ABW API Sprint 1 | ✅ Complete |
| v0.9.0 | Fermi Console (Research Cockpit) | ✅ Complete |
| v0.10.0 | Agent Presence & Evidence Richness | 🔄 Next |
| v0.11.0 | Scheduling & Coordination | Planned |
| v0.12.0 | Version Management | Planned |
| v0.13.0 | Portfolio Management & Scoring | Planned |
| v1.0.0 | Polish & Public Release | Planned |
| v1.1.0 | Collaboration & Tournaments | Planned |
| v2.0.0 | Intelligence Features | Planned |

---

## Decision Log

| Decision | Date | Rationale |
|----------|------|-----------|
| GPUI over egui/Clay | 2026-02-27 | Zed's framework, GPU-accelerated, CRDT potential |
| FPL stays as backbone | 2026-02-27 | Console is a visual FPL editor, not a replacement |
| Outside view first | 2026-02-28 | Tetlock discipline: anchor to base rate before inside view |
| Local agent execution | 2026-03-01 | Same as MCP server; ABW API only for cloud features |
| Plotters for charts | 2026-03-03 | Renders to pixel buffers; no WebView dependency |
| Three artifacts per forecast | 2026-03-03 | .fpl (program) + .evidence.md (wiki) + .state.json (state) |
| Governance model (user assigns agents) | 2026-03-03 | Controls costs; only Fermi auto-fires |
| Deterministic normalization | 2026-03-04 | `P = base_rate × (sim_mean / baseline_mean)` is stable and interpretable |
| No creature integration (yet) | 2026-03-05 | Rabble is a separate world; forecasting is desktop-only |

---

## Risk Register

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Evidence persistence bugs | High | Medium | Hardened state.json writes; add integration tests |
| GPUI limitations (no file dialogs, text wrapping) | Medium | Known | Use `rfd` crate; `min_w(px(0.0))` pattern |
| Agent response parsing fragility | Medium | Medium | `clean_fpl_string()` + markdown fence stripping |
| LLM costs at scale | Medium | Low | Governance model; user controls agent assignments |
| Single-developer bus factor | High | High | Comprehensive documentation; clean architecture |
| GPUI breaking changes (Zed upstream) | Medium | Low | Pin GPUI version; track Zed releases |

---

## Project Structure

```
fermi/
├── crates/fermi-console/src/
│   ├── main.rs         (~2,900 lines) — app shell, panels, auth, portfolio
│   ├── cockpit.rs      (~4,200 lines) — composer, FPL AST, agents, simulation
│   ├── charts.rs       (~250 lines)   — plotters rendering
│   ├── text_input.rs   (~736 lines)   — editable text fields
│   ├── cockpit_old.rs  (~968 lines)   — legacy composer (superseded)
│   └── api/client.rs   (~970 lines)   — ABW API client
├── src/                                — FPL core (lexer, parser, semantic, executor)
├── fermi-memory/                       — ADM system (episodic, semantic, consolidation)
├── fermi-lsp/                          — FPL Language Server
├── agents/curated/                     — 53 agent cards (5 fermi-orchestra)
├── extensions/fermi/                   — Zed extension
├── forecasts/                          — saved .fpl + .evidence.md + .state.json
├── docs/fermi/                         — all documentation
│   ├── ROADMAP.md                      — this file
│   ├── discussions/                    — design documents
│   ├── sessions/                       — session notes
│   ├── architecture/                   — whitepaper, ADM design
│   └── decisions/                      — ADRs
└── api/                                — Vercel serverless functions
```

---

## Quick Reference

```bash
# Run the console
cd /home/ilabra/fermi
cargo run -p fermi-console

# Run tests
cargo test --workspace

# Run just FPL tests
cargo test --lib

# Run memory tests
cargo test --package fermi-memory

# Environment variables
export ANTHROPIC_API_KEY=sk-ant-...    # Required for agents
export FERMI_API_KEY=...               # For ABW API access
```

---

## References

- [Console MVP Architecture](discussions/CONSOLE_MVP_ARCHITECTURE.md) — data flows, three artifacts, sprint progress
- [Research Cockpit Design](discussions/RESEARCH_COCKPIT.md) — OODA loop UX, six zones, interaction flows
- [Console Redesign](discussions/CONSOLE_REDESIGN.md) — FPL-native visual editor design
- [Fermi Native Console](discussions/FERMI_NATIVE_CONSOLE.md) — initial GPUI exploration
- [Architecture Whitepaper](architecture/whitepaper.md) — FPL language + Tetlock methodology
- [ADM Implementation Roadmap](ROADMAP_ADM_IMPLEMENTATION.md) — memory system phases
- [Module Architecture](roadmap/MODULE_ARCHITECTURE.md) — original 10-module plan
- [Marathon Session Notes](sessions/SESSION_2026-02-27_28_MARATHON.md) — the big build sprint
- [Project Rules](PROJECT_RULES.md) — context management, workflows

---

**Next Review:** After Phase 7 completion  
**Contact:** Replicant Partners  
**Repository:** https://github.com/Replicant-Partners/fermi