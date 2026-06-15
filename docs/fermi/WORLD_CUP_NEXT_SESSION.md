# World Cup System — Next Session Plan

**Written:** 2026-06-16 01:45 CEST  
**Status:** Ready to execute  
**Context:** 60 workspaces spawned (48 team priors + 12 groups), infrastructure (outputs, DAG, batch spawn) complete. Workspaces are empty shells — need real forecast models.

---

## Decision: Build correctly, not quickly

The current Fermi decomposer produces generic 5-driver models. The spec requires a 6-factor orthogonal model with Cobb-Douglas response function, PCA/OLS residualization, and learnable elasticities. We will NOT approximate with current syntax — we will extend the system to support the spec natively.

---

## Execution Order

### Step 1: FPL Parser Extensions (Phase 5 from roadmap)

Extend `lexer.rs`, `parser.rs`, `ast.rs`, `semantic.rs` in the `fermi` crate:

**New tokens:**
- `FACTOR` — factor block declaration
- `PARAM` — parameter declaration  
- `IMPORT` — import factor/program
- `RESIDUAL` — residualization expression
- `LEARNABLE` — learnable parameter with prior

**New AST nodes:**
```rust
pub struct FactorStmt {
    pub name: String,
    pub label: String,
    pub inputs: Vec<FactorInput>,
    pub formulation: Expression,      // PCA, weighted sum, etc.
    pub variance_share: f64,
    pub update_frequency: UpdateFreq, // Static, PerMatch, PerFixture
}

pub struct ParamDecl {
    pub name: String,
    pub param_type: ParamType,        // String, Real, Int, Bool
}

pub struct ImportStmt {
    pub factor_name: String,
    pub bindings: Vec<(String, Expression)>, // input_name = value
}

pub enum UpdateFreq {
    Static,
    PerMatch,
    TournamentStart,
    PerFixture,
}
```

**New expression types:**
```rust
Expression::PcaPc1(Vec<Expression>)
Expression::Residual { raw: Box<Expression>, upstream: Vec<String> }
Expression::CobDouglas { factors: Vec<(String, Expression)> } // name, elasticity
Expression::LearnablePrior { initial: f64, sigma: f64 }
```

### Step 2: Factor Agents

Define 6 specialized agents (or enhance `football_analyst`) to compute factor inputs:

| Factor | Agent | Data Sources |
|--------|-------|-------------|
| X1 Socioeconomic Capital | `macro_data_agent` (new or extend `macro_forecaster`) | World Bank API (GDP, population, HDI) |
| X2 Institutional Capacity | `football_institution_agent` (new) | FIFA registration data, league revenue indices |
| X3 Dynamic Performance | `football_analyst` (existing, enhanced) | Elo ratings, match stats (FBref), xG data |
| X4 Squad Quality | `football_analyst` (existing, enhanced) | Transfermarkt market values, squad age profiles |
| X5 Tactical Efficiency | `football_analyst` (existing, enhanced) | StatsBomb/FBref tactical metrics |
| X6 Exogenous Context | `fixture_context_agent` (new) | Venue data, climate, rest days |

**Pragmatic approach:** Start with `football_analyst` handling X3-X5 (it already researches these). Add lightweight data-fetch agents for X1, X2, X6 later. The factor VALUES can be seeded from the params CSV initially.

### Step 3: Orthogonality Enforcement (Phase 4)

Add to the Fermi executor (`src/executor.rs` or new `src/orthogonality.rs`):

```rust
pub fn residualize_factors(factors: &[FactorValues]) -> Vec<FactorValues> {
    // Sequential OLS: X2' = X2 - proj(X2|X1), X3' = X3 - proj(X3|X1,X2'), ...
    // Validate: max(|corr(Xi,Xj)|) < 1e-6
}
```

This runs BEFORE Sobol decomposition. Existing `fermi_sensitivity_analysis` already does SALib Sobol — just need to ensure inputs are orthogonalized first.

