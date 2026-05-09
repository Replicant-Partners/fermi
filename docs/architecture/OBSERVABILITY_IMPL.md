# Social Agent Observability — Implementation Map

> Living document. Reconciles
> [`social_agent_observability_architecture.html`](./social_agent_observability_architecture.html)
> with the running ABW/Fermi codebase. Updated at the close of every
> phase.

## Track structure

The architecture-doc work is split into two parallel tracks. Track A is
the platform spine and is built strictly sequentially. Track B is the
native-Rust evaluator family and ships in parallel after the
`EvalModel` trait lands in Phase 1.

```
Track A — Platform observability spine            Track B — Native evaluators
─────────────────────────────────────────────     ──────────────────────────
  Phase 0  Foundations (DB + types)                (waits for Phase 1)
  Phase 1  EvalModel trait + registry      ─────►  Sotopia
  Phase 2  Wire registry into eval pipeline        LifelongBench
  Phase 3  Longitudinal observability               CharacterEval
  Phase 4  HITL + observatory UI                    WildGuard
  Phase 5  Intervention encoder + coherence gate    Faithfulness
  Phase 6  Telemetry + docs cleanup
```

See [`EVALUATOR_DESIGN.md`](./EVALUATOR_DESIGN.md) for Track B.

## Phase status

| Phase | Status | Migration | Crate(s) touched | Verified by |
|---|---|---|---|---|
| 0 — Foundations | ✅ done | `103_observability_foundations.sql` | `agent-bestiary-memory` | `cargo check --workspace` |
| 1 — Evaluator registry | ✅ done | – (no DB changes; signal table lands in Phase 2) | new `agent-bestiary-evaluators` | `cargo check -p agent-bestiary-evaluators`, 16 unit/integration tests |
| 2 — Wire registry into eval | ✅ done | `104_evaluator_signals.sql` | `agent-bestiary-memory`, `fermi` (`src/handlers/eval.rs`, `eval_judge.rs`, `eval_brier.rs`) | `cargo check --workspace` |
| 3 — Longitudinal observability | ✅ done | `105_longitudinal_observability.sql` | new `agent-bestiary-observability`, `agent-bestiary-memory`, `fermi` | `cargo check --workspace`, 24 unit/integration tests |
| 4 — HITL + observatory UI | ✅ done | `106_hitl_actions.sql` | `agent-bestiary-memory`, `fermi` (`src/handlers/observatory.rs`, `pages.rs`), templates | `cargo check --workspace` |
| 5 — Intervention feedback loop | ✅ done | `108_intervention_feedback_loop.sql` | new `agent-bestiary-coherence-gate`, `agent-bestiary-memory`, `fermi` (`src/handlers/observatory.rs`) | `cargo check --workspace`, 11 unit/integration tests |
| 6 — Telemetry + docs cleanup | ✅ done | – | `templates/observatory.html` | visual inspection |

## Decision log

| # | Decision | Source |
|---|---|---|
| D1 | Build incrementally, one phase per PR | user, this thread |
| D2 | Clean factoring — new crates per concern | user |
| D3 | `persona_version` increments on agent-wide interventions **and** `AgentVersion` writes (DB trigger handles the latter) | user |
| D4 | Dyad identity wiring deferred — exercised at the application layer | user |
| D5 | Native Rust evaluators only; separate design discussion (Track B) | user |
| D6 | Coherence engine acts as **infra helper** to seed self-improvement loops via episodic-memory writes | user |
| D7 | UI fits into existing ABW Askama templates + dark theme | user |
| D8 | `BrierEvaluator` is a thin read-only wrapper over the existing forecast resolver | user |
| D9 | One migration per phase | user |
| D10 | Coherence as gatekeeper — pattern (c): synchronous gate for `agent_wide` scope; settler mode for `episode` and `dyad` scope | user, Q1 |
| D11 | Episode immutability enforced via separate `episode_corrections` table + DB trigger blocking UPDATEs | user, Q2 |
| D12 | Synthetic corrected episodes embed `original_query + corrected_response` | user, Q3 |
| D13 | `EpisodeBundle` lives in `agent-bestiary-memory` (next to `Episode`) | user, Q4 |
| D14 | Provenance enum: `auto_pass`, `auto_fail`, `human_approved`, `human_relabeled`, `human_corrected`, `synthetic_correction` | user, Q5 |
| D15 | Phase 0 migration scheme as proposed (persona_version, provenance, authority_weight, dyad_id, persona_version_at_write, episode_corrections, agent_versions trigger) | user, Q6, Q7 |
| D16 | Phase 2 — `aggregated_signal` granularity = run-level aggregate (Q1.b: mean per dim across cases, conflict union) | user |
| D17 | Phase 2 — `case_results[].signal` is **additive**: keep legacy `judge_scores`, add new `signal` (Q2.a) | user |
| D18 | Phase 3 — populate `dyad_id` for eval-pipeline executions only (Q1.a). Workspace + agent-to-agent / system left for later. | user |
| D19 | Phase 3 — drift threshold ships **infra for adaptive, static value today** (Q2). Per-agent override on `agents.capability_gates.drift_threshold`, default 0.20. Treat current drift values as scaffolding until data accumulates. | user |
| D20 | Phase 3 — anomaly defaults: 3-episode rolling-conflict window, 0.20 rapport drop in 5-entry window for rupture, binary safety flag (Q3) | user |
| D21 | Phase 3 — hybrid scheduling (Q4.c): timeline written inline, drift + anomaly scanned by `ObservabilityWorker` on demand (cadence mirrors `ConsolidationWorker`) | user |
| D22 | Phase 3 — `TrendAnalyzer::compute` is on-demand only (Q5). Snapshot caching deferred to Phase 4. | user |
| D23 | Phase 3 — no backfill (Q7.a). Timeline is forward-only from deploy. | user |
| D24 | Phase 4 — HITL access = agent owner OR platform admin (Q1.a + admin override). `curated` agents fall to admin. | user |
| D25 | Phase 4 — global cross-agent observatory + per-agent drill-down (Q2.b). No "Observatory tab" cramped onto agent_detail. | user |
| D26 | Phase 4 — `agent_wide` interventions + two-reviewer consensus deferred to Phase 5 (Q3.c). UI surfaces "Intervene" button as disabled with explanatory tooltip. | user |
| D27 | Phase 4 — server-rendered HTML+CSS, JS-fetch from `/api/observatory/*` (Q4.a + Q5.a corrected). Matches existing `dashboard.html` / `agent_detail.html` pattern; no chart-library dependency, no live updates (Phase 6 polish). | user (corrected mid-build) |
| D28 | Phase 4 — manual scan trigger exposed at `POST /api/observatory/agents/:id/scan` (Q6.a). Owner+admin only. Synchronous response with `ScanReport`. | user |
| D29 | Phase 4 — eval-run rendering on `agent_detail.html` extended **additively** with per-dimension means + conflict pill + prefilter-blocked indicator (Q8.a). Legacy `judge_scores` rendering preserved. | user |

