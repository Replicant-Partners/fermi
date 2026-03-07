# Fermi Project Status

**Last Updated:** 2026-03-07
**Version:** 0.9.0
**Current Phase:** Phase 7 — Agent Presence & Evidence Richness

---

## 🎯 Current Status

### Fermi Console (GPUI Native App) — v0.9.0

The Research Cockpit is **functional and feature-rich** after a marathon 6-day sprint (March 1–6, 2026) producing 98 commits and ~13,500 lines of Rust code. The console is a visual FPL editor with a six-zone OODA loop layout, local agent execution, Monte Carlo simulation, plotters-based visualization, and full state persistence.

**Current focus:** Bug fixes (evidence persistence, agent state sync) and agent presence/liveness features.

---

## ✅ What's Working

### FPL Core Engine (v0.4.0)
- Lexer, Parser, Semantic Analyzer, Execution Engine
- Monte Carlo simulation (10K iterations <100ms)
- 59 core tests passing

### FPL Language Experience (v0.5.0)
- FPL Language Server (tower-lsp, basic diagnostics)
- tree-sitter grammar for syntax highlighting
- Zed extension with inline diagnostics, hover, autocomplete

### Agent Bestiary & Backend (v0.6.0)
- 53 agents across 5 tiers (5 fermi-orchestra)
- AgentRegistry + LLMExecutor (Claude Haiku)
- MCP server for Zed integration
- ABW API: 122 routes, auth, forecasts, portfolio, leaderboard

### Active Dreaming Memory (v0.7.0, Phases 0–5)
- PostgreSQL + pgvector (Neon, 12 tables)
- Episode storage, embedding generation, vector similarity search
- DBSCAN clustering, distributed locking
- Semantic memory (rules, entities, facts, bi-temporal tracking)
- Consolidation workflow (automated 9-step process)
- Multi-provider LLM (Anthropic, Mistral, Qwen, OpenRouter)
- 25 tests passing

### ABW API Sprint 1 (v0.8.0)
- Forecast CRUD, portfolio management
- Brier scoring, materialized leaderboard
- Teams, sharing, OAuth authentication

### Fermi Console — Research Cockpit (v0.9.0)
- **Cockpit:** Six-zone OODA layout (question hub, outside view, evidence, drivers, agents, timeline)
- **Orchestration:** Question → Fermi decomposes → base rate + drivers + evidence
- **Agent system:** 5 fermi-orchestra agents, per-driver assignment, custom queries, scheduling
- **Simulation:** ⌘R → FPL generation → parse → execute (10K iterations), deterministic normalization
- **Visualization:** Plotters histogram, index chart, treemap, sparklines, confidence dots
- **Right panel:** Edit / FPL / Wiki tabs with evidence organized by driver
- **Persistence:** .fpl + .evidence.md + .state.json — agent assignments survive reload
- **Portfolio:** Rich cards with probability, base rate comparison, confidence badge, sparklines
- **Navigation:** ⌘N new, ⌘O import, ⌘R simulate, ⌘P publish, ⌘S save, ⌘E toggle FPL
- **Auth:** Google/GitHub OAuth with localhost callback
- **Window:** Native menus, traffic lights, fullscreen, ⌘1-5 panel switching

---

## 📊 Test Status

| Suite | Tests | Status |
|-------|-------|--------|
| FPL Core (lexer, parser, semantic, executor) | 59 | ✅ Passing |
| fermi-memory (ADM Phases 0–5) | 25 | ✅ Passing |
| **Total** | **84** | **✅ All passing** |

---

## ⚠️ Known Issues

| Issue | Severity | Description |
|-------|----------|-------------|
| Evidence persistence gaps | High | Agent-attributed evidence sometimes lost on reload in certain flows |
| Agent state indicators | Medium | Completed/Idle inferred from evidence count, not actual execution state |
| Wiki inside view ordering | Low | Inside view not always at top of wiki on all save paths |
| Confidence not user-settable | Medium | Per-driver confidence is derived, should be editable |

---

## 📅 Roadmap

