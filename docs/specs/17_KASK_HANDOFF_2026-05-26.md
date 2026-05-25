# Kask Handoff — 2026-05-26
**Time:** 00:02 CEST  
**From:** ABW platform session (Ivan Labra)  
**To:** kask.bio frontend / integration team  
**Commits since last handoff:** `6177306`, `2d6dbcd`, `752475e`, `809ea4b`, `cae7aec`  
**Previous handoff:** `docs/specs/16_KASK_HANDOFF_2026-05-25.md`

---

## TL;DR — What Changed Since Yesterday

| Item | Change | Action for kask |
|---|---|---|
| `simops_dynamics_runner` admin views | Fixed — was missing owner | None |
| `simops_dynamics_runner` in workspace | Fixed — was not in `auto_hire` | Re-spawn or manually hire on existing workspaces |
| `POST /api/simops/rheology` | **New endpoint** | Wire to pump-sizing / viscosity probe UI |
| `GET /api/simops/rheology/models` | **New endpoint** | Feed model-picker |
| `POST /api/simops/dynamics` response | **New field: `derived_quantities`** | Render viscosity tracks alongside primary trajectories |
| `simops_dynamics_runner` agent | v1.1.0 — now passes through `derived_quantities` | None — auto |

Everything from the previous handoff (`/api/simops/project`, `/api/simops/dynamics`, `simops_dynamics_runner` shim) is unchanged and still live.

---

## 1. Two Bugs Fixed — No kask Code Change Needed

### 1a. `simops_dynamics_runner` had no owner → no admin views

**What was wrong:** every curated agent seeded after an early migration landed with `user_id = NULL` in the DB. Admin views (Eval / Intelligence / Manage tabs on ABW) require the calling user to match `user_id`. With NULL, tabs never appeared.

**Fixed in:** migration `129` (runs on next Railway deploy). Also permanently fixed the seeder — curated agents now get the admin user's id on every reseed, so this can't recur for future agents.

**kask action:** none.

### 1b. `simops_dynamics_runner` not appearing in `listWorkspaceAgents`

**What was wrong:** the agent was not in the `auto_hire` list in `apps/kask_simops.json`. Auto-hire only fires at workspace spawn time — the agent never got added to existing or new workspaces.

**Fixed in:** `apps/kask_simops.json` now includes `"simops_dynamics_runner"` in `auto_hire`.

**kask action:**
- **New workspace spawns** after Railway deploys: agent is included automatically.
- **Existing workspaces** spawned before this deploy: hire it once manually:

```bash
POST /api/workspaces/<workspace_id>/agents
Authorization: Bearer $TOKEN
Content-Type: application/json

{ "agent_name": "simops_dynamics_runner" }
```

After that, `listWorkspaceAgents` will return it and the shim swap from `source=browser_fallback` to `source=abw_agent` will work.

---

## 2. New: `POST /api/simops/rheology`

Instantaneous fluid property calculator. No time integration, no LLM. Given operating conditions, returns viscosity and flow regime immediately. Powers pump sizing, inline viscosity probes, and any UI element that needs a single-point answer rather than a trajectory.

### Request

```json
{
  "model_uri": "kask:rheology/algae_viscosity@v1",
  "temperature_c": 30.0,
  "shear_rate_per_s": 160.0,
  "volume_fraction": 0.15,
  "params_override": {}
}
```

`model_uri` is optional — defaults to `"kask:rheology/algae_viscosity@v1"` (the only model currently registered).

**`shear_rate_per_s` guidance:**

| Context | Typical range |
|---|---|
| Sedimentation / quiescent | 0.001 – 0.1 |
| Static fermentation vessel | 0.01 – 1 |
| Stirred tank (160 rpm) | ~3200 (N_imp=20 × rpm) |
| Pump inlet | 100 – 1000 |
| Pipe flow | 10 – 10000 |

**`volume_fraction`:** algae or BC/pellicle concentration as a fraction, not a percentage. `0.15` = 15%.

### Response

```json
{
  "viscosity_pa_s": 0.00124,
  "flow_index_n": 0.88,
  "consistency_index_k": 0.000235,
  "regime": "shear_thinning",
  "kinematic_mm2_per_s": 1.18
}
```

