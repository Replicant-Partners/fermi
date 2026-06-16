# BayesOps ↔ Fermi Contract

**Status:** Live as of 2026-06-16. Two contract surfaces — generic learnable
drivers (the product) and learnable elasticities inside factor models (the
World Cup demo).
**Audience:** BayesOps implementation (separate string) needs this to plug in.

## Summary

Fermi exposes two kinds of externally-overrideable parameters:

1. **Learnable drivers** — any `driver continuous foo { ... learnable: true }`
   declaration. Acts on the driver's whole distribution: BayesOps writes a
   `FittedDistribution` JSON to `params.<driver_name>_fitted` and the executor
   substitutes it for the static prior at sim time. This is the **generic
   product surface** — any forecast can opt drivers into BayesOps management.

2. **Learnable elasticities** — `learnable(initial, sigma)` literals inside
   factor formulations and estimate expressions. Acts on a scalar coefficient.
   Names are auto-assigned positionally (`<owner>_l<idx>`); BayesOps writes
   point estimates as plain numbers. This is the **factor-model contract**
   used by the World Cup 2026 demo (48 team prior workspaces).

Both flow through the same mechanism: `.app/params.json` written via
`PUT /api/workspaces/:id/outputs/params`. The executor reads at sim time, no
FPL rewriting required.

## Surface 1: Learnable Drivers (generic)

### Declaration

A user marks any driver as learnable in FPL:

```fpl
driver continuous yield_kg {
    distribution: triangular(3.0, 5.0, 7.0)
    learnable: true
    unit: "kg"
    rationale: "Cold-start prior from agronomist elicitation."
}
```

The `distribution:` block is now interpreted as the **prior** rather than the
sampling distribution. It's used (a) as a cold-start when no fit exists yet,
and (b) as the conjugate prior in BayesOps' updating math.

### Read side: discovering learnable drivers

Every sim run publishes a `learnable_drivers` output listing how each
learnable driver was resolved this run:

```json
[
  {
    "name": "yield_kg",
    "status": "fitted",
    "fitted": {
      "family": "normal",
      "mean": 4.8,
      "std_dev": 0.55,
      "ci_low": 3.7,
      "ci_high": 5.9,
      "n_eff": 23.0
    },
    "fpl_params": "normal(4.8000, 0.5500)",
    "ci_width": 2.2,
    "n_eff": 23.0,
    "params_key": "yield_kg_fitted"
  },
  {
    "name": "other_driver",
    "status": "prior_fallback",
    "reason": "no params.other_driver_fitted in scope"
  }
]
```

Statuses:
- `fitted`: A valid `FittedDistribution` was found in `params.<name>_fitted`
  and used at sim time. Returns full fitted parameters + diagnostics.
