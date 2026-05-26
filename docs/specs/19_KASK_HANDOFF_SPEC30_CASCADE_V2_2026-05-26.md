# Kask Handoff — Spec 30 / 30.5: Multi-input Cascade Engine v2
**Time:** 20:05 CEST, 2026-05-26  
**From:** ABW platform (Ivan Labra)  
**To:** kask.bio frontend / integration team  
**Commit:** `1fd620d`  
**Companion spec:** 30_5_ABW_HANDOFF.md (round-trip complete)  
**Previous handoff:** `docs/specs/18_KASK_HANDOFF_COUPLED_ODE_2026-05-26.md`

---

## O1 / O2 / O3 — Resolved

All three open items from the spec round-trip are now closed:

| Item | Resolution |
|---|---|
| **O1 — ETA** | Commit `1fd620d`, deployed this Railway cycle |
| **O2 — field name** | `schema_version: 2` numeric field confirmed |
| **O3 — canonical numbers** | Embedded in `bottle_conditioned_saison_v2.toml` comments — see §5 |

---

## TL;DR — What Changed

One endpoint changed shape. Old v1 requests still work (with a deprecation path). New v2 requests use `inputs[]`/`outputs[]` and get fully-resolved mass-balance + economics back.

| | Before | After |
|---|---|---|
| `POST /api/simops/cascade` | Accepts `{ process: {stages:[{input,output,bom}]}, direction, quantity }` | Also accepts v2: `{ process: {schema_version:2, throughput, stages:[{inputs[],outputs[]}]}, direction, scale }` |
| v1 with explicit `schema_version: 1` | Accepted | **400** — spec 30 migration message |
| v1 without `schema_version` (singular `input` field) | Works as before | Still works (legacy path unchanged) |
| v2 `direction: backward` | N/A | **400** — deferred to spec 30.6 |
| Response shape | `{stages:[{input_quantity, output_quantity, opex_usd, ...}], system_ner, ...}` | `{stages:[{inputs_resolved[], outputs_resolved[], mass_balance, economics, cascade_notes[]}], process_totals, provenance}` |

---

## 1. Request Shape (v2)

```json
POST /api/simops/cascade
{
  "process": {
    "schema_version": 2,
    "name": "bottle_conditioned_saison",
    "throughput": {
      "basis_stage": "primary_fermentation",
      "basis_input": "wort",
      "qty_per_run": 100,
      "qty_unit": "L",
      "runs_per_year": 26
    },
    "stages": [
      {
        "id": "primary_fermentation",
        "inputs": [
          { "name": "wort", "qty": 1, "qty_unit": "L", "per_basis": "principal",
            "role": "principal", "unit_cost": 0.80, "cost_unit": "eur_per_L",
            "density_kg_per_unit": 1.04 },
          { "name": "dry_yeast", "qty": 0.5, "qty_unit": "g",
            "per": 1, "per_unit": "L", "per_basis": "principal",
            "role": "catalyst", "unit_cost": 0.05, "cost_unit": "eur_per_g",
            "density_kg_per_unit": 0.001 }
        ],
        "outputs": [
          { "name": "green_beer", "role": "downstream_feed",
            "qty_unit": "L", "density_kg_per_unit": 1.01 },
          { "name": "trub", "role": "sidestream", "qty_per_input_kg": 0.02,
            "qty_unit": "kg", "capture_fraction": 0.0, "value_per_unit_usd": 0 }
        ],
        "efficiency": 0.97,
        "power_kwh_per_input_kg": 0.12,
        "labor_hours_per_input_kg": 0.001,
        "carbon_intensity": { "mode": "synthetic", "value": 0.05 }
      },
      {
        "id": "secondary_fermentation",
        "inputs": [
          { "name": "green_beer", "from_stage": "primary_fermentation", "role": "principal" },
          { "name": "priming_sugar", "qty": 3.5, "qty_unit": "g",
            "per": 1, "per_unit": "L", "per_basis": "principal",
            "role": "consumable", "unit_cost": 0.002, "cost_unit": "eur_per_g",
            "density_kg_per_unit": 0.0016 }
        ],
        "outputs": [
          { "name": "carbonated_beer", "role": "product",
            "qty_unit": "L", "density_kg_per_unit": 1.005,
            "value_per_unit_usd": 4.50 }
        ],
        "efficiency": 0.99
      }
    ]
  },
  "direction": "forward",
  "scale": { "kind": "from_throughput" }
}
```

