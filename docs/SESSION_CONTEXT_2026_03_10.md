# Session Context — 2026-03-10 (Evening)

## Session Focus
Evidence affordances, equity_analyst agent with FMP API, orchestra convention standardization, Phase 8A interaction polish, and critical agent pipeline debugging.

## What Was Built

### 1. Evidence Affordances (Phase 8B) ✅
Complete evidence feedback loop in the Fermi Console cockpit:

- **Evidence Quality Scoring** (`score_evidence_quality`) — automatic 0–100% scoring based on specificity (numbers, %, $), source quality (Bloomberg/Reuters/URL vs manual), findings richness (count), and relevance field. Colored quality bars (green/gold/red) on every evidence item.
- **Evidence→Parameter Suggestion Pipeline** (`extract_suggested_p50`) — parses agent output for "Suggested p50: X.XX" patterns, creates `EvidenceSuggestion` structs stored in `pending_suggestions`.
- **Accept/Reject Feedback Loop** — gold-bordered cards with agent name, delta summary (e.g., "p50 1.00 → 1.15 ↑15%"), reasoning excerpt, ✓ Accept (applies change + scales p95) and ✗ Reject buttons.
- **Expandable/Collapsible Evidence** — click to toggle, collapsed shows 120-char summary + 2 findings with "… N more" indicator.
- **Quality Badges Everywhere** — driver editor, wiki tab, agent picker, treemap, markdown export all show quality scores.
- **Suggestion Badge on Driver Cards** — gold "💡 N suggestion(s)" badge when pending adjustments exist.

### 2. Equity Analyst Agent ✅
New `equity_analyst` agent with 9 live FMP (Financial Modeling Prep) API tools:
- `fmp_company_profile`, `fmp_income_statement`, `fmp_balance_sheet`, `fmp_cash_flow`
- `fmp_ratios`, `fmp_key_metrics`, `fmp_dcf`, `fmp_analyst_estimates`, `fmp_historical_price`

Generic `execute_fmp_api` handler in `tools.rs` builds GET requests from tool input parameters, appends API key from `FMP_API_KEY` env var (fallback hardcoded for dev).

Routing: stock/valuation/earnings/EPS/margin/DCF keywords in driver names → equity_analyst. Domain detection: "stocks"/"equity" → equity_analyst.

API key: `xadhcaZJ9suK6jthYq2axsDINSE31Nxj` (hardcoded fallback, env var preferred for production).

### 3. Orchestra Convention Standardization ✅
All 7 research agents brought to 11/11 convention compliance:

| Convention | Applied to |
|---|---|
| `fermi-orchestra` tag | All 8 |
| `executor: "llm"` | All 8 |
| `performance.total_queries` | All 8 |
| `metadata.domain_knowledge` with base rates | All 7 research agents |
| `taxonomy.phylum: "Prognosticales"`, `class: "Domainria"`, `family: "Investigatidae"` | All 7 research |
| `dependencies.optional` as string array | All 8 |
| `produces` includes `"evidence"` | All 7 research |
| `never-refuses` personality trait | All 7 research |
| CARDINAL RULES in system prompt | All 7 research |
| `wallet` / `ontology_stats` removed | All updated agents |
| `data_sources` in capabilities | macro, sentiment, entity, equity, nba |

Agents updated: `macro_forecaster`, `market_research`, `sentiment_analyzer`, `entity_investigator`, `biotech_analyst`, `equity_analyst` (new), `fermi` (added equity_analyst to deps).

### 4. Phase 8A: Interaction Flow Polish ✅
- **Debounce Ctrl+Enter** — "⏳ Already researching" warning if pressed during orchestration
- **Debounce Ctrl+R** — "⏳ Simulation already running" if pressed during sim
- **Loading Skeleton** — 4 pulsing placeholder driver cards with skeleton bars during 20-30s decomposition wait
- **Workflow State Banner** — green "✓ Research complete — ready to simulate" after agents finish, cyan "⟳ Running Monte Carlo simulation" during sim
- **Context-Sensitive Hints** — bottom bar adapts: "Ctrl+Enter research" → "⏳ Researching…", "Ctrl+R simulate" → "✓ Simulated · Ctrl+R re-run"
- **Drivers header** shows "Drivers (decomposing…)" during orchestration