## Concept ↔ code map

The architecture doc names concepts; this table tracks where each one
lives in the repo. Each phase fills more rows.

### Plane A — Agent capabilities (existing)

| Doc concept | Status | Code location |
|---|---|---|
| LLM judge loop | ✅ existing | `src/handlers/eval.rs::score_with_judge` |
| Brier scoring | ✅ existing | `src/handlers/forecasts.rs` (resolver), `src/handlers/polymarket.rs` |
| Coherence agent | ✅ existing | `agent-bestiary/coherence/crates/coherence-{core,engine}` |
| Episodic memory | ✅ existing | `agent-bestiary/memory` (`Episode`, `MemoryStore::store_episode`) |
| `EpisodeBundle` | ✅ Phase 0 | `agent-bestiary/memory/src/bundle.rs` |

### Plane B — Evaluator registry

| Doc concept | Status | Code location |
|---|---|---|
| `EvalModel` trait | ✅ Phase 1 | `agent-bestiary/evaluators/src/model.rs` |
| `EvalTier` (PreFilter / Dimensional) | ✅ Phase 1 | `agent-bestiary/evaluators/src/tier.rs` |
| `EvalResult` + `Dimension` + `EvalFlag` | ✅ Phase 1 | `agent-bestiary/evaluators/src/result.rs` |
| `EvalError` (incl. `Inapplicable` skip path) | ✅ Phase 1 | `agent-bestiary/evaluators/src/error.rs` |
| `EvaluatorRegistry` (serial pre-filter + parallel dimensional) | ✅ Phase 1 | `agent-bestiary/evaluators/src/registry.rs` |
| `Aggregator` + `AggregatedSignal` + `ConflictFlag` | ✅ Phase 1 | `agent-bestiary/evaluators/src/aggregator.rs` |
| `LlmJudgeEvaluator` (reference impl, dimensional) | ✅ Phase 1 | `agent-bestiary/evaluators/src/judge.rs` |
| `BrierEvaluator` (reference impl, thin read wrapper, D8) | ✅ Phase 1 | `agent-bestiary/evaluators/src/scoring.rs` |
| `LlmJudgeAnthropic` (production `LlmJudge` impl) | ✅ Phase 2 | `src/handlers/eval_judge.rs` |
| `BrierLookupSqlx` (production `BrierLookup` impl) | ✅ Phase 2 | `src/handlers/eval_brier.rs` |
| `eval_signals` table (per-evaluator scoring history) | ✅ Phase 2 | `migrations/104_evaluator_signals.sql`, `agent-bestiary-memory` |
| `eval_runs.aggregated_signal` + `conflict_flags` + `prefilter_blocked` | ✅ Phase 2 | `migrations/104_evaluator_signals.sql` |
| Registry wired into eval pipeline | ✅ Phase 2 | `src/handlers/eval.rs::run_eval_cases` |
| Run-level signal aggregation (mean across cases, conflict union) | ✅ Phase 2 | `src/handlers/eval.rs::aggregate_run_signals` |
| Per-dimension regression detection | ✅ Phase 2 | `src/handlers/eval.rs::detect_regression` |
| `eval_conflict` notification | ✅ Phase 2 | `src/handlers/eval.rs::format_conflict_body` |
| Pre-filter tier real impls (WildGuard, Faithfulness) | ⏳ Track B | (planned) `agent-bestiary/evaluator-wildguard`, `agent-bestiary/evaluator-faithfulness` |
| Dimensional tier real impls (Sotopia, LifelongBench, CharacterEval) | ⏳ Track B | (planned) `agent-bestiary/evaluator-sotopia` etc. |

### Plane C — Longitudinal observability

| Doc concept | Status | Code location |
|---|---|---|
| Episode scorer (inline timeline write) | ✅ Phase 3 | `agent-bestiary/observability/src/scorer.rs::EpisodeScorer` |
| Persona drift monitor (cosine of mean embeddings across persona_version) | ✅ Phase 3 | `agent-bestiary/observability/src/drift.rs::PersonaDriftMonitor` |
| `DriftThreshold::{Static, Adaptive}` (D19) | ✅ Phase 3 | `agent-bestiary/observability/src/drift.rs` |
| Social interaction tracker (per dyad: rapport, trust, reciprocity) | ✅ Phase 3 (scaffolding values until data accrues) | `agent-bestiary/observability/src/social.rs::SocialInteractionTracker` |
| Rupture detection (peak-to-trough rapport drop) | ✅ Phase 3 | `agent-bestiary/observability/src/social.rs::detect_rupture` |
| Agent timeline store | ✅ Phase 3 | `migrations/105_longitudinal_observability.sql` — `agent_timeline_entries` |
| Dyad state store | ✅ Phase 3 | `migrations/105_…` — `dyad_state` |
| Anomaly detector (drift, rolling_conflict, rupture, safety) | ✅ Phase 3 | `agent-bestiary/observability/src/anomaly.rs::AnomalyDetector` |
| `anomaly_events` table + HITL queue read path | ✅ Phase 3 | `migrations/105_…`, `MemoryStore::list_pending_anomaly_events` |
| Trend analyser (on-demand) | ✅ Phase 3 | `agent-bestiary/observability/src/trend.rs::TrendAnalyzer::compute` |
| `ObservabilityWorker` (background scanner, two-pass) | ✅ Phase 3 | `agent-bestiary/observability/src/worker.rs` |
| Per-agent worker checkpoint | ✅ Phase 3 | `migrations/105_…` — `agent_observability_state` |
| Observatory dashboard | ⏳ Phase 4 | (planned) `templates/observatory.html` |
| Trend snapshot cache | ⏳ Phase 4 | (planned) — Phase 3 ships only on-demand `TrendAnalyzer::compute` (D22) |

