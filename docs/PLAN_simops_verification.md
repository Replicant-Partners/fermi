# Plan: SimOps fully verifiable end-to-end

**Goal:** Every agent-to-agent call in the SimOps fleet produces typed,
gate-enforced output. The composition test validates that the companion
reads only fields the specialists actually declare.

**Reference implementation:** `tests/weather_composition.rs` (weather
pipeline — 4 typed agents, 1 coordinator).  
**Target:** `tests/simops_composition.rs` (SimOps fleet — companion +
oracle + cascade + comparator).

---

## Dependencies

```
Step 1: type simops_cascade
Step 2: type comparator
    (both required before the composition test can have anything to check)
Step 3: simops_composition.rs
    (validates steps 1 + 2 + the oracle already done)
Step 4: xaman_ek update
    (always last — the card must reflect the real state, not the plan)
```

---

## Step 1 — Type `simops_cascade`

**What the cascade produces** (from system prompt + sample queries):
```json
{
  "stage_table": [ { "stage", "input", "output", "carbon_delta", "ner", "opex" } ],
  "system_summary": { "total_input", "final_output", "net_carbon", "system_ner", "total_cost" },
  "interpretation": "prose",
  "cascade_result": { /* raw CascadeResult JSON */ }
}
```

**Sketch blocks:**

| Block | Status | Source | Why |
|---|---|---|---|
| `stage_table` | `inferred` | Cascade arithmetic over input config | Deterministic calculation, no external tool. The cascade engine IS the tool, but it is this agent's own computation. |
| `system_summary` | `inferred` | Derived from stage_table values | Same — rollup arithmetic, no retrieval. |
| `interpretation` | `narrative` | n/a | Prose reading of numbers — model judgement on its own output. |
| `cascade_result` | `inferred` | Same computation as stage_table | The raw SOSA-ready object is the agent's computed output. |

**Sketch file:** `agents/curated/simops_cascade/output_contract.sketch.json`

```json
{
  "domain": "process-optimisation",
  "produces_schema": "simops/cascade_result",
  "title": "SimOps cascade result",
  "synthesis": "pipeline",
  "calibration": {
    "signal": "sosa_observation",
    "comparison": "predicted_vs_measured",
    "resolution_delay": "process_dependent"
  },
  "blocks": [
    {
      "name": "stage_table",
      "source": { "status": "inferred", "from": "cascade arithmetic over process config" },
      "why": "Stage-by-stage energy and mass balance computed by the cascade engine from the process configuration supplied by the caller. No external tool. The computation is this agent's primary job.",
      "fields": {
        "stage": "string",
        "input_quantity": "number?",
        "output_quantity": "number?",
        "carbon_delta": "number?",
        "ner": "number?",
        "opex": "number?"
      }
    },
    {
      "name": "system_summary",
      "source": { "status": "inferred", "from": "rollup over stage_table" },
      "why": "System-level totals derived from the per-stage values. Arithmetic over inferred values is itself inferred.",
      "fields": {
        "total_input": "number?",
        "final_output": "number?",
        "net_carbon": "number?",
        "system_ner": "number?",
        "total_cost": "number?"
      }
    },
    {
      "name": "interpretation",
      "source": { "status": "narrative" },
      "why": "Plain-language reading of the cascade results with actionable flags. Model judgement over its own computed numbers.",
      "value": "string"
    }
  ]
}
```

**Compile + splice:**
```bash
cargo run --bin contract-sketch -- simops_cascade
python3 scripts/splice_contract.py simops_cascade  # or manual splice
```

**TYPED_TIER_EXEMPT:** remove `"simops_cascade"`, lower BASELINE to 77.

**`produces` update:** `["simops/cascade_result"]`

**`accepts` update:** `["simops/process_config"]` — structured JSON input,
not free-text labels.

---

## Step 2 — Type `comparator`

**What the comparator produces** (from system prompt — already returns valid JSON):
```json
{
  "task": "compare_experiment",
  "winner_by_metric": { "<metric>": "<scenario_slug>" },
  "narrative": "<prose>",
  "trade_offs": [{ "scenario", "wins", "loses" }],
  "recommendation": "<1 sentence>",
  "next_questions": ["<q1>", "<q2>"]
}
```

**Sketch blocks:**

| Block | Status | Why |
|---|---|---|
| `winner_by_metric` | `inferred` | Model's analytical judgement over scenario distributions |
| `trade_offs` | `inferred` | Model's trade-off analysis |
| `recommendation` | `inferred` | Model's single-sentence verdict |
| `next_questions` | `inferred` | Model's follow-on suggestions |
| `narrative` | `narrative` | Prose channel — no provenance stamp |

**Sketch file:** `agents/curated/comparator/output_contract.sketch.json`

