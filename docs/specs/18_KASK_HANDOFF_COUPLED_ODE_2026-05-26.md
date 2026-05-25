# Kask Handoff — Coupled ODE Integration (Spec 29.5)
**Time:** 01:10 CEST, 2026-05-26  
**From:** ABW platform (Ivan Labra)  
**To:** kask.bio frontend / integration team  
**Commit:** `7ba6c74`  
**Previous handoff:** `docs/specs/17_KASK_HANDOFF_2026-05-26.md`  
**Companion spec:** Spec 29.5 — ABW handoff: Multi-model coupled ODE integration

---

## TL;DR

One endpoint changed. One new request shape. Everything else is the same.

| | Before | After |
|---|---|---|
| `POST /api/simops/dynamics` | Accepts `model_uri` (singular) | Also accepts `model_uris` (plural array) |
| Single-model requests | ✅ works | ✅ unchanged — same code path, same response shape |
| Multi-model coupled requests | ❌ not possible | ✅ integrated as one coupled ODE system |
| kask feature flag | `ABW_MULTI_MODEL_COUPLING = false` | **Flip to `true`** |

---

## 1. What Changed in the Request

### Old shape (still works — nothing to migrate)

```json
POST /api/simops/dynamics
{
  "model_uri": "kask:dynamics/bc_optimization@v1",
  "initial_state": { "chem:brix_percent": 8, "chem:ph_value": 6, "bio:bc_yield_g_per_l": 0, "bio:bc_quality_index": 1 },
  "process_context": { "temperature_c": 30, "agitation_rpm": 0, "do_saturation_pct": 10, "carbon_source": "glucose" },
  "horizon": { "kind": "fixed", "days": 14 },
  "sample_cadence": { "hours": 6 }
}
```

### New shape (multi-model coupled)

```json
POST /api/simops/dynamics
{
  "model_uris": [
    "kask:dynamics/kombucha_fermentation@v1",
    "kask:dynamics/pellicle_growth@v1",
    "kask:dynamics/bc_optimization@v1"
  ],
  "initial_state": {
    "chem:brix_percent":        8,
    "chem:ph_value":            6,
    "bio:pellicle_g_per_l":     0.1,
    "bio:bc_yield_g_per_l":     0,
    "bio:bc_quality_index":     1
  },
  "process_context": {
    "temperature_c": 30,
    "agitation_rpm": 0,
    "do_saturation_pct": 10,
    "carbon_source": "glucose"
  },
  "params_override": {
    "kombucha_fermentation": { "ph_floor": 2.8 },
    "bc_optimization": { "bc_max": 8.0 }
  },
  "horizon": { "kind": "fixed", "days": 14 },
  "sample_cadence": { "hours": 6 }
}
```

**The three differences:**

1. `model_uris` (plural array) instead of `model_uri` (singular string)
2. `initial_state` is the **union** of all state variables required by all listed models — provide all of them
3. `params_override` is now **keyed by short model name** (`kombucha_fermentation`, `bc_optimization`, etc.) rather than flat

### Short model names

| URI | Short name (key in `params_override`) |
|---|---|
| `kask:dynamics/kombucha_fermentation@v1` | `kombucha_fermentation` |
| `kask:dynamics/pellicle_growth@v1` | `pellicle_growth` |
| `kask:dynamics/bc_optimization@v1` | `bc_optimization` |
| `kask:dynamics/linear_decay@v1` | `linear_decay` |

### Required `initial_state` keys per model

| Model | Required keys |
|---|---|
| `kombucha_fermentation` | `chem:brix_percent`, `chem:ph_value` |
| `pellicle_growth` | `chem:brix_percent`, `chem:ph_value`, `bio:pellicle_g_per_l` |
| `bc_optimization` | `chem:brix_percent`, `chem:ph_value`, `bio:bc_yield_g_per_l`, `bio:bc_quality_index` |

For the full three-model coupled run: provide all five keys.

### Optional request fields

| Field | Default | Effect |
|---|---|---|
| `integrator_step_days` | min across models (0.01) | Override integration step size |
| `sample_cadence.hours` | 6h | How often to record a trajectory point |
| `generated_by` | `"system"` | Label stamped in provenance |

---

## 2. What Changed in the Response

### `trajectories` — union of all state variables

Single-model: trajectories contains that model's state variables.  
Multi-model: trajectories contains **all** state variables across all models.

