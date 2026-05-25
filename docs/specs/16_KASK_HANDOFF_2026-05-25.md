# Kask Handoff — 2026-05-25
**Updated:** 17:58 CEST  
**From:** ABW platform session (Ivan Labra)  
**To:** kask.bio frontend / integration team  
**Commits this session:** `b4826fa`, `ff1db59`, `3553fa3`  
**Re:** What is live, what each endpoint returns, smoke-test curls

---

## TL;DR — Status Board

| Capability | Status | Endpoint |
|---|---|---|
| Workspace spawn / agent invocation / state reads | ✅ Unchanged | same as before |
| `monte_carlo_sim` — real MC + Sobol indices | ✅ Live | `POST /api/agents/monte_carlo_sim/execute` |
| `+ Generate distribution` (Digital Twin histogram) | ✅ Live | `POST /api/simops/project` |
| ODE time-series projection (Digital Twin sparkline) | ✅ Live | `POST /api/simops/dynamics` |
| Model catalogue (for model-picker UI) | ✅ Live | `GET /api/simops/dynamics/models` |
| `simops_dynamics_runner` agent | ✅ Live | `listWorkspaceAgents` returns it |
| BayesOps base rate fitting | 🗓 Roadmap | nothing to wire yet |

**Nothing in the existing integration changed.** Spawn, auth, agent invocation, SOSA writes, ProcessConfig schema — all identical.

---

## 1. `monte_carlo_sim` — Now Real (v2.0.0)

Previously LLM-approximated percentiles in text. Now calls two real deterministic tools.

**What changed internally:** `fermi_execute_fpl` (10,000-sample MC via `executor.rs`) and `fermi_sensitivity_analysis` (real Sobol indices via `sensitivity.rs`) are now wired as MCP tools. The agent writes FPL, executes it, reports actual computed numbers.

**Invocation — unchanged:**

```bash
POST /api/agents/monte_carlo_sim/execute
{
  "query": "What is the probability batch yield exceeds 5kg given lighting=135kWh, nutrients=6.8g, temp=27.5C?",
  "workspace_id": "<workspace_id>"
}
```

**Smoke test:**
```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "Estimate the probability a SaaS startup reaches $1M ARR in 18 months given $500k runway, 15% MoM growth, 8% churn.", "workspace_id": "<any>"}' \
  https://agent-bestiary.world/api/agents/monte_carlo_sim/execute
```

The response now contains **real computed percentiles**. Pass `seed: 42` in the query text for reproducible runs.

---

## 2. `POST /api/simops/project` — Distributional Projection

N-run Monte Carlo over the SimOps cascade with sampled inputs. Powers the `+ Generate distribution` button in the Digital Twin tab. **No LLM. No credits charged.**

### Request

```json
{
  "model": {
    "kind": "simops_cascade",
    "config": { /* ProcessConfig JSON — same shape as /api/simops/cascade */ }
  },
  "sweep": {
    "kind": "monte_carlo",
    "variables": [
      {
        "path": "/stages/0/efficiency",
        "distribution": { "type": "normal", "mean": 0.85, "std": 0.04 }
      },
      {
        "path": "/elec_price_per_kwh",
        "distribution": { "type": "uniform", "low": 0.09, "high": 0.16 }
      }
    ]
  },
  "n_runs": 100,
  "seed": 42,
  "output_format": "aggregate"
}
```

**Variable paths** use JSON Pointer (RFC 6901) into ProcessConfig:

| What to vary | Path |
|---|---|
| Stage 0 efficiency | `/stages/0/efficiency` |
| Stage N efficiency | `/stages/N/efficiency` |
| Electricity price | `/elec_price_per_kwh` |
| Primary input quantity | `/primary_input_quantity` |
| Stage N OPEX/unit | `/stages/N/opex_per_input_unit` |

**Distributions:** `uniform {low, high}` · `normal {mean, std}` · `triangular {p5, p50, p95}` · `beta {alpha, beta}`

**`output_format`:** `"aggregate"` (default — histograms + percentiles) · `"raw_runs"` · `"both"`

### Response

