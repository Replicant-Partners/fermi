# Spec 20 — SimOps Projection Scoring: Deferred Hard Verifier Loop

**Status:** design — ready to implement  
**Filed:** 2026-05-30  
**Author:** Ivan Labra  
**Motivation:** SIA paper (arXiv:2605.27276) — ABW-native analogy for gradient-free model quality signal

---

## 0. The one-sentence idea

When a real batch completes and the operator enters actual measurements as SOSA observations, compute the delta between those measurements and the prior projection from `simops_dynamics_runner` / `POST /api/simops/cascade`. Write that delta as an `EvalSignal` on the episode that produced the projection. The dreaming cycle then consolidates on quality-weighted episodes — poor projections generate semantic rules like "model X underestimates yield at high temperature"; good projections reinforce the conditions under which a model is reliable.

---

## 1. Why this signal is trustworthy (unlike acceptance)

The problem with user-acceptance signals (the SIA paper relies on deterministic verifiers for this reason): operators accept model outputs because they look plausible, not because they're correct. Acceptance is noisy, gameable, and doesn't reflect ground truth.

The SOSA observation delta is a **deferred hard verifier**:

- The operator enters real batch measurements (yield, carbon, opex) as SOSA observations after the batch completes
- Those measurements are independent of what the model predicted
- The delta between prediction and measurement is computed mechanically — no human judgment in the scoring step
- The only way to game it is to enter false measurements, which costs the operator real-world accuracy on their own data

This is the same structure as Brier score on resolved forecasts: the future resolves independently of the forecast. The batch completes independently of the projection.

---

## 2. What already exists

**Already built — no changes needed:**

| Component | Location | Role |
|---|---|---|
| `sosa_observations` table | `migrations/052_sosa_observations.sql` | Stores real batch measurements |
| `extra JSONB` on observations | Same | Carries `projection_id`, `run_id` metadata |
| `produced_by_agent_id` on observations | `migrations/128_sosa_observations_produced_by.sql` | Links observations to the agent that produced synthetic data |
| `episodes` table | `agent-bestiary/memory/` | Stores every agent execution |
| `eval_signals` table | `agent-bestiary/memory/src/types.rs:EvalSignal` | Stores per-dimension quality scores per episode |
| `EvalSignal.dimension` + `EvalSignal.score` | `types.rs` | The slot where projection quality lands |
| `ConsolidationWorker` | `agent-bestiary/memory/src/consolidation.rs` | DBSCAN → semantic rules → knowledge graph |
| `BrierEvaluator` pattern | `agent-bestiary/evaluators/src/scoring.rs` | Reference implementation for a deferred evaluator |
| `POST /api/simops/dynamics` | `src/handlers/simops.rs` | Produces `provenance.projection_id` in every response |
| `POST /api/workspaces/:id/cascade` | `src/handlers/simops.rs` | Same |

**What's missing — three small pieces:**

1. A `projection_id` written into the SOSA observation `extra` when synthetic observations are created by the dynamics runner
2. A `ProjectionScoringEvaluator` that wakes up when a real observation arrives, finds the prior projection for the same sensor/stage, computes the delta, and writes an `EvalSignal`
3. A migration that adds a `projection_id` index to `sosa_observations.extra` for efficient lookup

---

## 3. Data flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  Projection time                                                     │
│                                                                      │
│  Operator runs: POST /api/workspaces/:id/cascade                    │
│    → cascade_v2() produces CascadeResponseV2                        │
│    → provenance.projection_id = "proj-coupled-abc123"               │
│                                                                      │
│  simops_dynamics_runner writes synthetic SOSA observations          │
│    → sosa_observations.extra = {                                     │
│        "projection_id": "proj-coupled-abc123",                      │
│        "run_id": "...",                                             │
│        "source": "simops_simulation",                               │
│        "predicted_value": 4.2,                                      │
│        "model_uri": "kask:dynamics/kombucha_fermentation@v1"        │
│      }                                                              │
│    → produced_by_agent_id = "simops_dynamics_runner"                │
└────────────────────┬────────────────────────────────────────────────┘
                     │  (batch runs in the real world)
                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│  Resolution time (operator enters real measurements)                 │