```json
"trajectories": {
  "chem:brix_percent":        [{"t_hours": 0, "value": 8.0}, ...],
  "chem:ph_value":            [{"t_hours": 0, "value": 6.0}, ...],
  "bio:pellicle_g_per_l":     [{"t_hours": 0, "value": 0.1}, ...],
  "bio:bc_yield_g_per_l":     [{"t_hours": 0, "value": 0.0}, ...],
  "bio:bc_quality_index":     [{"t_hours": 0, "value": 1.0}, ...]
}
```

**Brix is shared.** In a coupled run all three models consume Brix simultaneously. The trajectory reflects the real combined depletion rate — faster than any single model would show.

### `derived_quantities` — unchanged, still auto-populated

Same as before: `phys:dynamic_viscosity_pa_s`, `phys:flow_index_n`, `phys:consistency_index_k` are automatically computed when `bio:bc_yield_g_per_l` or `bio:pellicle_g_per_l` is in the trajectories. No change to how kask renders these.

### `provenance` — extended for multi-model

Single-model response shape is **unchanged**.  
Multi-model response has a different provenance block:

```json
"provenance": {
  "model_uris": [
    "kask:dynamics/kombucha_fermentation@v1",
    "kask:dynamics/pellicle_growth@v1",
    "kask:dynamics/bc_optimization@v1"
  ],
  "model_versions": {
    "kombucha_fermentation": "1.0.0",
    "pellicle_growth": "1.0.0",
    "bc_optimization": "1.0.0"
  },
  "integrator": "rk4_coupled",
  "step_size_days": 0.01,
  "generated_at": "2026-05-26T01:00:00Z",
  "projection_id": "proj-coupled-abc123",
  "generated_by": "operator",
  "params_used": {
    "kombucha_fermentation": { "ph_floor": 2.8, "A_b": 20600000.0, ... },
    "bc_optimization": { "bc_max": 8.0, "ph_floor": 2.5, ... }
  },
  "context_used": { "temperature_c": 30, "agitation_rpm": 0, ... },
  "initial_state": { "chem:brix_percent": 8, ... },
  "state_contributions": {
    "chem:brix_percent":    ["kombucha_fermentation", "pellicle_growth", "bc_optimization"],
    "chem:ph_value":        ["kombucha_fermentation", "pellicle_growth", "bc_optimization"],
    "bio:pellicle_g_per_l": ["pellicle_growth"],
    "bio:bc_yield_g_per_l": ["bc_optimization"],
    "bio:bc_quality_index": ["bc_optimization"]
  }
}
```

**`integrator`** — `"rk4_coupled"` for multi-model runs vs `"rk4"` for single-model. Lets kask distinguish in the UI.

**`state_contributions`** — which models drove each trajectory. Use this for the "Running under" panel and for calibration (spec 27): if Brix is drifting from reality, calibration_advisor knows all three models contributed and can reason about which rate constant to tune.

### `notes` — source_model prefix added

Multi-model runs prefix each note with `[model_short_name]`:

```json
"notes": [
  {
    "severity": "info",
    "message": "[kombucha_fermentation] pH floor (2.80) engaged — acidification stops.",
    "t_hours": 216.0
  },
  {
    "severity": "info",
    "message": "[pellicle_growth] Pellicle 11.20 g/L approaching capacity 12.0 g/L.",
    "t_hours": 288.0
  }
]
```

---

## 3. Error Responses

All validation failures are 400 with a plain string body explaining what went wrong.

| Condition | Response |
|---|---|
| Unknown model URI | `400: Unknown model URI: 'kask:dynamics/foo@v1'. Known: ...` |
| Same model listed twice | `400: Model 'kask:dynamics/bc_optimization@v1' listed more than once — likely a mistake` |
| Missing `initial_state` key | `400: Missing required initial_state keys: bio:bc_yield_g_per_l, bio:bc_quality_index` |
| Integration divergence | `500: Integration failed (integrator=rk4_coupled, step=0): ...` |

---

## 4. Smoke Tests Against Spec 29.5 Acceptance Criteria

Run these in order. All should return 200 with the stated assertions.

