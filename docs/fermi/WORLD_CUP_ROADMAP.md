# Fermi World Cup System — Implementation Roadmap

**Goal:** 104 coordinated forecast workspaces (32 team priors + 9 tournament paths + 63 H2H matches) with cross-workspace dependency propagation, orthogonal factor models, Polymarket price tracking, and Brier-scored self-improvement.

**Architectural principle:** Every forecast is an ABW workspace. The workspace is the unit of observability, composition, and benchmarking. If it doesn't have a workspace, it doesn't exist.

**Benchmarking story:** "We built a 104-workspace forecast system for the 2026 FIFA World Cup. Each team prior workspace tracks 6 orthogonal factors. Tournament paths simulate 10K bracket draws. H2H matches use Dixon-Coles Poisson models. The whole system auto-updates as matches resolve, propagates updates through a dependency DAG, and scores itself against Polymarket crowd prices. Here's how it performed."

---

## Phase 0: Foundation Validation (CURRENT)

**Status:** Mostly complete. Validating that the basic workspace-per-forecast pattern works.

| Item | Status | Notes |
|------|--------|-------|
| `fermi_forecast` app registered | Done | `apps/fermi_forecast.json` |
| Workspace spawn on forecast creation | Done | `orchestrate_question()` calls spawn API |
| Agent messages bridged to workspace | Done | Evidence, decomposition, param updates |
| PM price persistence across save/restore | Done | `polymarket` block in state.json |
| PM continuous polling with schedule UI | Done | 5min/15min/30min/1hr/daily selector |
| Dashboard visibility (Loop 3 coherence) | Done | `fermi_forecast` origin included |

**Remaining Phase 0 work:**
- [ ] Verify workspace appears on ABW dashboard
- [ ] Verify Loop 3 coherence evaluates fermi workspaces
- [ ] Deploy server code (uuid fixes, migrations 138-139) to production

---

## Phase 1: Workspace Output & Cross-Workspace Read

**Prerequisite for:** Everything downstream. Without typed workspace outputs, no workspace can consume another's results.

### 1a. Workspace Output Table

New table: `workspace_outputs` — typed key-value store for workspace results.

```sql
CREATE TABLE workspace_outputs (
    workspace_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    key          TEXT NOT NULL,           -- e.g. "tournament_strength", "p_win"
    value        JSONB NOT NULL,          -- structured value
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (workspace_id, key)
);
```

**API:**
- `PUT /api/workspaces/:id/outputs/:key` — set a workspace output
- `GET /api/workspaces/:id/outputs` — read all outputs
- `GET /api/workspaces/:id/outputs/:key` — read single output

**Console integration:** After simulation runs, Fermi writes outputs:
```json
{
  "predicted_probability": 0.04,
  "tournament_strength": 1.35,
  "factor_scores": {"X1": 0.82, "X2": 0.91, ...},
  "sobol_indices": {"X1": 0.23, "X2": 0.19, ...}
}
```

### 1b. Cross-Workspace Output Read (Agent Tool)

New agent tool: `read_workspace_output(workspace_id, key)` — lets agents in one workspace read outputs from another.

This is the mechanism by which Tournament Path reads Team Prior outputs.

### 1c. Workspace Status Lifecycle

Add `status` to `teams`: `active | completed | failed | archived`

Fermi workspaces move: `active` → `completed` (on resolution) → scored (Brier).

---

## Phase 2: Workspace Dependency DAG

**Prerequisite for:** Event propagation, tournament path simulation.

### 2a. Dependency Table

```sql
CREATE TABLE workspace_dependencies (
    upstream_id   UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    downstream_id UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    dependency_type TEXT NOT NULL DEFAULT 'output',  -- 'output' | 'event'
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (upstream_id, downstream_id)
);
```

### 2b. Event Propagation

When a workspace's output changes, emit `workspace_output_updated` to all downstream workspaces. Downstream workspaces can auto-trigger re-simulation.

**Implementation:** Extend `WorkspaceEvent` broadcast to fan out to downstream workspaces. The DAG prevents cycles (enforce acyclicity on INSERT).

### 2c. Console UI

Show dependency graph in the Composer — which workspaces feed into this one, which ones consume its outputs.

---

## Phase 3: Batch Workspace Spawn

**Prerequisite for:** Instantiating 32 team priors from a CSV.

### 3a. Batch Spawn Endpoint

```
POST /api/apps/:slug/workspaces/batch
Body: { instances: [{ name, description, params }] }
Returns: { workspaces: [{ workspace_id, name, status }] }
```

Internally loops the existing `spawn_workspace_handler` logic. Atomic: all succeed or none.

### 3b. Parameter Binding

Workspace template gains `params` support. On spawn, params are:
1. Written to `.app/params.json` in the workspace git
2. Available to agents via `read_workspace_file(".app/params.json")`
3. Used by FPL template instantiation (string substitution initially, then parser support in Phase 5)

### 3c. Batch Spawn Script

Python/Rust script that:
1. Reads team data CSV (32 rows, 26 columns per the spec)
2. Calls batch spawn endpoint
3. Sets up dependency graph (team priors → tournament paths)
4. Associates Polymarket markets per workspace

---

## Phase 4: Orthogonality Enforcement

**Prerequisite for:** Valid Sobol decomposition. Without orthogonal factors, sensitivity indices are meaningless.

### 4a. Residualization Pipeline

Add to Fermi executor: before running Sobol, apply sequential OLS residualization:

```
X2' = X2 - proj(X2 | X1)
X3' = X3 - proj(X3 | X1, X2')
X4' = X4 - proj(X4 | X1, X2', X3')
...
```