### Key field reference

**`scale` kinds:**

| `kind` | Behaviour |
|---|---|
| `from_throughput` (default) | Uses `process.throughput.qty_per_run` as the basis-input absolute quantity |
| `explicit` | `{ kind: "explicit", stage_id, input_name, qty, qty_unit }` — override for any input on any stage |

**`role` on inputs:**

| role | Mass-balance | Cost counted |
|---|---|---|
| `principal` | ✅ include (default) | ✅ |
| `consumable` | ✅ include (default) | ✅ |
| `catalyst` | ❌ exclude (default) | ✅ |

Override with `mass_balance: "include"` or `"exclude"` to flip the default.

**`per_basis` on external inputs:**

| per_basis | Scaling rule |
|---|---|
| `principal` | `qty × (principal_qty_in_per_unit / per)` — scales with principal flow |
| `batch` | `qty × (basis_qty / throughput.qty_per_run)` — absolute per run |

**`per_unit` constraint:** must match one principal input's `qty_unit` on the same stage. Mismatch → 400.

**`role` on outputs:**

| role | Links downstream? | Value/cost |
|---|---|---|
| `downstream_feed` | ✅ via `from_stage` | — |
| `product` | ❌ | `value_per_unit_usd` → revenue |
| `sidestream` | ❌ | `capture_fraction × value_per_unit_usd` → credit |
| `waste` | ❌ | `disposal_cost_per_unit_usd` → cost |

**Residual rule:** exactly one `downstream_feed` per stage may omit `qty_per_input_kg` — it takes the residual mass. Two or more omitting it → 400 ambiguous residual.

---

## 2. Response Shape (v2)

```json
{
  "stages": [
    {
      "stage_id": "primary_fermentation",
      "inputs_resolved": [
        { "name": "wort", "qty": 100.0, "unit": "L", "kg": 104.0,
          "source": "external", "role": "principal",
          "mass_balance_contribution_kg": 104.0, "cost_eur": 80.0 },
        { "name": "dry_yeast", "qty": 50.0, "unit": "g", "kg": 0.05,
          "source": "external", "role": "catalyst",
          "mass_balance_contribution_kg": 0.0, "cost_eur": 2.50,
          "mass_balance_excluded_reason": "role=catalyst (catalysts default to mass_balance=exclude)" }
      ],
      "outputs_resolved": [
        { "name": "green_beer", "qty": 97.82, "unit": "L", "kg": 98.80,
          "role": "downstream_feed", "qty_basis": "residual", "value_eur": null },
        { "name": "trub", "qty": 2.08, "unit": "kg", "kg": 2.08,
          "role": "sidestream", "qty_basis": "declared_yield:0.02",
          "capture_fraction": 0.0, "value_eur": 0.0 }
      ],
      "mass_balance": {
        "total_input_kg": 104.0,
        "total_mass_balance_input_kg": 104.0,
        "efficiency": 0.97,
        "total_output_kg": 100.88,
        "residual_assigned_to": "green_beer",
        "unaccounted_kg": 0.0
      },
      "economics": {
        "materials_eur_per_kg": 0.793,
        "upstream_cost_per_kg": 0.0,
        "energy_eur_per_kg": 0.014,
        "labor_eur_per_kg": 0.025,
        "carbon_eur_per_kg": 0.0013,
        "sidestream_credit_eur": 0.0,
        "waste_disposal_cost_eur": 0.0,
        "opex_per_kg_total_input": 0.834,
        "opex_per_unit_principal_input_display": {
          "value": 0.867, "unit": "eur_per_L_wort"
        }
      },
      "cascade_notes": [
        { "severity": "info", "kind": "catalyst_excluded_from_mass_balance",
          "input_name": "dry_yeast",
          "message": "dry_yeast (role=Catalyst) excluded from mass-balance; cost still counted." },
        { "severity": "info", "kind": "residual_assigned",
          "output_name": "green_beer",
          "message": "'green_beer' (downstream_feed) has no qty_per_input_kg; assigned residual 98.80kg." }
      ]
    },
    {
      "stage_id": "secondary_fermentation",
      "inputs_resolved": [
        { "name": "green_beer", "qty": 97.82, "unit": "L", "kg": 98.80,
          "source": "from_stage:primary_fermentation", "role": "principal",
          "mass_balance_contribution_kg": 98.80,
          "upstream_cost_carried_eur": 0.0 },
        { "name": "priming_sugar", "qty": 342.4, "unit": "g", "kg": 0.548,
          "source": "external", "role": "consumable",
          "mass_balance_contribution_kg": 0.548, "cost_eur": 0.68 }
      ],
      "outputs_resolved": [
        { "name": "carbonated_beer", "qty": 97.37, "unit": "L", "kg": 97.87,
          "role": "product", "qty_basis": "residual",
          "value_eur": 438.17 }
      ],
      "mass_balance": {
        "total_input_kg": 99.35,
        "total_mass_balance_input_kg": 99.35,
        "efficiency": 0.99,
        "total_output_kg": 98.36,
        "residual_assigned_to": "carbonated_beer",
        "unaccounted_kg": 0.0
      },
      "economics": { "..." : "..." },
      "cascade_notes": []
    }
  ],
  "process_totals": {
    "total_opex_per_run_eur": 91.0,
    "total_revenue_per_run_eur": 438.17,
    "total_sidestream_credit_eur": 0.0,
    "total_waste_disposal_eur": 0.0,
    "margin_per_run_eur": 347.17,
    "carbon_kg_co2_per_run": 5.2
  },
  "provenance": {
    "cascade_version": "2.0.0",
    "schema_version": 2,
    "computed_at": "2026-05-26T20:00:00Z"
  }
}
```

