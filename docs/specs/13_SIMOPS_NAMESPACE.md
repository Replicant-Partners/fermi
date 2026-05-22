# SimOps Namespace — `xi:simops/*` URIs catalogued

**Status:** living document; updated as new URIs land
**Audience:** anyone reading SimOps process YAML, sosa_observation
rows, agent_card.json sample_queries, or fermi traces; anyone
building an ABW app that wants to interop with SimOps
**Prerequisite vocab:** [W3C SOSA / SSN](https://www.w3.org/TR/vocab-ssn/)

## What this document is

SimOps models physical processes (fermentation, cultivation,
extraction, manufacturing) on ABW. Every observation-producing
entity in a SimOps process — every sensor, every sampler, every
derivation method, every property — is named with a URI. Where the
W3C SOSA / SSN vocabulary covers the concept, SimOps uses the
standard term directly (`sosa:Sensor`, `sosa:Sampler`,
`sosa:Observation`, `sosa:Procedure`, `sosa:Actuator`, `sosa:hasResult`,
`sosa:observedProperty`, etc.). Where SOSA doesn't cover the
concept, SimOps extends through URIs under the **`xi:simops/`**
prefix.

This document catalogues every `xi:simops/...` URI: what it means,
where it lives in a SimOps YAML or observation row, what its SOSA
relationship is, and an example. **It is the source of truth for
the SimOps extension vocabulary** — when in doubt about whether a
URI is canonical or invented, check here.

The companion spec for the SimOps domain MoE is
[docs/specs/12_AGENT_VERSION_FIRST_CLASS.md](./12_AGENT_VERSION_FIRST_CLASS.md);
the canonical agent cards live in `agents/curated/simops_*` and
`agents/curated/sensor_advisor/`.

## Namespace prefix conventions

| Prefix              | Resolves to                                            | Use                                                  |
|---------------------|--------------------------------------------------------|------------------------------------------------------|
| `sosa:`             | `http://www.w3.org/ns/sosa/`                           | W3C standard sensor/observation vocabulary           |
| `ssn:`              | `http://www.w3.org/ns/ssn/`                            | W3C semantic sensor network (sosa's superset)         |
| `xi:simops/`        | `https://abw.dev/ns/xi/simops/` (informal)             | SimOps extensions — covered by this document          |
| `env:`              | `https://abw.dev/ns/env/`                              | Environmental properties (temperature, light, etc.)  |
| `chem:`             | `https://abw.dev/ns/chem/`                             | Chemical properties (pH, BRIX, % ABV, etc.)          |
| `bio:`              | `https://abw.dev/ns/bio/`                              | Biological properties (cell density, biomass)         |
| `proc:`             | `https://abw.dev/ns/proc/`                             | Process-state properties (flow, pressure, mass)      |

The `env:`, `chem:`, `bio:`, `proc:` namespaces are SimOps-side
recommendations for common scientific properties. They're not
formal ontologies — there's no fetchable RDF schema behind them.
Where a property has a stable IRI in OBO / SSN / QUDT, prefer that;
otherwise use the SimOps prefix conventions.

The `xi:` prefix nods to `xi` as the eleventh letter of the Hebrew
alphabet — placeholder for "SimOps is an ABW extension; this isn't
W3C". The placeholder is intentional. When a `xi:simops/...` URI
proves load-bearing and stable across multiple ABW apps, it gets a
W3C-style canonical IRI in a future namespace consolidation.

---

## Section 1 — Sensor types (extend `sosa:Sensor`)

### `xi:simops/Sensor/Predicted`

**Extends:** `sosa:Sensor` via `rdfs:subClassOf`.
**Used in:** `stage.sensors[].type` field in a SimOps process YAML.
**Required companion:** every Predicted sensor MUST carry a
`derivation` block (see Section 2).

A sensor whose value is **derived** from one or more other sensors
via a named method, not measured directly. SOSA itself has
`sosa:Sensor` (a thing that measures) but not a distinct concept
for "computed observation source"; this URI fills that gap.

Predicted sensors emit `sosa:Observation` rows just like regular
sensors. The cascade engine writes the derived value to
`sosa:hasSimpleResult`; the `derivation.method` is recorded on each
observation as `sosa:usedProcedure` (Section 2 catalogues the
procedure URIs).

**Example:**

```yaml
- id: ph_continuous_projected
  type: xi:simops/Sensor/Predicted
  observes: chem:ph_value
  result_unit: pH
  feature_of_interest: scoby_fermentation
  cadence: derived
  derivation:
    method: xi:simops/method/recalibrated_projection
    base_model: xi:simops/model/ph_fermentation_curve
    recalibrate_against: ph_daily_sample
    recalibration_strategy: anchor_residual
```

### `xi:simops/Sensor/Bound`  *(reserved — not yet emitted)*

**Planned use:** marker for sensors currently in `mode: 'live'` —
i.e. bound to an external SOSA source. Today this is tracked via
the metric-field `mode: 'live'` flag and a stamp on the sensor
row (`sensor.bound = true`); the URI is reserved for a future
serialisation where sensor state is the canonical encoding.

---

## Section 2 — Procedures (`sosa:Procedure` subtypes)

Every derivation method is a `sosa:Procedure` describing how a
Predicted sensor's value is computed. These URIs are referenced
from two places:

1. The `derivation.method` field on a Predicted sensor row in the
   process YAML (the *declaration* of how this sensor produces values).
2. The `sosa:usedProcedure` predicate on `sosa:Observation` rows
   that the cascade engine writes (the *audit record* that this
   observation came from this method).

### `xi:simops/method/physics_mass_balance`

**Extends:** `sosa:Procedure`.
**Determinism:** deterministic — the value is fully determined by
the inputs and the base_model.
**Required derivation fields:** `base_model` (a `xi:simops/model/...`
URI from Section 3). `inputs` is optional (some base_models read
batch metadata rather than other sensors).

Evaluates a named base_model over a timeline using the inputs as
parameters. The result is a closed-form function of the inputs;
running the same evaluation twice with the same inputs yields the
same outputs. Used when the relationship between inputs and the
observed property is captured by a first-principles or analytical
model (Michaelis-Menten kinetics, logistic growth, log-decay pH
descent, mass-conservation cascades).

**Example use:** an `xi:simops/Sensor/Predicted` declaring
`base_model: xi:simops/model/logistic_growth` produces a biomass
curve over time; no other sensors required.

### `xi:simops/method/agent_predictor`

**Extends:** `sosa:Procedure`.
**Determinism:** non-deterministic — the value comes from an LLM
invocation, subject to model temperature and (post fermi #5,
commit 47783b0) the predictor agent's current `agent_version`.
**Required derivation fields:** `inputs` (sensor.id list — the
historical streams the predictor reads). `agent` (defaults to
`simops_predictor`).

Invokes a member agent (typically `simops_predictor`) on the
historical observation streams of the input sensors, parsing the
response as a value series. Used when there's a known correlation
between inputs and a downstream observable but the relationship
isn't captured by a first-principles model — empirical or
data-driven prediction.

Per fermi #5, every observation the predictor produces carries a
`produced_by_agent_id` + `produced_by_version_number` stamp so
per-version calibration partitioning (the agent's
`/calibration?partition_by=version` endpoint) works correctly.

**Example use:** kombucha `alcohol_yield_predicted` reads
`ambient_temp`, `ph_continuous_projected`, `brix_daily_sample`
histories and emits an % ABV estimate.

### `xi:simops/method/recalibrated_projection`

**Extends:** `sosa:Procedure`.
**Determinism:** deterministic given a fixed sample set + base_model
+ strategy. Re-evaluates whenever a new sample lands or the base
model parameters change.
**Required derivation fields:** `base_model` (the synthetic shape),
`recalibrate_against` (sensor.id of a `sosa:Sampler` whose
observations anchor the residual). `recalibration_strategy` is
optional, defaults to `anchor_residual`.

The load-bearing pattern for properties where a continuous sensor
is operationally desirable but doesn't survive the process
environment (pH probes in low-pH fermentation, dissolved-oxygen
probes fouling, redox probes drifting). The base_model captures
the expected shape; the sampler observations anchor the residual.
The cascade engine computes:

```
derived_value(t) = base_model(t) + residual(t)

where residual(t) is interpolated from the residuals at each sample time:
   δ_i = sample_i - base_model(t_sample_i)

   for t between two samples: linear interp(δ_i, δ_{i+1})
   for t before/after all samples: held flat at nearest δ
   confidence decays with distance from nearest sample
```

This is the canonical kombucha pH pattern. The `sensor_advisor` agent
(`agents/curated/sensor_advisor/agent_card.json`, v0.2.0) proposes
the sampler+projection pair together when the operator's stage
description matches the pattern criteria.

#### Recalibration strategies (values for `recalibration_strategy`)

| Strategy                | Status        | Semantics                                                                  |
|-------------------------|---------------|----------------------------------------------------------------------------|
| `anchor_residual`       | **v1, default** — implemented in kask-side evaluator | Linear interp of residuals between samples; held flat outside |
| `bayesian_update`       | reserved      | Kalman-style posterior fusion; requires base_model variance bands           |
| `fit_residual_dynamics` | reserved      | Fit a residual function (e.g. exponential decay term) once N≥6 samples land |

The kask client (`simops-page.js` 2E, commit `0a4cdf0`) implements
`anchor_residual` only. Unknown / unimplemented strategies surface
as a `note` on the evaluator output; the chart shows the
base_model line only and reports the missing strategy.

---

## Section 3 — Base-model URIs (referenced by procedure derivations)

Base models are pure functions of (timeline, parameters) → time
series. They're not SOSA entities themselves — they're parameters
to the procedure URIs above. The kask client carries a catalogue of
known base_models (`simops-page.js`, `BASE_MODELS`) that it can
evaluate locally; unknown URIs are treated as opaque (the
derivation produces a `note` instead of a curve).

### `xi:simops/model/constant`

Flat baseline at a constant value.

| Parameter | Type   | Default | Meaning                       |
|-----------|--------|---------|-------------------------------|
| `value`   | number | `1.0`   | the constant returned at all t|

**Use:** properties with no time evolution (ambient temperature in
a climate-controlled lab, fixed efficiency before a real model
exists).

### `xi:simops/model/linear_ramp`

Linear interpolation from `start` at t₀ to `end` at t₁.

| Parameter | Type   | Default | Meaning                                |
|-----------|--------|---------|----------------------------------------|
| `start`   | number | `1.0`   | value at the first timeline point      |
| `end`     | number | `0.0`   | value at the last timeline point       |

**Use:** simple progress estimates — BRIX depletion during early
batch, linear-ramp mass accumulation under known input flow.

### `xi:simops/model/ph_fermentation_curve`

Log-decay pH descent. The canonical kombucha SCOBY model.

```
pH(t) = pH_start − decay × log(1 + (t − t₀) / τ)
```

| Parameter   | Type   | Default | Meaning                                       |
|-------------|--------|---------|-----------------------------------------------|
| `pH_start`  | number | `5.0`   | pH at batch start                              |
| `decay`     | number | `0.85`  | log-decay coefficient                          |
| `tau_hours` | number | `24`    | characteristic time (hours; 1 day default)     |

**Use:** SCOBY fermentation, vinegar production, any low-pH-trending
microbial bioprocess with organic-acid evolution. Defaults reproduce
a 14-day kombucha SCOBY batch from ~pH 5 down to ~pH 3.

### `xi:simops/model/logistic_growth`

Sigmoidal biomass growth approaching carrying capacity.

```
biomass(t) = K / (1 + ((K − x₀) / x₀) × exp(−r × (t − t₀)))
```

| Parameter     | Type   | Default | Meaning                                   |
|---------------|--------|---------|-------------------------------------------|
| `K`           | number | `0.5`   | carrying capacity                          |
| `x0`          | number | `0.01`  | initial biomass                            |
| `r_per_hour`  | number | `0.04`  | intrinsic growth rate (1/h)                |

**Use:** SCOBY pellicle yield, microalgae density, microbial cell
mass — any closed-vessel growth bounded by substrate or space.

---

## Section 4 — Property URIs (extend `sosa:ObservableProperty`)

These URIs name physical/chemical/biological properties being
observed. They appear in `stage.sensors[].observes` AND in the
`observable_property` column of `sosa_observation` rows.

The `xi:simops/property/...` namespace is for properties without
stable IRIs in established ontologies. Where standard terms exist
(SSN / OBO / QUDT / NIST), prefer those. SimOps recommends but
doesn't enforce — the operator is free to use any URI and the
system treats them as opaque strings for joining/filtering.

### Recommended URIs by domain

#### Environmental (`env:`)

| URI                            | Unit hint  | Description                              |
|--------------------------------|------------|------------------------------------------|
| `env:ambient_temperature`      | degC       | Air temperature in the process environment|
| `env:ambient_humidity`         | %RH        | Relative humidity                        |
| `env:light_intensity`          | µmol/m²/s  | PAR for photobioreactors                 |
| `env:carbon_intensity_kg_co2eq_per_kg` | kg_CO2eq/kg | Carbon intensity (used by simops_cascade) |

#### Chemical (`chem:`)

| URI                            | Unit hint        | Description                                  |
|--------------------------------|------------------|----------------------------------------------|
| `chem:ph_value`                | pH               | pH of a liquid medium                        |
| `chem:brix_percent`            | degBx            | Sugar content (refractive index proxy)       |
| `chem:alcohol_concentration`   | percent_abv      | Ethanol concentration by volume              |
| `chem:dissolved_oxygen`        | mg/L             | DO concentration                             |
| `chem:redox_potential`         | mV               | ORP / Eh                                     |
| `chem:conductivity`            | mS/cm            | Solution conductivity                        |
| `chem:total_dissolved_solids`  | mg/L             | TDS                                          |

#### Biological (`bio:`)

| URI                            | Unit hint     | Description                            |
|--------------------------------|---------------|----------------------------------------|
| `bio:optical_density_600nm`    | OD600         | Turbidity / cell-density proxy         |
| `bio:cell_count`               | cells/mL      | Direct microscopy count                 |
| `bio:dry_biomass`              | g/L           | Dried biomass concentration             |

#### Process (`proc:`)

| URI                            | Unit hint  | Description                              |
|--------------------------------|------------|------------------------------------------|
| `proc:flow_rate`               | L/h        | Volumetric flow                          |
| `proc:mass_flow_rate`          | kg/h       | Mass flow                                |
| `proc:pressure_drop`           | bar        | Pressure differential                    |
| `proc:torque`                  | Nm         | Mixer / pump torque                      |

#### SimOps-internal extension properties (`xi:simops/property/`)

For properties that don't have stable IRIs elsewhere. The
sensor_advisor v0.2.0 uses these for fermentation-specific
properties not covered by SSN/OBO.

| URI                                            | Unit hint           | Description                                 |
|------------------------------------------------|---------------------|---------------------------------------------|
| `xi:simops/property/bc_pellicle_yield_fraction`| g_BC/g_sucrose      | Bacterial cellulose pellicle yield          |
| `xi:simops/property/harvest_mass_recovery_fraction` | dimensionless  | Fraction of biomass recovered at harvest    |
| `xi:simops/property/cellulose_purity_mass_fraction` | dimensionless  | Purified cellulose mass / wet pellicle mass |
| `xi:simops/property/nanofibrillation_yield_fraction` | dimensionless | BNC → bioink mechanical yield               |

When proposing a new property URI, the sensor_advisor first checks
the recommended domains above. Only mints under `xi:simops/property/`
when no standard term covers the concept.

---

## Section 5 — Cadence (extend `sosa:Procedure` metadata)

SOSA models procedures but doesn't enumerate sampling cadence. The
`xi:simops/cadence` extension carries a coarse-grained enum on each
sensor row.

### `xi:simops/cadence` values

| Value         | Implied sensor type             | Meaning                                        |
|---------------|----------------------------------|------------------------------------------------|
| `continuous`  | `sosa:Sensor`                    | Reading every few seconds (probes, meters)      |
| `per_batch`   | `sosa:Sampler`                   | One reading per batch (sample + analyse)        |
| `manual`      | `sosa:Sampler`                   | Ad-hoc operator measurement; cadence irregular  |
| `derived`     | `xi:simops/Sensor/Predicted`     | Cadence inherited from inputs; fires on update  |

Cadence is stored verbatim on `stage.sensors[].cadence`. It's
operator-facing UX — the cascade engine doesn't depend on it for
math (the actual observation timestamps drive time-series
computations). It's still serialised because the v2 spec had a
`sampling: per_batch` field and the migration path preserves
information.

---

## Section 6 — Cascade output URIs (observable properties written by simops_cascade)

When `simops_cascade` runs a forward or backward cascade, it writes
`sosa:Observation` rows tagged with these `observable_property`
URIs. The cascade engine is internal to ABW (Rust crate
`crates/simops`); these URIs are the contract between the cascade
and any downstream consumer (kask UI, sensor_advisor, simops_predictor
training, fermi forecast triangulation).

| URI                                  | Unit                | Feature of interest | Description                                              |
|--------------------------------------|---------------------|---------------------|----------------------------------------------------------|
| `xi:simops/stage_output`             | (stage's output unit) | stage.id            | One per stage; the cascaded output quantity              |
| `xi:simops/final_output_quantity`    | (process output unit) | process.name        | Overall yield across the cascade                         |
| `xi:simops/net_carbon_kg`            | kg_CO2eq            | process.name        | Carbon balance summed across stages                      |
| `xi:simops/total_opex_usd`           | USD (or EUR)        | process.name        | Operational cost rollup (materials + energy + labor − sidestream credits) |
| `xi:simops/system_ner`               | dimensionless       | process.name        | Net Energy Ratio — only written when meaningful (energy-balance mode) |
| `xi:simops/predictor_forecast`       | (forecast unit)     | stage.id or process | A `simops_predictor` forecast observation               |
| `xi:simops/fermi_forecast`           | probability         | process.name        | A `fermi` orchestra forecast observation                |
| `xi:simops/cascade_result`           | structured          | process.name        | Aggregated cascade result envelope (cross-references stage_output etc.) |

These URIs are stable. Downstream consumers (the maturity-aware
companion context bundle, the activity-feed counters, the
calibration delta computations) join on these property URIs and
break if they change. Treat as part of the public contract.

### Provenance stamping

Every observation row written under these URIs carries the
fermi #5 `produced_by_*` columns (commit `47783b0`):

  - `produced_by_agent_id`  — typically `simops_cascade`
  - `produced_by_version_id` + `produced_by_version_number` —
    the cascade agent version active at write time

This makes the calibration query
`GET /api/agents/simops_cascade/calibration?partition_by=version`
work across cascade versions.

---

## Section 7 — Capability URIs (planned features)

Reserved for ABW capability negotiation. Not yet implemented.

| URI                              | Status     | Description                                                       |
|----------------------------------|------------|-------------------------------------------------------------------|
| `xi:simops/cap/sensor_ingest`    | reserved   | A platform-side ingest endpoint that accepts sensor-keyed observations (not yet field-keyed). Lets bare sensors (no `derived_for_field`) bind to URLs. Referenced from the v3 sensor editor's "unlinked" hint. |
| `xi:simops/cap/derivation_eval`  | reserved   | Server-side evaluation of `xi:simops/method/...` so derived sensors emit canonical observations (today the kask client evaluates locally for display only — see 2E commit 0a4cdf0). |
| `xi:simops/cap/actuator`         | reserved   | `sosa:Actuator` + actuation-rule plumbing. The next major arc.    |

---

## Section 8 — Actuators (reserved — next arc)

Actuators are SOSA-symmetric with sensors: a sensor produces
observations from a feature of interest; an actuator performs
actuations on a feature of interest. Reserved URIs:

| URI                                          | SOSA relation              | Status   |
|----------------------------------------------|----------------------------|----------|
| `xi:simops/Actuator/AgentTriggered`          | extends `sosa:Actuator`    | reserved |
| `xi:simops/Actuator/RuleTriggered`           | extends `sosa:Actuator`    | reserved |
| `xi:simops/ActuationRule`                    | new concept                | reserved |
| `xi:simops/actuation/method/proportional`    | extends `sosa:Procedure`   | reserved |
| `xi:simops/actuation/method/setpoint_pid`    | extends `sosa:Procedure`   | reserved |

The actuator arc adds HITL gating, observation-rule triggers
(observation crosses threshold → propose actuation → require human
approval → execute), and audit-traceable actuation logs. Out of
scope for issue 2; will get its own design doc when work starts.

---

## Section 9 — Versioning + lineage of this catalogue

This document is a **living catalogue**. It will be updated as new
URIs land. The expected rhythm:

- A new URI is minted in a kask commit or an agent card change.
- The corresponding spec doc (e.g. an agent_card markdown) gets
  updated.
- This document gets a row added with status `new` (or `reserved`
  if not yet implemented).
- After ~2 commits of stable usage the row's status drops the
  `new` marker.

URIs in this document with status **v1** are guaranteed stable —
the platform won't rename them. URIs marked **reserved** may
change shape before first ship. URIs marked **new** are
implemented but may receive non-breaking field additions.

The companion to this is the kask-side
`adaptogen/simops-v2/specs/v3/agent_versions.yaml` audit trail
(commit `412b23e`); together they reconstruct the full lineage of
every agent prompt × every URI it referenced × every observation
it produced.

---

## References

- **W3C SOSA / SSN**: https://www.w3.org/TR/vocab-ssn/
- **Doc 12 — Agent version as first-class** (issue #5, commit 47783b0):
  [12_AGENT_VERSION_FIRST_CLASS.md](./12_AGENT_VERSION_FIRST_CLASS.md)
- **kask thesis whitepaper** (the digital-twin learning loop):
  `kask/whitepapers/simops-learning-digital-twin.md`
- **sensor_advisor agent card** (the design-side consumer of this catalogue):
  `agents/curated/sensor_advisor/agent_card.json`
- **kask spec source-of-truth for sensor_advisor**:
  `kask/adaptogen/simops-v2/specs/v3/15_SENSOR_ADVISOR_AGENT_CARD.md`
- **kask client derivation evaluator** (the runtime that consumes these URIs):
  `kask/simops-page.js` (functions: `_evaluateDerivation`, `_evaluateBaseModel`, `BASE_MODELS`)
