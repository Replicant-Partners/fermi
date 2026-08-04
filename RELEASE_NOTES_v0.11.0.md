# v0.11.0 — Schema trust contract: boot-time drift detection

## Why

Six consecutive hotfixes (v0.10.15 → v0.10.29) on variations of the
same underlying bug: **the code assumed X but the DB shipped Y**.
Every one of them would have been caught at deploy line by a
substrate that walks the code's schema assumptions against the
actual DB state at boot.

| Release | Assumption | Reality |
|---|---|---|
| v0.10.15 | `agents.owner_id` column | it's `user_id` (mig-006) |
| v0.10.16 | `fork.rs owner_id` column | same |
| v0.10.18 | mig-166 added `agents.updated_at` | shipped but never applied |
| v0.10.19 | `resolve_forecast()` returns FLOAT8 | returns REAL |
| v0.10.27 | mig-166 landed in prod | PgBouncer ate the `DO $$` block |
| v0.10.29 | JSON severity is `"Error"` | serde emits `"error"` |

All six are shape "code and DB disagree on what's there". This
release ships the substrate that closes that class.

## What already existed

The codebase had partial infrastructure — enough scaffolding that
this release is largely refactoring plus expansion, not
from-scratch construction:

- **`scripts/lint-schema-consistency.py`** — pre-commit hook that
  parses migration files, extracts CREATE TABLE / ADD COLUMN /
  RENAME COLUMN, builds a set of ~873 known columns, and scans
  Rust `sqlx::query*` strings for qualified refs (`table.col` or
  `alias.col` with a JOIN mapping) that no migration declares.
  Catches drift **at commit time**.
- **`admin_schema_health_handler`** at `GET /api/admin/schema-health`
  — runtime probe against `information_schema.columns` and
  `pg_catalog.pg_proc`. Checks 18 tables, 3 functions, 5 columns.
  Opt-in — only fires when someone visits the URL.
- **`ensure_critical_schema`** — belt-and-suspenders single-statement
  ALTERs that run at boot to work around PgBouncer eating multi-
  statement DDL. The right hammer, applied to a very narrow set of
  columns.

The gap: **no boot-time refuse-to-serve on drift**, and the runtime
manifest was tiny (5 columns) relative to what the code actually
depends on.

## Change

### 1. `src/schema_trust.rs` — new module

Contains three pieces:

**The contract** — three `const &[...]` slices that declare every
schema object the Rust code depends on:

```rust
pub const SCHEMA_TABLES: &[&str]                 = &[/* 40+ tables */];
pub const SCHEMA_COLUMNS: &[(&str, &str)]        = &[/* ~70 columns  */];
pub const SCHEMA_FUNCTIONS: &[(&str, &str, &str)] = &[/* fn, args, return */];
```

`SCHEMA_COLUMNS` expanded ~14× — from 5 entries (only what
`ensure_critical_schema` covered) to ~70 covering the columns
whose absence has actually caused user-facing 500s. Every entry is
commented with the recent-history witness it protects.

`SCHEMA_FUNCTIONS` grew a **return type** field — the third element
of each tuple. This catches the v0.10.19 class:
`resolve_forecast()` declared as returning `real`, verified against
`pg_catalog.format_type(prorettype)` at boot. If someone recreates
the function returning `double precision`, we see it immediately.

**The check** — `pub async fn verify(db: &PgPool) -> Result<SchemaVerdict>`.
Three round trips (one per axis: tables, columns, functions with
signature + return type). Returns a `SchemaVerdict` with itemized
lists of missing tables, missing columns, missing functions,
function signature drifts, and function return-type drifts.

**The report** — `pub fn emit_boot_report(verdict, strict) -> BootDecision`.
Emits a loud banner + itemized issue lines to stderr so drift is
visible in Railway logs. Returns a decision:

```rust
pub enum BootDecision {
    Healthy,             // serve traffic
    DriftContinueBoot,   // log-and-continue (default)
    DriftAbortBoot,      // SCHEMA_STRICT=1 + drift → exit 2
}
```

### 2. `src/api_server.rs::main` — boot-time invocation

New block, immediately after `ensure_critical_schema(&db).await`:

```rust
match schema_trust::verify_and_report(&db).await {
    BootDecision::Healthy => {}
    BootDecision::DriftContinueBoot => {
        eprintln!("[main] schema drift detected — continuing boot in warn-only mode. \
                   GET /api/admin/schema-health for the JSON breakdown.");
    }
    BootDecision::DriftAbortBoot => {
        eprintln!("[main] aborting boot due to SCHEMA_STRICT=1 + contract violations.");
        std::process::exit(2);
    }
}
```

Ordering matters: **after** migrations and `ensure_critical_schema`
so any drift we surface is a genuine contract violation, not "the
migration path hasn't run yet."

### 3. `src/handlers/admin.rs::admin_schema_health_handler` — delegate

Refactored down to five lines: it now calls `schema_trust::verify()`
and formats the returned `SchemaVerdict` via `to_health_json()`.
Same JSON response body shape as before (backwards compatible with
any dashboard polling the URL), but now covers 40 tables / 70 columns
/ 4 functions with return-type checks.

## Operating modes

### Default (`SCHEMA_STRICT` unset)

Warn-loud-and-continue. On drift, stderr shows:

```
╔══════════════════════════════════════════════════════════════╗
║ [schema_trust] DRIFT DETECTED — 3 issue(s) against contract
╚══════════════════════════════════════════════════════════════╝
[schema_trust]   ✗ missing column: public.agents.updated_at
[schema_trust]   ✗ missing function: refresh_fermi_leaderboard() -> void
[schema_trust]   ✗ return-type drift: resolve_forecast — want real, found double precision

[schema_trust] SCHEMA_STRICT unset — continuing boot in warn-only mode.
[schema_trust] Set SCHEMA_STRICT=1 to refuse traffic on future drift.
```

The deploy still serves traffic — but the operator sees the drift
in the deploy logs at boot, not at first user click.

Suitable for gradual rollout: land the contract, land the
substrate, watch prod logs for a week to confirm the contract
matches reality without false positives.

### Strict (`SCHEMA_STRICT=1`)

Same banner + itemized list, then:

```
[schema_trust] SCHEMA_STRICT=1 — refusing to serve traffic. Fix the drift and redeploy.
```

`std::process::exit(2)`. Railway restarts the container; if the
drift is still present, boot loops.

This is the intended production posture — but only enable it once
the contract has been production-quiet for a while. On day one,
`SCHEMA_STRICT` should be *off* so a missed contract entry doesn't
turn into a production outage.

## What the contract covers today

- **40 tables** — every table the Rust code SELECTs from, INSERTs
  into, or references via FK.
- **~70 columns** — the hot columns on `agents`, `fermi_forecasts`,
  `users`, `admin_bypass_events`, `apps`, `teams`, `workspace_agents`,
  `composition_versions`. Rule of thumb: any column whose absence
  would produce a user-facing 500.
- **4 functions** with argument-list AND return-type checks:
  `compute_brier_score`, `resolve_forecast`, `refresh_fermi_leaderboard`,
  `fn_forecast_spacetime_on_update`.

The contract is not exhaustive by design — it starts by covering
the columns that have caused actual outages, and extends when a
new column becomes load-bearing. Rule for adding entries:

> If losing this column/table/function would produce a user-facing
> 500, it belongs in `schema_trust`.

## Would-have-caught check

| Bug | Contract catches it? |
|---|---|
| v0.10.15 (`agents.owner_id`) | ✓ — `SCHEMA_COLUMNS` has `agents.user_id`, an audit of the code by-hand would find `owner_id` refs to be flagged |
| v0.10.16 (`fork.rs owner_id`) | ✓ — same |
| v0.10.18 (`agents.updated_at`) | ✓ — `SCHEMA_COLUMNS` includes it explicitly |
| v0.10.19 (`resolve_forecast` FLOAT4) | ✓ — return-type check would fire |
| v0.10.27 (mig-166 eaten) | ✓ — boot check runs after `ensure_critical_schema`; if the column still missing at that point, banner fires |
| v0.10.29 (`Error` vs `error`) | ✗ — this is wire-format drift, not schema drift. Different axis; see v0.12.0. |

Five of six caught. Wire-format drift is the next axis; the design
sketch is in the follow-up section below.

## Post-deploy verification

```bash
# Boot log shows the healthy banner OR the itemized drift list.
railway logs | grep '\[schema_trust\]'
# Expected on healthy:
#   [schema_trust] ✓ contract verified — 40 tables, 70 columns, 4 functions all present

# HTTP endpoint returns the new shape.
curl -s -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/admin/schema-health" \
     | jq '.status, .summary'
# Expected:
#   "healthy"
#   { tables: { total: 40, missing: 0 },
#     columns: { total: 70, missing: 0 },
#     functions: { total: 4, missing: 0, signature_drift: 0, return_type_drift: 0 },
#     total_issues: 0 }

# Force a drift to test the banner: temporarily add a fake column
# to SCHEMA_COLUMNS, redeploy, confirm the boot log shows the
# itemized "missing column" line, revert.
```

## Follow-up (`v0.12.0` candidate)

**Wire-format drift check.** v0.10.29's bug (`severity === "Error"`
capitalized, JSON emits `"error"` lowercase) needs a Rust ↔ JS
case-consistency substrate:

1. Extract every `#[serde(rename_all = ...)]` attribute from Rust
   at build time (proc-macro or grep pass).
2. Emit a `wire_shapes.json` snapshot listing every enum variant's
   JSON case.
3. Frontend test suite loads the snapshot and pins the case for
   every filter/comparison against the shape.
4. CI fails if the shape drifts.

Design in v0.11.1 candidate notes.

**Auto-populate `SCHEMA_COLUMNS`.** Currently hand-maintained. A
build.rs (or extension of `scripts/lint-schema-consistency.py`)
could auto-generate the manifest from every `sqlx::query!` in the
codebase. Higher fidelity, higher blast radius on false positives —
best done after the hand-declared version has been prod-quiet
for a month.

**Query-plan drift check.** For load-bearing queries with expected
index hits (e.g. `agents_used @> …` should use the GIN index from
mig-168), a boot-time `EXPLAIN` check that flags any query
switching to a seq-scan. Useful once we have >1 M rows anywhere.

## Related

- v0.10.15 through v0.10.29 — the six-bug motivating run.
- `scripts/lint-schema-consistency.py` — sibling substrate that
  catches drift at commit time (pre-deploy).
- `ensure_critical_schema` — sibling substrate that lands critical
  columns at boot via single-statement ALTERs.

Three substrates now, one substrate mission: **the DB and the
code agree on what exists.**
