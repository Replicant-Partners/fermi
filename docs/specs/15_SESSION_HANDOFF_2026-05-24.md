# Session Handoff — 2026-05-24
**Time:** 02:15 CEST  
**Session focus:** AutoStan / Bayesian forecasting research → spec → implementation  
**Next engineer / session:** picks up from here

---

## What Was Discussed (Research Layer)

### AutoStan paper (arxiv:2603.27766)
- A CLI coding agent (Claude Code) autonomously writes and improves Stan Bayesian models guided by a single scalar reward: **NLPD** (Negative Log Predictive Density) on held-out data
- Agent edits `model.stan`, runs MCMC via cmdstanpy, keeps or reverts changes
- No critic, no search algorithm — just NLPD + MCMC diagnostics (R-hat, ESS, divergences)
- Demonstrated on 5 datasets: regression with outliers, hierarchical models, varying slopes, Bundesliga soccer
- Key insight: NLPD is a strictly proper scoring rule — same mathematical family as Fermi's Brier Score

### Two MC loops — the core conceptual clarification
This distinction was established and must be preserved:

```
Loop A — Parameter Fitting (BayesOps, offline, runs once per dataset refit)
  Historical observations → HMC sampler → posterior over parameters
  Output: Beta(9.4, 13.6) or Normal(4.8, 0.7)
  → feeds into Loop B as Driver distribution parameters

Loop B — Forecast Simulation (executor.rs, online, runs per question)  
  FPL Driver distributions → executor.rs → MC samples → outcome distribution
  UNCHANGED. Knows nothing about Loop A.
```

These are mathematical inverses. Loop A inverts observations to find parameters. Loop B propagates parameters forward to outcomes. The seam between them is the `Distribution` type in the FPL AST.

### Why the existing `monte_carlo_sim` agent was wrong
Before this session it had `executor: "llm"` with no MCP tools — it was LLM-approximating percentiles in text. Now rewired to call real tools (see below).

---

## What Was Built (Code Changes)

### 1. `agents/curated/monte_carlo_sim/agent_card.json` — v1.0.0 → v2.0.0
**What changed:** complete rewrite. Agent now declares two MCP tools it must call instead of reasoning in text:
- `fermi_execute_fpl` — real Monte Carlo via executor.rs
- `fermi_sensitivity_analysis` — real Sobol indices via sensitivity.rs

System prompt explicitly forbids fabricating values. LLM's job is now FPL authoring + result interpretation only.

**File:** `agents/curated/monte_carlo_sim/agent_card.json`

### 2. `src/bin/agent-mcp-server.rs` — two new MCP tools added

```rust
// Tool: fermi_execute_fpl
// Input: fpl_program: String, iterations: Option<u32>, seed: Option<u64>
// Does:  Lexer → Parser → SemanticAnalyzer → Executor::execute()
// Returns: mean, median, std_dev, p5, p25, p75, p95, min, max,
//          base_rate, divergence_relative, divergence_absolute

// Tool: fermi_sensitivity_analysis  
// Input: fpl_program: String, iterations: Option<u32>
// Does:  full_sensitivity_analysis() → Sobol first-order + total-order per driver
// Returns: baseline stats + per-driver {first_order_index, total_order_index,
//          variance_contribution, standard_error, ci_low, ci_high} + top_driver
```

Helper function added: `parse_fpl(source: &str) -> std::result::Result<Program, String>`
runs the full lexer → parser → semantic pipeline, returns human-readable error strings.

**Both tools are wired and verified — `cargo build --bin agent-mcp-server` passes clean.**

### 3. `crates/projections/` — new crate, fully scaffolded

This is the platform-level distributional projection engine per the feature request doc filed by the kask team. **Not SimOps-specific by design.**

```
crates/projections/
├── Cargo.toml          — optional feature: simops-executor
└── src/
    ├── lib.rs           — project_distribution() + project_timeseries() (stub)
    ├── executor.rs      — ModelExecutor trait (registration contract)
    ├── registry.rs      — ExecutorRegistry (lookup by kind string)
    ├── sweep.rs         — SweepConfig, SweepKind, SamplingDistribution
    ├── distribution.rs  — DistributionSummary + Freedman-Diaconis histogram
    ├── types.rs         — ProjectionRequest / ProjectionResponse (wire contract)
    └── simops_executor.rs — SimOps cascade executor (behind simops-executor feature)
```

**Test results:** 8/8 passing (6 core + 2 simops-executor)
```
cargo test -p projections                           → 6 passed
cargo test -p projections --features simops-executor → 8 passed
```

**Key design decisions baked in:**
- `project_timeseries` exists but returns stub error — interface fixed, deferred
- `FromTypicalRange` sweep kind exists but returns stub error — deferred until field annotation convention agreed
- `simops_cascade` registered as `kind: "simops_cascade"` — first executor
- Seeded runs are byte-identical (verified by test)
- Output is shape-stable: same JSON fields whether N=1 or N=10000

### 4. `docs/specs/14_BAYESOPS_SPEC.md` — written and revised twice

Current state: v0.2, with explicit scope decision at the top.

**Phase 1 (`crates/posterior`, simple marginal fitting) — ACTIVE, not yet built**  
**Phases 2–5 — ROADMAP, implementation deferred**

Key spec decisions:
- Two crates: `posterior` (simple, conjugate/bootstrap) and `posterior-reg` (advanced, HMC)
- Shared contract: `FittedDistribution` enum (Beta/Normal/Lognormal/Triangular)
- `FittedDistribution::to_fpl_params()` → valid FPL Driver syntax string
- No changes to executor.rs, sensitivity.rs, or FPL AST (until Phase 5)
- `posterior-reg` explicitly deferred: evaluate `nuts-rs` crate before any custom HMC

