# State of the Project — Agent Bestiary / Fermi

**Date:** 2026-05-13  
**Scope:** Code quality, architecture, test coverage, technical debt, deployment, scalability, MVP readiness

---

## 1. Codebase overview

| Crate | Lines | Role |
|---|---|---|
| `fermi` (bin `api-server`) | 61,600 | The monolith — API server + FPL parser + agent executor + all handlers |
| `agent-bestiary-memory` | 10,438 | ADM store: episodes, rules, entities, embeddings, consolidation worker |
| `fermi-auth` | 3,071 | OAuth2, JWT, API keys, SIWE, credits, teams, visibility |
| `agent-bestiary-evaluators` | 1,669 | EvalModel trait, registry, aggregator |
| `agent-bestiary-observability` | 1,594 | Drift, anomaly, social tracker, trend analyser, worker |
| `agent-bestiary-coherence-gate` | 799 | Intervention encoder, TEC gate, two-write memory |
| `agent-bestiary/consolidate` | 528 | Standalone dreaming binary |
| `fermi-lsp` | ~800 | FPL language server (WIP, excluded from server build) |
| `fermi-memory` | ~1,200 | Legacy memory library (superseded by ADM, not imported by api-server) |

---

## 2. Deployment topology

**Single binary, single process, single Postgres.** The `api-server` binary built from `src/api_server.rs` is the entire platform. It:

- Runs DB migrations on startup
- Seeds curated agent cards from `agents/curated/`
- Serves both the JSON API and all server-rendered HTML templates
- Handles SSE streams for workspace, rabble chat, and creature events via three in-process broadcast channels (capacity: 256 / 256 / 512)
- Runs the `ObservabilityWorker` as `tokio::spawn` tasks (best-effort, non-blocking)

**DB connection pools:**
- `api-server` creates one `PgPool` with `max_connections: 10`
- `MemoryStore` wraps the **same pool** via `MemoryStore::from_pool(db.clone())` — no second physical pool
- Consolidation binary creates its own separate pool when run independently

**Infrastructure:** Railway (auto-deploy from `main`), Neon Postgres (US-East, pgvector enabled). Custom domain `agent-bestiary.world`. No Redis, no message queue, no separate worker process.

---

## 3. What is monolithic vs. clean

### The `fermi` bin — what it actually contains

```
src/
├── api_server.rs        2,638 lines   Route table (342 routes) + AppState + startup
├── agent_backend/       9,214 lines   Agent execution family (all 5 executors + tools)
│   ├── tools.rs         5,588 lines   ← 30 MCP tool implementations, all inline
│   ├── agent_card.rs      838 lines
│   ├── tool_executor.rs   643 lines
│   ├── multi_model_executor.rs  460 lines
│   └── llm_executor.rs    471 lines
├── handlers/           26,251 lines   42 handler modules
├── FPL parser/          6,600 lines   ast, lexer, parser, semantic, evaluator, executor,
│   (10 modules)                       distributions, sensitivity, symbol_table, types
├── report/              ~600 lines    sparkline, charts, markdown, mermaid, theme
└── other                ~500 lines    gas, workflows, polymarket client, voice, api
```

**The clean factoring (should stay as-is):**
- `fermi-auth` — completely independent, no platform coupling
- `agent-bestiary-*` crates — one concern each, pure-function test suites, no handler dependencies
- Handler modules — one file per domain, appropriately sized (50–800 lines each)
- `AppState` — single source of truth, immutable shared references, correct `Arc<>` usage throughout
- DB pool — correctly shared (not duplicated)
- SSE broadcasts — correct use of Tokio broadcast for fan-out

**The genuinely monolithic parts:**

### `src/agent_backend/tools.rs` — 5,588 lines, 53 functions
This is the biggest structural problem in the codebase. It contains 30+ MCP tool implementations spanning completely unrelated domains: spatial (H3, geocoding, beacons), biological (GBIF taxonomy, wing segmentation), financial (Polymarket, FMP), media (Reduct.video), commerce (shopping marketplace), creature management (minting, art generation), coherence, workspace operations, and knowledge graph queries. Every new tool lands here.