│                                                                      │
│  Operator / sensor writes real SOSA observation:                    │
│    → observable_property: "bio:bc_yield_g_per_l"                   │
│    → result_value: 3.8   (actual yield)                             │
│    → feature_of_interest: "xi:simops/Feature/vessel_A"              │
│    → procedure: "xi:simops/method/batch_harvest"                    │
│    → extra.projection_id: "proj-coupled-abc123"  (if operator       │
│      links it) OR matched by (session, property, stage) lookback    │
└────────────────────┬────────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│  ProjectionScoringEvaluator (new, Tier: dimensional)                │
│                                                                      │
│  Triggered by: new real observation with procedure ≠ simulation     │
│                                                                      │
│  1. Find prior synthetic observation for same                       │
│     (session OR stage, observable_property) from simops_simulation  │
│  2. Compute delta: |predicted - actual| / |actual|  (relative error)│
│  3. Compute score: 1.0 - min(delta, 1.0)  (1.0 = perfect, 0 = 100%)│
│  4. Write EvalSignal:                                               │
│       dimension: "projection_accuracy"                              │
│       score: 0.91  (9% relative error)                              │
│       confidence: f(n_prior_observations)                           │
│       flags: { model_uri, stage_id, observable_property,            │
│                predicted, actual, relative_error, delta_direction } │
│  5. Link EvalSignal to the episode that ran the projection          │
└────────────────────┬────────────────────────────────────────────────┘
                     │
                     ▼