```json
{
  "domain": "process-optimisation",
  "produces_schema": "simops/comparison_result",
  "title": "SimOps comparison result",
  "synthesis": "pipeline",
  "calibration": {
    "signal": "sosa_observation",
    "comparison": "predicted_vs_measured",
    "resolution_delay": "process_dependent"
  },
  "blocks": [
    {
      "name": "winner_by_metric",
      "source": { "status": "inferred", "from": "scenario distributions provided by caller" },
      "why": "The comparator reads scenario result distributions and identifies which scenario wins on each requested metric. This is the agent's analytical judgement — no external tool. All values must trace to the input scenarios (a rule in the system prompt).",
      "fields": {
        "metric_name": "string?",
        "winning_scenario": "string?"
      }
    },
    {
      "name": "trade_offs",
      "source": { "status": "inferred", "from": "scenario distributions provided by caller" },
      "why": "Trade-off analysis comparing scenarios across metrics — where one wins, what it sacrifices.",
      "fields": {
        "scenario": "string",
        "wins": "string[]",
        "loses": "string[]"
      }
    },
    {
      "name": "recommendation",
      "source": { "status": "inferred", "from": "winner_by_metric + trade_offs synthesis" },
      "why": "One-sentence verdict synthesising the full comparison. Model judgement.",
      "value": "string"
    },
    {
      "name": "next_questions",
      "source": { "status": "inferred", "from": "comparison results + process context" },
      "why": "Follow-on experiments the user should run to sharpen the decision. Model's suggestions based on the comparison.",
      "value": "string"
    },
    {
      "name": "narrative",
      "source": { "status": "narrative" },
      "why": "2-4 paragraphs interpreting the comparison for the operator. Prose channel — scanned for numeric leaks from uncomputed blocks but not stamped with a provenance verdict.",
      "value": "string"
    }
  ]
}
```

**Compile + splice:**
```bash
cargo run --bin contract-sketch -- comparator
```

**TYPED_TIER_EXEMPT:** remove `"comparator"`, lower BASELINE to 76.

**`produces` update:** `["simops/comparison_result"]`

**`accepts` update:** `["simops/comparison_request"]` — structured JSON.

---

## Step 3 — `tests/simops_composition.rs`

Modelled directly on `tests/weather_composition.rs`. The key difference:
SimOps uses an action grammar (companion emits `invoke_agent` blocks) rather
than a direct pipeline. The test validates that the **callee schemas exist
and are internally consistent**, not that the companion reads specific field
names (which it doesn't, because the kask client renders results to the user
rather than the companion processing them programmatically).

**What to assert:**

```
1. Each callee has a compiled output_contract.schema (typed)
2. Each callee's produces[] matches their declared produces_schema
3. Each callee is not in TYPED_TIER_EXEMPT
4. Each callee the companion optionally depends on is A2A-eligible
   (published + public in practice — this is a card-level assertion,
   not a DB query)
```

**The pipeline declaration** (written out, not derived, per the
`weather_composition.rs` pattern):

```rust
const COMPANION: &str = "simops_companion";

/// (companion action type, callee agent, callee declared produces_schema)
const A2A_CONTRACTS: &[(&str, &str, &str)] = &[
    ("invoke_agent", "supply_chain_oracle", "scro/bom_response"),
    ("invoke_agent", "simops_cascade",      "simops/cascade_result"),
    ("invoke_agent", "comparator",          "simops/comparison_result"),
];
```

**Test functions:**

```rust
fn each_callee_has_a_compiled_schema()
    // card.pointer("/capabilities/output_contract/schema").is_object()

fn each_callee_port_references_its_type()
    // card["produces"] == vec![declared_type]

fn no_callee_is_still_grandfathered()
    // !is_typed_tier_exempt(callee_id)

fn each_callee_is_an_optional_dependency_of_the_companion()
    // companion card dependencies.optional contains the callee
```

The composition test does NOT assert leaf-name matching (the weather pattern)
because the companion does not read the callee's schema fields directly — it
delegates and the kask client renders the result. The meaningful check is that
the types exist and the ports reference them, not that the companion reads
specific field names.

---

## Step 4 — xaman_ek card update

The maintenance rule in xaman_ek's own card: *"Whenever a new agent, skill,
or platform endpoint is added to the codebase, this agent card must be updated
in the same commit."*

This is overdue. The card needs:

1. **New endpoints** in the SimOps pipeline description:
   - `supply_chain_oracle` — now typed (scro/bom_response), A2A-callable
   - `simops_cascade` — now typed (simops/cascade_result)
   - `comparator` — now typed (simops/comparison_result)

2. **A2A provider surface** — new platform capability:
   ```
   GET  /.well-known/agent-directory.json
   GET  /a2a/:slug/agent-card.json
   POST /a2a/:slug/message:send
   POST /a2a/:slug/message:stream
   GET  /a2a/:slug/tasks/:episode_id
   POST /a2a/:slug/tasks/:episode_id/pushNotificationConfigs
   ```

3. **TYPED_TIER_EXEMPT count** — xaman_ek's prompt references the exemption
   count. Update to reflect current BASELINE (76 after steps 1–2).

4. **DESIGN docs** — reference the new design documents:
   - `docs/DESIGN_a2a_contracting.md`
   - `docs/DESIGN_a2a_provider.md`
   - `docs/A2A_DEVELOPER_GUIDE.md`

**File:** `agents/curated/xaman_ek/agent_card.json`

---

## Validation sequence

After completing steps 1–3:

```bash
cargo run --bin contract-sketch -- simops_cascade --check
cargo run --bin contract-sketch -- comparator --check
cargo test --test contract_sketch_corpus
cargo test --test simops_composition      # new test
cargo test --lib -p fermi typed_tier_exemption
```

All should pass before xaman_ek is updated (step 4 documents the real state).