**Problem:** it is one file with no internal module structure, no trait boundary, and no separation between tool dispatch and tool implementation. Adding a new tool to `tools.rs` requires reading or touching all 5,588 lines to find the right place.

**Fix:** split into `tools/` subdirectory with one file per domain group (spatial, bio, financial, media, social, agents, coherence). The dispatch table at the top of `tools.rs` becomes `tools/mod.rs`. No trait changes needed — this is purely a file organization change.

### `src/api_server.rs` — 2,638 lines + 342 routes
The route table is in one function. It works and is correctly organized (routes are grouped by domain with comments) but it is long enough that adding a route requires searching. Not a correctness issue; a navigation issue.

### `src/handlers/workspace.rs` — 2,805 lines
Handles workspace CRUD, SSE message stream, coherence evaluation, file operations, agent execution delegation, budget management, and workspace gas charging. Seven distinct concerns in one file. The `evaluate_coherence_handler` alone is ~200 lines.

**The three handlers worth splitting:**

| Current | Split into |
|---|---|
| `workspace.rs` | `workspace_core.rs` + `workspace_messages.rs` + `workspace_coherence.rs` |
| `social.rs` (1,816 lines) | `social_contacts.rs` + `social_creatures.rs` + `social_feed.rs` |
| `creatures/` (already split into 11 files) | ✅ correctly factored |

---

## 4. Vertical domains

The platform serves six distinct product verticals. Each has a handler group:

| Vertical | Handler files | Lines | Status |
|---|---|---|---|
| **Agent platform** | agents, eval, eval_judge, eval_brier, execution, execution_stream, consolidation, kg, ontology, observatory, composition, admin | ~6,500 | ✅ Core — well structured |
| **Workspace / AI collaboration** | workspace, workspace coherence, pages, wizard | ~3,500 | ✅ Core — workspace.rs needs split |
| **Forecasting (FPL)** | forecasts, polymarket, notebooks | ~3,800 | ✅ Stable — FPL parser should become `fermi-fpl` crate |
| **Creature / Rabble / Social** | creatures (11 files), rabble_workspace, rabble_chat, social, beacons, swarm_telemetry, swarm_algorithms, governance | ~11,000 | ⚠️ Largest vertical, correctly decomposed but dense |
| **Commerce / Marketplace** | marketplace, agent_wallet, wallet, billing, push | ~1,700 | ✅ Lean |
| **Platform infra** | auth, profile, users, teams, misc, metrics, qr_codes, streams | ~2,000 | ✅ Lean |

The creature/rabble vertical is the most code-dense (11,000 lines of handlers, plus `tools.rs` has ~2,000 lines of creature-specific tool implementations). It is correctly decomposed — the 11-file `creatures/` subdirectory is the right model — but it represents the most domain-specific surface area, which has implications for MVP scoping (see §7).

---

## 5. Scalability analysis

### What scales well today

**Stateless request handling.** Every handler takes `State<AppState>` and `AuthPrincipal`. There is no per-request mutable state, no session server, no sticky sessions required. Any number of `api-server` replicas can handle any request (given the same DB).

**Connection pool is shared correctly.** `MemoryStore` wraps `db.clone()` — same pool, not a second connection. The pool limit of 10 is low but acceptable for a single-instance deployment. For multi-replica, each replica would hold 10 connections; Neon supports this.

**SSE broadcasts are in-process only.** The `ws_broadcast`, `rabble_broadcast`, and `creature_broadcast` channels are Tokio broadcast senders inside the single process. This is correct for a single-replica deployment.

### What does not scale beyond a single replica

**The three broadcast channels.** When you run two `api-server` replicas, a workspace SSE stream connected to replica A will not receive events posted on replica B. This is the primary horizontal scaling blocker. The fix is to move broadcasts to a shared pub/sub layer (Redis Pub/Sub or Postgres `LISTEN`/`NOTIFY` — Neon supports NOTIFY).

**`COHERENCE_AUTO_EVAL_INTERVAL` is in-process state.** The background coherence evaluation count (`interval: messages between auto-evaluations`) is a per-process counter. With two replicas it would double-fire. This is minor but noteworthy.