- `prior_fallback`: Driver is marked `learnable: true` but no fit exists yet
  (cold start or BayesOps hasn't run). The static `distribution:` block was
  used as a prior.

### Write side: BayesOps updates a driver's distribution

BayesOps fits a `posterior::FittedDistribution` from observations, serializes
it as JSON, and writes it to the workspace's `params` output:

```http
PUT /api/workspaces/<ws_id>/outputs/params
{
  "value": {
    ... (existing params) ...,
    "yield_kg_fitted": {
      "family": "normal",
      "mean": 4.8,
      "std_dev": 0.55,
      "ci_low": 3.7,
      "ci_high": 5.9,
      "n_eff": 23.0
    }
  }
}
```

The four supported families match the `posterior::FittedDistribution` enum:
`beta`, `normal`, `lognormal`, `triangular`. The JSON shape matches
`FittedDistribution`'s `#[serde(tag="family", rename_all="lowercase")]`
serialization (see `crates/posterior/src/lib.rs:82`).

### Driver-type / family constraints

| Driver type | Valid `FittedDistribution` families | Behavior on mismatch |
|-------------|--------------------------------------|----------------------|
| `continuous` | Beta, Normal, Lognormal, Triangular | — |
| `binary`    | **Beta only** (over success probability p ∈ [0,1]) | Logged warning, fall back to static prior |
| `discrete`  | Not yet supported — fits to a categorical distribution are Phase 2+ | No-op (prior used) |

For **binary** learnable drivers, the executor samples `p ~ Beta(α, β)`
fresh each iteration before running the Bernoulli trial. This propagates
BayesOps' epistemic uncertainty into the outcome distribution. When n_eff
is small, the Beta is wide and individual `p` draws vary, widening the
outcome spread; as n_eff grows, the Beta tightens and outcomes look like
a sharp Bernoulli at the posterior mean.

When the executor finds `<driver_name>_fitted` in `json_params`, it:
1. Deserializes to `FittedDistribution`
2. Converts to FPL `Distribution` via `Self::fitted_to_fpl_distribution`
3. Samples from the converted distribution instead of the static prior

Any deserialization error is logged to stderr and the executor silently
falls back to the prior — i.e. a malformed fit can't break a forecast.

### Verified end-to-end

Cold start with prior `triangular(3, 5, 7)`:
- mean 5.01, p5..p95 = [3.66, 6.36], std_dev 0.80
- `learnable_drivers[0].status = "prior_fallback"`

With BayesOps fit `Normal(4.8, 0.55)` from 23 observations:
- mean 4.81, p5..p95 = [3.91, 5.73], std_dev 0.55  ← **tightened by the fit**
- `learnable_drivers[0].status = "fitted"`, `n_eff = 23.0`, `ci_width = 2.2`

---

## Surface 2: Learnable Elasticities (factor models)

### Read side: discovering what's learnable

Every factor-model run publishes a `learnable_manifest` output. Example
(`PUT /api/workspaces/:id/outputs/learnable_manifest`, set by
`initialize-workspace`):

```json
[
  {
    "name": "tournament_strength_l0",
    "initial": 1.0,
    "sigma": 0.2,
    "owner": "tournament_strength",
    "current_value": 1.0,
    "is_overridden": false
  },
  {
    "name": "tournament_strength_l1",
    "initial": 0.25,
    "sigma": 0.05,
    "owner": "tournament_strength",
    "current_value": 0.25,
    "is_overridden": false
  },
  ...
]
```

**Fields:**

- `name`: Stable identifier of the form `<owner>_l<idx>`, where
  - `<owner>` is the containing statement (factor name like `X3` or estimate
    name like `tournament_strength`)
  - `<idx>` is the 0-based positional index of the `learnable(...)` literal
    within that statement, traversed depth-first source-order
- `initial`: The prior's point estimate (the first arg to `learnable(...)`)
- `sigma`: The prior's stddev (the second arg). Treat as a Gaussian prior
  on the parameter — `N(initial, sigma²)`.
- `owner`: The containing statement, for context. Informational.
- `current_value`: The value used in the most recent sim. Equal to `initial`
  if no override has been written; equal to the override otherwise.
- `is_overridden`: True iff `params.<name>` is present in
  `.app/params.json`.

**Stability guarantee:** As long as the FPL template doesn't change, names are
stable across runs. Adding/removing/reordering `learnable(...)` literals in
the template renumbers everything from that point forward. Treat `(template_path,
name)` as the unique key. If you need stronger guarantees later, the parser
can be extended to accept `learnable[name](...)` syntax (AST already supports
this via `LearnablePrior.name`).

## Write side: updating a learnable

To update `tournament_strength_l3` (X3 elasticity) from its prior of 0.25 to
the posterior point estimate of 0.31, BayesOps writes:

```http
PUT /api/workspaces/<ws_id>/outputs/params
Content-Type: application/json

{
  "value": {
    "team_id": "ARG",
    "team_name": "Argentina",
    ... (existing params) ...,
    "tournament_strength_l3": 0.31
  }
}
```

The `params` workspace output is the canonical source. The Rust executor
binary (`initialize-workspace`) reads it via the workspaces API and writes
it back into `.app/params.json` at sim time.

**Convention:** BayesOps reads the full `params` object, merges its updated
keys, and writes back the full object. Last-writer-wins.

## Re-running the sim

After BayesOps writes updated params:

```bash
# Manual:
./target/debug/initialize-workspace \
    --template templates/world_cup/team_prior.fpl \
    --params <(curl -s ".../params" | jq .value) \
    --iterations 10000 --seed 42

# Or via the publish wrapper:
python3 scripts/world_cup/publish_team_priors.py --only ARG
```

The wrapper PUTs the resulting outputs (including a fresh
`tournament_strength` distribution) back to the workspace. The DAG fans the
update out to dependent workspaces (group paths, etc.).

## Brier feedback loop placement

BayesOps owns the math; Fermi owns the data plumbing:

```
match resolves
    ↓
Polymarket/match API → workspace_outputs/match_outcome  (Fermi writes)
    ↓
BayesOps reads:
  - learnable_manifest (priors)
  - tournament_strength (forecast distribution)
  - match_outcome (ground truth)
    ↓
BayesOps fits posterior → updated learnable point estimates
    ↓
BayesOps writes back to params (PUT /outputs/params)
    ↓
Fermi re-runs sim → publishes new tournament_strength
    ↓
DAG fans update to group paths, H2H matches
```

## Per-team-prior learnable inventory

For the 48 team-prior workspaces using `templates/world_cup/team_prior.fpl`,
each workspace exposes 7 learnables (one per Cobb-Douglas term):

| name | meaning | prior | range |
|------|---------|-------|-------|
| `tournament_strength_l0` | intercept A (Cobb-Douglas multiplier) | 1.0 | (0.5, 2.0) |
| `tournament_strength_l1` | elasticity α on X1 (socio capital) | 0.25 | [0, 1] |
| `tournament_strength_l2` | elasticity β on X2 (institutional) | 0.20 | [0, 1] |
| `tournament_strength_l3` | elasticity γ on X3 (dynamic perf / Elo) | 0.25 | [0, 1] |
| `tournament_strength_l4` | elasticity δ on X4 (squad quality) | 0.15 | [0, 1] |
| `tournament_strength_l5` | elasticity ε on X5 (tactical eff) | 0.10 | [0, 1] |
| `tournament_strength_l6` | elasticity ζ on X6 (exogenous, log-linear) | 0.05 | [0, ∞) |

The variance-share constraint (Σ variance_share = 1.0) does NOT bind the
elasticities themselves. BayesOps is free to fit elasticities however the
data supports, but the executor's semantic check will warn if variance
shares drift more than 5% from unity. Variance shares are not currently
learnable (they live as `variance_share: <const>` in the factor block, not
as `learnable(...)`); promoting them to learnable is a future change.

## What's NOT in the contract (yet)

- **Cross-workspace updates.** If BayesOps wants to update elasticities
  consistently across all 48 team priors (because the Cobb-Douglas
  elasticities are population-level, not per-team), it currently has to
  PUT 48 separate `params` payloads. A `PATCH /api/apps/:slug/learnables`
  shared-elasticity endpoint is a future improvement.
- **Variance share updates.** Per above.
- **Residualization coefficients.** Phase 4 will compute OLS projection
  weights at sim time; those aren't `learnable(...)` and aren't exposed.
- **The softmax temperature** in `publish_team_priors.py`. It's a scoring
  head, not a model parameter. Update via PR if needed.

## Failure modes & guarantees

- **Override is ignored if name doesn't match a learnable.** The evaluator
  falls back to `initial`. No error, no warning. (Future: add a strict-mode
  flag that errors on dangling overrides.)
- **Override of wrong type breaks the sim.** If `tournament_strength_l3 =
  "fast"` (string), the JSON loader silently drops it (string params go to
  metadata, not numeric_params). The sim runs with the prior.
- **Override outside reasonable range may NaN the Cobb-Douglas.** If
  BayesOps writes an elasticity of -5, the executor will produce NaN
  responses and skip those iterations (see `execute_factor_model`'s finite
  guard). If too many iterations fail, the executor returns
  `EvaluationError("Factor model produced no finite response samples")`.
  BayesOps should clamp updates to sane ranges.

## Research agent orchestra (factor inputs → BayesOps inputs)

Factor-model inputs that BayesOps eventually fits against don't appear from
thin air — they come from research agents in the Fermi orchestra. Each
factor in `team_prior.fpl` has a designated agent that emits evidence with
structured numeric values:

| Factor | Agent | Reads | Emits |
|--------|-------|-------|-------|
| X1 Socioeconomic Capital | `macro_data_agent` | World Bank, UNDP HDR | gdp_per_capita_log, population_log, hdi_logit |
| X2 Institutional Capacity | `football_institution_agent` | FIFA Big Count, Deloitte | player_penetration_rate, league_revenue_log, confederation_coefficient |
| X3 Dynamic Performance | `football_analyst` (v1.1+) | API-Football, FBref, StatsBomb | elo_current, elo_trend, goal_difference, pass_completion, xg_delta |
| X4 Squad Quality | `football_analyst` (v1.1+) | Transfermarkt, Big-5 leagues | market_value_concentration, top5_league_pct, squad_depth_score, avg_age_adjusted |
| X5 Tactical Efficiency | `football_analyst` (v1.1+) | StatsBomb, FBref | shot_conversion_rate, defensive_duel_win_pct, pressing_intensity, set_piece_efficiency |
| X6 Exogenous Context | `fixture_context_agent` | Weather APIs, venue geodata | host_status, climate_delta, rest_days, altitude_delta |

All four agents emit the standard Fermi-orchestra `[MULTIPLIER]` finding
format and conform to the agent-card schema (validated by
`test_all_curated_agents_have_valid_cards`). They are auto-hired into every
`fermi_forecast` workspace via `apps/fermi_forecast.json::auto_hire`.

## Harness benchmarking integration

Each agent run captured by the workspace contributes to the
`harness_snapshots.specialist_roster` JSONB: `[{agent_id, version,
calibration_score}, …]`. The composite `content_hash` over conductor +
roster + routing weights + bayesops_params makes the entire
configuration reproducible: anyone re-creating the same hash gets the
same forecast.

Implications when an agent changes:
- Bumping an agent's `version` (e.g. `football_analyst` 1.0.0 → 1.1.0
  for factor awareness) **invalidates the previous harness hash**. New
  forecasts get a new snapshot row; old forecasts stay tied to the
  prior hash. This is intentional: it lets BayesOps separate
  performance signal pre- and post-agent-change.
- `performance.avg_brier_impact` per agent is auto-updated as
  forecasts resolve. Over time this surfaces which agents are
  carrying signal vs noise for a given forecast type.

## Files

- Contract source: `src/executor.rs::execute_factor_model` (and the
  `assign_learnable_names` / `collect_learnable_info` helpers)
- Evaluator override path: `src/evaluator.rs::evaluate` →
  `Expression::LearnablePrior`
- CLI emitter: `scripts/initialize_workspace.rs`
- Batch wrapper: `scripts/world_cup/publish_team_priors.py`
- Template: `templates/world_cup/team_prior.fpl`
- Research agents:
  - `agents/curated/macro_data_agent/` (X1)
  - `agents/curated/football_institution_agent/` (X2)
  - `agents/curated/football_analyst/` (X3-X5, v1.1+)
  - `agents/curated/fixture_context_agent/` (X6)
- Auto-hire wiring: `apps/fermi_forecast.json::workspace_template.auto_hire`
- Harness snapshot capture: `src/handlers/forecast_benchmark.rs::capture_harness_snapshot`