**`regime`:** `"newtonian"` (n ≈ 1.0) | `"shear_thinning"` (n < 1.0, typical for algae) | `"shear_thickening"` (n > 1.0, rare).

**`kinematic_mm2_per_s`:** mm²/s = cSt. Useful for pump selection tables that work in kinematic rather than dynamic viscosity.

**`params_override` fields:**

| Key | Default | Effect |
|---|---|---|
| `k0` | 0.001 | Consistency index at 25°C (Pa·sⁿ) |
| `ea` | 15000 | Arrhenius activation energy (J/mol) |
| `t_ref_k` | 298.15 | Reference temperature (K) — K=k0 at this T |
| `c_n` | 0.8 | Concentration sensitivity of flow index |
| `n_min` | 0.1 | Floor on n (prevents unphysical values at high φ) |
| `density_kg_m3` | 1050 | Suspension density — for kinematic viscosity output |

### Smoke test

```bash
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "temperature_c": 26.0,
    "shear_rate_per_s": 100.0,
    "volume_fraction": 0.15
  }' \
  https://agent-bestiary.world/api/simops/rheology
```

Expect 200 with `regime: "shear_thinning"` and `viscosity_pa_s` in the range 5e-4 – 5e-3.

---

## 3. New: `GET /api/simops/rheology/models`

Returns the manifest for every registered rheology model. Currently one model.

```bash
curl -H "Authorization: Bearer $TOKEN" \
  https://agent-bestiary.world/api/simops/rheology/models
```

Returns an array of `RheologyManifest` objects, each with `uri`, `name`, `description`, `input_schema` (all parameters with defaults and typical ranges), `output_dimensions`, `citations`.

---

## 4. Updated: `POST /api/simops/dynamics` — New `derived_quantities` Field

The dynamics endpoint response now includes a `derived_quantities` array for the `bc_optimization` and `pellicle_growth` models. Nothing in the request changes. No extra calls needed.

### What's new in the response

```json
{
  "trajectories": {
    "chem:brix_percent":      [{"t_hours": 0, "value": 8.0}, ...],
    "chem:ph_value":          [...],
    "bio:bc_yield_g_per_l":   [...],
    "bio:bc_quality_index":   [...]
  },
  "derived_quantities": [
    {
      "property_uri": "phys:dynamic_viscosity_pa_s",
      "label": "Dynamic viscosity",
      "units": "Pa·s",
      "points": [
        {"t_hours": 0,   "value": 9.5e-4},
        {"t_hours": 24,  "value": 9.7e-4},
        {"t_hours": 168, "value": 1.8e-3}
      ],
      "source_model_uri": "kask:rheology/algae_viscosity@v1"
    },
    {
      "property_uri": "phys:flow_index_n",
      "label": "Flow behaviour index (n)",
      "units": "dimensionless",
      "points": [...]
    },
    {
      "property_uri": "phys:consistency_index_k",
      "label": "Consistency index K(T)",
      "units": "Pa·sⁿ",
      "points": [...]
    }
  ],
  "provenance": {...},
  "converged": false,
  "notes": [...]
}
```

**`derived_quantities`** is always present in the response — empty array `[]` for models with no compatible state (kombucha, linear_decay), populated for bc_optimization and pellicle_growth.

**Time axis:** `derived_quantities[*].points` has the **identical `t_hours` values** as the primary trajectories. You can zip them directly onto the same x-axis as `bio:bc_yield_g_per_l` for a combined chart.

**What the viscosity curve tells you:**
- Starts near water (9e-4 Pa·s at φ≈0) 
- Rises as BC accumulates (more solid → higher φ → higher viscosity)
- At ~0.01 Pa·s (10× water) pumping becomes non-trivial — worth surfacing to the operator
- `flow_index_n` dropping toward 0.7 signals strong shear-thinning — mixing zones possible

**Controlling shear rate for derived quantities:**

By default the derivation uses:
- Agitated culture: `γ̇ = 20 × agitation_rpm` (N_imp=20, Rushton turbine)  
- Static culture: `γ̇ = 0.05 s⁻¹` (natural convection)