Validate: `max(|corr(Xi, Xj)|) < 1e-6` for all i != j.

### 4b. Sobol Integration

Already exists as MCP tool (`fermi_sensitivity_analysis`). Wire it to:
1. Run automatically after simulation
2. Store results as workspace outputs (`sobol_indices`)
3. Alert if variance budget drifts >5% from priors

---

## Phase 5: FPL Parser Extensions

**Prerequisite for:** Formal factor definitions, not just string substitution.

### 5a. New FPL Syntax

```fpl
param team_id: string
factor X1 "Socioeconomic Capital" {
  inputs: gdp_per_capita_log, population_log, hdi_logit
  formulation: pca_pc1(gdp_per_capita_log, population_log, hdi_logit)
  variance_share: 0.25
  update: static
}
import factor X1 with (gdp_per_capita_log = param.gdp_per_capita_log, ...)
```

### 5b. Parser Changes

Extend `lexer.rs`, `parser.rs`, `ast.rs`, `semantic.rs` to handle:
- `PARAM` declarations
- `FACTOR` blocks with formulation and variance share
- `IMPORT FACTOR ... WITH (...)` bindings
- `RESIDUAL(X ~ Y, Z)` expressions

### 5c. Executor Changes

`execute_program` needs to:
1. Resolve params from workspace `.app/params.json`
2. Compute factor values from formulations
3. Apply residualization (Phase 4)
4. Run Sobol on orthogonalized factors

---

## Phase 6: Self-Improvement Harness

**Prerequisite for:** Learning from match outcomes.

### 6a. Learnable Parameters

Extend FPL with `LEARNABLE PRIOR(initial, sigma)`:
- Store current values in workspace outputs
- After ground truth (match result), compute Brier score
- Gradient update on elasticities (alpha through zeta)
- Constraint: `sum(elasticities) = 1.0` (variance budget integrity)
- Re-run orthogonality check after update

### 6b. Brier Feedback Loop

On match resolution:
1. Fetch forecast from workspace output
2. Compute Brier score against actual outcome
3. Log to `fermi_forecast_updates` and workspace action log
4. Update learnable parameters
5. Propagate updated prior to downstream workspaces

### 6c. Dixon-Coles Bivariate Poisson

Add to Fermi executor for H2H matches:
- Bivariate Poisson with rho correlation parameter
- Initialize rho = -0.13 (empirical World Cup value)
- rho is learnable, updated post-tournament

---

## Phase 7: World Cup Instantiation

**The payoff.** All infrastructure is in place. Now instantiate the system.

### 7a. Data Collection
- 32 team parameter CSVs (Elo from footballdatabase.com, GDP from World Bank, squad data from Transfermarkt)
- 2026 WC group draw + bracket structure (48 teams, 12 groups of 4)
- Venue data (altitude, climate zones)

### 7b. Batch Spawn
1. Spawn 32 team prior workspaces (actually 48 for 2026 format)
2. Spawn 12 group path workspaces
3. Register dependency edges
4. Associate Polymarket outright winner markets

### 7c. Tournament Execution
- Pre-tournament: team priors computed from static + historical data
- Match day: H2H workspaces spawn per fixture
- Post-match: update priors, propagate through DAG
- Resolution: Brier score everything, update learnable parameters

### 7d. Benchmarking
- Compare Fermi forecasts vs Polymarket crowd prices over time
- Sobol decomposition: which factors drove the most variance?
- Brier calibration: how well-calibrated were we?
- Self-improvement: did the learnable parameters converge?

---

## Dependency Graph

```
Phase 0 (done) ──► Phase 1 (outputs + cross-read)
                        │
                        ├──► Phase 2 (DAG + propagation)
                        │         │
                        │         ├──► Phase 3 (batch spawn)
                        │         │         │
                        │         │         └──► Phase 7 (WC instantiation)
                        │         │
                        │         └──► Phase 6 (self-improvement)
                        │
                        └──► Phase 4 (orthogonality)
                                  │
                                  └──► Phase 5 (FPL extensions)
                                            │
                                            └──► Phase 7 (WC instantiation)
```

**Critical path:** Phase 0 → 1 → 2 → 3 → 7
**Can parallelize:** Phase 4 + 5 alongside Phase 2 + 3

---

## Timeline Estimate

| Phase | Effort | Can Start |
|-------|--------|-----------|
| 0 | Done | Now |
| 1 | 2-3 sessions | Now |
| 2 | 2-3 sessions | After Phase 1 |
| 3 | 1-2 sessions | After Phase 2 |
| 4 | 1 session | After Phase 1 |
| 5 | 3-4 sessions | After Phase 4 |
| 6 | 2-3 sessions | After Phase 2 |
| 7 | 2-3 sessions | After Phases 3, 5 |

**World Cup 2026 starts:** June 11, 2026 — 4 days from now.
**Realistic target:** Have team priors + group paths running by match day 3-4 (June 14-15). H2H matches spawn as fixtures are confirmed. Self-improvement kicks in after first round of results.

---

## Open Decisions

1. **48 vs 32 teams:** 2026 WC has 48 teams in 12 groups of 4. The spec says 32. Update to 48.
2. **BayesOps first?** The self-improvement harness (Phase 6) is the Bayesian update loop. It can run before the FPL parser extensions — just store learnable params in workspace outputs and update via Python/Rust scripts post-match.
3. **Polymarket granularity:** Outright winner markets exist for most teams. Group winner, advance-from-group, and match result markets are sparser. Need to map available markets to workspace associations.
4. **Data sources:** Elo ratings (footballdatabase.com), GDP/HDI (World Bank API), squad data (Transfermarkt scrape or FBref), match stats (FBref/StatsBomb).
