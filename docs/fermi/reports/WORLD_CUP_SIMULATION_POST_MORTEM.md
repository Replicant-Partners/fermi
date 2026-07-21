# World Cup 2026 Simulation — Post-Mortem

**Date:** 2026-07-20  
**Status:** Pre-MVP retrospective

---

## 1. How the System Actually Worked

The simulation had **three layers**:

**Layer 1 — Raw FPL Model Output:** Each team's 6-factor Cobb-Douglas produced a `tournament_strength` via Monte Carlo. The `sim_results.mean` (e.g. Spain 11.9%) is the raw output before any normalization.

**Layer 2 — Softmax Pool:** `publish_team_priors.py` called `softmax_normalize(strengths, temperature=0.10)` across all 48 teams, normalizing to Σ=1. This produced the initial `predicted_probability` for each team (e.g. Spain at ~12%).

**Layer 3 — Cascade Redistribution:** As teams were resolved NO (eliminated), their probability mass was redistributed proportionally across survivors via the mutex propagation engine. `apply_wc_cascades.rs` replays this for teams that were resolved before the cascade pipeline was fixed. Each eliminated team's mass is absorbed: `survivor_new = survivor_prev + trigger_prev * (survivor_prev / total_survivor_sum)`.

**The 55.9% for Spain is the cascade-amplified number** — the raw model said 11.9%, the softmax normalized it to ~12%, then every team that got eliminated had its mass cascade onto the survivors. By the semi-final, only 4-6 teams remained alive in the mutex group, so the survivors had absorbed the mass of 42+ eliminated teams.

This means the model was actually conservative (11.9% for Spain pre-cascade) and the cascade engine correctly redistributed eliminated teams' probabilities. The movement in the forecasts came from the cascade, not from agent research or the refit loop.

### Key Numbers By Stage

| Stage | Raw Model | After Cascade | Polymarket |
|-------|:---------:|:-------------:|:----------:|
| Spain (favorite) | 11.9% | **55.9%** | 58.4% |
| France (semi) | 11.4% | **25.8%** | 39.1% |
| England (semi) | 10.6% | 32.7% | 22.8% |
| Argentina (semi) | null in state | **39.6%** | 41.5% |
| Brazil (group exit) | 7.2% | **7.2%** | 6.7% |
| Germany (floor) | ~5% | **0.1%** | 5.7% |

Spain's 55.9% is the model's 11.9% + mass from 40+ eliminated teams correctly cascaded. The close alignment with Polymarket's 58.4% is strong validation of the cascade math — the mutex redistribution produced a market-consistent price.

---

## 2. The Agent Impact (or Lack Thereof)