**Fields kask renders from:**

| kask surface | Response field |
|---|---|
| Resources sub-section inputs table | `stages[].inputs_resolved[]` |
| Resources sub-section outputs table | `stages[].outputs_resolved[]` |
| Mass balance summary line | `stages[].mass_balance` |
| Economics strip | `stages[].economics` |
| Cascade notes (ℹ warnings) | `stages[].cascade_notes[]` |
| Process-level KPIs | `process_totals` |

---

## 3. Validation Errors (400)

All validation returns 400 with a plain string body. Exact messages:

| Condition | Message |
|---|---|
| `schema_version != 2` | `ProcessConfig schema_version must be 2 (got: {n}). See kask spec 30.` |
| `direction: backward` on v2 | `backward cascade is not yet implemented for schema_version 2. See spec 30 §'Backward cascade direction deferred'. May land in spec 30.6.` |
| Stage has no inputs | `Stage '{id}' has no inputs. Every stage needs at least one input.` |
| Stage has no outputs | `Stage '{id}' has no outputs. Every stage needs at least one output.` |
| `from_stage` references unknown stage | `Input '{name}' on stage '{stage}' references from_stage='{target}' but no such stage exists.` |
| `from_stage` output name mismatch | `Input '{name}' on stage '{stage}' references from_stage='{target}' but '{target}' has no output named '{name}'. Did you mean: {list}?` |
| `from_stage` is a forward reference | `Input '{name}' on stage '{stage}' references from_stage='{target}' but '{target}' appears AFTER '{stage}' in the cascade. Upstream links only.` |
| Multiple residual `downstream_feed` | `Stage '{stage}' has multiple downstream_feed outputs ({list}) all omitting qty_per_input_kg; ambiguous residual.` |
| Declared outputs exceed mass-balance | `Stage '{stage}' declared outputs sum to {n}kg but mass-balance yields only {n}kg (efficiency={n}). Reduce yield ratios or check efficiency.` |
| No `downstream_feed` for referenced stage | `Stage '{stage}' is referenced by downstream stages but has no output with role=downstream_feed.` |
| `throughput.basis_stage` unknown | `throughput.basis_stage='{target}' but no such stage exists.` |
| `throughput.basis_input` unknown | `throughput.basis_input='{input}' but stage '{stage}' has no input named '{input}'.` |
| External input missing `unit_cost` | `External input '{name}' on stage '{stage}' has qty but no unit_cost. Either declare unit_cost or remove the input.` |
| `per_unit` no matching principal | `Consumable '{name}' on stage '{stage}' declares per_unit='{unit}' but no principal input on this stage has qty_unit='{unit}' (principals: {list}). Recipe ratios scale against a principal's native unit — they must match.` |
| Mixed-unit principals, ambiguous `per_unit` | `Stage '{stage}' has multiple principal inputs with different qty_units ({list}). Consumable '{name}' with per_unit='{unit}' is ambiguous — declare per_basis='batch' for absolute qty, or restructure principals to share one unit.` |