```json
{
  "output": {
    "executor_kind": "simops_cascade",
    "sweep_kind": "monte_carlo",
    "n_requested": 100,
    "n_completed": 100,
    "seed": 42,
    "dimensions": [
      {
        "dimension": "final_output_quantity",
        "n_runs": 100,
        "n_failed": 0,
        "mean": 4.31,
        "std_dev": 0.48,
        "min": 3.12,
        "p5": 3.51,
        "p25": 3.94,
        "p50": 4.28,
        "p75": 4.67,
        "p95": 5.14,
        "max": 5.89,
        "histogram": [[3.1, 4], [3.5, 12], [3.9, 23], [4.3, 31], [4.7, 22], [5.1, 8]]
      },
      { "dimension": "net_carbon_kg", ... },
      { "dimension": "total_opex_usd", ... },
      { "dimension": "system_ner", ... }
    ]
  }
}
```

**Histogram bins:** `[bin_low_value, count]` pairs. Auto-sized via Freedman-Diaconis, capped at 50 bins. Feed directly to your chart renderer.

### Smoke test

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model": { "kind": "simops_cascade", "config": { "name": "test", "stages": [{"id": "step1", "efficiency": 0.85, "carbon_intensity": 0.1, "input": {"name": "in", "unit": "kg"}, "output": {"name": "out", "unit": "kg"}, "opex_per_input_unit": 1.0}] } },
    "sweep": { "kind": "monte_carlo", "variables": [{ "path": "/stages/0/efficiency", "distribution": {"type": "normal", "mean": 0.85, "std": 0.04} }] },
    "n_runs": 50, "seed": 1, "output_format": "aggregate"
  }' \
  https://agent-bestiary.world/api/simops/project
```

Expect 200 with `dimensions[0].dimension = "final_output_quantity"` and 50 runs.

---

## 3. `POST /api/simops/dynamics` — ODE Time-Series Projection

Runs an ODE dynamics model forward in time and returns per-sensor trajectories. This is the Digital Twin time-series panel — pH curves, Brix curves, pellicle growth, BC yield. **No LLM. No credits charged.**

### Available models

| `model_uri` | State dimensions | Use case |
|---|---|---|
| `kask:dynamics/kombucha_fermentation@v1` | `chem:brix_percent`, `chem:ph_value` | Primary fermentation monitoring |
| `kask:dynamics/pellicle_growth@v1` | + `bio:pellicle_g_per_l` | SCOBY mat growth tracking |
| `kask:dynamics/bc_optimization@v1` | + `bio:bc_yield_g_per_l`, `bio:bc_quality_index` | BC yield/quality trade-off |
| `kask:dynamics/linear_decay@v1` | any single property | Generic first-order relaxation |

All models are Arrhenius temperature-dependent. `temperature_c` in `process_context` controls the rates (default 26°C if omitted).

### Request

```json
{
  "model_uri": "kask:dynamics/kombucha_fermentation@v1",
  "initial_state": {
    "chem:brix_percent": 10.0,
    "chem:ph_value": 5.0
  },
  "process_context": {
    "temperature_c": 26.0
  },
  "params_override": {
    "ph_floor": 2.8
  },
  "horizon": { "kind": "fixed", "days": 14 },
  "sample_cadence": { "hours": 6 },
  "generated_by": "operator"
}
```

**`horizon` kinds:**
- `{ "kind": "fixed", "days": N }` — integrate for exactly N days
- `{ "kind": "until_property_reaches", "property": "chem:ph_value", "value": 3.0, "max_days": 21 }` — stop when property crosses threshold

**`sample_cadence`:** how often to record a trajectory point. Default 6h. Use 1h for smoother curves, 24h for daily summaries.

**`process_context` fields by model:**

| Field | Models | Default |
|---|---|---|
| `temperature_c` | all | 26.0 |
| `agitation_rpm` | `bc_optimization` | 0 (static) |
| `do_saturation_pct` | `bc_optimization` | 10.0 |
| `carbon_source` | `bc_optimization` | `"glucose"` |

**`params_override` fields (common):**

| Field | Default | Effect |
|---|---|---|
| `ph_floor` | 2.5 | Acidification floor — pH never drops below this |
| `p_max` | 8.0 (g/L) | Pellicle carrying capacity |
| `bc_max` | 6.0 (g/L) | BC carrying capacity |
| `step_size_days` | 0.01 | Integration step (smaller = more accurate, slower) |

### Response

```json
{
  "trajectories": {
    "chem:brix_percent": [
      { "t_hours": 0.0,   "value": 10.0 },
      { "t_hours": 6.0,   "value": 9.87 },
      { "t_hours": 12.0,  "value": 9.74 },
      ...
      { "t_hours": 336.0, "value": 1.23 }
    ],
    "chem:ph_value": [
      { "t_hours": 0.0,   "value": 5.0 },
      { "t_hours": 6.0,   "value": 4.91 },
      ...
    ]
  },
  "provenance": {
    "model_uri": "kask:dynamics/kombucha_fermentation@v1",
    "model_version": "1.0.0",
    "integrator": "rk4",
    "step_size_days": 0.01,
    "generated_at": "2026-05-25T17:58:00Z",
    "projection_id": "a3f8c2d1-...",
    "generated_by": "operator",
    "params_used": { "ph_floor": 2.8, ... },
    "context_used": { "temperature_c": 26.0 },
    "initial_state": { "chem:brix_percent": 10.0, "chem:ph_value": 5.0 }
  },
  "converged": false,
  "notes": [
    { "severity": "info", "message": "pH floor (2.80) engaged — acidification stops.", "t_hours": 216.0 }
  ]
}
```

**`trajectories`:** keyed by property URI — arrays of `{t_hours, value}`. Map `t_hours` to your x-axis, `value` to y-axis. Overlay live sensor readings on the same chart using their `phenomenon_time` converted to hours since batch start.

**`notes`:** surface these to the operator — pH floor engagements, substrate exhaustion warnings, quality degradation alerts.

**`converged`:** true when the model detects steady state (rolling window variance < threshold). Use this to suppress "projection still running" UI states.

### Smoke test — kombucha 14-day fermentation

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "model_uri": "kask:dynamics/kombucha_fermentation@v1",
    "initial_state": { "chem:brix_percent": 10.0, "chem:ph_value": 5.0 },
    "process_context": { "temperature_c": 26.0 },
    "horizon": { "kind": "fixed", "days": 14 },
    "sample_cadence": { "hours": 6 }
  }' \
  https://agent-bestiary.world/api/simops/dynamics
```