┌─────────────────────────────────────────────────────────────────────┐
│  ConsolidationWorker (existing, unchanged)                          │
│                                                                      │
│  Reads episodes for simops_dynamics_runner + simops_cascade agents  │
│  Quality-weighted DBSCAN clustering:                                │
│    - Low-score cluster: { kombucha_fermentation, high temp, low n } │
│    → SemanticRule: "kombucha_fermentation overestimates yield       │
│       by ~15% when temperature_c > 65°C and sample_n < 8"          │
│                                                                      │
│    - High-score cluster: { bc_optimization, 3 vessels, 30°C }      │
│    → SemanticRule: "bc_optimization is well-calibrated for          │
│       static culture at 30°C with ≥3 parallel instances"           │
│                                                                      │
│  Semantic rules written to knowledge graph                          │
│  → Injected into next dynamics_runner execution via KG context      │
└────────────────────────────────────────────────────────────────────┘
```

---

## 4. Spec: ProjectionScoringEvaluator

### 4.1 Trigger

Fired when a new SOSA observation is ingested via `POST /api/simops/ingest-observations` (or the standard ingest endpoint) where:
- `procedure` ≠ `"simops_simulation"` (real measurement, not synthetic)
- `observable_property` ∈ known SimOps properties (yield, carbon, opex, Brix, pH, etc.)
- A prior synthetic observation exists for the same `(session_id OR stage_feature_of_interest, observable_property)` with `procedure = "simops_simulation"`

### 4.2 Matching logic

**Preferred:** explicit `projection_id` in real observation's `extra` field — operator or sensor system links the measurement to the prediction.

**Fallback:** look back N days for the most recent synthetic observation matching `(observable_property, feature_of_interest)` where `extra.source = "simops_simulation"`. N = 7 days default (configurable). If multiple matches, take the most recent.

**No match found:** no EvalSignal emitted. Silent — missing predictions shouldn't pollute the quality record.

### 4.3 Score computation

```
relative_error = |predicted - actual| / max(|actual|, 1e-9)
score = 1.0 - min(relative_error, 1.0)
```

Range: `[0.0, 1.0]`. 1.0 = exact match. 0.0 = ≥100% relative error.

Confidence: `min(n_prior_observations_for_this_model_property / 10.0, 1.0)`. Low confidence when few observations exist for this model/property combination.

### 4.4 EvalSignal fields

```rust
EvalSignal {
    dimension: "projection_accuracy",
    score: f64,           // 0.0 – 1.0
    confidence: f64,      // 0.0 – 1.0
    flags: json!({
        "model_uri":            String,  // "kask:dynamics/kombucha_fermentation@v1"
        "stage_id":             String,  // "primary_fermentation"
        "observable_property":  String,  // "bio:bc_yield_g_per_l"
        "predicted":            f64,
        "actual":               f64,
        "relative_error":       f64,
        "delta_direction":      String,  // "over" | "under" | "exact"
        "projection_id":        String,  // from CascadeResponseV2.provenance.projection_id
        "temperature_c":        Option<f64>,  // context from the projection's process_context
        "n_instances":          Option<usize>,
        "model_step_size_days": Option<f64>,
    }),
    // Links to the episode that ran the projection
    episode_id: Option<Uuid>,
    agent_id: Uuid,  // simops_dynamics_runner or simops_cascade agent UUID
}
```

### 4.5 Linking to the episode

The `projection_id` from `CascadeResponseV2.provenance` and `SkillOutput.provenance` is written into both:
1. The synthetic SOSA observation `extra` (by the dynamics runner / cascade handler)
2. The agent episode `context` (by the standard episode writing path)

The evaluator looks up the episode where `context->>'projection_id' = <projection_id>` to link the EvalSignal.

---

## 5. Spec: projection_id propagation

### 5.1 Cascade handler

When `POST /api/workspaces/:id/cascade` or `POST /api/simops/cascade` returns a `CascadeResponseV2`, the response already contains `provenance.projection_id`. No change needed to the response.

The **new behaviour**: when the response is written as an agent episode, `projection_id` goes into `episodes.context`:

```json
{
  "projection_id": "proj-coupled-abc123",
  "model_uris": ["kask:dynamics/bc_optimization@v1"],
  "stage_count": 2,
  "twin_id": "primary"
}
```

This already happens through the standard episode-writing path if the skill output includes `projection_id` in its response — verify this is wired.

### 5.2 Dynamics runner skill output → synthetic observations

When `simops_cascade` or `simops_dynamics_runner` writes synthetic SOSA observations (via `simops_write_observation` skill), include `projection_id` in `extra`:

```json
{
  "projection_id": "proj-coupled-abc123",
  "source": "simops_simulation",
  "model_uri": "kask:dynamics/kombucha_fermentation@v1",
  "stage_id": "primary_fermentation",
  "predicted_value": 4.2
}
```

This is a **small change to the `SimopsWriteObservation` skill** — add `projection_id` as an optional field that passes through from the dynamics response.

### 5.3 Real observation tagging (optional, operator-assisted)

kask can optionally write `projection_id` into the real observation's `extra` when the operator is recording measurements against a known batch. This makes matching deterministic. When absent, the evaluator uses the fallback lookup.

---

## 6. Spec: SemanticRule patterns the consolidation worker should emit

The consolidation worker is already built. The new behaviour emerges automatically once quality-weighted episodes flow in. But we should document the expected rule patterns so the agent prompt can be calibrated:

**Model underperformance rules (low projection_accuracy score):**
```
"kask:dynamics/kombucha_fermentation@v1 systematically overestimates 
 bio:bc_yield_g_per_l by 12–18% when temperature_c > 65°C. 
 Consider: (a) using bc_optimization model instead, 
            (b) recalibrating Arrhenius parameters for high-temp regime."
```

**Model reliability rules (high projection_accuracy score):**
```
"kask:dynamics/bc_optimization@v1 is well-calibrated for 
 bio:bc_yield_g_per_l in static culture (agitation_rpm=0) at 28–32°C 
 with ≥2 active instances. Confidence: 0.82 (n=14 observations)."
```

**Condition-specific rules:**
```
"All dynamics models show >20% relative error on bio:pellicle_g_per_l 
 for workspace 9691316e... (foo5). Likely cause: density_kg_per_unit 
 miscalibrated for this SCOBY strain. Recommend: run density calibration."