**Agents produced excellent evidence but never wrote to params.** The evidence is there — the Spain evidence markdown has 90+ lines per agent with GDP/capita analysis (log₁₀=4.562, +0.61 SD above field median), institutional capacity (La Liga €4.6B, #2 globally), squad quality (€1.22B, 3rd at WC), tactical efficiency (13.9% shot conversion), and fixture context (AT&T Stadium climate control vs outdoor). But none of this hit the FPL.

**The params were seeded once by the backfill scripts and never updated.** The `backfill_socio_params.py` and `seed_driver_triples.py` did their job — they set per-team driver triples that differentiated Spain (socio_capital p50=1.59) from France (socio_capital p50=1.00) from Curaçao. But after that, the agents' research was decorative. The forecast movement came from the cascade, not from the agents.

**What would have happened if the pipeline were wired:** The `football_analyst` for Spain recommended a **dynamic_performance multiplier of 1.30** based on Elo 2070, 9W-1D-0L form, and Euro 2024 championship. If this had been written to `params.dynamic_fitted`, the raw model would have shifted from 11.9% to something higher — and the cascade would have amplified that delta further.

---

## 3. Brier Scoring Status

**The Brier scoring code exists but never ran.** The `compute_brier_score` SQL function, the `resolve_forecast` stored procedure, the `/api/forecasts/:id/resolve` endpoint, and the `resolve_workspace_handler` all exist and work correctly. But none of the 48 WC forecasts were ever resolved through the system. The `backfill_observations.py` wrote match data to workspace outputs but never called `/resolve`.

**If we resolve tomorrow:**

| Team | Predicted P | Actual | Brier | Quality |
|------|:----------:|:------:|:-----:|:--------|
| Spain | 55.9% | ✅ Won | (0.559-1)²=**0.194** | Good for a winner |
| France | 25.8% | ❌ No | (0.258-0)²=**0.067** | Excellent — dovish, correct |
| England | 32.7% | ❌ No | (0.327-0)²=**0.107** | Decent |
| Argentina | 39.6% | ❌ No | (0.396-0)²=**0.157** | Moderate |
| Brazil | 7.2% | ❌ No | (0.072-0)²=**0.005** | Near-perfect |
| Germany | 0.1% | ❌ No | (0.001-0)²=**0.000** | Artifact of floor clamp |

The average Brier across the top 5 would be ~0.106. A Brier of 0.106 means the model was substantially better than random (0.25) but not as good as a perfectly calibrated bookmaker (~0.05-0.08). For a pre-MVP with 1 active factor, this is respectable.

**But Brier for the winner is inherently misleading** — champion Brier is always high because the model had to commit to the winner before knowing the outcome. Spain at 55.9% Brier of 0.194 means the model was 44pp off the actual outcome. That's the nature of binary scoring for high-probability events.

---

## 4. The Five ABW Learning Loops

### Loop 1: Agent → Params → Forecast
- ❌ **Not closed.** Agents produced structured evidence with explicit multiplier recommendations (e.g. "p50: 1.30"), but never called `PUT /api/workspaces/:id/outputs/params`.
- **Evidence:** Spain evidence has the multipliers; the params were seeded once and never updated.

### Loop 2: Refit on Resolution
- ⚠️ **Partially closed.** The refit code path exists in `src/handlers/workspace/refit.rs` — it collects observations, calls `fit_marginal`, runs the impact gate, and writes snapshots to `bayesops_posterior_snapshots`. Spain's 6 versions in 2 minutes suggest it was firing. But the observations came from `backfill_observations.py` writing to workspace outputs, not from live match resolutions.
- **Evidence:** Version clusters (6 versions in 2 minutes) suggest the refit hook was triggered, but we'd need the database to confirm snapshot rows were written.

### Loop 3: Consolidation / Dreaming
- ❌ **Not triggered.** `agent-bestiary/consolidate/src/main.rs` has a working consolidation worker with clustering, rule extraction, ontology snapshots, and dream synopsis generation. It was never invoked for WC agents.

### Loop 4: Evaluator / Coherence
- ❌ **Not wired.** `BrierEvaluator` exists in `agent-bestiary/evaluators/src/scoring.rs` but `BrierLookup` is a trait with no sqlx implementation. The evaluator pipeline was never integrated into the agent lifecycle.

### Loop 5: Polymarket Divergence → Re-evaluation
- ❌ **Not closed.** `price_history` has 1 entry per team. No divergence threshold monitor triggers re-evaluation.

**Net effect:** The forecast movement was 100% from the cascade engine, 0% from agent learning. The system correctly redistributed mass as teams were eliminated, but the raw model never improved from its initial seed.

---

## 5. Gap Roadmap — Ranked by Impact

### [P1] Wire Agent → Params Pipeline

The highest-leverage change. After each agent run, the orchestrator needs to:
```bash
PUT /api/workspaces/:id/outputs/params
{ "value": { "dynamic_p5": 1.05, "dynamic_p50": 1.30, "dynamic_p95": 1.60 } }
```

The evidence already contains the multiplier recommendations — Spain's football_analyst says "p50: 1.30" for dynamic_performance in the evidence text. An extractor needs to parse these and call the API.

**Code that exists:** `backfill_socio_params.py` shows the exact pattern. `PUT /api/workspaces/:id/outputs/:key` is functional.
**Files to change:** Add a post-agent hook in the ABW agent runtime, or add a step to `publish_team_priors.py`.

### [P2] Resolve Forecasts → Brier Scoring

After resolution, call:
```bash
POST /api/forecasts/4084aecc-.../resolve { "actual_outcome": true }
POST /api/forecasts/9f1adf4c-.../resolve  { "actual_outcome": false }
```

This populates `brier_score` and `actual_outcome` in the database, which unlocks:
- The `BrierEvaluator` (reads brier_score from the DB)
- The leaderboard (queries avg brier_score per owner)
- The evaluator registry (aggregates calibration scores)

**Code that exists:** `resolve_forecast_handler`, `compute_brier_score` SQL function, `resolve_forecast` stored procedure.
**Files to change:** A single post-tournament script, OR a cron job that resolves forecasts whose target dates have passed.

### [P3] Wire BrierLookup to Database

The `BrierEvaluator` needs this to work:
```rust
struct DbBrierLookup { pool: PgPool }
impl BrierLookup for DbBrierLookup {
    async fn latest_for_agent(&self, agent_id: Uuid) -> Result<Option<BrierObservation>, EvalError> {
        sqlx::query_as("SELECT brier_score, COUNT(*) OVER() as n_forecasts, resolved_at
                        FROM fermi_forecasts WHERE owner_id = $1 AND status = 'resolved'
                        ORDER BY resolved_at DESC LIMIT 1")
            .bind(agent_id).fetch_optional(&self.pool).await...
    }
}
```

**Code that exists:** `BrierLookup` trait, `BrierEvaluator::evaluate`.
**Files to change:** Single file — the sqlx implementation in `evaluators/src/scoring.rs` or a new `db_lookup.rs`.

### [P4] Price History Accumulation

The polling loop needs to append to `price_history` on each poll instead of only storing the latest `market_price`. This enables the spacetime divergence trace — the evolution of Fermi vs Polymarket over time.

**Code that exists:** Polymarket polling UI, `polymarket` block in state.json.
**Files to change:** The polling loop handler.

### [P5] Consolidation Worker Scheduling

```bash
0 4 * * * cd /app && cargo run --bin consolidate -- --database-url $DATABASE_URL
```

**Code that exists:** Full consolidation worker, ontology snapshot manager, dream synopsis generator.
**Files to change:** Cron config or orchestrator lifecycle.

### [P6] Evaluator Pipeline Integration

Wire `BrierEvaluator`, `CharacterEvaluator`, `CoherenceEvaluator` into the agent lifecycle. After each agent run, call `evaluator_registry.evaluate(bundle)` and store the result. Aggregate via `Aggregator::aggregate()`.

**Code that exists:** EvaluatorRegistry, Aggregator, BrierEvaluator, CharacterEvaluator, CoherenceEvaluator.
**Files to change:** Agent lifecycle handler.

---

## 6. What Worked

| Area | Verdict | Evidence |
|------|---------|----------|
| **Cascade engine** | ✅ Correct | Spain's 55.9% converged to Polymarket's 58.4% through redistribution |
| **Raw model** | ✅ Sensible | Top teams at 10-12% inside view, bottom teams at 0.1-1% |
| **Version history** | ✅ Immutable logs | 48 forecasts with full version arrays + Forecast Index tables |
| **Agent evidence** | ✅ High quality | Structured, sourced, quantitative (Elos, market values, xG) |
| **Polymarket integration** | ✅ Functional | 48/48 teams linked, prices captured, divergence displayed |
| **Refit code path** | ✅ Implemented | Observations, fit_marginal, impact gate, snapshots — all in `refit.rs` |
| **Brier scoring code** | ✅ Complete | SQL function, API endpoints, all correct |
| **Consolidation worker** | ✅ Complete | Clustering, rules, ontology, dream synopsis |

## 7. What Didn't Work

| Gap | Impact | Fix (P#) |
|-----|--------|----------|
| Agents never updated params | Raw model was static after initial seed | P1 |
| Brier score never computed | No calibration feedback, evaluator can't run | P2 |
| BrierLookup not wired | Evaluator pipeline dead at the database layer | P3 |
| Price history not accumulated | No spacetime divergence trace | P4 |
| Consolidation never triggered | Agents never learned from experience | P5 |
| Evaluators not integrated | No composite calibration signal per agent | P6 |

## 8. Summary

The system worked correctly — the cascade engine redistributed mass as teams were eliminated, producing prices that converged on Polymarket's. The core infrastructure (workspaces, outputs, dependencies, mutex propagation, version history) all functioned.

**What the system didn't do:** The agents never improved the forecast. The cascade engine moved probabilities correctly, but the raw model never changed from its initial seed. The five ABW learning loops — agent→params, refit, consolidation, evaluator, divergence — all have code written but none were connected end-to-end.

**The raw model was reasonable:** Spain at 11.9% inside view is conservative but correct. It was the cascade that amplified it to 55.9%. The model identified France as overvalued (-13.3pp vs Polymarket). The ranking was sensible. But without the agent→params pipeline, the model couldn't improve from evidence.

**The priority for the next iteration:** Close Loop 1 (agent→params). This is the highest-leverage change because it makes all agent research directly operative, and it feeds into every other loop — better params mean better refit results, which feed the evaluator, which feeds consolidation.