Expect 200. `trajectories.chem:brix_percent` should start at 10.0 and decay toward 0. `trajectories.chem:ph_value` should start at 5.0 and drop toward ~2.5–3.0. `notes` should contain a pH floor message around day 8–12.

### Smoke test — BC optimization (agitated vs static comparison)

```bash
# Static culture (higher quality)
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"model_uri":"kask:dynamics/bc_optimization@v1","initial_state":{"chem:brix_percent":8,"chem:ph_value":6,"bio:bc_yield_g_per_l":0,"bio:bc_quality_index":1},"process_context":{"temperature_c":30,"agitation_rpm":0,"do_saturation_pct":10,"carbon_source":"glucose"},"horizon":{"kind":"fixed","days":14},"sample_cadence":{"hours":12}}' \
  https://agent-bestiary.world/api/simops/dynamics

# Agitated culture (higher yield, lower quality)
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"model_uri":"kask:dynamics/bc_optimization@v1","initial_state":{"chem:brix_percent":8,"chem:ph_value":6,"bio:bc_yield_g_per_l":0,"bio:bc_quality_index":1},"process_context":{"temperature_c":30,"agitation_rpm":160,"do_saturation_pct":10,"carbon_source":"glucose"},"horizon":{"kind":"fixed","days":14},"sample_cadence":{"hours":12}}' \
  https://agent-bestiary.world/api/simops/dynamics
```

Agitated run should show higher `bio:bc_yield_g_per_l` final value and lower `bio:bc_quality_index` than static.

---

## 4. `GET /api/simops/dynamics/models` — Model Catalogue

Returns the manifest for every registered dynamics model. Use for:
- A model-picker dropdown in the Digital Twin panel
- Informing the `simops_dynamics_runner` agent which model covers a given sensor set