### ✅ Completed
- Phase 0: FPL Core Engine (v0.4.0)
- Phase 1: FPL Language Experience (v0.5.0)
- Phase 2: Agent Bestiary & Backend (v0.6.0)
- Phase 3: Active Dreaming Memory (v0.7.0)
- Phase 4: ABW API Sprint 1 (v0.8.0)
- Phase 5: Console GPUI Scaffold
- Phase 6: Research Cockpit — Full Build (v0.9.0)

### 🔄 In Progress
- **Phase 7: Agent Presence & Evidence Richness** (v0.10.0)
  - [ ] Critical bug fixes (evidence persistence, agent state sync)
  - [ ] Real-time agent activity indicators (researching → found → idle)
  - [ ] Agent speech bubbles showing latest finding
  - [ ] User-settable confidence per driver
  - [ ] Evidence hyperlinks with previews
  - [ ] Distribution curve explanations

### 📋 Planned
- Phase 8: Agent Scheduling & Coordination (v0.11.0)
- Phase 9: Version Management & Diff (v0.12.0)
- Phase 10: Portfolio Management & Scoring (v0.13.0)
- Phase 11: Collaboration Foundation
- Phase 12: Intelligence Features
- Phase 13: Polish & Distribution (v1.0.0)

See [ROADMAP.md](docs/fermi/ROADMAP.md) for full details.

---

## 📁 Project Structure

```
fermi/
├── crates/fermi-console/src/
│   ├── main.rs         (~2,900 lines) — app shell, panels, auth, portfolio
│   ├── cockpit.rs      (~4,200 lines) — composer, FPL AST, agents, simulation
│   ├── charts.rs       (~250 lines)   — plotters rendering
│   ├── text_input.rs   (~736 lines)   — editable text fields
│   └── api/client.rs   (~970 lines)   — ABW API client
├── src/                                — FPL core engine
├── fermi-memory/                       — ADM system (episodic + semantic)
├── fermi-lsp/                          — FPL Language Server
├── agents/curated/                     — 53 agent cards
├── extensions/fermi/                   — Zed extension
├── forecasts/                          — saved forecasts (.fpl + .evidence.md + .state.json)
├── docs/fermi/                         — documentation
└── api/                                — Vercel serverless functions
```

---

## 🔧 Configuration

```bash
# Required
ANTHROPIC_API_KEY=sk-ant-...       # For agent execution + embeddings
FERMI_API_KEY=...                  # For ABW API access

# Optional (ADM multi-provider)
MISTRAL_API_KEY=...
QWEN_API_KEY=...
OPENROUTER_API_KEY=...

# Database (ADM)
DATABASE_URL=postgresql://...       # Neon PostgreSQL
```

---

## 🎯 Quick Commands

```bash
# Run the console
cargo run -p fermi-console

# Run all tests
cargo test --workspace

# Run FPL core tests
cargo test --lib

# Run memory system tests
cargo test --package fermi-memory

# Build release
cargo build -p fermi-console --release
```

---

## 📊 Key Metrics

| Metric | Value |
|--------|-------|
| Total Rust code | ~30,000+ lines |
| Console code | ~13,500 lines |
| FPL core code | ~4,950 lines |
| ADM code | ~3,500 lines |
| Tests passing | 84 |
| Agents available | 53 (5 fermi-orchestra) |
| Console commits (March sprint) | 98 |
| Saved test forecasts | 13 |
| Plotters visualizations | 5 (histogram, index, treemap, sparklines, confidence) |
| Simulation speed | <100ms (10K iterations) |

---

## 🔗 Links

- **Repository:** https://github.com/Replicant-Partners/fermi
- **Database:** Neon PostgreSQL (via Vercel)
- **ABW API:** https://agent-bestiary.world
- **Documentation:** `docs/fermi/`
- **Roadmap:** [docs/fermi/ROADMAP.md](docs/fermi/ROADMAP.md)

---

**Status:** Active Development 🚀
**Next Milestone:** v0.10.0 — Agent Presence & Evidence Richness
**Progress:** 6/13 phases complete (46%)

Built by Replicant Partners