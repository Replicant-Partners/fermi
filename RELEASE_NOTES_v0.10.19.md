# v0.10.19 — REAL/f64 mismatch on numeric aggregates, family closed

> **Note on numbering.** This release also carries
> `migrations/166_agents_updated_at.sql`, which was authored for a
> v0.10.18 hotfix. That version number went to an unrelated console
> updater fix shipped from a parallel session, so mig-166 lands here
> instead. Everything below about mig-166 as "already shipped in
> v0.10.18" should be read as "shipped in this release, alongside
> mig-167." Both are schema-drift closures and deploy together; they
> touch different objects (`agents.updated_at` vs the
> `fermi_leaderboard` matview) and are independent.

## Why

Mo hit this in the **Resolve Forecast** modal (Will Carlos Alcaraz
win the 2026 Men's US Open?):

```
Server error: error occurred while decoding column 0:
mismatched types; Rust type `f64` (as SQL type `FLOAT8`) is not
compatible with SQL type `FLOAT4`
```

Same family as the `agents.owner_id` and `agents.updated_at` bugs
of v0.10.15/v0.10.16/v0.10.18: **the code assumes a schema shape
the DB doesn't have.** This time the shape is *numeric precision*:

- `fermi_forecasts.brier_score` and `predicted_probability` are
  `REAL` (FLOAT4, single-precision) per mig-048.
- `resolve_forecast()` SQL function returns `REAL` per mig-094.
- MIN/MAX preserve their input type, so `MIN(REAL)` returns `REAL`.
- AVG/STDDEV widen to `DOUBLE PRECISION` on numeric inputs.

sqlx enforces exact SQL ↔ Rust type match on scalar reads. Every
site that binds `f64` (FLOAT8) but reads a `REAL` (FLOAT4) column
400s at runtime. The AVG/STDDEV sites happened to work because
Postgres widened them; MIN/MAX and the `resolve_forecast()` return
did not.

Same "why hadn't we seen it" story: forecast resolution requires
auth + RBAC + non-admin FK realignment + owner check. Every gate
before v0.10.15 masked this bug. Mo's account is now clean end-to-end
after v0.10.9 → v0.10.13, so his click is the first request to
reach the type-decoding step.

## The substrate rule

**Every numeric aggregate or scalar-returning SQL function published
to Rust returns `DOUBLE PRECISION` (`float8`), either naturally
(AVG, STDDEV) or by explicit `::float8` cast (MIN, MAX,
`resolve_forecast()`).** Rust reads them idiomatically as `f64` /
`Option<f64>`. No `f32` in the read path. One rule, applied
platform-wide.

## Changes

### 1. `src/handlers/forecasts.rs::resolve_forecast_handler`

The confirmed bug from Mo's screenshot. Cast the function return
in SQL:

```sql
SELECT resolve_forecast($1, $2, $3, $4)::float8
```

Rust binding stays `let brier_score: f64 = …`. Downstream sites
(`record_forecast_calibration_signals`, JSON response, `.clamp()`
on `calibration_quality`) all expect `f64` — no changes needed.

### 2. `src/handlers/forecasts.rs::portfolio_stats_handler`

MIN/MAX cast to `float8` in the aggregate SELECT. Parens around
the whole `AGG(x) FILTER (WHERE …)` expression before the cast
so precedence is unambiguous across Postgres versions:

```sql
(MIN(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL))::float8 AS best_brier,
(MAX(f.brier_score) FILTER (WHERE f.brier_score IS NOT NULL))::float8 AS worst_brier,
```

AVG/STDDEV unchanged — already double precision.

### 3. `src/handlers/forecasts.rs::my_stats_handler`

Same fix pattern. This was latent — would break the moment any
user had ≥1 resolved forecast.

### 4. `src/handlers/forecasts.rs::leaderboard_handler` (fallback branch)

The handler tries `fermi_leaderboard` view first, falls back to a
live aggregate if the view is empty or missing. The fallback used
raw `MIN/MAX(brier_score)`. Cast to `::float8`.

### 5. Migration 167 — recreate `fermi_leaderboard` with `::float8`

`migrations/167_fermi_leaderboard_float8_minmax.sql`

`CREATE OR REPLACE` doesn't exist for materialized views — column
types are fixed at creation. Migration DROPs the view (CASCADE
takes the indexes with it) and recreates it with the `::float8`
casts on `MIN(brier_score)` / `MAX(brier_score)`. `WITH DATA`
rebuilds in-place from `fermi_forecasts`; no data loss.

Same pattern as mig-165/mig-166: idempotent (probes `pg_matviews`),
PgBouncer-safe DO blocks, EXCEPTION handlers, RAISE NOTICE
observability, post-migration validation that asserts
`information_schema.columns` reports `best_brier_score` and
`worst_brier_score` as `double precision`.

Registered in `src/api_server.rs::run_migrations`.
`test_all_migrations_registered` passes.

## What this release does NOT do

**No blanket audit sweep of every REAL column in the DB.** Only
the four sites in `handlers/forecasts.rs` and the leaderboard view
are covered by direct evidence + code review. Other tables have
REAL columns:

- `forecast_commitments.predicted_probability` — REAL. Only read
  as `f32` in `handlers/forecast_benchmark.rs` (correct).
- `fermi_forecasts.confidence_interval_low/high` — REAL, read as
  `Option<f32>` at every site (correct).
- `fermi_forecasts.sim_probability` (added post-mig-048) — need
  audit, out of scope for this hotfix.
- `simops_dynamics` / `dynamics_traces` numeric columns — DOUBLE
  PRECISION throughout mig-141 (confirmed above), safe.

The general audit belongs in the v0.11.0 trust contract: a
boot-time check that walks every `sqlx::query`/`query_scalar`
call, resolves its column types via `information_schema`, and
errors on any FLOAT4 ↔ f64 or FLOAT8 ↔ f32 mismatch. Third
column-drift family in five releases is a strong argument.

## Post-deploy verification

Mo's exact flow — the smoke test:

```bash
# Get a resolvable forecast Mo owns.
FORECAST_ID=$(psql -tA -c "SELECT id FROM fermi_forecasts \
  WHERE owner_id = (SELECT user_id FROM users WHERE email = 'mo@axolotl.partners') \
    AND status = 'active' \
  LIMIT 1")

curl -si -X POST \
     -H "Authorization: Bearer $MO_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"actual_outcome": true, "resolution_notes": "post-v0.10.19 smoke"}' \
     "https://agent-bestiary.world/api/forecasts/$FORECAST_ID/resolve"
# → HTTP/2 200
# → { forecast_id, actual_outcome: true, brier_score: 0.xxxx,
#     status: "resolved", resolved_by: "mo@…", resolution_notes: … }
```

Leaderboard read:

```bash
curl -si -H "Authorization: Bearer $TOKEN" \
     "https://agent-bestiary.world/api/forecasts/leaderboard?limit=20"
# → HTTP/2 200 (was: 500 FLOAT4/FLOAT8 mismatch the moment the
#   materialized view had ≥1 entry with best_brier_score populated)
```

`my_stats` and `portfolio_stats` — same pattern, exercise via the
dashboard once Mo's had ≥1 resolved forecast.

Direct schema validation:

```sql
SELECT column_name, data_type
  FROM information_schema.columns
 WHERE table_schema = 'public'
   AND table_name   = 'fermi_leaderboard'
   AND column_name IN ('avg_brier_score','best_brier_score','worst_brier_score','brier_stddev');
-- All four → double precision.
```

## The three drift families, closed

| Release | Family | Site pattern |
|---|---|---|
| v0.10.15 | `agents.owner_id` (column never existed) | `eval_brier.rs` (2× SQL) |
| v0.10.16 | `agents.owner_id` (column never existed) | `fork.rs` (SELECT + INSERT) |
| v0.10.18 | `agents.updated_at` (column never existed, mig-166 adds) | `publish_pipeline.rs` (3×), `lifecycle.rs` (1×) |
| **v0.10.19** | **REAL vs f64** on numeric aggregates | `forecasts.rs` (4× SQL), `fermi_leaderboard` view (mig-167) |

All three families are exactly what the v0.11.0 trust contract is
designed to catch pre-deploy. Escalating it from "next release,
blocking on spec" to **"start immediately, spec inline."** Four
consecutive hotfixes shipping the same underlying invariant is
enough evidence.

## Related

- v0.10.15 — admin force-publish wired.
- v0.10.16 — creation-time bypasses closed.
- v0.10.17 — Activity panel (parallel, unrelated).
- v0.10.18 — `agents.updated_at` added (mig-166).
- v0.10.20 (candidate) — `abw-cli agents legacy-slugs`
  dry-run/apply for un-routable data still in the DB.
- v0.11.0 (elevated) — boot-time trust-contract schema check.