---

## 4. Cascade Notes (`cascade_notes[]`)

Notes are non-blocking. They surface decisions the engine made that the operator should see.

| `kind` | `severity` | When |
|---|---|---|
| `catalyst_excluded_from_mass_balance` | `info` | Input has `role=catalyst` — excluded from mass-balance pool by default |
| `density_missing_input_excluded` | `warn` | Input with `role∈{principal,consumable}` has no `density_kg_per_unit` and `qty_unit ≠ "kg"` — contributes 0 to mass-balance, cost still counted |
| `density_missing_output_unit_mismatch` | `warn` | Output has `qty_unit ≠ "kg"` and no `density_kg_per_unit` — qty reported in kg |
| `residual_assigned` | `info` | A `downstream_feed` output had no `qty_per_input_kg`; cascade filled it as the residual |
| `unaccounted_mass_treated_as_waste` | `warn` | Declared outputs sum to less than `total_output_kg` and no `downstream_feed` took the residual — implicit waste created |
| `low_efficiency_warning` | `info` | `efficiency < 0.5` — soft heads-up, not blocking |

Render these in the Resources sub-section with the ℹ/⚠ icons per spec 30. Each note carries `input_name` or `output_name` for pinning to the specific row.

---

## 5. Canonical Numerical Fixture (O3)

From `crates/simops/process/bottle_conditioned_saison_v2.toml`:

**Primary fermentation** (100 L wort basis):
- `total_input_kg`: 104.0 (100 L × 1.04 kg/L)
- `total_mass_balance_input_kg`: 104.0 (dry_yeast catalyst excluded)
- `total_output_kg`: 100.88 (104.0 × 0.97)
- trub declared: 104.0 × 0.02 = 2.08 kg
- green_beer residual: 100.88 − 2.08 = **98.80 kg → 97.82 L** (at 1.01 kg/L)

**Secondary fermentation**:
- green_beer from upstream: 97.82 L → 98.80 kg carried
- priming_sugar: 3.5 g/L × 97.82 L = 342.37 g × 0.0016 kg/g = **0.548 kg**
- `total_mass_balance_input_kg`: 98.80 + 0.548 = **99.35 kg**
- `total_output_kg`: 99.35 × 0.99 = **98.36 kg** → carbonated_beer residual

Use these numbers as your cross-stack validation targets in `scripts/test-multi-input-cascade.js`.

---

## 6. Smoke Tests

These are the spec 30.5 ACs verbatim — copy-paste runnable:

```bash
TOKEN="$ABW_TOKEN"
BASE="https://agent-bestiary.world"

# AC 1: v1 process rejected
echo "=== AC 1 ==="
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"stages":[{"id":"s1","input":{"name":"w","unit":"L"},"output":{"name":"o","unit":"L"},"efficiency":0.9}]},"direction":"forward","quantity":100}' \
  $BASE/api/simops/cascade)
[ "$STATUS" = "400" ] && echo "PASS: v1 → 400" || echo "FAIL: got $STATUS"

# AC 2: v2 simple cascade
echo "=== AC 2 ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"schema_version":2,"name":"t","throughput":{"basis_stage":"s1","basis_input":"w","qty_per_run":100,"qty_unit":"L","runs_per_year":10},"stages":[{"id":"s1","inputs":[{"name":"w","qty":1,"qty_unit":"L","per_basis":"principal","role":"principal","unit_cost":0.001,"cost_unit":"eur_per_L","density_kg_per_unit":1.0}],"outputs":[{"name":"o","role":"product","qty_unit":"L","density_kg_per_unit":1.0,"value_per_unit_usd":1.0}],"efficiency":0.9}]},"direction":"forward","scale":{"kind":"from_throughput"}}' \
  $BASE/api/simops/cascade | python3 -c "
import sys, json; d = json.load(sys.stdin)
assert d['stages'][0]['mass_balance']['total_input_kg'] == 100.0
assert d['stages'][0]['mass_balance']['total_output_kg'] == 90.0
print('PASS: simple cascade mass-balance correct')
"

# AC 3: catalyst excluded from mass-balance
echo "=== AC 3 ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"schema_version":2,"name":"t","throughput":{"basis_stage":"s1","basis_input":"w","qty_per_run":100,"qty_unit":"L","runs_per_year":10},"stages":[{"id":"s1","inputs":[{"name":"w","qty":1,"qty_unit":"L","per_basis":"principal","role":"principal","unit_cost":0.001,"cost_unit":"eur_per_L","density_kg_per_unit":1.0},{"name":"sugar","qty":50,"qty_unit":"g","per":1,"per_unit":"L","per_basis":"principal","role":"consumable","unit_cost":0.001,"cost_unit":"eur_per_g","density_kg_per_unit":0.001},{"name":"yeast","qty":0.5,"qty_unit":"g","per":1,"per_unit":"L","per_basis":"principal","role":"catalyst","unit_cost":0.05,"cost_unit":"eur_per_g"}],"outputs":[{"name":"o","role":"product","qty_unit":"L","density_kg_per_unit":1.0,"value_per_unit_usd":4.0}],"efficiency":0.95}]},"direction":"forward","scale":{"kind":"from_throughput"}}' \
  $BASE/api/simops/cascade | python3 -c "
import sys, json; d = json.load(sys.stdin)
stage = d['stages'][0]
inputs = {i['name']: i for i in stage['inputs_resolved']}
assert inputs['yeast']['mass_balance_contribution_kg'] == 0.0
assert 'mass_balance_excluded_reason' in inputs['yeast']
assert inputs['yeast']['cost_eur'] > 0
note_kinds = [n['kind'] for n in stage['cascade_notes']]
assert 'catalyst_excluded_from_mass_balance' in note_kinds
print('PASS: catalyst excluded, cost counted, note emitted')
"

# AC 4: upstream linkage
echo "=== AC 4 ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"schema_version":2,"name":"t","throughput":{"basis_stage":"s1","basis_input":"w","qty_per_run":100,"qty_unit":"L","runs_per_year":10},"stages":[{"id":"s1","inputs":[{"name":"w","qty":1,"qty_unit":"L","per_basis":"principal","role":"principal","unit_cost":0.001,"cost_unit":"eur_per_L","density_kg_per_unit":1.0}],"outputs":[{"name":"intermediate","role":"downstream_feed","qty_unit":"L","density_kg_per_unit":1.0}],"efficiency":0.9},{"id":"s2","inputs":[{"name":"intermediate","from_stage":"s1","role":"principal"}],"outputs":[{"name":"final","role":"product","qty_unit":"L","density_kg_per_unit":1.0,"value_per_unit_usd":5.0}],"efficiency":0.95}]},"direction":"forward","scale":{"kind":"from_throughput"}}' \
  $BASE/api/simops/cascade | python3 -c "
import sys, json; d = json.load(sys.stdin)
assert d['stages'][0]['mass_balance']['total_output_kg'] == 90.0
s2_in = d['stages'][1]['inputs_resolved'][0]
assert s2_in['source'] == 'from_stage:s1'
assert abs(s2_in['mass_balance_contribution_kg'] - 90.0) < 0.01
assert abs(d['stages'][1]['mass_balance']['total_output_kg'] - 85.5) < 0.01
print('PASS: upstream link resolves, mass propagates')
"

# AC 5: broken upstream link → 400
echo "=== AC 5 ==="
STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X POST \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"schema_version":2,"name":"t","throughput":{"basis_stage":"s1","basis_input":"w","qty_per_run":100,"qty_unit":"L","runs_per_year":10},"stages":[{"id":"s1","inputs":[{"name":"w","qty":1,"qty_unit":"L","per_basis":"principal","role":"principal","unit_cost":0.001,"cost_unit":"eur_per_L","density_kg_per_unit":1.0}],"outputs":[{"name":"intermediate","role":"downstream_feed","qty_unit":"L","density_kg_per_unit":1.0}],"efficiency":0.9},{"id":"s2","inputs":[{"name":"wrong_name","from_stage":"s1","role":"principal"}],"outputs":[{"name":"final","role":"product","qty_unit":"L","density_kg_per_unit":1.0,"value_per_unit_usd":5.0}],"efficiency":0.95}]},"direction":"forward","scale":{"kind":"from_throughput"}}' \
  $BASE/api/simops/cascade)
[ "$STATUS" = "400" ] && echo "PASS: broken link → 400" || echo "FAIL: got $STATUS"

# AC 6: missing density on consumable → warn note, cost still counted
echo "=== AC 6 ==="
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"process":{"schema_version":2,"name":"t","throughput":{"basis_stage":"s1","basis_input":"w","qty_per_run":100,"qty_unit":"L","runs_per_year":10},"stages":[{"id":"s1","inputs":[{"name":"w","qty":1,"qty_unit":"L","per_basis":"principal","role":"principal","unit_cost":0.001,"cost_unit":"eur_per_L","density_kg_per_unit":1.0},{"name":"mystery_powder","qty":10,"qty_unit":"g","per":1,"per_unit":"L","per_basis":"principal","role":"consumable","unit_cost":0.001,"cost_unit":"eur_per_g"}],"outputs":[{"name":"o","role":"product","qty_unit":"L","density_kg_per_unit":1.0,"value_per_unit_usd":2.0}],"efficiency":0.95}]},"direction":"forward","scale":{"kind":"from_throughput"}}' \
  $BASE/api/simops/cascade | python3 -c "
import sys, json; d = json.load(sys.stdin)
stage = d['stages'][0]
inputs = {i['name']: i for i in stage['inputs_resolved']}
assert inputs['mystery_powder']['mass_balance_contribution_kg'] == 0.0
assert 'mass_balance_excluded_reason' in inputs['mystery_powder']
assert inputs['mystery_powder']['cost_eur'] > 0
note_kinds = [n['kind'] for n in stage['cascade_notes']]
assert 'density_missing_input_excluded' in note_kinds
print('PASS: missing density → 0 MB contribution, cost counted, warn note emitted')
"

echo ""
echo "=== AC 7: role-flip test ==="
echo "Run kask scripts/test-roles-flip.js against the fixture pair:"
echo "  crates/simops/process/scoby_kombucha_v2.toml  (kombucha_liquid as downstream_feed)"
echo "  crates/simops/process/bc_optimization_v2.toml (pellicle as downstream_feed)"
echo "Both must cascade with identical mass-balance totals; outputs differ only in role labels."
```