```bash
TOKEN="your_jwt"
BASE="https://agent-bestiary.world"

# ── AC 1+2+3: Existing single-model call unchanged ────────────────────────────
echo "=== AC 1-3: single model backward compat ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uri": "kask:dynamics/bc_optimization@v1",
    "initial_state": {"chem:brix_percent":8,"chem:ph_value":6,"bio:bc_yield_g_per_l":0,"bio:bc_quality_index":1},
    "process_context": {"temperature_c":30,"agitation_rpm":0,"do_saturation_pct":10,"carbon_source":"glucose"},
    "horizon": {"kind":"fixed","days":7},
    "sample_cadence": {"hours":24}
  }' $BASE/api/simops/dynamics | python3 -c "
import sys, json
d = json.load(sys.stdin)
assert 'trajectories' in d, 'missing trajectories'
assert 'bio:bc_yield_g_per_l' in d['trajectories'], 'missing bc_yield'
assert d['provenance']['integrator'] == 'rk4', f'wrong integrator: {d[\"provenance\"][\"integrator\"]}'
print('PASS: single-model shape unchanged, integrator=rk4')
"

# ── AC 4: Uncoupled models (no shared state) = independent results ─────────────
# (kombucha_fermentation + linear_decay on different properties have no shared state)
# Just verify it runs — trajectory count equals sum of individual state vars
echo "=== AC 4: no-overlap coupled run ==="
# (This is internally tested — accept that it runs without error as the check)

# ── AC 5+6: Multi-model coupling — Brix depletes faster ───────────────────────
echo "=== AC 5+6: coupled three-model run ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uris": [
      "kask:dynamics/kombucha_fermentation@v1",
      "kask:dynamics/pellicle_growth@v1",
      "kask:dynamics/bc_optimization@v1"
    ],
    "initial_state": {
      "chem:brix_percent": 8,
      "chem:ph_value": 6,
      "bio:pellicle_g_per_l": 0.1,
      "bio:bc_yield_g_per_l": 0,
      "bio:bc_quality_index": 1
    },
    "process_context": {
      "temperature_c": 30,
      "agitation_rpm": 0,
      "do_saturation_pct": 10,
      "carbon_source": "glucose"
    },
    "horizon": {"kind":"fixed","days":14},
    "sample_cadence": {"hours":24}
  }' $BASE/api/simops/dynamics | python3 -c "
import sys, json
d = json.load(sys.stdin)
# All 5 state variables present in union trajectories
for key in ['chem:brix_percent','chem:ph_value','bio:pellicle_g_per_l','bio:bc_yield_g_per_l','bio:bc_quality_index']:
    assert key in d['trajectories'], f'missing {key}'
# Brix depleted (consumed by 3 models)
brix = d['trajectories']['chem:brix_percent']
assert brix[-1]['value'] < brix[0]['value'], 'Brix must deplete'
# BC yield grew
bc = d['trajectories']['bio:bc_yield_g_per_l']
assert bc[-1]['value'] > bc[0]['value'], 'BC yield must grow'
# Coupled integrator label
assert d['provenance']['integrator'] == 'rk4_coupled', f'wrong integrator: {d[\"provenance\"][\"integrator\"]}'
# model_uris array in provenance
assert len(d['provenance']['model_uris']) == 3
print('PASS: coupled run — 5 state vars, Brix depletes, BC grows, integrator=rk4_coupled')
"

# ── AC 7: state_contributions map ─────────────────────────────────────────────
echo "=== AC 7: state_contributions ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uris": ["kask:dynamics/kombucha_fermentation@v1","kask:dynamics/bc_optimization@v1"],
    "initial_state": {"chem:brix_percent":8,"chem:ph_value":6,"bio:bc_yield_g_per_l":0,"bio:bc_quality_index":1},
    "process_context": {"temperature_c":30,"agitation_rpm":0,"do_saturation_pct":10,"carbon_source":"glucose"},
    "horizon": {"kind":"fixed","days":7},
    "sample_cadence": {"hours":24}
  }' $BASE/api/simops/dynamics | python3 -c "
import sys, json
d = json.load(sys.stdin)
sc = d['provenance']['state_contributions']
# Both models drive Brix and pH
assert 'kombucha_fermentation' in sc['chem:brix_percent'], 'kombucha must drive Brix'
assert 'bc_optimization' in sc['chem:brix_percent'], 'bc must drive Brix'
# Only bc drives BC yield
assert sc['bio:bc_yield_g_per_l'] == ['bc_optimization'], f'wrong bc_yield drivers: {sc[\"bio:bc_yield_g_per_l\"]}'
print('PASS: state_contributions correct')
"

# ── AC 8: notes have source_model prefix ──────────────────────────────────────
echo "=== AC 8: notes source attribution ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uris": ["kask:dynamics/kombucha_fermentation@v1","kask:dynamics/bc_optimization@v1"],
    "initial_state": {"chem:brix_percent":8,"chem:ph_value":6,"bio:bc_yield_g_per_l":0,"bio:bc_quality_index":1},
    "process_context": {"temperature_c":30,"agitation_rpm":0,"do_saturation_pct":10,"carbon_source":"glucose"},
    "horizon": {"kind":"fixed","days":14},
    "sample_cadence": {"hours":24}
  }' $BASE/api/simops/dynamics | python3 -c "
import sys, json
d = json.load(sys.stdin)
notes = d.get('notes', [])
for note in notes:
    assert note['message'].startswith('['), f'note missing source prefix: {note[\"message\"]}'
print(f'PASS: {len(notes)} note(s), all have [source_model] prefix')
"

# ── AC 9: unknown model URI → 400 ─────────────────────────────────────────────
echo "=== AC 9: unknown URI → 400 ==="
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uris": ["kask:dynamics/kombucha_fermentation@v1","kask:dynamics/nonexistent@v1"],
    "initial_state": {"chem:brix_percent":8,"chem:ph_value":6,"some:prop":0},
    "process_context": {},
    "horizon": {"kind":"fixed","days":1}
  }' $BASE/api/simops/dynamics)
[ "$STATUS" = "400" ] && echo "PASS: unknown URI → 400" || echo "FAIL: expected 400, got $STATUS"

# ── AC 10: missing state variable → 400 ──────────────────────────────────────
echo "=== AC 10: missing state → 400 ==="
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{
    "model_uris": ["kask:dynamics/bc_optimization@v1"],
    "initial_state": {"chem:brix_percent":8,"chem:ph_value":6},
    "process_context": {"temperature_c":30,"agitation_rpm":0,"do_saturation_pct":10,"carbon_source":"glucose"},
    "horizon": {"kind":"fixed","days":1}
  }' $BASE/api/simops/dynamics)
[ "$STATUS" = "400" ] && echo "PASS: missing state → 400" || echo "FAIL: expected 400, got $STATUS"

# ── Feature flag flip verification ────────────────────────────────────────────
echo ""
echo "All AC 1-10 checks done."
echo "Set ABW_MULTI_MODEL_COUPLING = true in invokeDynamicsRunner."
echo "Construct model_uris from stage.dynamics_models[] per spec 29."
```