### Plane D — Human surfacing & intervention feedback

| Doc concept | Status | Code location |
|---|---|---|
| Observatory dashboard (per-agent) | ✅ Phase 4 | `templates/observatory.html`, `src/handlers/pages.rs::observatory_view` |
| Observatory cross-links from `dashboard.html` and `agent_detail.html` | ✅ Phase 4 | `templates/dashboard.html`, `templates/agent_detail.html` |
| Eval-run signal rendering on agent detail page (Q8) | ✅ Phase 4 | `templates/agent_detail.html` (`eval-runs-list` JS) |
| HITL review queue UI | ✅ Phase 4 | `templates/observatory_hitl.html` |
| HITL queue read API | ✅ Phase 4 | `src/handlers/observatory.rs::list_hitl_queue_handler` |
| HITL action API (approve / relabel; intervene gated to Phase 5) | ✅ Phase 4 | `src/handlers/observatory.rs::record_hitl_action_handler` |
| `hitl_actions` table (append-only audit trail) | ✅ Phase 4 | `migrations/106_hitl_actions.sql` |
| Manual observability scan trigger | ✅ Phase 4 | `src/handlers/observatory.rs::trigger_agent_scan_handler` |
| Per-agent timeline / dyad / anomaly read endpoints | ✅ Phase 4 | `src/handlers/observatory.rs` |
| Intervention encoder (scope, classification) | ✅ Phase 5 | `agent-bestiary/coherence-gate/src/encoder.rs::InterventionEncoder` |
| Coherence check (gatekeeper for `agent_wide`, settler for others) | ✅ Phase 5 | `agent-bestiary/coherence-gate/src/gate.rs::CoherenceGate` |
| Two-write memory pattern (annotation + synthetic episode) | ✅ Phase 5 | `agent-bestiary/coherence-gate/src/two_write.rs::TwoWriteMemory` |
| Two-reviewer consensus for `agent_wide` interventions | ✅ Phase 5 | `migrations/108_intervention_feedback_loop.sql`, `MemoryStore::create_two_reviewer_request`, `POST /api/observatory/hitl/consensus/:id` |
| `corrections[]` audit trail | ✅ Phase 0 | `episode_corrections` table |
| Provenance enum on episodes | ✅ Phase 0 | `agent-bestiary/memory/src/types.rs::Provenance` |
| `persona_version` field on agents | ✅ Phase 0 | `agent-bestiary/memory/src/types.rs::Agent` |
| Authority weight (HumanAuthority = 1.0) | ✅ Phase 0 | `episodes.authority_weight`, `episode_corrections.authority_weight` |

## Phase 0 — Foundations (shipped)

### Migration

`migrations/103_observability_foundations.sql` — idempotent. PgBouncer-safe (constraint mutations wrapped in DO blocks). Adds:

- `agents.persona_version INTEGER NOT NULL DEFAULT 1` + supporting index
- `episodes.provenance TEXT NOT NULL DEFAULT 'auto_pass'` (enum-checked)
- `episodes.authority_weight DOUBLE PRECISION NOT NULL DEFAULT 0.5` (bounds-checked 0..=1)
- `episodes.dyad_id TEXT` (nullable; deferred per D4)
- `episodes.persona_version_at_write INTEGER` (drift snapshot)
- `episode_corrections` table — append-only HITL audit trail with row-level UPDATE-blocking trigger
- `bump_agent_persona_version()` trigger on `agent_versions` INSERT (D3 case b)

### Rust types

In `agent-bestiary/memory`:

- `Provenance` enum (`AutoPass`, `AutoFail`, `HumanApproved`, `HumanRelabeled`, `HumanCorrected`, `SyntheticCorrection`) + `Display`/`FromStr`/`Default`/`is_human_originated`/`is_human_authority`
- `EpisodeCorrection` struct
- `ReviewerAction` enum (`Approve`, `Relabel`, `Intervene`)
- `CorrectionScope` enum (`Episode`, `Dyad`, `AgentWide`)
- `CorrectionClassification` enum (`Belief`, `Behaviour`)
- `Episode` extended with `provenance`, `authority_weight`, `dyad_id`, `persona_version_at_write`
- `Agent` extended with `persona_version`
- `EpisodeBundle` (in new `bundle.rs`) with `TranscriptTurn`, `TranscriptRole`, `AgentCardSnapshot` — the normalized signal Plane B will consume

### Storage methods

- `MemoryStore::store_episode` — writes the four new episode columns
- All eight episode-read sites read the new columns (defensive `try_get` defaults)
- `MemoryStore::create_episode_correction`
- `MemoryStore::list_corrections_for_episode`
- `MemoryStore::list_corrections_for_agent`
- `MemoryStore::get_episode_correction`

### What does NOT change in Phase 0

- No HTTP API endpoints
- No template changes
- No agent-card or seed-data changes
- No behaviour change in eval pipeline, episode storage, or workspace handlers — every existing call path produces episodes with `provenance = auto_pass` and `authority_weight = 0.5` by default

This is intentional: Phase 0 is a pure foundation. Behaviour change starts in Phase 1.

## Phase 1 — Evaluator Registry (shipped)

### Crate

`agent-bestiary/evaluators/` — new sibling crate under `agent-bestiary/`. Workspace member registered in root `Cargo.toml`. Single dependency on `agent-bestiary-memory` for the `EpisodeBundle` input contract; otherwise self-contained.

### Public surface

The trait every evaluator implements:

```rust
#[async_trait]
pub trait EvalModel: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn tier(&self) -> EvalTier;                  // PreFilter | Dimensional
    fn dimensions(&self) -> Vec<Dimension>;
    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError>;
}
```

Supporting types: `EvalTier`, `Dimension` (newtype over `String`), `EvalFlag`, `EvalResult` (with builder methods, scores clipped to `[0,1]`), `EvalError` (`Inapplicable` is the canonical "skip me" return path).

