# Kask Handoff — 2026-05-24
**Time:** 02:17 CEST  
**From:** ABW platform session (Ivan Labra)  
**To:** kask.bio frontend / integration team  
**Re:** What changed tonight, what you can call now, what stays greyed out and why

---

## TL;DR

Two things shipped. One thing is newly ready for you to wire up.
One thing stays greyed out exactly as planned.

| Capability | Status | What kask does |
|---|---|---|
| Workspace spawn, agent invocation, state reads | ✅ Unchanged | Same API as before |
| `monte_carlo_sim` — real MC + Sobol | ✅ Now real | Call via `@monte_carlo_sim` or `/api/agents/monte_carlo_sim/execute` |
| `+ Generate distribution` (Digital Twin) | 🟡 MCP tool built, HTTP endpoint pending | Keep button greyed out, see §3 |
| BayesOps base rate fitting | 🗓 Roadmap | Nothing to wire yet |

---

## 1. What Changed on the ABW Side

### `monte_carlo_sim` is now real (v2.0.0)

Previously this agent reasoned about distributions in text — it was LLM-approximating percentiles. It was **not running actual simulations**.

As of tonight it calls two real MCP tools:

**`fermi_execute_fpl`** — runs a genuine 10,000-sample Monte Carlo simulation via the Fermi execution engine. Returns:
```json
{
  "iterations": 10000,
  "mean": 0.71,
  "median": 0.73,
  "std_dev": 0.12,
  "p5": 0.48,
  "p25": 0.62,
  "p75": 0.81,
  "p95": 0.91,
  "min": 0.21,
  "max": 0.99,
  "base_rate": 0.65,
  "divergence_relative": 0.092,
  "divergence_absolute": 0.06
}
```

**`fermi_sensitivity_analysis`** — runs real Sobol index computation. Returns ranked drivers with `first_order_index`, `total_order_index`, and 90% confidence intervals. Not heuristic guesses.

**How to invoke from kask:**

No API change needed. Use the existing agent invocation path:

```
POST /api/workspaces/:workspace_id/messages
{
  "content": "@monte_carlo_sim What is the probability TSMC revenue exceeds $100B in 2027?",
  "message_type": "agent_invocation"
}
```

Or background invocation:
```
POST /api/agents/monte_carlo_sim/execute
{
  "query": "Model the probability distribution for batch yield given these inputs...",
  "workspace_id": "<workspace_id>"
}
```

The agent now writes valid FPL, executes it, and returns real numbers. The output format is unchanged — it still produces a structured text report. The difference is the numbers in that report are now computed, not approximated.

**Reproducibility:** pass `seed` in the query if you want byte-identical runs for comparisons.

---

## 2. New Platform Primitive: `crates/projections`

A new Rust crate `projections` was built tonight. This is the foundation for the Digital Twin "Generate distribution" button. It is **not yet exposed as an HTTP endpoint** — that's the one remaining wiring step before you can call it.

### What it does

Runs any registered deterministic model N times with inputs sampled from declared distributions. Returns distribution summaries (percentiles + histogram) per output dimension. No LLM in the path.

### The request shape (final — won't change)

```json
{
  "model": {
    "kind": "simops_cascade",
    "config": { /* ProcessConfig JSON — same shape you already send to simops_cascade */ }
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

### The response shape (final — won't change)

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
      {
        "dimension": "net_carbon_kg",
        ...
      },
      {
        "dimension": "total_opex_usd",
        ...
      },
      {
        "dimension": "system_ner",
        ...
      }
    ]
  }
}
```

### Variable path syntax

Uses **JSON Pointer** (RFC 6901) into the ProcessConfig:

| What you want to vary | Path |
|---|---|
| Stage 0 efficiency | `/stages/0/efficiency` |
| Stage 2 efficiency | `/stages/2/efficiency` |
| Electricity price | `/elec_price_per_kwh` |
| Primary input quantity | `/primary_input_quantity` (virtual — not in ProcessConfig, resolved by executor) |
| Stage 1 OPEX per unit | `/stages/1/opex_per_input_unit` |