**The `ObservabilityWorker` spawns per eval run.** `tokio::spawn` is per-process. Multiple replicas means multiple concurrent worker spawns for the same agent. The worker is designed to be incremental (checkpoint pointer) so double-scanning is safe but wasteful. An advisory DB lock (like `ConsolidationLock`) would prevent it.

### What would break under heavy load

**`max_connections: 10` on the API server pool.** Under concurrent load (many simultaneous agent executions + SSE connections + API requests), 10 connections can saturate. The memory store already uses 20 connections — the API server's separate pool (for fermi-auth queries, direct SQL in handlers) is limited to 10. Under load this causes request queuing at the DB layer.

**No HTTP-level caching.** Every page load for `/agent/:id` re-fetches the agent from the DB. The `AgentRegistry` is in-memory (seeded from files) but `resolve_agent()` still does a DB lookup to pick up runtime changes. For read-heavy pages, a short TTL cache (in-process DashMap, already used for `ProjectionCache`) would help.

**No request timeout on LLM calls.** The LLM executor calls Anthropic/etc. with no explicit `reqwest` timeout beyond the default. A slow provider causes the handler to hold a DB connection for the duration of the LLM call. For the current agent execution pattern (synchronous, wait for response) this is an inherent limitation, not a bug — but it means concurrent execution slots are precious.

---

## 6. Test coverage

### Lib tests — all passing, 0 failures, 0 ignored

| Crate | Tests |
|---|---|
| `fermi` (lib) | 96 passed |
| `agent-bestiary-memory` | 19 passed |
| `agent-bestiary-observability` | 26 passed |
| `agent-bestiary-evaluators` | 16 passed |
| `agent-bestiary-coherence-gate` | 11 passed |
| `fermi-auth` | 59 passed |
| Coherence crates | 44 passed |
| Other crates | 55 passed |
| **Total** | **~326 lib tests, 0 failures** |

### Integration tests — compile clean

| File | Tests | Requires |
|---|---|---|
| `tests/api_tests.rs` | 16 | DATABASE_URL |
| `agent-bestiary/memory/tests/test_seed.rs` | 12 | DATABASE_URL |
| `agent-bestiary/memory/tests/test_llm_providers.rs` | 10 | DATABASE_URL + API keys |

### Coverage gaps

- HTTP handler tests: **none**. No test client, no request-response tests at the HTTP layer. All handler logic is tested indirectly via the lib/integration tests. For MVP stability, the highest-value additions would be:
  - `POST /api/agents/:id/execute` — the core execution path
  - `POST /api/workspaces/:id/coherence/evaluate` — coherence evaluation
  - `POST /api/workspaces/:id/composition/dream` + accept/reject — the new composition loop
- The 4 FPL/report tests that were previously ignored are now fixed and passing.

---

## 7. MVP readiness assessment

### What is production-ready

| Component | Readiness | Notes |
|---|---|---|
| Auth (OAuth, JWT, API keys, secrets) | ✅ Production | fermi-auth is clean, tested, well-factored |
| Agent execution (LLM, MCP, tool-use) | ✅ Production | Multi-model, model ladder, cognition tiers all working |
| Episodic memory + ADM | ✅ Production | 10k-line tested crate, consolidation binary deploys separately |
| Eval framework + observability stack | ✅ Production | 6-phase implementation complete, 53 tests |
| Workspace + coherence + HITL | ✅ Production | Deployed and functional |
| Credit system + billing | ✅ Production | Stripe integrated, wallet isolation tested |
| Agent card conformance testing | ✅ Production | Conformance tests enforce invariants on every card |
| Migrations | ✅ Production | 117 registered, test-enforced |

### What is functional but needs attention before scaling

| Component | Status | Action needed |
|---|---|---|
| SSE broadcasts | ✅ Single-replica | Move to Redis Pub/Sub or Postgres NOTIFY before multi-replica |
| DB connection pool (API server) | ⚠️ Low headroom | Raise `max_connections` from 10 to 25; add `min_idle` |
| `tools.rs` (5,588 lines) | ⚠️ Maintainability | Split into `tools/` subdirectory by domain |
| `workspace.rs` (2,805 lines) | ⚠️ Maintainability | Split into 3 focused files |
| LLM call timeouts | ⚠️ Reliability | Add `reqwest` timeout (e.g. 90s) to all provider calls |
| ObservabilityWorker concurrency | ⚠️ Wastefulness | Add advisory lock (mirrors ConsolidationLock) |