The registry composes them:

```rust
let mut reg = EvaluatorRegistry::new();
reg.register(my_prefilter);
reg.register(my_dimensional_a);
reg.register(my_dimensional_b);
let outcome: RegistryOutcome = reg.run(&bundle).await;
//   outcome.signal: AggregatedSignal { per_dimension, conflicts, flags, ... }
```

### Execution model

- **Pre-filters** run **serially** in registration order. When a pre-filter scores any of its dimensions below `prefilter_block_threshold` (default `0.5`), the registry **short-circuits**: dimensional evaluators are skipped and the partial signal is returned with `prefilter_blocked = true`.
- **Dimensional** evaluators run **concurrently** via `futures::future::join_all`.
- A failing or `Inapplicable` evaluator never aborts the run — the outcome is captured per-evaluator and the aggregator skips it. Failures land in `signal.failed_evaluators`; opt-outs land in `signal.inapplicable_evaluators`.

### Aggregation + conflict detection

Per architecture-doc mock:

- For each dimension scored by ≥ 1 evaluator, compute a **confidence-weighted mean** of the contributors' scores.
- Flag the dimension as **in conflict** when ≥ 2 evaluators scored it AND `max(scores) - min(scores) > conflict_threshold` (default `0.20`, matching the doc's mock).
- Aggregated output (`AggregatedSignal`) is `Serialize` and is the canonical wire/storage shape consumed by Phase 2's `eval_signals` table and Phase 4's HITL surfaces.

### Reference implementations

Two ship inside the same crate to prove the trait shape end-to-end.

**`LlmJudgeEvaluator`** — dimensional. Wraps the legacy `score_with_judge` semantics behind a provider-agnostic `LlmJudge` trait. Three dimensions (`relevance`, `accuracy`, `completeness`); 1–5 Likert input is normalized to `[0, 1]`. Phase 2 will adapt the existing Anthropic-backed judge in `src/handlers/eval.rs` to implement `LlmJudge` so this evaluator becomes the production path.

**`BrierEvaluator`** — dimensional, **read-only** (per decision D8). Wraps a `BrierLookup` trait and emits a single `forecast_calibration` dimension (inverted: `1.0 - clamp(brier, 0, 1)` so higher = better). Confidence rises with sample size, saturating at n = 20. Phase 2 will plug the real lookup against `src/handlers/forecasts.rs`.

### Tests

16 tests, all passing:

| File | Tests |
|---|---|
| `aggregator.rs` | 5 — agreement does not flag, disagreement flags, single-evaluator never conflicts, inapplicable vs failure separation, confidence weights the mean |
| `judge.rs` | 2 — Likert normalization, out-of-range clamping |
| `scoring.rs` | 3 — perfect Brier → full calibration, coin-flip Brier → half, missing observation → `Inapplicable` |
| `tests.rs` (integration) | 6 — two-dimensional aggregation, conflict detection through registry, pre-filter short-circuit, pre-filter pass-through, inapplicable + failure recording, both reference impls running together |

### What does NOT change in Phase 1

- **No** wiring into the existing eval pipeline (`src/handlers/eval.rs`). That is **Phase 2** — at which point per-dimension scores and conflict flags become visible in real eval-run results.
- **No** new database tables. The `eval_signals` table lands with Phase 2.
- **No** Track B evaluators (Sotopia / LifelongBench / CharacterEval / WildGuard / Faithfulness). Those are designed in `EVALUATOR_DESIGN.md` and ship in parallel after Phase 2 unblocks them.
- **No** template / API / agent-card changes.

### Decisions captured in code

- Confidence-weighted mean (not unweighted) — falls back to unweighted when all confidences are zero (defensive).
- `RegistryResult` and `RegistryOutcome` are intentionally **not** `Serialize`/`Deserialize` — they are in-process orchestration values. Wire/storage formats are `AggregatedSignal` and `EvalResult` (both serialize).
- Pre-filter short-circuit threshold is configurable via `EvaluatorRegistry::with_prefilter_block_threshold(Some(t))` and can be disabled with `None` (every evaluator always runs).
- Aggregator conflict threshold is configurable via `EvaluatorRegistry::with_conflict_threshold(t)`.

## Phase 2 — Wire registry into the eval pipeline (shipped)

### Migration

`migrations/104_evaluator_signals.sql` — idempotent. Adds:

- New table `eval_signals` — one row per `(run_id, episode_id, evaluator_name, dimension)` with score, confidence, flags, provenance, persona_version, model, cost, latency, rationale. Indexed for run-level dashboard, per-agent trend analysis (Phase 3), and per-episode HITL drill-down (Phase 4).
- `eval_runs.aggregated_signal JSONB` — full serialized `AggregatedSignal` for the run.
- `eval_runs.conflict_flags JSONB` — denormalized conflict list for cheap "this run had conflicts" queries.
- `eval_runs.prefilter_blocked BOOLEAN` — true when any case in the run was blocked by a pre-filter short-circuit.

### Production `EvalModel` adapters

Two production implementations live in the `fermi` crate (not in the evaluators crate, because they pull in HTTP / sqlx / env-secrets that the evaluators crate is intentionally free of):

**`LlmJudgeAnthropic`** (`src/handlers/eval_judge.rs`)
- Implements `agent_bestiary_evaluators::LlmJudge`
- Wraps the legacy `score_with_judge` body — same Anthropic Haiku model, same prompt template, same JSON output shape (1–5 Likert on relevance/accuracy/completeness/overall plus reasoning string)
- Optional `rubric` and `expected_output` fields plumbed through from `EvalTestCase`
- Maps missing API key, HTTP errors, and JSON parse failures to `EvalError::{Provider, Malformed}`

**`BrierLookupSqlx`** (`src/handlers/eval_brier.rs`)
- Implements `agent_bestiary_evaluators::BrierLookup`
- Read-only — never recomputes Brier scores. Reads `fermi_forecasts` filtered on `agents_used @> '[{"agent_id": "..."}]'::jsonb`, mean of last `window` resolved scores (default 50)
- `AgentNameResolver` trait so callers can supply both UUID and name forms (the bestiary stores agents by UUID; forecasts may reference them by either form). The eval pipeline registers `StaticAgentNameResolver` from the already-loaded `Agent`, so no extra DB hit.

### `run_eval_cases` refactor

Behaviour change visible to operators:

1. The registry is built **once** before the case loop (`build_registry`). Judge is registered only when `judge_enabled`; Brier is always registered (it returns `Inapplicable` for agents without resolved forecasts, which the aggregator skips silently).
2. Per case, after the executor runs and the episode is stored:
   - `EpisodeBundle::from_parts(&episode, &agent, transcript, goal_spec)` — transcript synthesises `(User, query) + (Agent, reasoning)`; `goal_spec` carries the rubric + expected output when present.
   - `episode.persona_version_at_write` is stamped with `agents.persona_version` so Phase 3 drift monitoring can compare embeddings across versions.
   - `registry.run(&bundle).await` produces a `RegistryOutcome`.
   - `registry_outcome_to_signals` projects per-evaluator results into `EvalSignal` rows; persisted via `create_eval_signals` (bulk).
3. Per-case `case_results[i]` payload is **additive** (Q2.a):
   - Existing `judge_scores` field preserved (synthesised from registry's judge dimensions for callers that read it)
   - New `signal` field carries the full `AggregatedSignal` for the case
4. After the loop, `aggregate_run_signals` produces a run-level aggregate (Q1.b):
   - **Mean per dimension** across all cases (no contributions list at run level — per-case detail lives in `eval_signals`)
   - **Union of conflict-flagged dimensions**, with the maximum spread observed across cases
   - **Union of evaluator names** for active / inapplicable / failed
5. The aggregate is serialized into `eval_runs.aggregated_signal`; conflict list into `eval_runs.conflict_flags`; `prefilter_blocked` set if any case had its dimensional evaluators short-circuited.
6. **`eval_conflict` notification** fires once per run when run-level conflicts > 0 (option a). One body line per conflicted dimension naming the disagreeing evaluators and the spread. Mirrors the existing `eval_regression` notifier pattern. The HITL queue surface lands in Phase 4; until then this notification is the operator's primary hook.
7. **`detect_regression` extended**: per-dimension drops > 0.10 vs. the previous run's `aggregated_signal.per_dimension` are added to the regression list as `dim:<name>`. `format_regression_body` renders these with the dimension name and the previous → current mean.

### Legacy `score_with_judge`

Marked `#[deprecated]` but retained as `pub` for backward compat. No in-tree callers remain. May be removed in a later phase.

### What does NOT change in Phase 2

- No template / UI changes — Phase 4 owns the eval-run dashboard surface that consumes `aggregated_signal` / `conflict_flags`.
- No new HTTP endpoints — `list_eval_runs` returns the new fields automatically (memory crate update); the handler already serializes the whole `EvalRun` struct.
- Track B evaluators (Sotopia / LifelongBench / CharacterEval / WildGuard / Faithfulness) are still pending — when they ship they'll register with the same `EvaluatorRegistry` that Phase 2 wired in.

### Visible behaviour change (operator view)

Trigger an eval run on any agent with the LLM judge enabled. The completed run now has:

- A populated `aggregated_signal` JSON with per-dimension means, conflict flags, and active-evaluator list.
- A populated `eval_signals` table with one row per `(case, evaluator, dimension)` — durable record for trend analysis.
- An `eval_conflict` notification when evaluators disagreed.
- A more sensitive regression detector that catches per-dimension drops not visible to the legacy 5-point judge mean.

Run an eval against a forecasting agent (one with `agents_used` rows in `fermi_forecasts`), and `forecast_calibration` shows up as an additional dimension in the aggregate. Run it against a non-forecasting agent and the Brier evaluator silently skips itself via `Inapplicable`.

## Phase 3 — Longitudinal Observability (shipped)

### Migration

`migrations/105_longitudinal_observability.sql` — idempotent, PgBouncer-safe. Adds:

- `agent_timeline_entries` — one row per scored episode. Carries denormalized per-dimension means, drift fields, anomaly flags, persona / dyad / session context, and provenance.
- `dyad_state` — per-(agent, human) running rapport / trust / reciprocity. Includes a bounded JSON array of recent rapport for rupture detection.
- `anomaly_events` — append-only HITL-routable log. Indexed for the Phase 4 review-queue read path (`requires_review = TRUE AND resolved_at IS NULL`).
- `agent_observability_state` — per-agent worker checkpoint with last-scanned-entry pointer and run-count counters.

### New crate `agent-bestiary-observability`

Single sibling crate under `agent-bestiary/`, depending only on `agent-bestiary-memory` and `agent-bestiary-evaluators`. Module layout mirrors the architecture-doc concepts:

```
agent-bestiary/observability/
├── Cargo.toml
└── src/
    ├── lib.rs        # re-exports + module wiring
    ├── error.rs      # ObservabilityError (Inapplicable / Storage / Embedding / Invalid)
    ├── scorer.rs     # EpisodeScorer (inline timeline write)
    ├── drift.rs      # PersonaDriftMonitor + DriftThreshold (Static/Adaptive)
    ├── social.rs     # SocialInteractionTracker + detect_rupture
    ├── anomaly.rs    # AnomalyDetector + detect_in_window_with_window
    ├── trend.rs      # TrendAnalyzer + compute_series
    ├── worker.rs     # ObservabilityWorker (two-pass background scanner)
    └── tests.rs      # cross-module integration smoke tests
```

### Hybrid scheduling (D21)

Per Q4.c — the **inline scorer** writes a timeline entry on the hot path (during `run_eval_cases` in the eval handler) so the dashboard never lags. The **background worker** runs after the eval-run loop completes (spawned, non-blocking) and:

1. **Pass 1 — drift**: for each entry where `drift_norm IS NULL` and `persona_version > 1`, compares the persona_version's mean embedding to the previous version's mean. Anomalous-flag entries get `drift:anomalous` appended.
2. **Pass 2 — anomaly detection**: re-fetches entries (so flags from pass 1 are visible), runs `AnomalyDetector::detect_in_window`, persists each detected anomaly to `anomaly_events`.
3. **Checkpoint advance**: bumps `agent_observability_state.last_scanned_entry_id` so the next scan is incremental.

The worker is also independently triggerable via `ObservabilityWorker::scan_agent(agent_id)` for HTTP-driven on-demand scans (Phase 4 will surface this).

### Drift thresholds (D19)

`DriftThreshold` enum with two variants:

- `Static(f64)` — the production default. Per-agent override read from `agents.capability_gates.drift_threshold`, falls back to the platform-default `0.20`.
- `Adaptive { window, sigma_multiplier, min_samples }` — flagged anomalous when `drift_norm > rolling_mean + sigma_multiplier * rolling_stddev` over the last `window` observations, requiring at least `min_samples` data points.

Phase 3 ships infrastructure for both modes; production runs on `Static`. The architecture-doc Q2 plan was to revisit once we have N episodes per persona_version that make rolling stddev meaningful.

### Anomaly detector — four kinds (D20)

| Kind | Source flag | Detection |
|---|---|---|
| `safety` | `safety:*` on entry's `anomaly_flags` | Pre-filter evaluator wrote it; surfaced as `critical` |
| `drift` | `drift:anomalous` on entry's `anomaly_flags` (written by worker pass 1) | `warning` severity, payload carries `drift_norm` and `persona_version` |
| `rolling_conflict` | Same `conflict:<dim>` appears in N consecutive entries (default N=3) | `warning`, payload carries dimension name and contributing entry_ids |
| `rupture` | `rupture:<dyad_id>` on entry's `anomaly_flags` (written by `SocialInteractionTracker`) | `warning`, payload carries dyad_id |

The `detect_in_window` method is implemented as a thin delegate over a free function `detect_in_window_with_window` so the algorithmic logic is pure-testable without constructing a `MemoryStore`.

### Social tracker (scaffolding values per D19 spirit)

`SocialInteractionTracker` updates dyad rapport / trust / reciprocity using exponential smoothing (α = 0.3) against signals from the registry's per-dimension means. Mapping (placeholder until Track B evaluators score these explicitly):

- `rapport` ← signal's `rapport` dimension
- `trust` ← signal's `persona_fidelity` dimension ("do I get the same agent each time?")
- `reciprocity` ← mean of `social_capital` and `goal_completion`

Treat values as scaffolding-quality until Track B evaluators (Sotopia, CharacterEval) ship to populate these dimensions properly.

Rupture detection: any peak-to-trough drop > 0.20 within the rolling 5-entry rapport history triggers a `rupture:<dyad_id>` flag on the entry, which the anomaly detector picks up.

### Trend analyzer (on-demand, D22)

`TrendAnalyzer::compute(agent_id, window)` returns a per-dimension summary (mean, std_dev, min, max, n, latest) over the agent's most-recent N timeline entries. No caching — Phase 4 owns dashboard caching once we know read shapes.

### Eval pipeline integration

`src/handlers/eval.rs::run_eval_cases` updated to:

1. Stamp `episode.persona_version_at_write = db_agent.persona_version`
2. Stamp `episode.dyad_id = "eval:<agent_id>:<user_id>"` (Q1.a — eval-only path)
3. After registry runs: `EpisodeScorer::write_inline(episode, &outcome.signal, run_id, session_id)` (inline timeline write)
4. After case loop: `tokio::spawn` an `ObservabilityWorker::scan_agent` call (best-effort, non-blocking; errors logged not surfaced)

### Tests

24 tests across 7 modules, all algorithmic / pure-function (no DB):

| Module | Tests |
|---|---|
| `drift.rs` | 6 — cosine math, static threshold, adaptive warmup, capability-gates override |
| `social.rs` | 5 — rupture detection in steady / sharp-drop / gradual-decline histories, smoothing math |
| `anomaly.rs` | 5 — safety / rolling_conflict / rupture / drift detection, no-flag baseline |
| `scorer.rs` | 1 — dim_scores serialization |
| `trend.rs` | 4 — empty / mean+stddev / latest-by-timestamp / dimension independence |
| `tests.rs` | 3 — integration: cosine→drift→threshold pipeline, series+anomaly, rupture isolation |

### What does NOT change in Phase 3

- **No UI** — Phase 4 owns the observatory dashboard. The new tables exist; rendering happens later.
- **No new HTTP endpoints** — `ObservabilityWorker::scan_agent` is wired into the eval pipeline; the manual-trigger HTTP endpoint lands in Phase 4 alongside the dashboard.
- **No backfill** (D23) — the timeline is forward-only from deploy. Charts in Phase 4 will simply show "history starts here."
- **No Phase 5 hook yet** — the coherence agent's role as gatekeeper for HITL writes lands in Phase 5, not here.

### Visible behaviour change

After deploying Phase 3 and triggering an eval run:

- New rows appear in `agent_timeline_entries` (one per case)
- The `tokio::spawn`'d worker completes ~ms-to-seconds later (non-blocking on the run completion)
- New rows may appear in `anomaly_events` if drift / rolling-conflict / rupture / safety conditions fire
- `dyad_state` accumulates running rapport for `eval:<agent_id>:<user_id>` dyads
- `agent_observability_state` updates per-agent counters

Operators see this through `psql` and the existing `eval_runs` / `case_results` JSON payload (which already exposes `aggregated_signal`). Phase 4 will surface it visually.

## Phase 4 — HITL + Observatory UI (shipped)

### Migration

`migrations/106_hitl_actions.sql` — idempotent. Adds:

- `hitl_actions` — append-only audit trail of reviewer decisions on `anomaly_events`. One row per reviewer-action; the same anomaly may have multiple rows. Includes a DB-level UPDATE-blocking trigger (same pattern as `episode_corrections` from Phase 0).
- Indexes for queue read by anomaly / agent / reviewer.

### Storage

In `agent-bestiary-memory`:

- `HitlAction` type
- `MemoryStore::create_hitl_action`, `list_hitl_actions_for_anomaly`, `resolve_anomaly_event`, `get_anomaly_event`

### JSON API (registered on `api-server`)

```text
GET  /api/observatory/agents/:id/timeline?window=N   →  TrendReport + recent timeline entries
GET  /api/observatory/agents/:id/dyads               →  list_dyads_for_agent
GET  /api/observatory/agents/:id/anomalies?limit=N   →  list_anomaly_events_for_agent
POST /api/observatory/agents/:id/scan                →  ObservabilityWorker::scan_agent (synchronous; returns ScanReport)
GET  /api/observatory/hitl?limit=N                   →  list_pending_anomaly_events, ownership-filtered
POST /api/observatory/hitl/:event_id/action          →  approve | relabel (intervene returns 501 — Phase 5)
```

All routes require auth. Per **D24**: agent-scoped routes require owner-of-agent OR platform admin. The HITL queue route returns all pending events to admins; for non-admins it filters in-process to events on agents they own (Phase 4 scale; queue grows → push to SQL).

### HTML pages

Pattern matches existing production pages (`dashboard.html`, `agent_detail.html`): static HTML+CSS+JS templates served as files via `pages.rs`, fetching data client-side from the JSON API. No Askama derives, no chart library, no live updates (per **D27**).

- **`templates/observatory.html`** — per-agent dashboard. Header bar (agent + persona-version + window). Per-dimension trend bars (mean as %, σ, latest direction arrow). Anomaly list. Dyad table (rapport / trust / reciprocity / n). Timeline list (newest first, with provenance badge + drift_norm + flags). Manual `Trigger Scan` button.
- **`templates/observatory_hitl.html`** — review queue. One row per pending event with kind badge, severity, payload (collapsible), and three action buttons. Approve / Relabel POST to the action endpoint and remove the row optimistically. Intervene is `disabled` with a "Phase 5" tooltip per **D26**.

Routes registered:
- `GET /observatory` (with optional `?agent=<id>` deep-link)
- `GET /observatory/hitl`

### Eval-run integration on `agent_detail.html` (D29)

The existing eval-run history rows are extended with a third line surfacing the Phase 2 registry data:

- Per-dimension means as `<dim> <pct>%` chips, color-coded by score band
- `⚠ N conflicts` pill when `conflict_flags` is non-empty
- `prefilter blocked` indicator when the run was short-circuited by a pre-filter

Plus an `Observatory →` link in the Run History header that pivots to `/observatory?agent=<agent_id>`. The legacy `judge_scores` rendering and `regression_detected` flag stay — additive, no behaviour change for existing readers.

### Cross-links from `dashboard.html`

The Collection section header gains two small links: `Observatory` and `Review queue`. Visible only when authed users land on the dashboard. Matches the dashboard's existing "+ New" button affordance.

### Phase 5 surface preparation

Phase 4 wires `intervene` end-to-end **except** for the actual destructive step. The button exists but disabled; the API endpoint accepts the action keyword but returns `501 Not Implemented` with a clear payload. When Phase 5 lands the coherence gate + two-write memory pattern, the button enables and the 501 becomes a 200 — the surrounding UX, audit trail, and routing already exist.

### What does NOT change in Phase 4

- **No live-updating charts** — server returns latest data on page load. Phase 6 polish concern.
- **No two-reviewer consensus** for `agent_wide` interventions — that's a Phase 5 deliverable; the architecture-doc requirement is surfaced in the UI tooltip but not enforced in the data layer.
- **No dedicated reviewer role** — Q1.c was deferred. Owner + admin only (Q1.a + admin override).
- **No trend snapshot caching** — D22 deferred this. `TrendAnalyzer::compute` runs on every page load.
- **No new HTTP routes for cross-agent global views** beyond the HITL queue. The existing `/dashboard` is user-scoped; admin global view is the HITL queue itself.

### Visible behaviour change (operator view)

After deploying Phase 4:

- `/observatory?agent=<id>` shows live per-agent timeline, drift, dyads, anomalies for any agent the user owns (or any agent for admins).
- `/observatory/hitl` shows the pending review queue. Owners see anomalies on their agents; admins see all.
- The agent-detail page's eval-run history now renders per-dimension scores and flags conflicts, instead of just pass-rate + judge mean.
- A reviewer can `Approve` a noted anomaly (records to `hitl_actions`, marks `resolved_at` on the event) or `Relabel` it (same plus `score_overrides` payload). The queue refreshes; resolved events drop off.
- `Intervene` is visibly present but disabled until Phase 5.
- `Trigger Scan` on the observatory page invokes the worker on demand and returns the report inline.

## Phase 5 — Intervention Feedback Loop (shipped)

### Migration

`migrations/108_intervention_feedback_loop.sql` — idempotent. Adds:

- `two_reviewer_requests` — pending two-reviewer consensus records for `agent_wide`
  interventions. Unique-partial index ensures only one pending request per anomaly.
  `updated_at` maintained by trigger. Status: `pending` | `approved` | `rejected` | `expired`.

### New crate `agent-bestiary-coherence-gate`

```
agent-bestiary/coherence-gate/
├── Cargo.toml
└── src/
    ├── lib.rs         # re-exports
    ├── error.rs       # GateError (Blocked, AwaitingSecondReviewer, Storage, ...)
    ├── encoder.rs     # InterventionEncoder + InterventionRequest + EncodedIntervention
    ├── gate.rs        # CoherenceGate + GateOutcome + GateVerdict
    ├── two_write.rs   # TwoWriteMemory + TwoWriteReceipt
    └── tests.rs       # 11 unit tests
```

### Intervention encoder (step 2)

`InterventionEncoder::encode(req)` validates and stamps:
- `authority_weight = 1.0` (HumanAuthority)
- `provenance = HumanCorrected`
- `gate_is_synchronous = true` for `AgentWide`; `false` for `Episode`/`Dyad`
- Enforces `classification` + `correction_text` required for `AgentWide`

### Coherence gate (step 3)

`CoherenceGate::check(&encoded)` builds a minimal two-utterance `CoherenceSystem`:
- U0 = existing agent response, U1 = proposed correction
- `Contradicts` incoherence relation between them
- Runs `SettlingEngine::settle` → reads `system.global_coherence.score` (Γ(C))
- **Synchronous** (`AgentWide`): Γ(C) < 0.5 → `Err(GateError::Blocked)`
- **Settler** (`Episode`/`Dyad`): always approves, stores outcome for audit
- Returns `GateOutcome` with `gamma`, `principle_scores`, `tensions`, `minimum_update_set`

Threshold configurable via `CoherenceGate::new(threshold)`. Default 0.5 (OQ-5).

### Two-write memory pattern (step 4)

`TwoWriteMemory::execute(&encoded, &gate_outcome, original_episode)`:

1. **Write 1 — Synthetic episode**: new `Episode` row with `provenance = SyntheticCorrection`,
   `authority_weight = 1.0`, context carries `corrected_response` + reviewer metadata.
2. **Write 2 — Annotation**: `episode_corrections` row linking original episode, reviewer,
   scope, classification, `coherence_check` (full `GateOutcome`), `minimum_update_set`,
   `tensions_flagged`, and `synthetic_episode_id`.
3. **Persona version bump** (AgentWide only): `MemoryStore::bump_persona_version` increments
   `agents.persona_version` and returns the new value.

### Two-reviewer consensus (step 4a for AgentWide)

For `agent_wide` scope:
1. First reviewer submits `POST /api/observatory/hitl/:event_id/action`
   `{action:"intervene", scope:"agent_wide", ...}`.
2. Encoder validates, gate approves (or blocks with 422). If approved:
   - A `two_reviewer_requests` row is created (status=`pending`).
   - Response: 200 with `status:"awaiting_second_reviewer"` + `request_id`.
3. Second reviewer (must be a different user) confirms via
   `POST /api/observatory/hitl/consensus/:request_id` `{approved:true}`.
4. Handler verifies reviewer differs, re-runs gate, executes `TwoWriteMemory`, marks anomaly resolved.

### HTTP API changes

New route: `POST /api/observatory/hitl/consensus/:request_id` → `confirm_two_reviewer_handler`

`record_hitl_action_handler` now accepts additional fields for `intervene`:
- `scope` ("episode" | "dyad" | "agent_wide")
- `classification` ("belief" | "behaviour")
- `dimension`, `correction_text`, `justification`

### UI changes

`templates/observatory_hitl.html`:
- `Intervene` button is now active (no longer disabled).
- Clicking opens a modal with scope/classification/dimension/correction fields.
- `agent_wide` submissions show a consensus-pending banner with the `request_id`
  and instructions for the second reviewer.

### Memory store additions

- `MemoryStore::bump_persona_version(agent_id)` — direct persona_version increment
- `MemoryStore::create_two_reviewer_request`
- `MemoryStore::get_pending_two_reviewer_request(anomaly_event_id)`
- `MemoryStore::get_two_reviewer_request(request_id)`
- `MemoryStore::confirm_two_reviewer_request(request_id, reviewer, approved, ...)`

### Tests

11 tests, all passing:

| File | Tests |
|---|---|
| `encoder.rs` / `tests.rs` | 4 — episode minimal, agent_wide requires classification, agent_wide requires correction_text, agent_wide gate_is_synchronous |
| `gate.rs` / `tests.rs` | 7 — episode settles, dyad settles, agent_wide approves threshold=0, agent_wide blocks threshold=1, principle scores present, minimum_update_set is Vec, threshold constant |

### Decisions captured

| # | Decision |
|---|---|
| D30 | Phase 5 coherence gate uses a minimal two-utterance system (U0=existing, U1=correction) with `Contradicts` incoherence. Simple, deterministic, no LLM call needed for the gate itself. |
| D31 | Write order: synthetic episode first (to get `synthetic_episode_id`), then annotation. Avoids a separate UPDATE on the correction row. |
| D32 | `bump_persona_version` is a direct SQL UPDATE (not via `agent_versions` INSERT) because HITL interventions create a synthetic episode, not an agent version row. |
| D33 | Two-reviewer flow: first reviewer creates the `two_reviewer_requests` row; second reviewer calls a separate endpoint. Row stores the full `EncodedIntervention` as JSONB so the second reviewer sees exactly what was proposed. |

## Open questions deferred to later phases

- **OQ-1 (Phase 2):** When the registry detects an evaluator conflict, do we surface it as a notification (`create_notification`, like the existing regression notifier) or only via the dashboard? *Defaults to notification when an HITL queue does not yet exist.*
- **OQ-2 (Phase 3):** What window size does the trend analyser use for rolling means? Per-agent configurable, or platform default? *Likely a default of 50 episodes with override on the agent card.*
- **OQ-3 (Phase 3):** What threshold defines "drift exceeds θ" for the anomaly detector? Cosine distance, std-dev units, or learned per-agent? *Suggest start with cosine > 0.15 and revisit after first run.*
- **OQ-4 (Phase 4):** Two-reviewer consensus for `agent_wide` interventions (per the architecture doc). Where does the second reviewer's queue live? *Either reuse the same HITL queue with a `requires_second_reviewer` flag, or create a separate consensus table.*
- **OQ-5 (Phase 5):** Coherence gate threshold for `agent_wide` interventions. The doc says "tension flagging, no silent overwrites" — what's the numeric threshold on `Γ(C)` below which the gate blocks the write? *Suggest 0.5 initially.*
- **OQ-6 (cross-cutting):** Brier as a global expectation-vs-outcome KPI — exact definition is still under development per user. *Track in a separate spec; this doc only references the wrapper.*

## Glossary (architecture-doc terms ↔ this codebase)

| Doc term | Code term |
|---|---|
| `EpisodeBundle` | `agent_bestiary_memory::EpisodeBundle` |
| `EvalModel` (trait) | (Phase 1) `agent_bestiary_evaluators::EvalModel` |
| `EvalTier` | `EvalTier::PreFilter`, `EvalTier::Dimensional` |
| Aggregated signal | `AggregatedSignal { per_dim, conflicts, ... }` |
| Conflict flag | `AggregatedSignal::conflicts: Vec<Dimension>` |
| Agent timeline store | (Phase 3) `agent_timeline_entries` table |
| Dyad | `(agent_id, human_id)` — `dyad_id` is a deterministic hash |
| HumanAuthority | `Provenance::HumanCorrected` or `Provenance::SyntheticCorrection` with `authority_weight = 1.0` |
| Synthetic corrected episode | A second episode row with `provenance = synthetic_correction`, pointed to by `episode_corrections.synthetic_episode_id` |
| Coherence check | (Phase 5) `agent_bestiary_coherence::settle` against the proposed update set |
| Minimum update set | (Phase 5) the subset of world-model nodes whose activations change after settling |