### Available sweep distributions

```json
{ "type": "uniform",    "low": 0.8,  "high": 0.95 }
{ "type": "normal",     "mean": 0.85, "std": 0.04 }
{ "type": "triangular", "p5": 0.75,  "p50": 0.85,  "p95": 0.95 }
{ "type": "beta",       "alpha": 8,  "beta": 2 }
```

### `output_format` options

| Value | What you get |
|---|---|
| `"aggregate"` | Distribution summaries only. Use this for the histogram render. Default. |
| `"raw_runs"` | All N run outputs as array. For offline analysis. |
| `"both"` | Both. |

---

## 3. The Digital Twin Button — Status

The button should **stay greyed out** until the HTTP endpoint exists.

**What's done:**
- `crates/projections` — the math engine, fully tested (8/8 tests pass)
- `simops_cascade` registered as the first executor
- Request/response wire contract is final (won't change — see §2 above)

**What's missing:**
- One HTTP handler: `POST /api/projections/run` or `POST /api/apps/kask_simops/project`
- This is ABW-side work, estimated small (the engine is done, it just needs an Axum route)

**When it lands:** The endpoint will be added to this doc with a smoke-test curl. Watch for `docs/integrations/KASK_INTEGRATION.md` update or a direct message.

**In the meantime:** The renderer can be built and tested against the response shape in §2. You can mock the response locally — the shape is final.

---

## 4. `project_timeseries` — Still Deferred

The interface is stubbed (calling it returns a clear error: "not yet implemented"), but the implementation is deferred. This covers:
- pH curve synthesis over 72-hour fermentation
- Any kinetic / time-evolving model

This is a separate primitive from `project_distribution`. When it lands it will be a second endpoint with the same request shape but `sweep.kind = "time_evolution"` and `steps` / `step_size_seconds` in the config.

**You do not need to handle this case now.** The greyed-out button covers both until the distribution endpoint lands.

---

## 5. BayesOps — Not Yet, But Designed

A spec exists (`docs/specs/14_BAYESOPS_SPEC.md`) for fitting distribution parameters from historical cultivation run data. This would make the `Beta(α, β)` parameters in SimOps forecasts data-driven rather than human-elicited.

**Nothing to wire on the kask side.** When Phase 1 ships, the output is a `Beta(α, β)` that flows into the existing FPL `Driver` syntax — no new API, no new UI needed. You'll just see better-calibrated base rates in `monte_carlo_sim` outputs.

---

## 6. Smoke Tests — Run These Now to Confirm Nothing Broke

The existing integration is unchanged. These should all still pass:

```bash
# 1. App still registered
curl https://agent-bestiary.world/api/apps/kask_simops

# 2. Auth still works
curl -H "Authorization: Bearer $TOKEN" \
  https://agent-bestiary.world/api/auth/me

# 3. Spawn still works
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Handoff smoke test"}' \
  https://agent-bestiary.world/api/apps/kask_simops/workspaces

# 4. monte_carlo_sim is now real — invoke it
curl -X POST \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "query": "What is the probability that Ambu batch yield exceeds 5kg given lighting=135kWh, nutrients=6.8g, temp=27.5C? Use triangular distributions with ±20% range.",
    "workspace_id": "<any_workspace_id>"
  }' \
  https://agent-bestiary.world/api/agents/monte_carlo_sim/execute
```

Test 4 should now return **real computed percentiles** (not LLM-estimated ones). The difference is subtle in the response format but the numbers will be mathematically correct and reproducible.

---

## 7. Nothing Else Changed

- Workspace spawn API: unchanged
- Auth flow: unchanged
- Agent invocation paths: unchanged
- SimOps cascade / predictor / optimizer / narrator agents: unchanged
- ProcessConfig schema (`kask-simops/2`): unchanged
- Budget / gas model: unchanged
- SOSA observation writes: unchanged

If anything in the existing integration is broken, it is **not** from tonight's changes. File against the last known-good commit (`19762f3`).