### 5. Agent Pipeline Debugging & Fixes

#### Critical Bug: Zero-Driver Model Collapse
**Root cause:** When LLM doesn't provide p5/p50/p95 values for a driver, they defaulted to `0.0`. Since the model is multiplicative (`driver_a * driver_b * ... * driver_n`), any zero driver makes the entire simulation output zero. Probability stayed stuck at base rate.

**Fix:** Changed defaults from `0.0` to `0.8/1.0/1.2` (neutral multiplier). Added zero-driver guard in `run_simulation` that detects all-zero drivers, resets to neutral, and warns user.

#### Critical Bug: Agent Directory Not Found
**Root cause:** `agents/curated` path was relative to CWD. When `cargo run` from `crates/fermi-console`, CWD is `crates/fermi-console`, so `agents/curated` resolves to nonexistent path. Console loaded 0 agents silently.

**Fix:** Multi-path search: `agents/curated`, `../../agents/curated`, `../../../agents/curated`, `../agents/curated`, then exe-relative. Falls back gracefully with clear log message.

#### Critical Bug: Empty System Prompts on ABW
**Root cause:** `resolve_agent_card` in `api_server.rs:2232` sets `card.system_prompt = db_agent.system_prompt.clone()` — DB overrides registry. The sync script only creates new agents, never updates existing ones. All 7 orchestra agents had stale/empty prompts on ABW.

**Fix:** Created `scripts/update-fermi-orchestra.sh` that PUTs updated system_prompt, description, tags to existing ABW agents. Ran it successfully — all 7 agents + equity_analyst now have full updated prompts on ABW.

#### Bug: Evidence Truncated to 500 Chars
**Root cause:** `ToolAwareExecutor::parse_evidence_text` fallback path had `text.chars().take(500).collect()` for summary and empty key_findings. Since agents have MCP tools, they use ToolAwareExecutor not LLMExecutor.

**Fix (local, needs ABW deploy):** Changed to preserve full text as summary, extract up to 15 key findings from bullet points, numbered items, and data-rich lines.

## ABW State After Session
- **9 fermi-orchestra agents** registered and published
- All with updated system prompts including CARDINAL RULES
- `equity_analyst` created fresh with FMP tools
- `monte_carlo_sim` is legacy (no CARDINAL RULES, not updated this session)

### ABW Prompt Verification
```
biotech_analyst        3279 chars  cardinal=True
entity_investigator    3892 chars  cardinal=True
equity_analyst         4154 chars  cardinal=True
fermi                   681 chars  cardinal=False (JSON agent)
macro_forecaster       3586 chars  cardinal=True
market_research        1838 chars  cardinal=True
monte_carlo_sim        2478 chars  cardinal=False (legacy)
nba_analyst            5449 chars  cardinal=False (has Never refuse)
sentiment_analyzer     1970 chars  cardinal=True
```

## Files Changed

### New Files
- `agents/curated/equity_analyst/agent_card.json` — new agent card (9 FMP tools)
- `scripts/update-fermi-orchestra.sh` — push updated prompts/tags to ABW

### Modified (Console)
- `crates/fermi-console/src/cockpit.rs` — evidence affordances, quality scoring, accept/reject suggestions, expand/collapse, skeleton loading, workflow banners, sim progress, zero-driver guard, debugging logs, equity_analyst routing
- `crates/fermi-console/src/main.rs` — debounce Ctrl+Enter/Ctrl+R, multi-path agent directory search

### Modified (Server — needs deploy)
- `src/agent_backend/tools.rs` — FMP API tool handlers (`execute_fmp_api` generic handler)
- `src/agent_backend/tool_executor.rs` — fix evidence truncation (full text + key findings extraction)