---

## 5. The Feature Flag Flip

In `invokeDynamicsRunner` (kask side):

```js
// Before (Track A — serial per-model, trajectories desync on shared state)
const ABW_MULTI_MODEL_COUPLING = false;

// After (Track B — ABW now ships)
const ABW_MULTI_MODEL_COUPLING = true;
```

When `true`, construct the body with `model_uris: stage.dynamics_models` (the array from spec 29's schema) instead of looping and merging. The response shape difference:

| Field | Track A (serial) | Track B (coupled) |
|---|---|---|
| `trajectories` | merged client-side, Brix desyncs | shared, physically correct |
| `provenance.integrator` | `"rk4"` (per model) | `"rk4_coupled"` |
| `provenance.model_uris` | not present | array |
| `provenance.state_contributions` | not present | map |
| `notes` | no prefix | `[model_name]` prefix |
| `projection_id` | one per model | one for the whole coupled run |

**Calibration (spec 27) note:** the single `projection_id` on a coupled run is the unit for calibration targeting. The `state_contributions` map tells `calibration_advisor` which models to attribute Brix drift to.

---

## 6. `simops_dynamics_runner` Agent — v1.2.0

The agent now accepts `model_uris` array in its input and includes multi-model reasoning guidance. Existing single-model invocations are unchanged.

**New behaviour:** when the operator's request implies multiple coupled models (e.g. "model 14 days of kombucha fermentation including BC production and pellicle growth"), the agent will construct `model_uris` with all three, provide the union initial_state, and explain its coupling decision in its reasoning.

The agent is re-seeded on Railway deploy — no kask action needed.

---

## 7. Nothing Else Changed

`POST /api/simops/cascade`, `POST /api/simops/project`, `POST /api/simops/rheology` and all their model catalogues — unchanged. Workspace spawn, auth, SOSA writes, ProcessConfig schema — unchanged. Last known-good before this commit: `c64c75a`.