---

## 7. What Is Unchanged

- `POST /api/simops/dynamics` — unchanged (dynamics models don't touch I/O accounting)
- `POST /api/simops/rheology` — unchanged
- `POST /api/simops/project` — updated to require v2 ProcessConfig (same 400 message); distributional sampling over per-input qty distributions is a follow-up spec (30.x)
- `GET /api/simops/dynamics/models`, `GET /api/simops/rheology/models` — unchanged
- Auth, workspace spawn, SOSA observation writes, agent invocations — all unchanged
- v1 ProcessConfigs without explicit `schema_version` field and with singular `input`/`output` fields — still accepted via legacy path (no operator action needed for existing v1 workspaces)

---

## 8. Reference Fixtures

Three v2 TOML fixtures are in the repo at `crates/simops/process/`:

| File | Purpose |
|---|---|
| `scoby_kombucha_v2.toml` | 3-stage kombucha; `kombucha_liquid` as `downstream_feed` |
| `bc_optimization_v2.toml` | Same biology; `pellicle` as `downstream_feed` — AC7 role-flip counterpart |
| `bottle_conditioned_saison_v2.toml` | 2-stage carbonation; canonical numbers for O3 cross-stack validation |

These are the ground-truth fixtures for `scripts/test-roles-flip.js` and `scripts/test-multi-input-cascade.js`.