```bash
curl -H "Authorization: Bearer $TOKEN" \
  https://agent-bestiary.world/api/simops/dynamics/models
```

Returns an array of `ModelManifest` objects, each with:
- `uri` — the `model_uri` to pass in projection requests
- `applies_to_set` — property URIs this model evolves
- `state_schema` — labels, units, typical ranges per state variable
- `params_schema` — overrideable parameters with defaults
- `context_schema` — temperature, agitation, etc.

---

## 5. `simops_dynamics_runner` Agent

Registered as `agent_id: "simops_dynamics_runner"`. Returned by `listWorkspaceAgents`. The kask shim (`invokeDynamicsRunner`) checks for this agent — when found, it calls it instead of the browser-side provider. **No kask code change needed.**

The agent's only job: receive the projection request JSON, call `apply_dynamics_model` skill, return a fenced JSON block matching the `SkillOutput` shape. No prose outside the JSON — the shim parses mechanically.

**Acceptance criterion for the kask side:** run `scripts/test-dynamics-runner-shim.js` with `KASK_TEST_ABW_URL=https://agent-bestiary.world`. Expect `[runProjection] dynamics-runner source=abw_agent` in the console instead of `source=browser_fallback`.

---

## 6. Full Smoke Test Sequence

Run these in order after Railway deploys. All should return 200.

```bash
TOKEN="your_jwt_here"
BASE="https://agent-bestiary.world"

# 1. Existing integration unchanged
curl $BASE/api/apps/kask_simops
curl -H "Authorization: Bearer $TOKEN" $BASE/api/auth/me

# 2. Spawn still works
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"name":"Smoke test 2026-05-25"}' \
  $BASE/api/apps/kask_simops/workspaces

# 3. Distributional projection
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"model":{"kind":"simops_cascade","config":{"name":"t","stages":[{"id":"s","efficiency":0.85,"carbon_intensity":0.1,"input":{"name":"i","unit":"kg"},"output":{"name":"o","unit":"kg"},"opex_per_input_unit":1.0}]}},"sweep":{"kind":"monte_carlo","variables":[{"path":"/stages/0/efficiency","distribution":{"type":"normal","mean":0.85,"std":0.03}}]},"n_runs":20,"seed":1}' \
  $BASE/api/simops/project

# 4. ODE dynamics — kombucha
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"model_uri":"kask:dynamics/kombucha_fermentation@v1","initial_state":{"chem:brix_percent":10,"chem:ph_value":5},"process_context":{"temperature_c":26},"horizon":{"kind":"fixed","days":7},"sample_cadence":{"hours":12}}' \
  $BASE/api/simops/dynamics

# 5. Model catalogue
curl -H "Authorization: Bearer $TOKEN" $BASE/api/simops/dynamics/models

# 6. dynamics_runner agent registered
curl -H "Authorization: Bearer $TOKEN" \
  "$BASE/api/workspaces/<workspace_id_from_step_2>/agents" | grep dynamics_runner
```

Tests 3–5 should all return data shapes matching §2, §3, §4 above. Test 6 should show `simops_dynamics_runner` in the agents list.

---

## 7. What Is Still Unchanged

- Workspace spawn / budget / wallet API
- Auth / OAuth flow  
- `@simops_advisor`, `@simops_cascade`, `@simops_narrator` agents
- `POST /api/simops/cascade` (single deterministic cascade — still works)
- ProcessConfig schema (`kask-simops/2`)
- SOSA observation writes
- All other ABW API endpoints

If anything in the pre-existing integration is broken, it is not from this session. Last known-good commit before this session: `19762f3`.

---

## 8. What's Still Deferred

| Capability | Why deferred | When |
|---|---|---|
| BayesOps base rate fitting | HMC sampler non-trivial; Phase 1 spec written | After Phase 1 ships — no kask action needed |
| `project_timeseries` (sweep over timesteps) | Superseded by the ODE dynamics engine for the immediate use cases | If a use case arises that needs it |
| Calibration advisor agent | Spec 28 Phase 2 | Separate handoff |
| `from_typical_range` sweep | Requires field annotation convention agreement | Open question §5 in projections spec |