### Modified (Agent Cards)
- `agents/curated/macro_forecaster/agent_card.json` — CARDINAL RULES, domain_knowledge, taxonomy, data_sources
- `agents/curated/market_research/agent_card.json` — CARDINAL RULES, domain_knowledge, taxonomy
- `agents/curated/sentiment_analyzer/agent_card.json` — CARDINAL RULES, domain_knowledge, taxonomy, agent_type→research
- `agents/curated/entity_investigator/agent_card.json` — CARDINAL RULES, domain_knowledge, taxonomy, agent_type→research
- `agents/curated/biotech_analyst/agent_card.json` — CARDINAL RULES, never-refuses, equity_analyst in deps
- `agents/curated/fermi/agent_card.json` — added equity_analyst to optional deps
- `docs/fermi/ROADMAP.md` — marked 8A and 8B items complete

## Known Issues / Next Steps

### Needs ABW Server Deploy
- Evidence truncation fix in `tool_executor.rs` (full text + key findings)
- FMP tool handlers in `tools.rs` (equity_analyst can't call FMP tools until server deployed)
- Important: until deployed, evidence summaries are capped at 500 chars server-side

### Agent Quality (Ongoing)
- `nba_analyst` — has "Never refuse" but not CARDINAL RULES in prompt (should add structured output format)
- `monte_carlo_sim` — legacy agent, not updated this session, no CARDINAL RULES
- Per-driver query formulation — same agent (market_research) returns similar content for different drivers
- Evidence→parameter suggestion parsing — agents need to consistently output "Suggested p50: X.XX" for the accept/reject pipeline to work
- Agent scheduling (Phase 9) — daily/weekly re-research not implemented yet

### Evidence Quality Gaps
- Wiki evidence still shows truncated summaries until server deploy
- Quality scoring is heuristic-based — could use LLM-based quality judgment in future
- Evidence doesn't yet link back to specific agent execution episodes
- No contradiction detection across evidence items

### Console UX Remaining (Phase 8)
- 8C: Agent Fleet Tab — live status, execution history, credit cost summary
- 8D: Keyboard navigation between drivers, text wrapping fixes, theme refinement
- Distribution curve explanation per driver (how evidence justifies the shape)
- Inside/outside view comparison sparklines in portfolio cards

## Architecture Insight: The DB Override Problem

The most impactful finding this session: **ABW's `resolve_agent_card` always overrides the registry system_prompt with the DB value.** This means:

1. Local agent card development doesn't reach production without explicit DB update
2. The `sync-fermi-orchestra.sh` script only creates — it doesn't update
3. `update-fermi-orchestra.sh` (new) solves this with PUT requests
4. Future: need a CI/CD step that auto-updates ABW prompts on deploy, or change `resolve_agent_card` to prefer non-empty registry prompts over empty DB prompts

## Running the Console
```bash
# From repo root:
cd crates/fermi-console
FERMI_API_KEY="<your-abw-token>" RUST_LOG=info ../../target/debug/fermi-console

# Or set FMP_API_KEY for equity analyst tools:
FMP_API_KEY="xadhcaZJ9suK6jthYq2axsDINSE31Nxj" FERMI_API_KEY="<token>" RUST_LOG=info ../../target/debug/fermi-console
```

## Updating ABW Agent Prompts
```bash
ABW_TOKEN="<your-token>" ./scripts/update-fermi-orchestra.sh
# With --create-missing to also register new agents
# With --dry-run to preview changes
# With --agent equity_analyst to update one specific agent
```

---

## Polymarket Integration (Started Late Session)

### Design Document
`fermi/docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md` — comprehensive design covering:

### Three Core Features (agreed scope)

1. **Import & Decompose**: Browse Polymarket from Portfolio view, import a question, run Fermi decomposition. Link to PM market and live price permanently preserved in forecast metadata.

2. **Three-Number Outside View**: Every linked forecast shows historical base rate, Polymarket crowd price (live), and Fermi inside view. Divergence = your edge signal.

3. **Auto-Resolution Brier Flywheel**: When Polymarket oracle resolves a market, auto-resolve linked Fermi forecast. Brier score computed automatically. Zero manual resolution needed.

### The Core Loop
```
Polymarket question → Import into Fermi → Decompose (drivers + agents)
         ↑                                        ↓
         │                               Probability estimate
         │                                        ↓
         │                          Divergence from crowd price = EDGE
         │                                        ↓
         └──── Resolution (automatic via oracle) → Brier score → Calibration
```

### Key Design Decisions

**Price Normalization:**
- PM price IS probability (0.0–1.0) — no transformation needed
- Use midpoint `(bid + ask) / 2` for stability, fall back to last trade
- Multi-outcome events: server-side aggregation (e.g., sum "25bp cut" + "50bp cut" for "Will Fed cut?")
- Spread + volume → confidence signal → evidence quality score
- PM price can optionally REPLACE the base rate ("Use as base rate" button)
- Divergence always computed: `fermi_probability - market_price` in pp

**Agent Roster:**
- Catalysts map to binary drivers in Fermi decomposition — no separate `event_catalyst` agent needed
- One NEW agent: `prediction_market` — interprets crowd prices, volume signals, spread dynamics
- One FUTURE agent: `resolution_analyst` — stress-tests resolution criteria edge cases
- All existing orchestra agents already serve PM workflow (macro, equity, entity, sentiment, biotech, nba)

### What Was Built

#### Migration 099: `fermi_market_observations`
- Append-only table for PM price snapshots
- Links to `fermi_forecasts` via `forecast_id`
- Stores: market_price, bid, ask, midpoint, spread, volume, liquidity, price changes
- Stores: fermi_probability + divergence_pp at observation time
- Stores: PM lifecycle state (active, closed, resolved, outcome)
- Confidence signal classification (very_high/high/medium/low)
- Observation types: search, import, manual_link, refresh, scheduled, agent_research, resolution_check
- New credit ledger tx_types: `polymarket_search`, `polymarket_import`, `polymarket_snapshot`
- Full index coverage for forecast lookups, market lookups, resolution checking

#### Server Module: `src/polymarket/mod.rs`
- `GammaClient` — stateless HTTP client for Gamma API (events, markets, search)
- `DataClient` — stateless HTTP client for Data API (positions, portfolio value — Mode 2)
- Response types: `PolyEvent`, `PolyMarket`, `PolyTag`, `PolyPosition`
- Processed types: `MarketMatch` (cleaned up for Fermi consumption)
- `ConfidenceSignal` enum with `classify(volume, spread)` and `quality_score()` methods
- `process_market()` — parses string-encoded fields, computes midpoint, classifies confidence
- `parse_yes_price()` — extracts YES probability from Gamma's JSON-string-encoded outcome prices
- `relevance_score()` — client-side scoring of events against search query (word overlap + tags + volume)
- `compute_divergence_pp()` + `interpret_divergence()` — edge analysis utilities
- `format_volume()` + `format_probability()` — display helpers
- 7 unit tests: all passing

#### Registered in crate
- `pub mod polymarket` added to `src/lib.rs`
- Migration 099 added to `api_server.rs` migration list

### What's Next (Build Order)

1. **Server handlers**: `POST /api/polymarket/search`, `POST /api/polymarket/snapshot` in `src/handlers/`
2. **Tool handler**: `polymarket_search` in `tools.rs` so orchestra agents can call it
3. **Console Portfolio**: "Import from Polymarket" button → search panel → select → create forecast
4. **Console Composer**: Three-number outside view (historical + PM crowd + model) with divergence
5. **Resolution checker**: Background poll for linked market resolutions → auto-resolve forecasts

### Polymarket API Summary

| API | Base URL | Auth | Used For |
|-----|----------|------|----------|
| Gamma | `gamma-api.polymarket.com` | None | Events, markets, prices, search |
| Data | `data-api.polymarket.com` | None | User positions, trades (Mode 2) |
| CLOB | `clob.polymarket.com` | Wallet HMAC | Orderbook, trading (future) |

### Files Created/Modified
- `migrations/099_polymarket_observations.sql` — new table + tx_types
- `src/polymarket/mod.rs` — Gamma/Data clients, types, processing, tests
- `src/lib.rs` — registered polymarket module
- `src/api_server.rs` — added migration 099 to startup list
- `docs/fermi/DESIGN_POLYMARKET_INTEGRATION.md` — full design document