```

These rules are injected into `simops_dynamics_runner`'s system prompt context on the next execution via the existing KG context enrichment path (`enrich_with_kg_context`).

---

## 7. Migration

One new migration:

```sql
-- Migration 130: Index sosa_observations.extra for projection_id lookup
-- Enables ProjectionScoringEvaluator to find prior synthetic observations
-- for a given (observable_property, feature_of_interest) efficiently.

CREATE INDEX IF NOT EXISTS idx_sosa_obs_projection_id
    ON sosa_observations ((extra->>'projection_id'))
    WHERE extra->>'projection_id' IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_sosa_obs_source_property
    ON sosa_observations (observable_property, feature_of_interest)
    WHERE extra->>'source' = 'simops_simulation';
```

---

## 8. Implementation checklist

| Task | File | Size | Blocking |
|---|---|---|---|
| Add `projection_id` to synthetic SOSA observation `extra` | `src/agent_backend/simops_tools.rs` | Small | Yes — needed before scoring can work |
| Write `projection_id` into episode `context` in dynamics/cascade handlers | `src/handlers/simops.rs` | Small | Yes |
| Migration 130 (index) | `migrations/130_sosa_projection_index.sql` | Trivial | Yes |
| `ProjectionScoringEvaluator` | `agent-bestiary/evaluators/src/projection_scoring.rs` | Medium (~150 LoC) | Yes |
| Register evaluator in `EvaluatorRegistry` | `agent-bestiary/evaluators/src/registry.rs` | 1 line | Yes |
| Update `simops_dynamics_runner` agent card skill list | `agents/curated/simops_dynamics_runner/agent_card.json` | Trivial | No |
| Add `projection_accuracy` to Loop 5.A Brier calibration dashboard query | `src/handlers/agents.rs` | Small | No |

**Estimated effort:** 2–3 days focused work. The evaluator is the bulk — the rest is plumbing.

---

## 9. What this is NOT

- **Not user-acceptance signal.** The score is computed from measurement delta, not from whether the operator clicked accept.
- **Not continuous training.** No weights are updated. The loop is: real observation → EvalSignal → episodes → consolidation → semantic rules → KG context injection → better next projection.
- **Not LoRA.** The semantic rules are retrievable text, not gradient-encoded parameters. This is the correct substitute for API models. The question of whether text-based knowledge accumulation reaches the same quality ceiling as gradient descent is open; for now, the text path is the right pragmatic choice.
- **Not automatic model replacement.** When the consolidation worker emits "model X is poorly calibrated for condition Y," it does not automatically swap the model. It injects a rule that the dynamics_runner reads at the next invocation — the runner can then propose a different model or flag the calibration gap to the operator.

---

## 10. Relationship to the broader SIA insight

SIA's finding: *"Harness shapes how the agent searches; weight updates change what the model knows."*

This spec closes the feedback loop on the **harness side** without requiring weight updates:

- The semantic rules from consolidation are harness-level changes — they modify what the dynamics_runner considers when selecting and parameterising models
- The projection_accuracy EvalSignal is the verifier that drives consolidation toward useful rules rather than noise
- The deferred-measurement structure makes the verifier honest — the batch doesn't know or care what the model predicted

The weight-update side remains open. If and when fine-tunable local models run via Ollama (the `min_provider_class: local` path), the same quality-weighted episode history becomes the training dataset for LoRA fine-tuning of domain-specific model selection behaviour. The infrastructure built here is a direct prerequisite for that path.

---

## 11. Open questions for the operator

**Q1 — Matching strictness.** When no explicit `projection_id` is in the real observation, should the 7-day lookback be workspace-scoped (any projection in this workspace), stage-scoped (same stage_id), or property-scoped (same observable_property + feature_of_interest)? Tighter matching = fewer scores but more reliable; looser matching = more scores but noisier.

**Recommendation:** property + feature_of_interest + 30-day window. Loose enough to accumulate signal, tight enough to be physically meaningful.

**Q2 — Score threshold for rule emission.** Should the consolidation worker only emit "this model is unreliable" rules when projection_accuracy < 0.7, or on any cluster of low-scoring episodes?

**Recommendation:** emit rules for any cluster where mean(projection_accuracy) < 0.75 AND n_episodes ≥ 3. Below n=3, the evidence is too thin to trust.

**Q3 — Feedback to the operator surface.** Should projection_accuracy scores surface in the kask Digital Twin UI as a "model calibration" indicator per stage? E.g. a green/amber/red badge showing "this model's last 5 projections were X% accurate on average."

**Recommendation:** yes — this is the most direct operator value. The badge is a kask-side rendering of `mean(eval_signals WHERE dimension='projection_accuracy' AND agent=dynamics_runner AND flags->>'stage_id'=X AND flags->>'model_uri'=Y ORDER BY created_at DESC LIMIT 5)`.

---

## 12. Relationship to BayesOps (Spec 14)

This spec and `docs/specs/14_BAYESOPS_SPEC.md` address the same root problem — historical data shaping SimOps predictions — via complementary mechanisms that operate at different levels of the stack. They are not alternatives; they compose.

**This spec (Spec 20) operates at the harness level — Loop 1.B (projection accuracy).**

The `ProjectionScoringEvaluator` produces `EvalSignal` rows. The `ConsolidationWorker` clusters them into semantic rules. Those rules are injected into `simops_dynamics_runner`'s KG context before each execution. The effect: the agent learns *which models are unreliable under which conditions* and can reason about that at inference time. No distribution parameters change. No model weights change. The harness changes.

Example rule produced: `"kask:dynamics/kombucha_fermentation@v1 overestimates bio:bc_yield_g_per_l by ~15% when temperature_c > 65°C"`. The runner reads this in its context, selects a different model or flags the calibration gap, and produces a better projection.

**Spec 14 (BayesOps) operates at the distribution-parameter level — Loop 5.B (offline parameter correction).**

BayesOps fits a posterior distribution over historical observations and produces `Beta(α, β)` or `Normal(μ, σ)` parameters that feed directly into FPL `Driver` declarations. The effect: the *uncertainty width* of the forecast is calibrated to the evidence available. 8 real runs produces a wide posterior; 80 runs produces a tight one. This is not a harness change — it changes the parameters of the distributions the Monte Carlo executor samples from.

Example output: given 12 real Ambu runs, BayesOps produces `Beta(7.2, 5.1)` for the yield-success base rate. An FPL model uses `Driver base_rate: Beta(7.2, 5.1)` — the uncertainty correctly reflects that 12 runs is thin evidence.

**How they compose:**

```
Real batch completes
    │
    ├─→ Spec 20: ProjectionScoringEvaluator
    │       → EvalSignal (projection_accuracy)
    │       → ConsolidationWorker → semantic rules
    │       → KG context: "model X unreliable at condition Y"
    │       → simops_dynamics_runner selects a better model next time
    │
    └─→ Spec 14: BayesOps refit trigger (N new observations)
            → fit_regression() on updated observation set
            → posterior: Normal(4.6, 0.8) at planned input conditions
            → FPL Driver updated: Driver yield: Normal(4.6, 0.8)
            → forecast interval narrows as evidence accumulates
```

Spec 20 improves *which model the agent chooses and how it reasons about its limitations*. Spec 14 improves *the calibrated uncertainty of the distribution parameters that model runs with*. A fully instrumented SimOps workspace benefits from both: better model selection (Spec 20) and better-calibrated uncertainty on the selected model's outputs (Spec 14).

**Implementation ordering:** Spec 20 should ship first. It requires only the `ProjectionScoringEvaluator` and a migration (§8 checklist: ~2–3 days). Spec 14 Phase 1 (`crates/posterior`, simple marginal fitting) can follow. The full BayesOps regression path (Spec 14 Phases 2–5) depends on sufficient real observation volume to validate — Spec 20's scoring infrastructure is a prerequisite for knowing when that volume threshold is reached.