### What is feature-incomplete (tracked, not blocking MVP)

| Component | Gap | Design doc |
|---|---|---|
| Track B evaluators | WildGuard, Sotopia, Faithfulness, LifelongBench, CharacterEval unbuilt | `EVALUATOR_DESIGN.md` |
| Social tracker dimension mappings | Placeholder until Track B ships Sotopia/CharacterEval | `OBSERVABILITY_IMPL.md` |
| Composition feedback loop | Implementation complete; no usage data yet | `COMPOSITION_FEEDBACK_LOOP_PLAN.md` |
| Learning mechanics simplification | dream_narrator/coherence_consultant archived; workspace.rs sub-call still calls `coherence_consultant` indirectly | `LEARNING_MECHANICS_SIMPLIFICATION.md` |
| `fermi-fpl` extraction | FPL parser in `fermi` bin; extraction scoped but deferred | This doc §8 |

### What is legacy / not MVP-critical

| Component | Status |
|---|---|
| `fermi-memory` crate | Superseded by ADM; not imported by api-server; safe to archive |
| `src/main.rs` (CLI REPL) | FPL REPL entrypoint; separate from api-server; not deployed |
| `src/bin/agent-web-ui.rs` | Old Askama-based UI prototype; superseded by the current template system |
| `src/bin/agent-server.rs` | Simple REST agent server prototype; superseded by api-server |
| `crates/fermi-console` | Desktop app (excluded from server builds); not deployed |

---

## 8. Recommended action sequence for MVP stabilisation

### Tier 1 — Before scaling to multiple replicas (blocking)

1. **Raise API server pool limit** (`max_connections: 10` → `25`, add `min_idle: 2`) in `src/api_server.rs:537`
2. **Add LLM call timeouts** — `reqwest::Client::builder().timeout(Duration::from_secs(90))` in `llm_executor.rs`, `multi_model_executor.rs`, and `tool_executor.rs`
3. **Replace broadcast channels with Postgres NOTIFY** — the workspace SSE handler subscribes via `LISTEN workspace_events_<id>`; publishers call `pg_notify()`. Removes the single-replica constraint.

### Tier 2 — Maintainability (before the codebase gets larger)

4. **Split `tools.rs`** into `tools/` subdirectory by domain. No interface changes. Estimated: 2–3 hours.
5. **Split `workspace.rs`** into `workspace_core.rs`, `workspace_messages.rs`, `workspace_coherence.rs`. Estimated: 2 hours.
6. **Remove `coherence_consultant` sub-call** from `evaluate_coherence_handler` in `workspace.rs` (use `cohere_and_coordinate` directly per `LEARNING_MECHANICS_SIMPLIFICATION.md`).

### Tier 3 — Test infrastructure

7. **Add HTTP-layer tests** for the 3 highest-value paths: agent execute, coherence evaluate, composition dream+accept.
8. **Add ObservabilityWorker advisory lock** (same pattern as `ConsolidationLock`).

### Tier 4 — Archive legacy

9. Archive `fermi-memory` crate (mark `publish = false`, add deprecation notice in README)
10. Archive `src/bin/agent-web-ui.rs` and `src/bin/agent-server.rs` (move to `src/bin/legacy/`)
11. Extract `fermi-fpl` crate (deferred — 4–6h, dedicated PR)

---

## 9. Verdict

The codebase is in **good shape for a single-instance MVP**. The architecture is sound: clean crate boundaries, correct use of shared state, append-only audit trails, no global mutable state, proper async throughout. The test suite is comprehensive at the unit level.

The two issues that would block growth are both well-understood and fixable without architectural redesign:

1. **The broadcast channels** — in-process pub/sub that prevents horizontal scaling. Fix: Postgres NOTIFY. No schema change needed.
2. **`tools.rs`** — a 5,588-line file that will slow every future tool addition. Fix: file split, no interface changes.

Everything else is a maintainability improvement, not a correctness issue. The platform can go to production on a single Railway instance with the Tier 1 fixes (pool limit + LLM timeouts). The Tier 2 splits improve developer velocity but don't affect runtime behaviour.
