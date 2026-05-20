# 11 — Cascade Unit Bridging Limitation

**For:** the fermi maintainer (simops cascade engine)
**From:** kask team (companion piece to `09_RESEARCH_AGENT_OUTPUT_STRIPPED.md`,
`10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md`)
**Status:** documented limitation, kask-side mitigations shipped
(`v=20260519v4j`), fermi-side fix in [`crates/simops/src/process.rs`](../../crates/simops/src/process.rs).

## The failure case

User-modelled process: 4-stage kombucha bioink (Tea Prep `L` →
SCOBY Fermentation `L→kg` → Alkali Purification `kg→kg` →
Mechanical Homogenisation `kg→kg` → Bioink Formulation `kg→kg`).

Live observation: every cascade run returns `final_output = 0.0 kg`
at every stage past Tea Prep. KPI strip shows zero NER, zero carbon
delta. The user sees an apparently broken process and can't tell why.

## Root cause

`crates/simops/src/cascade.rs::Stage::forward` propagates quantities
through energy:

```
in_qty
  → in.energy_kwh(in_qty)        # multiplies by energy_density
  → × efficiency
  → out_energy_kwh
  → out.quantity_from_kwh(out_e) # divides by energy_density
```

`Resource::energy_kwh` (`crates/simops/src/process.rs:49`) returns
`0.0` whenever the input is missing `energy_density` AND its unit is
not already `kWh`. For mass-tracking processes where the user never
declared an energy density (because energy is irrelevant to the
domain), this collapses the cascade to zero silently.

This was a perfectly reasonable design choice for the original
fuel-cell / energy-balance use case the cascade was built for. It
just doesn't fit mass-tracking biological/food/material processes,
which are the bulk of what users actually model.

## Evidence the agents already handle this

The comparator agent's analytical fallback proves this is salvageable:
given the same YAML, it reasons directly and produces a fully
traceable comparison (cumulative yield 0.523 base vs 0.394 aerogel
variant; carbon 2.8 vs 5.4 kg CO₂-eq/kg). **Preserve this fallback.**
It's a feature.

## Kask-side mitigations shipped (`v=20260519v4j`)

1. **`KaskSim.analyseCascadeViability(proc)`** — pure function in
   `kask-sim-client.js` that inspects a process and flags every stage
   where energy-density-bridge propagation will fail, with per-stage
   explanations.

2. **Cascade viability banner** — amber warning above the sankey on
   the Process tab when viability fails. Names the offending stage,
   explains the cascade engine's limitation, offers three fix paths
   (add `energy_density`, ask companion for proposed values, or use
   the comparator's analytical fallback).

3. **Pipeline panel DEGRADED state** — Activity tab's pipeline row
   surfaces `cascade ran but output collapsed to 0` as a distinct
   amber state rather than green OK.

4. **Companion context bundle** — viability findings are included in
   the companion's per-turn context as `cascade_viability` so the
   agent can acknowledge the issue and suggest fixes rather than
   pretending the zero numbers are real.

## Proposed fermi-side resolutions (any of these would close the gap)

### Option A — Same-unit pass-through *(shipped)*

When `stage.input.unit == stage.output.unit` and neither has an
energy_density, treat the cascade as mass-balance:
`out_qty = in_qty × efficiency`. No conversion attempted. This is
what users intuitively expect from a "fermentation efficiency of 72%
on tea media" — 72% of the mass survives, not "0 because we have no
calories table."

Touches: `crates/simops/src/process.rs` (`Stage::forward`,
`Stage::backward`, new `Stage::use_mass_balance` predicate).

### Option B — UCUM unit-conversion fallback *(not shipped)*

When inputs and outputs are dimensionally compatible (`L↔mL`,
`kg↔g`, `mol↔mmol`), bridge using a UCUM table directly rather than
going through kWh. Falls back to Option A if dimensions don't match
but units are equal.

### Option C — Explicit bridge declarations on stages *(not shipped)*

Add an optional `unit_bridge` field to stage YAML:
```yaml
stages:
  - id: fermentation
    input:  { name: media, unit: L }
    output: { name: pellicle, unit: kg }
    unit_bridge: { density_kg_per_L: 1.02 }  # explicit bridge
    efficiency: 0.72
```
When present, the bridge is used directly. When absent, fall back to
the energy-density path (current behaviour).

## What landed

**Option A.** The smallest change with the largest practical win for
the dominant user cohort (mass-tracking processes). Option C remains
the right long-term shape because it explicitly captures the
assumption; Option B is the most general but the largest
implementation surface and isn't justified by current evidence.

New predicate `Stage::use_mass_balance()` is true iff both:

- `input.unit == output.unit`
- neither resource carries an `energy_density` annotation

When true, `forward` returns `input_quantity * efficiency` directly
and `backward` returns `target_output / efficiency`. Otherwise the
existing energy-bridge path runs unchanged.

This is a strict subset of the cases where the energy bridge
currently returns 0, so it cannot change the output of any process
whose energy bridge worked previously. `kWh → kWh` stages also
satisfy the trigger and produce identical results, so they're
unaffected in practice.

## Post-fix verification

```bash
cargo test --package simops process::tests::forward_propagation_uses_mass_balance_when_no_energy_density
cargo test --package simops cascade::tests::mass_tracking_cascade_does_not_collapse_to_zero
```

Or end-to-end via the simops HTTP endpoint:

```bash
curl -sS -H "Authorization: Bearer $API_KEY" \
     -H "Content-Type: application/json" \
     -d '{"process":{"name":"test","stages":[
            {"id":"purify","efficiency":0.85,"carbon_intensity":0.02,
             "input":{"name":"pellicle","unit":"kg"},
             "output":{"name":"pellicle","unit":"kg"}}]},
          "quantity":10.0,"direction":"forward"}' \
     https://agent-bestiary.world/api/simops/cascade \
  | jq '.final_output_quantity'
# Expected: 8.5  (pre-fix: 0)
```

## What the companion agent should already do (verify in v3 prompt)

When `cascade_viability.viable === false` in the context bundle:

- Acknowledge the issue ("the cascade can't bridge L→kg without an
  energy_density on the fermentation media")
- Propose either explicit density values (kombucha media ≈ 1.02
  kg/L) via `edit_process` or recommend running the comparator's
  analytical fallback via `run_simulation`
- DO NOT narrate "the process looks healthy" when NER reads 0 and
  the user is staring at zero outputs in the sankey.

The companion's v3 agent card (`03_COMPANION_AGENT_CARD.md`) should
add an explicit instruction to this effect. (Kask-side change — not
part of this fix.)

## Cross-references

- `08_FILES_API_DIVERGENCE.md` — sibling platform-level bug
- `09_RESEARCH_AGENT_OUTPUT_STRIPPED.md` — sibling content-channel bug
- `10_RESEARCH_AGENTS_EMPTY_LLM_OUTPUT.md` — sibling executor bug
- Sample workspace where the failure was first surfaced: kombucha
  bioink process described above
