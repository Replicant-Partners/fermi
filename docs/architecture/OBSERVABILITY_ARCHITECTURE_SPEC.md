# Social Agent Observability Platform — Unified Architecture Design Specification

**Version:** 1.0  
**Date:** 2026-05-13  
**Status:** Reference specification — synthesises all six implementation phases,
the original architecture design document, and relevant research literature.

---

## Table of Contents

1. [Purpose and Scope](#1-purpose-and-scope)
2. [System Context](#2-system-context)
3. [Design Philosophy](#3-design-philosophy)
4. [Architectural Planes](#4-architectural-planes)
5. [Plane A — Agent Capabilities Foundation](#5-plane-a--agent-capabilities-foundation)
6. [Plane B — Evaluator Registry](#6-plane-b--evaluator-registry)
7. [Plane C — Longitudinal Observability](#7-plane-c--longitudinal-observability)
8. [Plane D — Human Surfacing and Intervention Feedback](#8-plane-d--human-surfacing-and-intervention-feedback)
9. [Data Model](#9-data-model)
10. [Execution and Scheduling Model](#10-execution-and-scheduling-model)
11. [Security and Access Control](#11-security-and-access-control)
12. [API Reference](#12-api-reference)
13. [Recursive Improvement Loop](#13-recursive-improvement-loop)
14. [Track B — Native Evaluator Family](#14-track-b--native-evaluator-family)
15. [Research Foundations](#15-research-foundations)
16. [Decision Log](#16-decision-log)
17. [Glossary](#17-glossary)
18. [Open Questions and Future Work](#18-open-questions-and-future-work)

---

## 1. Purpose and Scope

This document is the unifying architecture design specification for the **Social Agent
Observability Platform** (the "observability stack") as implemented in the
Agent Bestiary / Fermi codebase. It covers the complete system from episode
ingestion through multi-dimensional evaluation, longitudinal monitoring,
anomaly detection, human-in-the-loop (HITL) review, and the coherence-gated
intervention feedback loop.

**What this document is:**
- A single canonical reference that reconciles the original architecture intent
  (`social_agent_observability_architecture.html`) with the running implementation
  (`OBSERVABILITY_IMPL.md` Phases 0–6 and all supporting source)
- A design rationale document explaining *why* each component was built as it was
- A research-grounded specification that maps implementation decisions to their
  academic and engineering precedents

**What this document is not:**
- A migration guide or changelog (see `OBSERVABILITY_IMPL.md` Phase status table)
- An API tutorial (see §12 for the formal API reference)
- A Track B evaluator implementation guide (see `EVALUATOR_DESIGN.md`)

**Scope boundary:** This specification covers the domain-specific behavioral
observability of AI agents. It does not cover infrastructure observability
(no Prometheus/Grafana/OpenTelemetry integration exists or is planned), the
swarm telemetry subsystem (`swarm_telemetry` / `swarm_sessions`), or the
platform credit and billing systems.

---

## 2. System Context

The observability stack lives inside the **Agent Bestiary World (ABW)** platform,
a multi-tenant Rust/Axum service backed by PostgreSQL (Neon, pgvector enabled).

```
┌──────────────────────────────────────────────────────────────────┐
│                        ABW Platform                              │
│                                                                  │
│  Agent Executor ──► Episode Store ──► Evaluator Registry        │
│        │                                    │                    │
│        │                           Longitudinal Observer         │
│        │                                    │                    │
│        │                           Anomaly Detector              │
│        │                                    │                    │
│        │                           HITL Review Queue             │
│        │                                    │                    │
│        └────────────────────────► Coherence Gate                │
│                                            │                    │
│                                   Memory Writer                  │
│                                  (Two-Write Pattern)             │
│                                                                  │
│  Observatory UI  ◄──────────────── All of the above            │
└──────────────────────────────────────────────────────────────────┘
```

The platform uses `tracing 0.1` (Tokio ecosystem) for structured logging
throughout all workspace crates. There are no external observability tool
dependencies. All observability state is domain data stored in the same
PostgreSQL instance as the agents, episodes, and users.

### Workspace crates involved

| Crate | Path | Role |
|---|---|---|
| `agent-bestiary-memory` | `agent-bestiary/memory` | ADM store: foundation types, episode I/O, all observability table R/W |
| `agent-bestiary-evaluators` | `agent-bestiary/evaluators` | EvalModel trait, registry, aggregator, reference impls |
| `agent-bestiary-observability` | `agent-bestiary/observability` | Longitudinal scorer, drift monitor, social tracker, anomaly detector, trend analyser, background worker |
| `agent-bestiary-coherence-gate` | `agent-bestiary/coherence-gate` | Intervention encoder, coherence gate (TEC settling), two-write memory pattern |
| `fermi` (lib) | `.` | HTTP handlers wiring all of the above; production EvalModel adapters (`LlmJudgeAnthropic`, `BrierLookupSqlx`) |

---

## 3. Design Philosophy

Seven principles shaped every architectural decision:

### 3.1 Incremental, phase-gated construction

The stack was built in six sequential phases (Track A) plus one parallel track
(Track B). Each phase adds exactly one concern and ships with its own database
migration, one or more new Rust modules, and a defined test surface. This
sequencing lets each phase validate the foundations of the next and keeps PRs
reviewable.

### 3.2 Clean factoring — one crate per concern

Infrastructure helpers (`agent-bestiary-memory`), evaluation primitives
(`agent-bestiary-evaluators`), longitudinal monitoring
(`agent-bestiary-observability`), and the intervention gate
(`agent-bestiary-coherence-gate`) are separate crates with declared dependencies.
The application crate (`fermi`) holds only the HTTP adapters and wiring that pull
in platform-specific secrets (API keys, DB pool). This means the algorithmic core
is independently testable and deployable.

### 3.3 Episode immutability as an audit guarantee

Original episodes are never mutated. The `episode_corrections` table (backed by
a DB-level UPDATE-blocking trigger) and the `hitl_actions` table enforce
append-only semantics. The two-write memory pattern creates *new* episodes
(`provenance = SyntheticCorrection`) rather than overwriting old ones. This
preserves a complete, tamper-evident audit trail from first execution to HITL
intervention.

### 3.4 Authority weight as a signal quality dimension

Every episode and correction carries an `authority_weight` in `[0, 1]`. Automated
evaluation defaults to `0.5`. Human corrections carry `authority_weight = 1.0`
(HumanAuthority). This design enables downstream consumers (trend analysers,
calibration fitters) to weight human-originated signals higher without binary
if/else branches.

### 3.5 Hybrid scheduling — inline hot path + background cold path

The timeline entry for an episode is written synchronously during the eval
pipeline run so the dashboard never lags. Computationally expensive operations
(drift computation, anomaly scanning) are deferred to the `ObservabilityWorker`
which runs on-demand, non-blocking, via `tokio::spawn`. This mirrors the
`ConsolidationWorker` (dreaming) scheduling pattern used elsewhere in the platform.

### 3.6 Coherence as a write gate, not a write oracle

The coherence engine is used as a *gate* on the intervention feedback loop
(`AgentWide` scope), not as an oracle that decides what the correction should be.
A reviewer supplies the correction text; the gate decides whether applying it
would produce a coherent world-model update. The TEC settling engine quantifies
resistance to change; low Γ(C) blocks the write. For narrower scopes
(`Episode`, `Dyad`) the gate runs in settler mode — it records the coherence
outcome for audit but always approves the write.

### 3.7 Observability state is domain data, not infrastructure telemetry

The signals this stack produces — eval dimension scores, persona drift vectors,
dyad rapport trajectories, anomaly events, HITL corrections — are semantically
rich, relational domain data. They are not infrastructure telemetry (latency
histograms, error rates, resource consumption). That distinction drives a
consequential architectural choice: the entire observability state lives in the
same PostgreSQL database as agents, episodes, and users.

This co-location is not laziness; it is the correct fit. It means:

- **Joins are free.** An observatory query can filter timeline entries by agent
  tier, join against eval_signals for per-dimension detail, and cross-reference
  episode provenance in a single SQL statement. A separate time-series backend
  would require application-layer joins across two stores.
- **Relational integrity is enforced.** Foreign keys, UPDATE-blocking triggers,
  and CHECK constraints on `authority_weight` and `provenance` are only available
  in a relational store. They are the mechanism by which the append-only audit
  trail is guaranteed.
- **The schema is the contract.** JSONB payloads (e.g. `anomaly_events.payload`,
  `eval_runs.aggregated_signal`) carry kind-specific structure; the surrounding
  schema provides typed anchors (agent_id, episode_id, run_id FKs) that
  constrain queries and maintain referential consistency.
- **The Plane D feedback loop closes in the same transaction space.** Writing a
  synthetic corrected episode, appending an `episode_corrections` row, and
  bumping `persona_version` are three writes that span the observability and
  agent tables. They are coherent operations in PostgreSQL; they would be
  distributed coordination problems across separate backends.

---

## 4. Architectural Planes

The stack is organized into four vertical planes that progress from raw execution
data to human-actionable surfaces. Each plane's output is the next plane's input.

```
Plane A ── Agent capabilities + episodic memory
            │
            ▼
Plane B ── Evaluator registry → multi-dimensional scoring → AggregatedSignal
            │
            ▼
Plane C ── Longitudinal monitoring: timeline, drift, dyads, anomalies
            │
            ▼
Plane D ── HITL surfacing: observatory UI, review queue, intervention feedback loop
```

The planes are not runtime layers in a call stack. They are conceptual groupings
of concerns. Plane D components call back into Plane A (writing synthetic episodes)
and Plane B (the coherence gate runs the TEC settling engine from the coherence
subsystem).

---

## 5. Plane A — Agent Capabilities Foundation

### 5.1 Purpose

Plane A provides the data primitives that the observability stack reads from and
writes to. It defines what an agent *is*, what an episode *is*, and how they
relate to each other across time. Plane A was extended in Phase 0 to carry the
fields that the higher planes depend on.

### 5.2 Agent identity and persona versioning

Every agent has a `persona_version` integer that serves as the drift baseline
anchor. It increments on two triggers:

1. **System-prompt / model / visibility edits** — a `bump_agent_persona_version()`
   database trigger fires on every `agent_versions` INSERT and automatically
   increments the counter.
2. **Agent-wide HITL interventions** — the `TwoWriteMemory` pattern calls
   `MemoryStore::bump_persona_version(agent_id)` directly after a successful
   `AgentWide` scope two-write.

This design means persona drift is always measured against a version boundary
that corresponds to a real change event, not an arbitrary clock tick.

### 5.3 Episode schema extensions (Phase 0)

```sql
episodes.provenance              TEXT  -- auto_pass | auto_fail | human_approved |
                                       -- human_relabeled | human_corrected |
                                       -- synthetic_correction
episodes.authority_weight        FLOAT -- 0.0..1.0; default 0.5
episodes.dyad_id                 TEXT  -- deterministic hash of (agent_id, human_id)
episodes.persona_version_at_write INT  -- snapshot at write time for drift computation
```

The `provenance` enum is the canonical source of truth for "where did this
episode come from?" The six values form a total order of human involvement:
`auto_pass` (fully automated) → `synthetic_correction` (human-derived, maximum authority).

The `dyad_id` is a deterministic string identifier for a `(agent_id, human_id)`
pair. It is currently populated only for eval-pipeline executions
(`eval:<agent_id>:<user_id>`) per decision D18. Workspace and agent-to-agent
dyads are deferred.

### 5.4 EpisodeBundle — the normalized input contract

`EpisodeBundle` (`agent-bestiary/memory/src/bundle.rs`) is the normalized
representation of an episode that all evaluators consume. It packages:

- The episode's `query`, `response`, `context`
- A structured `TranscriptTurn` vector (role: `User | Agent`, content)
- An `AgentCardSnapshot` (system prompt, agent type, capability gates)
- `goal_spec` — the optional social goal for Sotopia-style evaluators
- `dyad_id` and `persona_version_at_write`

The bundle is assembled once per episode in the eval pipeline and passed to
`EvaluatorRegistry::run`. This assembly point is the only place that couples
the memory store's raw types to the evaluator interface.

### 5.5 Episode corrections (immutable audit trail)

The `episode_corrections` table records every HITL reviewer action at the
episode level. A DB trigger blocks all UPDATEs; only INSERTs are permitted.
Each row captures the reviewer identity, scope, classification, correction text,
score overrides, coherence check outcome, minimum update set, and a pointer to
any synthetic corrected episode created by the two-write pattern.

---

## 6. Plane B — Evaluator Registry

### 6.1 Purpose

Plane B provides a composable, extensible evaluation framework that runs
multiple evaluators against an `EpisodeBundle` in a single invocation and
produces an `AggregatedSignal` that summarizes multi-dimensional behavioral
quality scores, detected inter-evaluator conflicts, and pre-filter block status.

### 6.2 The EvalModel trait

Every evaluator implements the same async trait:

```rust
#[async_trait]
pub trait EvalModel: Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn tier(&self) -> EvalTier;          // PreFilter | Dimensional
    fn dimensions(&self) -> Vec<Dimension>;
    async fn evaluate(&self, bundle: &EpisodeBundle) -> Result<EvalResult, EvalError>;
}
```

`EvalResult` carries per-dimension scores clipped to `[0.0, 1.0]`, a confidence
value, optional flags (e.g. `safety:violence`), model identifier, latency, cost,
and a one-line rationale for HITL surfaces.

The `EvalError::Inapplicable` variant is the canonical "this evaluator does not
apply to this episode" skip path. Inapplicable evaluators are silently recorded
in `signal.inapplicable_evaluators` and do not abort the registry run.

### 6.3 Two-tier execution model

The registry separates evaluators into two tiers with different execution
semantics:

**Pre-filter tier** (runs serially, first):
- Cheap, deterministic or near-deterministic checks
- Any pre-filter that scores any dimension below `prefilter_block_threshold`
  (default 0.5) short-circuits the registry: dimensional evaluators are skipped
  and `prefilter_blocked = true` is set on the run
- Use cases: safety classification (WildGuard), faithfulness grounding check

**Dimensional tier** (runs concurrently, after all pre-filters pass):
- LLM-based or embedding-based multi-dimensional scoring
- All dimensional evaluators run via `futures::future::join_all`
- A failing evaluator captures its error in `signal.failed_evaluators` and does
  not abort other evaluators
- Use cases: social intelligence (Sotopia), persona fidelity (CharacterEval),
  lifelong consistency (LifelongBench), forecast calibration (Brier)

This two-tier design is an application of the **fail-fast pre-check** pattern:
cheap safety and grounding checks gate the expensive multi-dimensional scoring,
reducing both cost and latency for the common case where an episode is unsafe
or unfaithful.

### 6.4 Aggregation and conflict detection

After all evaluators complete, the `Aggregator` computes:

1. **Confidence-weighted mean** per dimension across all contributing evaluators.
   Falls back to unweighted mean when all confidences are zero.
2. **Conflict flag** per dimension when ≥ 2 evaluators scored it and
   `max(scores) - min(scores) > conflict_threshold` (default 0.20). This
   threshold matches the architecture document mock and is configurable.
3. **Union of flags** across all contributing evaluators.

The result is an `AggregatedSignal`:

```rust
pub struct AggregatedSignal {
    pub per_dimension: Vec<DimensionSummary>,  // mean, confidence, contributors
    pub conflicts: Vec<Dimension>,             // dimensions with evaluator disagreement
    pub flags: Vec<EvalFlag>,                  // union of all evaluator flags
    pub prefilter_blocked: bool,
    pub active_evaluators: Vec<String>,
    pub inapplicable_evaluators: Vec<String>,
    pub failed_evaluators: Vec<String>,
}
```

The `AggregatedSignal` is the canonical wire and storage format. It is
serialized to `eval_runs.aggregated_signal` (run-level mean) and to
`eval_signals` rows (per-evaluator, per-dimension granularity).

### 6.5 Production adapters

Two production `EvalModel` implementations live in the `fermi` application crate
because they depend on platform-specific infrastructure:

**`LlmJudgeAnthropic`** (`src/handlers/eval_judge.rs`):
- Implements the `LlmJudge` trait from `agent-bestiary-evaluators`
- Calls Anthropic Haiku with the structured judge prompt
- Normalizes 1–5 Likert scores on `relevance`, `accuracy`, `completeness`,
  `overall` to `[0.0, 1.0]`
- Maps API/parse errors to `EvalError::{Provider, Malformed}`

**`BrierLookupSqlx`** (`src/handlers/eval_brier.rs`):
- Implements the `BrierLookup` trait from `agent-bestiary-evaluators`
- Read-only — reads `fermi_forecasts` for resolved forecasts where the agent
  was used
- Returns `EvalError::Inapplicable` for non-forecasting agents (no resolved
  forecasts found)
- Inverted scoring: `forecast_calibration = 1.0 - clamp(brier_score, 0, 1)`
  so higher is better, consistent with all other dimensions

### 6.6 Per-evaluator signal persistence

Every `(run_id, episode_id, evaluator_name, dimension)` tuple produces one row
in `eval_signals`. The table supports:

- Run-level dashboard queries (all signals for a run)
- Per-agent trend analysis (all signals for an agent across time, by dimension)
- Per-episode HITL drill-down (all evaluator opinions on a single episode)
- Per-evaluator quality tracking (evaluator version splits for calibration)

The `evaluator_version` column enables before/after analysis when an evaluator's
prompt or weights change — a requirement noted in the Track B design doc (Q-CC4).

### 6.7 Regression detection

After aggregating run-level signals, the eval handler (`src/handlers/eval.rs`)
compares the new `aggregated_signal.per_dimension` against the previous run's
`aggregated_signal`. Any per-dimension drop > 0.10 is added to the regression
notification body as `dim:<name>: prev% → curr%`. This supplements the legacy
judge-mean regression detector, which only observes the blended score.

---

## 7. Plane C — Longitudinal Observability

### 7.1 Purpose

Plane C turns the per-episode `AggregatedSignal` snapshots from Plane B into
a longitudinal view of agent behavior across time. It maintains four persistent
data structures: the agent timeline, dyad state, anomaly events, and worker
checkpoints. It runs the mathematical machinery that transforms those data
structures into actionable signals: persona drift monitoring, social rupture
detection, and multi-type anomaly scanning.

### 7.2 Agent timeline entries

The `agent_timeline_entries` table is the primary read path for the observatory
dashboard. Each row corresponds to one episode that passed through the eval
registry and carries:

- Denormalized `dim_scores` (JSON map of dimension → mean score) for chart
  queries without joins
- `drift_norm` — the cosine-distance-based drift from the previous persona
  version's embedding mean (written by the background worker, null until computed)
- `within_version_cosine` — cohesion within the current persona version
  (distinguishes desired cross-version drift from undesired within-version noise)
- `anomaly_flags` — a JSON array of string flags written by the scorer, drift
  monitor, and social tracker during inline and background passes
- `provenance`, `persona_version`, `dyad_id`, `session_id` for slicing

### 7.3 Persona drift monitor

The `PersonaDriftMonitor` (`agent-bestiary/observability/src/drift.rs`) computes
embedding-based drift between consecutive `persona_version` baselines:

```
drift_norm = 1.0 - cosine_similarity(mean_embedding_v_n, mean_embedding_v_{n+1})
```

Where:
- `mean_embedding_v_n` = mean of up to `baseline_window` (default 50) episode
  embeddings at `persona_version = n`
- `cosine_similarity(a, b) = (a · b) / (‖a‖ · ‖b‖)`

This formulation is equivalent to the embedding-based persona consistency
approach proposed for the LifelongBench evaluator in `EVALUATOR_DESIGN.md §§`
and is consistent with standard representation similarity analysis in the
NLP/ML literature.

**Threshold modes** (decision D19):

| Mode | Formula | When to use |
|---|---|---|
| `Static(θ)` | `drift_norm > θ` | Default; θ = 0.20 per-platform, overridable per agent via `capability_gates.drift_threshold` |
| `Adaptive { window, σ, min_samples }` | `drift_norm > mean(recent[window]) + σ * std(recent[window])` | Future; requires ≥ `min_samples` data points before activating |

The adaptive mode ships as infrastructure but defaults to static in production
(decision D19). Per-agent threshold overrides are read from
`agents.capability_gates["drift_threshold"]` at scan time, enabling high-drift
agents (e.g. those undergoing rapid persona evolution via HITL) to have a looser
threshold without platform-wide changes.

### 7.4 Social interaction tracker

The `SocialInteractionTracker` (`agent-bestiary/observability/src/social.rs`)
maintains three per-dyad relational axes using **exponential smoothing**
(α = 0.3):

```
new_value = α * observed + (1 - α) * previous
```

| Axis | Source dimension from AggregatedSignal | Default (neutral) |
|---|---|---|
| `rapport` | `rapport` | 0.5 |
| `trust` | `persona_fidelity` (consistency proxy) | 0.5 |
| `reciprocity` | mean of `social_capital` + `goal_completion` | 0.5 |

The smoothing coefficient α = 0.3 provides a ~3-episode reaction time while
preventing single-episode thrash. This follows the classic EWMA design used in
process control and stock volatility estimation (α = 0.3 is typical for moderate
reactivity systems).

The `dyad_state` table stores a bounded rolling window of recent rapport scores
(`recent_rapport`, capped at `RUPTURE_WINDOW_LEN = 5`). The rupture detector
fires when the maximum peak-to-trough drop in this window exceeds
`RUPTURE_DROP_THRESHOLD = 0.20`:

```rust
max_drop = max over all (i, j) with i < j of (history[i] - history[j])
rupture  = max_drop > 0.20
```

This definition captures sharp drops regardless of when within the window they
occur, unlike a simple first-to-last comparison.

**Note:** The dimension mappings are placeholder quality until Track B evaluators
(Sotopia, CharacterEval) ship to score `rapport`, `social_capital`, and
`goal_completion` explicitly. Current values feed from the LLM judge dimensions
(`relevance`, `accuracy`, `completeness`), which are only partial proxies for
social dynamics.

### 7.5 Anomaly detector — four kinds

The `AnomalyDetector` (`agent-bestiary/observability/src/anomaly.rs`) scans a
window of timeline entries and produces a list of `DetectedAnomaly` values with
kind, severity, linked IDs, and a kind-specific JSON payload.

| Kind | Trigger | Severity | Payload |
|---|---|---|---|
| `safety` | `safety:*` flag on any entry's `anomaly_flags` | `critical` | `{ flag, entry_id }` |
| `drift` | `drift:anomalous` flag (written by worker pass 1 after `PersonaDriftMonitor`) | `warning` | `{ drift_norm, persona_version, entry_id }` |
| `rolling_conflict` | Same `conflict:<dim>` flag present in every entry of the last N (default 3) entries | `warning` | `{ dimension, window_len, entry_ids }` |
| `rupture` | `rupture:<dyad_id>` flag (written by `SocialInteractionTracker`) | `warning` | `{ entry_id }` |

Detection is implemented as a **pure function** (`detect_in_window_with_window`)
that takes a slice of `TimelineEntry` and the window length. The
`AnomalyDetector` struct is a thin wrapper that delegates to this function,
allowing the algorithmic logic to be unit-tested without a database connection.

All detected anomalies have `requires_review = true` by default, meaning they
appear in the Phase 4 HITL queue immediately upon detection.

### 7.6 Trend analyser

`TrendAnalyzer::compute(agent_id, window)` (`agent-bestiary/observability/src/trend.rs`)
returns a `TrendReport` with per-dimension statistics (mean, std_dev, min, max,
n, latest, direction) over the agent's most recent N timeline entries.

The analyser is **on-demand only** (decision D22). There is no background
caching. Every call to `GET /api/observatory/agents/:id/timeline` triggers a
fresh `TrendAnalyzer::compute`. This is acceptable for current query volumes;
snapshot caching is deferred to a future phase once read shapes stabilize.

### 7.7 ObservabilityWorker — two-pass background scanner

The `ObservabilityWorker` (`agent-bestiary/observability/src/worker.rs`) is the
background component of the hybrid scheduling model. It is constructed cheaply
(wraps an `Arc<MemoryStore>`) and triggered either:

1. **Post-eval-run** — `tokio::spawn`'d non-blocking after the eval pipeline
   completes (best-effort; errors are logged, not surfaced to the eval caller)
2. **On-demand** — via `POST /api/observatory/agents/:id/scan` (synchronous;
   returns a `ScanReport`)

**Scan algorithm:**

```
Pull entries since last checkpoint (batch_size = 200, oldest-first)

Pass 1 — Drift computation:
  For each entry where drift_norm IS NULL AND persona_version > 1:
    Call PersonaDriftMonitor::compute(prev_version, curr_version, recent_norms)
    If anomalous: append "drift:anomalous" to entry's anomaly_flags
    Write drift_norm + updated flags to agent_timeline_entries

Pass 2 — Anomaly detection:
  Re-fetch entries (so Pass 1 flags are visible)
  Call AnomalyDetector::detect_in_window(agent_id, refreshed)
  For each DetectedAnomaly: persist to anomaly_events

Checkpoint advance:
  Update agent_observability_state.last_scanned_entry_id to last processed entry
  Update counters + timestamps
```

The two-pass design is necessary because Pass 2's `drift` detector reads
`drift:anomalous` flags that were written by Pass 1. Re-fetching from the
database ensures consistency without mutating in-memory copies that could
diverge.

---

## 8. Plane D — Human Surfacing and Intervention Feedback

### 8.1 Purpose

Plane D closes the loop: it surfaces Plane C's anomaly events to human reviewers,
captures their decisions in a tamper-evident audit trail, and — for intervention
actions — executes a coherence-gated, two-write memory update that feeds back
into the agent's behavioral baseline.

### 8.2 Observatory UI

The observatory is served as static HTML + client-side JavaScript fetching from
the JSON API. It follows the existing ABW template pattern (`dashboard.html`,
`agent_detail.html`): no Askama templates, no chart libraries, no live updates.

**`templates/observatory.html`** — Per-agent observatory page:
- Header: agent name, persona version, scan window selector
- Per-dimension trend bars: mean percentage, σ, latest direction arrow
- Anomaly events list: kind badge, severity, payload (collapsible)
- Dyad table: rapport / trust / reciprocity / episode count per dyad
- Timeline list: newest-first, provenance badge, drift_norm, anomaly flags
- "Trigger Scan" button: POST to `/api/observatory/agents/:id/scan`
- "Observatory →" link in eval-run history: cross-links agent detail page to
  the observatory

**`templates/observatory_hitl.html`** — HITL review queue:
- One row per pending anomaly event
- Kind badge, severity indicator, collapsible JSON payload
- Three action buttons: Approve, Relabel, Intervene
- Intervene opens a modal collecting scope / classification / dimension /
  correction text / justification
- `agent_wide` submissions display a consensus-pending banner with `request_id`
  and second-reviewer instructions

### 8.3 HITL action lifecycle

```
Anomaly detected (Plane C)
        │
        ▼
anomaly_events row created (requires_review = true)
        │
        ▼
Appears in HITL queue (GET /api/observatory/hitl)
        │
        ├─── Approve ──► hitl_actions row (action=approve) + resolve anomaly
        │
        ├─── Relabel ──► hitl_actions row (action=relabel, score_overrides) + resolve anomaly
        │
        └─── Intervene ──► Phase 5 flow (see §8.4)
```

`hitl_actions` is append-only (UPDATE-blocking trigger). Multiple actions can
reference the same anomaly event (e.g. an approve followed later by a relabel
in a re-review scenario), though the first action that sets `resolved_at` on the
anomaly event takes precedence for queue removal.

### 8.4 Intervention feedback loop (Phase 5)

The intervention path executes a five-step choreography when a reviewer selects
"Intervene":

#### Step 1 — Reviewer act
Reviewer submits `POST /api/observatory/hitl/:event_id/action` with:
```json
{
  "action": "intervene",
  "scope": "episode | dyad | agent_wide",
  "classification": "belief | behaviour",
  "dimension": "social_capital",
  "correction_text": "...",
  "justification": "..."
}
```

#### Step 2 — Encode (`InterventionEncoder::encode`)
Validates and stamps:
- `authority_weight = 1.0` (HumanAuthority)
- `provenance = HumanCorrected`
- `gate_is_synchronous = true` iff `scope == AgentWide`
- Enforces `classification` + `correction_text` required for `AgentWide`

Returns `EncodedIntervention` (a strongly-typed, validated corrective signal).

#### Step 3 — Coherence gate (`CoherenceGate::check`)
Builds a minimal two-utterance TEC system:
- U0 = existing agent response (the belief/behaviour under review)
- U1 = proposed corrected response
- `Contradicts` incoherence relation between U0 and U1

Runs `SettlingEngine::settle` on the system. Reads `global_coherence.score`
as Γ(C).

**AgentWide (synchronous gate):** Γ(C) < threshold (default 0.5) → `Blocked`,
returns HTTP 422 with tensions list. Γ(C) ≥ threshold → `Approved`.

**Episode/Dyad (settler mode):** Always returns `GateOutcome { verdict: Settled }`.
The gate records tensions and minimum update set for audit but does not block.

The `GateOutcome` carries:
- `gamma: Option<f64>` — Γ(C) after settling
- `principle_scores: HashMap<String, f64>` — per-principle TEC scores
- `tensions: Vec<String>` — principles below 0.5 on incoherence-sensitive dimensions
- `minimum_update_set: Vec<MinimumUpdateNode>` — utterances with negative activation
  after settling (nodes that must change for the correction to be absorbed)

#### Step 4a — AgentWide: two-reviewer consensus

Because agent-wide interventions are the most destructive scope (they bump
`persona_version` and inject a synthetic episode at HumanAuthority weight),
they require a **second independent reviewer**:

1. First reviewer's submission creates a `two_reviewer_requests` row
   (status = `pending`), stores the `EncodedIntervention` as JSONB, returns
   HTTP 200 with `status: "awaiting_second_reviewer"` and `request_id`.
2. A unique partial index (`status = 'pending'` per anomaly) prevents duplicate
   pending requests.
3. Second reviewer calls `POST /api/observatory/hitl/consensus/:request_id`
   `{ "approved": true/false }`. Must be a **different user** from the first
   reviewer (enforced at the handler level).
4. If approved: the handler deserializes the stored `EncodedIntervention`,
   re-runs the coherence gate, executes `TwoWriteMemory::execute`, marks anomaly
   resolved.
5. If rejected: `two_reviewer_requests.status = 'rejected'`; anomaly remains open.

#### Step 4b — Episode/Dyad: immediate execution

For narrower scopes the handler proceeds directly to `TwoWriteMemory::execute`.

#### Step 5 — Two-write memory pattern (`TwoWriteMemory::execute`)

Two writes, in this order (decision D31):

**Write 1 — Synthetic corrected episode:**
A new `Episode` row with:
- `provenance = SyntheticCorrection`
- `authority_weight = 1.0`
- `query` copied from original (same question, corrected answer)
- `context` carries `corrected_response`, `original_episode_id`, reviewer
  metadata, scope, classification
- `tags = ["synthetic_correction", "hitl_intervention", scope]`
- `embedding = None` — will be re-embedded by the dreaming consolidation worker
  on its next run

**Write 2 — Annotation (episode_corrections row):**
Appended with:
- Scope, classification, dimension, correction text, score overrides
- Full `GateOutcome` as `coherence_check` JSON
- `minimum_update_set`, `tensions_flagged`
- `synthetic_episode_id` pointing to Write 1's new episode

**Persona version bump (AgentWide only):**
Direct SQL UPDATE: `agents.persona_version = persona_version + 1`. This is a
direct increment rather than an `agent_versions` INSERT (decision D32) because
HITL interventions create synthetic episodes, not agent version rows. The drift
monitor will recognize the new version boundary on its next scan.

### 8.5 Observable effects after a successful intervention

After `TwoWriteMemory::execute` completes:

1. `episodes` table has a new row with `provenance = synthetic_correction`
2. `episode_corrections` has a new audit row cross-linking the original episode,
   the synthetic episode, and the coherence gate outcome
3. `hitl_actions` has a new row linking the anomaly event to the correction
4. `anomaly_events.resolved_at` is set
5. (AgentWide only) `agents.persona_version` incremented
6. On the next `ObservabilityWorker::scan_agent` run: the synthetic episode's
   embedding (computed by the dreaming worker) will be included in the
   new persona version's baseline, causing future drift computations to compare
   against a baseline that includes the correction

---

## 9. Data Model

### 9.1 Schema overview

```
agents
  ├── persona_version (Phase 0)
  └── capability_gates -> { drift_threshold? }

episodes
  ├── provenance (Phase 0)
  ├── authority_weight (Phase 0)
  ├── dyad_id (Phase 0)
  └── persona_version_at_write (Phase 0)

episode_corrections (Phase 0)  [append-only]
  ├── episode_id
  ├── reviewer_id, reviewer_action
  ├── scope, classification
  ├── correction_text, score_overrides
  ├── coherence_check (Phase 5)
  ├── minimum_update_set (Phase 5)
  ├── tensions_flagged (Phase 5)
  └── synthetic_episode_id (Phase 5)

eval_signals (Phase 2)
  ├── run_id, episode_id, agent_id
  ├── evaluator_name, evaluator_version, evaluator_tier
  ├── dimension, score, confidence, flags
  ├── bundle_provenance, persona_version
  └── model_used, cost_credits, latency_ms, rationale

eval_runs (extended Phase 2)
  ├── aggregated_signal JSONB
  ├── conflict_flags JSONB
  └── prefilter_blocked BOOLEAN

agent_timeline_entries (Phase 3)
  ├── agent_id, episode_id, run_id
  ├── persona_version, dyad_id, session_id, provenance
  ├── dim_scores JSONB
  ├── drift_norm, within_version_cosine
  └── anomaly_flags JSONB

dyad_state (Phase 3)
  ├── dyad_id (PK), agent_id, human_id
  ├── rapport, trust, reciprocity [0.0..1.0]
  ├── episode_count
  └── recent_rapport JSONB  (bounded array for rupture detection)

anomaly_events (Phase 3)  [append-only by convention]
  ├── agent_id, episode_id, run_id, dyad_id
  ├── kind: drift | rolling_conflict | rupture | safety
  ├── severity: info | warning | critical
  ├── payload JSONB
  ├── requires_review BOOLEAN
  └── resolved_at, resolved_by

agent_observability_state (Phase 3)
  ├── agent_id (PK)
  ├── last_scanned_entry_id
  ├── last_scan_*
  ├── timeline_entry_count
  └── anomaly_event_count

hitl_actions (Phase 4)  [append-only]
  ├── anomaly_event_id, agent_id, reviewer_id
  ├── action: approve | relabel | intervene
  ├── score_overrides JSONB
  └── correction_id (Phase 5 back-link)

two_reviewer_requests (Phase 5)
  ├── anomaly_event_id, agent_id
  ├── encoded_intervention JSONB
  ├── first_reviewer_id, first_reviewed_at
  ├── second_reviewer_id, second_reviewed_at, second_approved
  ├── status: pending | approved | rejected | expired
  └── correction_id, synthetic_episode_id
```

### 9.2 Immutability enforcement

Three tables use DB-level triggers to enforce append-only semantics:

| Table | Trigger | Error on UPDATE |
|---|---|---|
| `episode_corrections` | `trg_episode_corrections_no_update` | `episode_corrections is append-only; row %id cannot be modified` |
| `hitl_actions` | `trg_hitl_actions_no_update` | `hitl_actions is append-only; row %id cannot be modified` |
| `two_reviewer_requests` | No UPDATE block — the second reviewer must UPDATE the row | Updated audit trail via `hitl_actions` |

The asymmetry for `two_reviewer_requests` is intentional (decision D33): the
workflow requires updating the row when the second reviewer acts, so the
UPDATE-blocking pattern cannot be applied. Audit continuity is maintained by
the `hitl_actions` table instead.

### 9.3 Indexing strategy

Indexes are designed around four primary access patterns:

1. **Dashboard timeline queries** — `(agent_id, created_at DESC)` on
   `agent_timeline_entries`
2. **Drift baseline queries** — `(agent_id, persona_version, created_at DESC)`
   on `agent_timeline_entries`
3. **HITL queue reads** — partial index on `anomaly_events`
   `WHERE requires_review = TRUE AND resolved_at IS NULL`
4. **Per-agent trend analysis** — `(agent_id, dimension, created_at DESC)` on
   `eval_signals`

---

## 10. Execution and Scheduling Model

### 10.1 Hot path (per eval-run case)

```
run_eval_cases():
  for each case:
    1. Execute agent via MultiModelExecutor
    2. Store episode (with provenance, authority_weight, dyad_id, persona_version_at_write)
    3. Build EpisodeBundle::from_parts(episode, agent, transcript, goal_spec)
    4. registry.run(&bundle).await  →  RegistryOutcome
    5. registry_outcome_to_signals  →  Vec<EvalSignal>
    6. bulk insert eval_signals
    7. EpisodeScorer::write_inline(episode, &signal, run_id, session_id)
       → INSERT agent_timeline_entries (dim_scores, provenance, persona_version, dyad_id)
    8. update case_results[i].signal

  aggregate_run_signals → AggregatedSignal (run-level mean)
  UPDATE eval_runs SET aggregated_signal = ..., conflict_flags = ..., prefilter_blocked = ...
  detect_regression → optional eval_regression notification
  emit eval_conflict notification if run has conflicts

  tokio::spawn ObservabilityWorker::scan_agent(agent_id)  [non-blocking, best-effort]
```

### 10.2 Background worker (per agent, on demand)

```
ObservabilityWorker::scan_agent(agent_id):
  Load agent_observability_state (or init default)
  Pull timeline entries since last checkpoint (batch=200, oldest-first)

  Pass 1 — drift:
    For entries where drift_norm IS NULL AND persona_version > 1:
      PersonaDriftMonitor::compute(prev_v, curr_v, recent_norms)
      If anomalous: append "drift:anomalous" to anomaly_flags
      UPDATE agent_timeline_entries (drift_norm, anomaly_flags)

  Pass 2 — anomaly detection:
    Re-fetch entries (flags from Pass 1 now visible)
    AnomalyDetector::detect_in_window(agent_id, entries)
    For each DetectedAnomaly: INSERT anomaly_events

  Advance checkpoint:
    UPSERT agent_observability_state (last_scanned_entry_id, counters, timestamps)
```

### 10.3 Intervention path (per HITL action)

```
POST /api/observatory/hitl/:event_id/action  {action: "intervene", ...}:
  Auth: owner or admin
  Load anomaly_event
  InterventionEncoder::encode(request) → EncodedIntervention
  CoherenceGate::check(encoded):
    AgentWide + blocked → HTTP 422
    AgentWide + approved → INSERT two_reviewer_requests → HTTP 200 (awaiting)
    Episode/Dyad → TwoWriteMemory::execute(encoded, gate_outcome, original_episode)

TwoWriteMemory::execute:
  1. Build synthetic Episode (provenance=SyntheticCorrection, authority_weight=1.0)
  2. store.store_episode(synthetic_episode)
  3. Build EpisodeCorrection (coherence_check, minimum_update_set, tensions_flagged)
  4. store.create_episode_correction(correction)
  5. If AgentWide: store.bump_persona_version(agent_id)
  6. Return TwoWriteReceipt { correction_id, synthetic_episode_id, persona_version_bumped, new_persona_version }
```

---

## 11. Security and Access Control

### 11.1 Authentication

All observatory endpoints require a valid session (JWT cookie) or API bearer
token, extracted by `auth_middleware` / `optional_auth_middleware` from
`fermi-auth`. Unauthenticated requests receive HTTP 401.

### 11.2 Authorization model

All agent-scoped observatory endpoints use the `require_owner_or_admin` helper:

```rust
async fn require_owner_or_admin(state, principal, agent_id)
  → Ok(Agent) | Err((403, "Owner or admin access required"))
```

The HITL queue endpoint (`GET /api/observatory/hitl`) uses a different model:

- **Admins** see all pending anomaly events across all agents
- **Non-admins** see only events on agents they own (filtered in-process;
  will move to SQL predicate push-down as queue volume grows)

The two-reviewer consensus endpoint enforces that the second reviewer is a
**different user** from the first reviewer at the handler level:

```rust
if two_req.first_reviewer_id == user_id {
    return Err((403, "Second reviewer must be a different user from the first reviewer"));
}
```

`curated` system agents (owner = NULL) are admin-only for HITL actions.

### 11.3 Append-only audit trail integrity

Three layers protect the audit trail:

1. **Application layer:** `MemoryStore` methods for corrections and HITL actions
   never expose UPDATE paths
2. **Database trigger layer:** `trg_episode_corrections_no_update` and
   `trg_hitl_actions_no_update` raise exceptions on any UPDATE
3. **Schema layer:** All audit tables use `UUID PRIMARY KEY DEFAULT gen_random_uuid()`
   with no mutable primary keys

---

## 12. API Reference

### 12.1 Timeline and state reads

```
GET /api/observatory/agents/:id/timeline?window=N
  Auth: owner or admin
  Returns: { agent_id, agent_name, persona_version, window, trend: TrendReport, entries: TimelineEntry[] }
  Default window: 50, max: 500

GET /api/observatory/agents/:id/dyads
  Auth: owner or admin
  Returns: { dyads: DyadState[] }

GET /api/observatory/agents/:id/anomalies?limit=N
  Auth: owner or admin
  Returns: { anomalies: AnomalyEvent[] }
  Default limit: 50, max: 500
```

### 12.2 Scan trigger

```
POST /api/observatory/agents/:id/scan
  Auth: owner or admin
  Returns: { report: ScanReport { agent_id, entries_scanned, anomalies_detected, drift_computations, duration_ms } }
```

### 12.3 HITL queue

```
GET /api/observatory/hitl?limit=N
  Auth: any authenticated user
  Returns: { queue: AnomalyEvent[] }
  Filter: owner sees own agents; admin sees all
  Default limit: 100, max: 500
```

### 12.4 HITL actions

```
POST /api/observatory/hitl/:event_id/action
  Auth: owner of agent or admin
  Body: {
    action: "approve" | "relabel" | "intervene",
    notes?: string,
    score_overrides?: object,        // relabel
    scope?: "episode"|"dyad"|"agent_wide",    // intervene
    classification?: "belief"|"behaviour",    // intervene
    dimension?: string,              // intervene
    correction_text?: string,        // intervene
    justification?: string           // intervene
  }
  Returns (approve/relabel): { action_id, anomaly_event_id, resolved: true }
  Returns (intervene, episode/dyad): { status:"intervention_complete", correction_id, synthetic_episode_id, gate, persona_version_bumped, ... }
  Returns (intervene, agent_wide): { status:"awaiting_second_reviewer", request_id, gate, message }
  Returns (intervene, gate blocked): HTTP 422 { tensions, gamma, threshold }
```

### 12.5 Two-reviewer consensus

```
POST /api/observatory/hitl/consensus/:request_id
  Auth: owner of agent or admin; must be different user from first reviewer
  Body: { approved: boolean, notes?: string }
  Returns (rejected): { status: "rejected", request_id, message }
  Returns (approved): { status: "intervention_complete", correction_id, synthetic_episode_id, gate, persona_version_bumped, ... }
```

### 12.6 Observatory pages

```
GET /observatory?agent=<id>   → observatory.html (per-agent dashboard)
GET /observatory/hitl          → observatory_hitl.html (review queue)
```

---

## 13. Recursive Improvement Loop

The observability stack is the *evidence layer* of a larger recursive self-
improvement (RSI) loop for AI agents. The loop currently operates with human
mediation at the improvement step; the automated path is not yet built.

```
Execution
    │
    ▼  (eval pipeline writes eval_signals, agent_timeline_entries)
Evidence accumulates
    │
    ▼  (ObservabilityWorker scans, AnomalyDetector fires)
Signals surface to HITL queue
    │
    ▼  (reviewer approves, relabels, or intervenes)
Human decision recorded (hitl_actions, episode_corrections)
    │
    ├── Intervene → TwoWriteMemory → synthetic episode injected at HumanAuthority weight
    │                             → (AgentWide) persona_version bumped
    │                             → Next dreaming cycle consolidates new episode into ontology
    │
    └── (Future: automated trigger from "drift detected" to "persona/config update queued")
```

The agent card is the *target* of the loop. Every field in `AgentCapabilities`
is in principle mutable based on observed performance:
- High-drift agent → tighten `capability_gates.drift_threshold`
- Unsafe agent → raise `min_tier`, gate capabilities
- Miscalibrated forecaster → update `model_params.temperature` (lower = less hedging)
- Persona-inconsistent agent → trigger HITL review → `AgentWide` intervention → persona_version bump

The Brier score (forecast calibration) feeds back into this loop as a global
expectation-vs-outcome KPI for forecasting agents. The `BrierEvaluator` wraps
the existing forecast resolver; its `forecast_calibration` dimension appears in
trend charts alongside behavioral dimensions.

---

## 14. Track B — Native Evaluator Family

Track B is the family of native-Rust evaluator implementations that plug into
the `EvalModel` trait from Plane B. They are designed and built in parallel
with Track A (after Phase 1 lands the trait). Each evaluator is a separate crate.

### 14.1 Pre-filter evaluators

**WildGuard** (`agent-bestiary/evaluator-wildguard`, planned)
- Tier: `PreFilter`
- Dimensions: `safety` (score = `1.0 - p(unsafe)`)
- Sources: Han et al. (2024), AllenAI WildGuard — NeurIPS 2024 Datasets & Benchmarks.
  Evaluates prompt harmfulness, response harmfulness, and refusal detection across
  13 risk categories. A fine-tuned Mistral-7B, achieves state-of-the-art on
  WildGuardTest across 10+ public benchmarks.
- ABW implementation: deterministic word/pattern filter → hosted moderation API
  (or local classifier). Output: `safety` dimension + `harm:<category>` flags.
- Short-circuit semantics: unsafe verdict sets `prefilter_blocked = true` on the
  run; dimensional evaluators are skipped; episode goes straight to HITL queue
  with severity `critical`.

**Faithfulness check** (`agent-bestiary/evaluator-faithfulness`, planned)
- Tier: `PreFilter`
- Dimensions: `grounding` (score = `supported / (supported + contradicted + unsupported)`)
- Sources: Ming et al. (2024), FaithEval (ICLR 2025) — evaluates contextual
  faithfulness in LLMs across unanswerable, inconsistent, and counterfactual
  contexts. Salesforce AI Research, 4.9K high-quality problems.
- ABW implementation: claim extraction from response → source matching against
  `bundle.context.tool_outputs` → grounding score.
- Opt-out: agents without a grounding context (creative agents) opt out via
  `capability_gates`.

### 14.2 Dimensional evaluators

**Sotopia** (`agent-bestiary/evaluator-sotopia`, planned)
- Tier: `Dimensional`
- Dimensions: `goal_completion`, `social_capital`, `rapport`
- Sources: Zhou et al. (2024), SOTOPIA: Interactive Evaluation for Social
  Intelligence in Language Agents (ICLR 2024). An open-ended environment for
  simulating and evaluating goal-directed social interactions between AI and
  human agents. Multi-dimensional SOTOPIA-Eval framework scoring goal completion,
  financial stability, relationship maintenance, secret preservation, and social
  norm adherence.
- ABW implementation: structured LLM scoring against SOTOPIA rubric; 1–10 Likert
  normalized to `[0, 1]`. Returns `EvalError::Inapplicable` when `goal_spec` is
  absent or transcript has fewer than 2 turns.
- This evaluator is the primary source of the `rapport`, `social_capital`, and
  `goal_completion` dimensions that the `SocialInteractionTracker` currently
  approximates with placeholder mappings.

**LifelongBench** (`agent-bestiary/evaluator-lifelong`, planned)
- Tier: `Dimensional`
- Dimensions: `persona_consistency`, `retention`
- Sources: Inspired by LIBERO, LAMP, and related lifelong learning benchmarks.
  Measures cross-session consistency: does the agent behave consistently with
  earlier sessions on the same dyad / topic?
- ABW implementation: embedding-based persona drift (cosine of current response
  embedding vs. mean of prior episode embeddings at same `persona_version`) for
  `persona_consistency`; declarative fact retrieval probe for `retention`
  (Phase 1 stub; Phase 2 wires probe). Requires read access to the episode store
  beyond the `EpisodeBundle`.
- Returns `EvalError::Inapplicable` below minimum history threshold (5+ prior
  episodes per dyad).

**CharacterEval** (`agent-bestiary/evaluator-character`, planned)
- Tier: `Dimensional`
- Dimensions: `persona_fidelity`, `value_alignment`
- Sources: Tu et al. (2024), CharacterEval: A Chinese Benchmark for Role-Playing
  Conversational Agent Evaluation (arXiv 2401.01275). 13-metric evaluation across
  four dimensions (conversational ability, character consistency, role-playing
  attractiveness, personality back-testing). Also Wang et al. (2024), RoleLLM
  (ACL Findings 2024): benchmarking and enhancing role-playing via RoleBench
  (168K samples).
- ABW implementation: system-prompt commitment extraction → per-commitment LLM
  compliance scoring → aggregation. Respects `persona_version` — scores fidelity
  to the *current* persona baseline, not v1.
- `persona_fidelity` is the dimension the `SocialInteractionTracker` currently
  maps to `trust`.

**BrierEvaluator** (`agent-bestiary/evaluators/src/scoring.rs` — shipped)
- Tier: `Dimensional`
- Dimensions: `forecast_calibration`
- Sources: Brier (1950) proper scoring rule; inverted for consistency
  (`1.0 - clamp(brier, 0, 1)` so higher = better calibration).
  Murphy (1973) decomposition: Brier = Reliability − Resolution + Uncertainty.
  Extremization / Platt scaling literature (Baron et al., 2014; Niculescu-Mizil
  and Caruana, 2005) informs the calibration correction approach.
- ABW implementation: thin read-only wrapper over `fermi_forecasts`; returns
  `EvalError::Inapplicable` for agents without resolved forecasts. Confidence
  saturates at `n = 20` resolved forecasts.

---

## 15. Research Foundations

This section maps the major design decisions to their research and engineering
precedents.

### 15.1 Multi-dimensional social intelligence evaluation

**SOTOPIA** (Zhou et al., 2024, ICLR 2024) is the primary inspiration for the
social goal and dyad dimensions (`goal_completion`, `social_capital`, `rapport`).
SOTOPIA establishes that social intelligence in language agents is not a single
score but a multi-dimensional construct spanning goal completion, financial
management, relationship preservation, secret keeping, and social norm adherence.
The SOTOPIA-Eval rubric is adopted as the scoring basis for the ABW Sotopia
evaluator (Track B).

The ABW observability stack extends SOTOPIA's per-episode evaluation into a
longitudinal framework: rather than scoring isolated episodes, it tracks
`rapport`, `trust`, and `reciprocity` across interaction histories using EWMA
smoothing and rupture detection, enabling *social trajectory* analysis that
SOTOPIA's static evaluation does not provide.

### 15.2 Safety pre-filtering

**WildGuard** (Han et al., 2024, NeurIPS 2024) provides the safety taxonomy
and evaluation methodology for the pre-filter tier. WildGuard's three-goal
architecture (prompt harmfulness, response harmfulness, refusal evaluation) maps
directly to ABW's pre-filter semantics: a single evaluator that can short-circuit
the registry and route the episode directly to HITL.

The 13-category risk taxonomy (Privacy, Misinformation, Harmful language,
Malicious uses) is adopted as the `harm:<category>` flag vocabulary in ABW's
`EvalFlag` type.

ABW's decision to use a deterministic word/pattern filter as a first stage before
the classifier follows the Llama-Guard / OpenAI Moderation design pattern: cheap
heuristics gate expensive model calls.

### 15.3 Faithfulness and grounding

**FaithEval** (Ming et al., 2024, ICLR 2025; Salesforce AI Research) establishes
that faithfulness hallucination — generating responses misaligned with the
provided context — remains a significant challenge even for state-of-the-art
models. The three FaithEval tasks (unanswerable, inconsistent, counterfactual
contexts) correspond to ABW's `grounding` dimension scoring cases:

| FaithEval task | ABW mapping |
|---|---|
| Unanswerable context | `unsupported` claims (no source match) |
| Inconsistent context | `contradicted` claims (conflicts with source) |
| Counterfactual context | `contradicted` claims (counterfactual assertions) |

ABW's grounding score `= supported / (supported + contradicted + unsupported)`
is a simplified version of FaithEval-style precision-over-claims scoring.

### 15.4 Persona fidelity and role consistency

**RoleLLM** (Wang et al., 2024, ACL Findings) introduces RoleBench (168K samples)
and demonstrates that role-playing quality requires structured benchmarking at the
character level — not just generic instruction following. RoleLLM's four-stage
framework (Profile Construction, Context-Based Instruction Generation, Role
Prompting, Role-Conditioned Instruction Tuning) informs ABW's CharacterEval
evaluator design, particularly the system-prompt commitment extraction step.

**CharacterEval** (Tu et al., 2024, arXiv 2401.01275) provides the four-dimension,
13-metric evaluation framework that ABW's CharacterEval evaluator adapts for English-
language curated agents. The `persona_fidelity` and `value_alignment` dimensions
come directly from CharacterEval's character consistency and personality back-testing
dimensions.

### 15.5 Forecast calibration

**Brier score** (Brier, 1950) is the foundational proper scoring rule for
probabilistic forecasting. The Murphy (1973) decomposition
(Brier = Reliability − Resolution + Uncertainty) is relevant for future
calibration analytics: the `forecast_calibration` dimension as currently
implemented measures only the combined score; a future decomposition would
distinguish calibration error from resolution.

ABW's inversion (`forecast_calibration = 1.0 - brier`) is a standard
normalization to align direction with all other dimensions (higher = better).
The confidence saturation at `n = 20` resolved forecasts follows the empirical
observation from forecasting literature that calibration estimates are unstable
below ~20 samples.

The extremization / Platt scaling connection is noted for the future calibration
correction planned in Track B Q-CC3: once a per-dimension calibration correction
polynomial is fit from HITL-labeled episodes, scores can be post-hoc corrected
for the characteristic over-hedging bias of LLMs documented in the LLM
calibration literature.

### 15.6 Coherence as a write gate

**Thagard's Theory of Explanatory Coherence (TEC)** (Thagard, 1989, 1992;
Thagard and Verbeurgt, 1998) provides the theoretical foundation for the
coherence gate. TEC models belief acceptance as constraint satisfaction: elements
(propositions, hypotheses) have positive constraints (cohere) or negative
constraints (incohere/contradict), and the maximally coherent interpretation
accepts elements that satisfy the most constraints.

The ABW coherence gate builds a minimal two-utterance system with a `Contradicts`
incoherence relation and measures Γ(C) (global coherence score after settling) as
a proxy for "how much does the existing belief system resist this update?" A low
Γ(C) means strong resistance; the gate blocks the write.

This design follows Thagard's insight that coherence is not just epistemic
consistency but *resistance to revision* — an agent's belief system naturally
resists changes that would require updating many strongly coherent nodes. The
`minimum_update_set` (nodes with negative activation after settling) is the
practical output of this resistance: it tells the reviewer which beliefs must
change for the correction to be absorbed, supporting more targeted interventions.

### 15.7 Episodic memory and dreaming consolidation

The synthetic correction episode injected by the two-write pattern is designed
to feed into the ADM (Agent Dreaming Memory) consolidation pipeline. The
`consolidated = false` flag on new episodes signals the `ConsolidationWorker`
to include them in the next dreaming cycle.

This connects to the neuroscience-inspired memory consolidation literature
(Diekelmann and Born, 2010; also Optimus-1 HDKG/AMEP, 2024; MyGO wake-sleep
cycle, 2024) that models memory as having a "wake" (episodic acquisition)
phase and a "sleep" (semantic consolidation / dreaming) phase. The ABW
architecture implements this as:

- **Wake:** Episode stored during agent execution (episodic memory)
- **Sleep:** `ConsolidationWorker` clusters recent episodes, extracts semantic
  rules, updates ontology (dreaming)
- **Synthetic correction:** A special-provenance episode injected at HumanAuthority
  weight (`authority_weight = 1.0`) that the dreaming worker consolidates as a
  high-confidence correction signal

### 15.8 Human-in-the-loop alignment monitoring

The HITL queue and two-reviewer consensus design draws on emerging practices in
AI safety monitoring. The two-reviewer consensus for `agent_wide` interventions
mirrors the **four-eyes principle** common in financial and safety-critical
systems: destructive actions require two independent approvers.

OpenAI's internal coding agent monitoring system (2026) — which uses a
low-latency GPT-powered monitor to flag misalignment-relevant behaviors for
human triage — validates the ABW approach of automated anomaly detection
(Plane C) gating human review (Plane D) rather than routing all episodes to
human review directly. ABW's pre-filter short-circuit (WildGuard) and the
HITL queue together implement a similar triage funnel.

The `NPO` alignment framework (Gaikwad and Doke, 2025) formalizes meta-alignment
as the fidelity of the monitoring process itself. ABW's append-only audit trail
(`episode_corrections`, `hitl_actions`) and the coherence gate serve as the
meta-alignment mechanism: every intervention is formally checked for coherence
before being applied and permanently recorded with full provenance.

---

## 16. Decision Log

This section restates the 33 architectural decisions from `OBSERVABILITY_IMPL.md`
with their rationale, organized by theme.

### Construction strategy

| # | Decision | Rationale |
|---|---|---|
| D1 | Build incrementally, one phase per PR | Validates foundations before adding complexity; keeps PRs reviewable |
| D2 | New crates per concern | Independent testability; prevents entanglement of platform secrets (API keys) with algorithmic logic |
| D9 | One migration per phase | Atomic schema evolution; easy rollback boundary |

### Episode and data model

| # | Decision | Rationale |
|---|---|---|
| D3 | `persona_version` increments on both intervention and `AgentVersion` writes | Both trigger a behavioral boundary that the drift monitor should track |
| D4 | `dyad_id` wiring deferred to application layer | Avoid premature abstraction; different call sites (eval, workspace, swarm) need different dyad semantics |
| D11 | Episode immutability via `episode_corrections` + trigger | Tamper-evident audit trail; prevents silent score manipulation |
| D12 | Synthetic corrected episodes embed `original_query + corrected_response` | Preserves the original question for context; the delta is in the response |
| D13 | `EpisodeBundle` in `agent-bestiary-memory` | Co-location with `Episode`; single import path for evaluators |
| D14 | Provenance enum with 6 values | Total order of human involvement; self-documenting in SQL |
| D15 | Phase 0 migration scheme | Foundation-first; no behavior change until Phase 1 |

### Evaluator registry

| # | Decision | Rationale |
|---|---|---|
| D5 | Native Rust evaluators only | Deterministic deployment; no Python sidecars; loose coupling via trait |
| D8 | `BrierEvaluator` is read-only | The Brier score is already computed; re-computing would be a correctness risk |
| D16 | Run-level aggregation = mean per dimension across cases | Statistically sound; conflict union captures the worst case |
| D17 | `case_results[].signal` is additive | Backward compatibility; legacy `judge_scores` readers still work |

### Longitudinal observability

| # | Decision | Rationale |
|---|---|---|
| D6 | Coherence engine as infra helper for self-improvement | Reuses existing TEC settling for a new purpose without coupling the architectures |
| D18 | Dyad populated for eval pipeline only | Scopes the Phase 3 footprint; other call sites deferred |
| D19 | Drift threshold: static default, adaptive infrastructure | Static is safe without sufficient history; adaptive activates when data matures |
| D20 | Anomaly defaults: 3-episode rolling conflict, 0.20 rapport drop, binary safety | Conservative defaults; tunable per-agent |
| D21 | Hybrid scheduling: inline timeline write + background worker | Keeps dashboard fresh without blocking the eval caller |
| D22 | Trend analyser on-demand | Caching deferred until read shapes stabilize (Phase 4 will inform this) |
| D23 | No backfill | Timeline is forward-only from deploy; simplifies migration |

### HITL and observatory

| # | Decision | Rationale |
|---|---|---|
| D7 | UI fits into existing Askama dark theme | Consistency with existing UX; no new design system dependency |
| D24 | HITL access = owner OR admin | Least-privilege; curated agents fall to admin |
| D25 | Global cross-agent observatory + per-agent drill-down | Admin needs fleet view; owners need per-agent detail |
| D26 | `agent_wide` interventions deferred to Phase 5 | Two-reviewer consensus is non-trivial; ship approve/relabel first |
| D27 | Server-rendered HTML + JS fetch | Consistency with existing pattern; no chart library dependency |
| D28 | Manual scan trigger via HTTP POST | On-demand debugging without redeploying; synchronous for immediate feedback |
| D29 | Eval-run rendering extended additively | Backward compatibility; legacy judge-score rendering preserved |

### Intervention feedback loop

| # | Decision | Rationale |
|---|---|---|
| D10 | Coherence as synchronous gate (AgentWide) / settler (Episode/Dyad) | AgentWide is most destructive; synchronous gate prevents incoherent rewrites. Narrower scopes are safer; settler mode records audit without blocking |
| D30 | Minimal two-utterance TEC system | Simple, deterministic, no LLM call needed in the gate itself |
| D31 | Write synthetic episode before annotation | Avoids UPDATE on the correction row to back-fill `synthetic_episode_id` |
| D32 | Direct SQL UPDATE for `bump_persona_version` | HITL interventions don't create `agent_versions` rows; separate UPDATE avoids double-bump |
| D33 | First reviewer creates `two_reviewer_requests`; second reviewer calls separate endpoint | Separation of concerns; stored `EncodedIntervention` JSONB ensures second reviewer sees exactly what first proposed |

---

## 17. Glossary

| Term | Definition |
|---|---|
| **ADM** | Agent Dreaming Memory — the platform's episodic/semantic memory architecture |
| **AggregatedSignal** | Run-level or case-level summary of all evaluator scores, conflicts, and flags |
| **AgentWide** | The most destructive `CorrectionScope` — applies a correction to the agent's entire persona baseline |
| **Authority weight** | A `[0.0, 1.0]` signal quality indicator. `0.5` = automated default; `1.0` = HumanAuthority |
| **Dyad** | A `(agent_id, human_id)` interaction pair, identified by a deterministic `dyad_id` string |
| **EpisodeBundle** | The normalized input contract for all evaluators: transcript, agent card snapshot, goal spec, context |
| **EvalModel** | The async Rust trait every evaluator implements |
| **Γ(C)** | Global coherence score from the TEC settling engine; used as the coherence gate threshold |
| **HITL** | Human-in-the-loop — the review and intervention workflow in Plane D |
| **HumanAuthority** | `authority_weight = 1.0`; `provenance = HumanCorrected` or `SyntheticCorrection` |
| **Minimum update set** | The set of world-model nodes (TEC utterances with negative activation) that must change for a correction to be absorbed |
| **Persona version** | A monotonic integer on `agents` that increments on system-prompt edits and `AgentWide` HITL interventions |
| **Pre-filter** | An `EvalTier` that runs first, serially; can short-circuit the registry |
| **Provenance** | A 6-value enum on episodes encoding the source of the episode (auto/human/synthetic) |
| **Rupture** | A peak-to-trough rapport drop > 0.20 within a 5-entry rolling window for a dyad |
| **SyntheticCorrection** | `Provenance` value for episodes created by the two-write memory pattern |
| **TEC** | Theory of Explanatory Coherence (Thagard 1989) — the constraint-satisfaction model underlying the coherence gate |
| **TrendReport** | Per-dimension statistics (mean, std_dev, min, max, n, latest, direction) over a rolling window |
| **Two-write pattern** | The memory update that creates (1) a synthetic corrected episode and (2) an immutable annotation row |
| **Two-reviewer consensus** | The `agent_wide` intervention workflow requiring two independent approvers |

---

## 18. Open Questions and Future Work

The following items are explicitly deferred. They are not design gaps — each is
a deliberate punt documented with a rationale.

### OQ-1 — Trend analyser window configuration

**Status:** Platform default = 50 episodes. Per-agent override planned.
**Blocker:** Need data on distribution of episode counts per agent before
choosing meaningful per-agent defaults.

### OQ-2 — Drift threshold calibration

**Status:** Static threshold 0.20, per-agent override via `capability_gates`.
Adaptive mode ships as infrastructure.
**Blocker:** Requires ≥ ~100 persona-version transitions before adaptive mode
produces meaningful rolling statistics.

### OQ-3 — Brier as global KPI

**Status:** `BrierEvaluator` ships as a `forecast_calibration` dimension in the
registry. A global cross-agent KPI (e.g. "the platform's collective forecasting
accuracy this week") is under separate specification.
**Blocker:** KPI definition and dashboard surface.

### OQ-4 — HITL queue scalability

**Status:** Admin filter runs in-process (load all pending events, filter by
ownership). Functional at current queue depths (<1000 events).
**Blocker:** When queue depth grows, push the ownership filter to SQL
(`JOIN agents ON anomaly_events.agent_id = agents.agent_id WHERE agents.user_id = $1`).

### OQ-5 — Coherence gate threshold tuning

**Status:** Default Γ(C) threshold = 0.5.
**Blocker:** Empirical calibration against HITL outcomes; requires a corpus of
approved and rejected `agent_wide` interventions.

### OQ-6 — Automated recursive improvement trigger

**Status:** Drift is captured; human review is required to act on it. The
"drift detected → persona/config update queued for review" automation is not built.
**Blocker:** Policy decisions on what automated changes are permissible without
human review; integration with the `composition_versions` table (tune-the-team RSI).

### OQ-7 — Observability composition and shelf

**Status:** Observatory UI exists as a standalone page. An `observability_coordinator`
composition (with `eval_runner`, `anomaly_triager`, `dyad_observer` members) and
an Observability workspace shelf (analogous to the Coherence shelf) are designed
in `AGENT_MODEL.md §3.2` but not yet implemented.
**Blocker:** Composition creation UX and strategist assignment (see `COMPOSITION_AS_FIRST_CLASS.md`).

### OQ-8 — Track B evaluator ordering

**Status:** Five evaluators designed (`EVALUATOR_DESIGN.md`); none yet shipped.
**Blocker:** Cross-cutting Q-CC1 (LLM provider strategy), Q-CC3 (calibration data),
and Q-CC5 (Inapplicable semantics) should be resolved before implementation.
Recommended shipping order: WildGuard (safety pre-filter) first, then Sotopia
(primary social dimensions), then CharacterEval, Faithfulness, LifelongBench.

### OQ-9 — Evaluator cost budget

**Status:** `eval_signals.cost_credits` is recorded but no per-agent eval budget
is enforced. Track B evaluators will add significant per-episode cost.
**Blocker:** `EVALUATOR_DESIGN.md` Q-CC2 — a skip rule and per-agent budget
(mirroring `dreaming_budget_credits`) must be designed before full Track B deployment.

---

*End of specification.*

*Maintainer note: this document should be updated whenever a new observability
phase ships, a Track B evaluator lands, or an open question is resolved.*