---

## What Is NOT Yet Built (Immediate Next Steps)

### Priority 1 — Wire `projections` into the MCP server
`crates/projections/` exists and compiles but is not yet exposed as an MCP tool. The kask Digital Twin "Generate distribution" button needs this endpoint.

Add to `agent-mcp-server.rs`:
```rust
// Tool: fermi_project_distribution
// Input: ProjectionRequest as JSON
// Does:  ExecutorRegistry::default() → project_distribution()
// Returns: ProjectionResponse (aggregate summaries per dimension)
```

The `ProjectionRequest` / `ProjectionResponse` types are already defined and serializable.

### Priority 2 — `crates/posterior` (BayesOps Phase 1)
Simple marginal distribution fitting. No sampler needed. Covers the base rate gap.

```rust
pub fn fit_marginal(
    observations: &[f64],
    weights: Option<&[f64]>,
    family: DistFamily,
) -> Result<(FittedDistribution, FitMetadata), PosteriorError>
```

Start here: `fit_beta_conjugate(successes, failures)` → exact `Beta(1+s, 1+f)`. No external math deps beyond `statrs`.

### Priority 3 — Resolve the 5 open questions from the projections feature request doc
File: `docs/specs/15_SESSION_HANDOFF_2026-05-24.md` (this doc) references them.
The questions live in the feature request (filed as a design doc, not yet in specs/).

1. **Executor registration** — how does a new model declare itself? (Manifest field, runtime discovery, both?)
2. **Variable path syntax** — JSON Pointer confirmed (`/stages/0/efficiency`). Array index handling for sensors?
3. **`from_typical_range` semantics** — walk config fields with `typical_range` annotation; which fields qualify?
4. **Histogram bin sizing** — Freedman-Diaconis implemented; configurable override needed?
5. **Observation rate limiting** — N=10000 × 50 sensors = 500k SOSA writes; does `ingestObservations` batch?

---

## Architecture Decisions Made This Session (Do Not Revisit Without Cause)

| Decision | Rationale |
|---|---|
| Two MC loops are distinct — never merge | Mathematical inverses; different inputs, outputs, speeds |
| `crates/projections/` not `crates/simops/` | Generalizability principle; cascade is one executor, not the identity of the crate |
| `project_timeseries` stub, not absent | Interface fixed now so future implementation doesn't break callers |
| `BayesOps Phase 1 only` active | HMC is non-trivial; evaluate `nuts-rs` before custom implementation |
| `FittedDistribution` as cross-crate seam | Only type that flows between fitting layer and FPL layer |
| `monte_carlo_sim` uses real tools not LLM math | LLM-approximated percentiles are not trustworthy; real executor is already there |
| Static FPL injection (Mode 1) before dynamic (Mode 2) | Mode 2 requires parser extension; Mode 1 (paste Beta params) covers immediate need |

---

## File Index — Everything Touched This Session

| File | Status | Notes |
|---|---|---|
| `agents/curated/monte_carlo_sim/agent_card.json` | Modified | v2.0.0 — rewired to real MCP tools |
| `src/bin/agent-mcp-server.rs` | Modified | +2 tools: fermi_execute_fpl, fermi_sensitivity_analysis |
| `crates/projections/Cargo.toml` | New | |
| `crates/projections/src/lib.rs` | New | project_distribution() + project_timeseries() stub |
| `crates/projections/src/executor.rs` | New | ModelExecutor trait |
| `crates/projections/src/registry.rs` | New | ExecutorRegistry |
| `crates/projections/src/sweep.rs` | New | SweepConfig, SamplingDistribution |
| `crates/projections/src/distribution.rs` | New | DistributionSummary, histogram |
| `crates/projections/src/types.rs` | New | Wire contract types |
| `crates/projections/src/simops_executor.rs` | New | SimOps cascade executor |
| `Cargo.toml` | Modified | Added `crates/projections` to workspace members |
| `docs/specs/14_BAYESOPS_SPEC.md` | New | BayesOps spec v0.2, phases 2–5 roadmap |
| `docs/specs/15_SESSION_HANDOFF_2026-05-24.md` | New | This document |

---

## Build State at Handoff

```bash
cargo build --bin agent-mcp-server --bin fermi   # ✓ clean
cargo test -p projections                          # ✓ 6/6
cargo test -p projections --features simops-executor  # ✓ 8/8
```

No uncommitted work is broken. All new code compiles against existing workspace dependencies without version changes (except `Cargo.lock` churn from adding `projections`).

---

## Context That Doesn't Live in Code

- **kask Digital Twin "Generate distribution" button** is greyed out pending the `fermi_project_distribution` MCP tool (Priority 1 above). The kask-side renderer already accepts `bins[]` alongside `history[]` — it will fall back to sparkline when bins are absent.
- **`simops_predictor`** agent still runs OLS. The BayesOps path (Phases 2–4 of spec 14) is deferred. The OLS engine in `crates/simops/src/predictor.rs` is untouched.
- **The AutoStan paper author** (Oliver Dürr, TIDIT Switzerland) has a companion skill repo at `https://github.com/tidit-ch/autostan-skill` — a self-contained Claude Code skill for running the full AutoStan loop on new data. Potentially useful if we ever want to run AutoStan directly on Ambu cultivation run history rather than building the custom Rust HMC.
- **`monte_carlo_sim` is now wired but the API endpoint `fermi_project_distribution` doesn't exist yet** — so the agent card's tools will return `unknown tool` from the MCP server until Priority 1 is done.