Override via `params_override`:
```json
{
  "params_override": {
    "rheology_n_imp": 15.0,
    "rheology_static_shear": 0.1
  }
}
```

---

## 5. Updated: `simops_dynamics_runner` Agent (v1.1.0)

The agent's output contract was stale — it was instructed to reply with only `trajectories`, `provenance`, `converged`, `notes`, causing it to silently drop `derived_quantities` even though the skill was computing them.

**Fixed in v1.1.0:** prompt now explicitly includes `derived_quantities` in the output shape and has a hard rule: never omit it. The agent also now handles explicit rheology queries (`apply_rheology_model` skill) in addition to ODE projections.

No kask action needed — the agent is re-seeded on Railway deploy.

---

## 6. Full Smoke Test Sequence

```bash
TOKEN="your_jwt"
BASE="https://agent-bestiary.world"

# 1. Viscosity single point
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"temperature_c":26,"shear_rate_per_s":100,"volume_fraction":0.15}' \
  $BASE/api/simops/rheology
# Expect: viscosity_pa_s ~5e-4 to 2e-3, regime "shear_thinning"

# 2. Rheology model catalogue
curl -H "Authorization: Bearer $TOKEN" $BASE/api/simops/rheology/models
# Expect: array with "kask:rheology/algae_viscosity@v1"

# 3. BC optimization with derived viscosity
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uri": "kask:dynamics/bc_optimization@v1",
    "initial_state": {
      "chem:brix_percent": 8,
      "chem:ph_value": 6,
      "bio:bc_yield_g_per_l": 0,
      "bio:bc_quality_index": 1
    },
    "process_context": {
      "temperature_c": 30,
      "agitation_rpm": 0,
      "do_saturation_pct": 10,
      "carbon_source": "glucose"
    },
    "horizon": {"kind": "fixed", "days": 14},
    "sample_cadence": {"hours": 24}
  }' \
  $BASE/api/simops/dynamics
# Expect: response has derived_quantities array with 3 entries
# derived_quantities[0].property_uri == "phys:dynamic_viscosity_pa_s"
# derived_quantities[*].points length == trajectories["bio:bc_yield_g_per_l"] length

# 4. Kombucha — derived_quantities is empty array
curl -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uri": "kask:dynamics/kombucha_fermentation@v1",
    "initial_state": {"chem:brix_percent": 10, "chem:ph_value": 5},
    "process_context": {"temperature_c": 26},
    "horizon": {"kind": "fixed", "days": 7},
    "sample_cadence": {"hours": 24}
  }' \
  $BASE/api/simops/dynamics
# Expect: derived_quantities is absent or empty array — no BC/pellicle state

# 5. dynamics_runner in workspace (replace with real workspace_id)
curl -H "Authorization: Bearer $TOKEN" \
  $BASE/api/workspaces/<workspace_id>/agents | python3 -c \
  "import sys,json; agents=json.load(sys.stdin); print([a.get('agent_name') for a in agents.get('agents',[])])"
# Expect: 'simops_dynamics_runner' in list
# If not: POST /api/workspaces/<workspace_id>/agents  {"agent_name":"simops_dynamics_runner"}
```

---

## 7. Complete Endpoint Reference (All SimOps Endpoints)

| Endpoint | Method | Purpose | Auth |
|---|---|---|---|
| `/api/simops/cascade` | POST | Single deterministic cascade (forward/backward) | Bearer |
| `/api/simops/project` | POST | N-run MC distribution over cascade inputs | Bearer |
| `/api/simops/dynamics` | POST | ODE time-series projection + derived rheology | Bearer |
| `/api/simops/dynamics/models` | GET | ODE model catalogue | Bearer |
| `/api/simops/rheology` | POST | Single-point viscosity calculation | Bearer |
| `/api/simops/rheology/models` | GET | Rheology model catalogue | Bearer |

All are CPU-only, no credits charged, no LLM in path.

---

## 8. Nothing Else Changed

Workspace spawn, auth, agent invocation, SOSA writes, ProcessConfig schema, budget model, all other ABW endpoints — unchanged. Last known-good before this session: `d82f00f`.