### Step 4: Cobb-Douglas Response Function

New model expression type in the executor:

```
model tournament_strength = A * (X1 ^ alpha) * (X2 ^ beta) * (X3 ^ gamma) * (X4 ^ delta) * (X5 ^ epsilon) * exp(zeta * X6)
```

Where `alpha` through `zeta` are learnable parameters initialized from variance shares.

In the executor, this means:
- Parse Cobb-Douglas expression
- During Monte Carlo: sample each factor, apply elasticities, compute product
- Store elasticities as workspace outputs for the self-improvement harness

### Step 5: TEAM_PRIOR FPL Template

Write a concrete FPL program using the new syntax:

```fpl
param team_id: string
param elo_current: real
param group: string
...

factor X1 "Socioeconomic Capital" {
  inputs: gdp_per_capita_log, population_log, hdi_logit
  formulation: pca_pc1(param.gdp_per_capita_log, param.population_log, param.hdi_logit)
  variance_share: 0.25
  update: static
}

factor X3 "Dynamic Performance Signal" {
  inputs: elo_current, elo_trend, goal_difference, pass_completion, xg_delta
  formulation: residual(
    0.30 * param.elo_current + 0.15 * param.elo_trend + ...,
    X1, X2
  )
  variance_share: 0.25
  update: per_match
}

model tournament_strength = learnable(1.0, 0.2) * (X1 ^ learnable(0.25, 0.05)) * ...

simulate 10000 iterations
```

### Step 6: Batch Initialize in Console

Console "Initialize from spec" button that:
1. Reads workspace params
2. Applies TEAM_PRIOR template with param bindings
3. Runs simulation (local, cheap — no LLM)
4. Publishes outputs (tournament_strength, factor_scores, sobol_indices)

Then optionally: "Research all" button fires `football_analyst` on X3-X5 drivers for evidence gathering (the expensive part, done in configurable batches).

### Step 7: Self-Improvement Harness (Phase 6)

After match results come in:
1. Fetch ground truth (match outcome)
2. Compute Brier score against forecast
3. Gradient update on learnable elasticities
4. Re-run orthogonality check
5. Re-simulate with updated params
6. Propagate to downstream workspaces

---

## File Impact Estimate

| File | Changes |
|------|---------|
| `src/lexer.rs` | ~100 lines (new tokens: FACTOR, PARAM, IMPORT, RESIDUAL, LEARNABLE) |
| `src/parser.rs` | ~200 lines (new parse functions for factor blocks, param decls) |
| `src/ast.rs` | ~100 lines (new AST nodes) |
| `src/semantic.rs` | ~50 lines (validation for factor references, variance budget) |
| `src/executor.rs` | ~150 lines (Cobb-Douglas eval, factor sampling) |
| `src/orthogonality.rs` | ~100 lines (new file: residualization pipeline) |
| `src/sensitivity.rs` | ~30 lines (wire orthogonalization before Sobol) |
| `agents/curated/football_analyst/` | ~50 lines (enhance system prompt for 6-factor awareness) |
| `crates/fermi-console/src/cockpit.rs` | ~100 lines (batch initialize UI, factor display) |
| Template FPL files | ~80 lines each (TEAM_PRIOR, TOURNAMENT_PATH, H2H_MATCH) |

**Total:** ~960 lines of new/modified code across ~10 files.

---

## Sequence for Tomorrow

1. Start with FPL parser extensions (Step 1) — this unblocks everything
2. Write the TEAM_PRIOR template (Step 5) — validates the parser works  
3. Wire batch initialize (Step 6) — populates all 48 workspaces
4. Enhance football_analyst (Step 2, partial) — for evidence gathering
5. Add orthogonality (Step 3) + Cobb-Douglas (Step 4) — makes Sobol valid
6. Self-improvement harness (Step 7) — post first round of results

Steps 1-3 get us live forecasts in workspaces. Steps 4-7 make them mathematically rigorous.
