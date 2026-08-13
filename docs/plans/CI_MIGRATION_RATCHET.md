# CI has never been green: the migration ratchet was born tripped

**Status:** fixed — baseline met (26/26), all downstream steps pass locally
**Affects:** `main` 2026-08-07 → 2026-08-13 (`CI / Test Suite`)
**Reproduce:** `./scripts/migration-baseline-probe.sh`

## Resolution

Migration 181 now applies to an empty database, so the count is back to 26
and `BASELINE` needed no edit. Three declarations were missing — all the
same defect, all fixed in 181 using the idiom the file already established
for `users.id` ("a no-op in production and the thing that makes a rebuild
faithful"):

| declared | why it was missing |
|---|---|
| `users.password_hash` | referenced by 004b/171/181, created by none |
| `users.password_salt` | same |
| `users_auth_provider_check` admitting `'legacy'` | 004 creates the CHECK without it; 004b tries to widen it via `ADD COLUMN IF NOT EXISTS`, a no-op once the column exists, so the wider constraint never lands |

That third one is the same class again, one layer down: a CHECK constraint
cannot be widened by re-adding a column that is already there.

Both columns are declared `NOT NULL DEFAULT ''` and then have the default
dropped, so a rebuild matches production's audited "NOT NULL without
default" rather than acquiring a default production does not have.
Verified by `scripts/verify-users-shape.sh`, which also asserts 181 is
re-runnable (it executes on every boot) and that `abw-system` does not
duplicate across runs.

Side effect worth noting: **mig 171 now succeeds on the second boot**,
which is what 181's comment always claimed ("Restoring the identifier below
also repairs 171") but could not deliver while 181 aborted first. Second
pass drops from 26 failures to 15.

### What the skipped steps were hiding

With the ratchet clear, the steps that had been skipped on every push for
weeks ran for the first time — and two were genuinely failing:

1. **`evaluator-faithfulness` unit test `is_prefilter`.** The implementation
   was deliberately changed from `PreFilter` to `Dimensional`, with a
   rationale on `tier()`: a PreFilter short-circuits the whole registry
   below 0.5, which would stop CharacterEval and Sotopia from ever running
   on a low-grounding response. The assertion was never updated. Test
   corrected to match the documented decision and renamed
   `is_dimensional_not_a_prefilter`.

2. **Taxonomy derived-rank gate.** `weather_market_analyst` declared
   `order: Prognosticales` while its own `produces`
   (`trade_recommendation`, `position-sizing`, …) derives `Consiliales`.
   Fixed with the gate's own documented remedy,
   `scripts/taxonomy.py apply --derived`; it touched exactly that one field.

Neither was detectable while CI stopped at the database step. Both were
introduced after 2026-08-07, i.e. entirely within the window where the gate
could not report.

Run them yourself without a database:

```bash
./scripts/ci-skipped-steps.sh
```

### Still to do

The 26 remaining failures are untouched — this restored the signal, it did
not fix the schema. The four clusters below are the real work, and each one
lowers `BASELINE`.

## Symptom

Every push to `main` fails CI at `Test Suite → Set up database`, before a
single line of Rust is compiled:

```
Applying 195 migrations in runner order
Migrations applied with 27 failure(s)
::error::27 migrations fail to apply, baseline is 26.
A migration that previously applied cleanly now does not.
```

**100 of the last 100 CI runs on `main` failed** (2026-07-27 → 2026-08-13).
There is no green run in the queryable history.

## Root cause

`b51d5909 integrity: make the guards provably capable of failing` did two
things in one commit:

1. Froze the ratchet at `BASELINE=26`.
2. Added `migrations/181_integrity_reconciliation.sql`, which **itself fails
   to apply to a fresh database.**

The baseline was measured *before* the commit's own migration existed.
Bisected with `scripts/migration-baseline-probe.sh`:

| ref | failures | note |
|---|---|---|
| `b51d5909^` | **26** | exactly the frozen baseline |
| `b51d5909` | 27 + 1 missing file | 181 added; `180` referenced in `api_server.rs` before the file existed |
| `origin/main` today | **27** | identical failing set; 180 since added |

The failing set has not changed at all between `b51d5909` and today. Nobody
broke a migration. **The gate was unsatisfiable from the moment it was
committed**, and has been red on every push since.

Worth stating plainly, because it is the same failure mode the commit was
written to eliminate: a commit titled *"make the guards provably capable of
failing"* installed a guard that could only fail.

## Why 181 fails

```
psql:migrations/181_integrity_reconciliation.sql:78:
  ERROR: column "password_hash" of relation "users" does not exist
```

`users.password_hash` is referenced by three migrations and **created by
none**:

- `004b` — comment only: *"keeping existing password_hash for backward
  compatibility"*, i.e. it assumes a table shape no migration in the repo
  produces.
- `171` line 48 — `INSERT INTO users (user_id, email, password_hash, …)`
- `181` line 78 — same INSERT, same failure.

So 181 inherited 171's defect. Both are instances of the exact class
`b51d5909`'s own message describes: *"nothing in the repo creates
users.id"*. 181 was written to repair the mig-171/004b identity corruption
and reproduces its root cause.

## The fix is not to raise the baseline

`ci.yml` says, correctly: *"When you fix migrations, LOWER this number.
Never raise it."* Raising it to 27 would make CI green while cementing the
thing that is actually wrong, and would burn the one number that tells us
whether the schema is recoverable.

Two defensible options, in order of preference:

1. **Make 181 apply on a fresh database.** Guard the `users` INSERT on the
   columns that actually exist (or on `to_regclass`/`information_schema`
   presence, the pattern several later migrations already use). Baseline
   returns to 26 with no edit to `ci.yml`, and CI goes green. Smallest
   change that restores the signal.
2. **Create `users` properly**, giving `password_hash` and `id` a migration
   that makes them exist — which fixes 171, 181, 004b, 161 and 165 together
   and lowers the baseline by ~5. This is Phase 2 of
   `docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md` and is the real fix.

Option 1 unblocks; option 2 resolves. They are not alternatives, just an
order.

## Why this matters more than one red build

The ratchet exists to catch a *newly* broken migration. While it is
tripped, it cannot: any migration broken from here on adds 28, 29, 30 to a
number that was already failing, and the message is identical every time.
The check has been converted from a tripwire into noise, and 100
consecutive red runs is long enough that the red is now the expected state
— which is precisely the *"failure mode that teaches people to ignore a
check"* this codebase has named in at least two commit messages.

It also means the other `Test Suite` gates never execute. These are
**skipped on every push**, not passing:

- Lint — no env-sourced LLM provider credentials
- Lint — SQL column refs resolve to a migration
- Lint — user-reference columns FK to `users(user_id)`
- Lint — agent taxonomy conformance
- Check all binaries compile
- the entire test suite

They pass locally and in the pre-commit hook, which is the only reason this
has not already caused a regression.

## The full failing set (27, `origin/main`)

Nothing here is recent; all are ≤181.

| migration | error |
|---|---|
| `004b_migrate_users_for_auth` | column "id" does not exist |
| `005_add_api_keys` | column "id" referenced in FK constraint does not exist |
| `006_add_user_id_to_agents` | relation "public.agents" does not exist |
| `007_add_user_id_to_memory` | relation "public.episodes" does not exist |
| `089_dashboard_spatial_queries` | PostGIS extension not found |
| `090_social_layer` | relation "migrations_log" does not exist |
| `091_swarm_participants` | constraint "idx_swarm_participants_unique_active" does not exist |
| `094_fermi_forecasting` | column "status" does not exist |
| `095_saved_locations` | type "geography" does not exist |
| `096_performance_indexes` | column "creature_id" does not exist |
| `097_governance` | functions in index predicate must be marked IMMUTABLE |
| `113_composition_as_first_class` | syntax error at or near "\|\|" |
| `120_composition_versions_rejection` | relation "public.composition_versions" does not exist |
| `140_forecast_benchmark` | relation "fermi_forecast_updates" does not exist |
| `142_performance_indices` | CREATE INDEX CONCURRENTLY cannot be executed from a function |
| `149_forecast_updates_trigger_kind` | relation "fermi_forecast_updates" does not exist |
| `150_forecast_relationships` | relation "public.fermi_forecast_updates" does not exist |
| `156_pending_cascades_extensions` | relation "public.fermi_forecast_updates" does not exist |
| `161_backfill_users_user_id` | column "id" does not exist |
| `163_rbac_orphans_view` | column "location_name" does not exist |
| `165_fermi_forecasts_owner_fk_realign` | column u.id does not exist |
| `166_agents_updated_at` | column reference "is_nullable" is ambiguous |
| `171_agent_credentials` | column "password_hash" of relation "users" does not exist |
| `174_fermi_forecasts_brier_integrity` | function compute_brier_score(real, boolean) does not exist |
| `175_forecast_spacetime_loop5_backfill` | relation "forecast_spacetime" does not exist |
| `176_collab_attribution` | relation "public.fermi_forecast_updates" does not exist |
| `181_integrity_reconciliation` | column "password_hash" of relation "users" does not exist |

Clusters worth noting, since they suggest four fixes rather than 27:

- **`fermi_forecast_updates` renamed or dropped** — 140, 149, 150, 156, 176.
- **`users` never created with `id` / `password_hash`** — 004b, 005, 161,
  165, 171, 181.
- **PostGIS absent from the CI image** — 089, 095. Either add PostGIS to the
  service container or guard those two; they may not be genuine defects.
- **Genuine SQL bugs** — 097 (non-IMMUTABLE predicate), 113 (syntax error),
  142 (`CONCURRENTLY` inside a function). These fail everywhere, including
  production's runner, meaning those objects have never been created by a
  migration.

## Also red, separately

`Security Audit` fails on every run: `RUSTSEC-2026-0097` (`rand` 0.8.5 and
0.9.2, unsound), plus yanked `keccak` 0.1.5 and `spin` 0.9.8. GitHub
reports 46 Dependabot advisories on the default branch (14 high, 21
moderate, 11 low). Separate issue, same consequence: two of five signals
are permanently red, so neither can report anything new.

## Tooling added with this report

`scripts/migration-baseline-probe.sh` — applies every migration in
`run_migrations()` order to a throwaway Postgres and **names** the ones that
fail. CI reports only a count, which is what made this take a bisect rather
than a glance.

```bash
./scripts/migration-baseline-probe.sh              # working tree
./scripts/migration-baseline-probe.sh <git-ref>    # any ref, via a temp worktree
ORDER=filename ./scripts/migration-baseline-probe.sh   # pre-b51d5909 loop order
```

Uses a detached worktree so it never touches uncommitted work, and removes
its container on exit.

## Acceptance

- [x] 181 applies cleanly to an empty database.
- [x] `Test Suite` reaches the lint and compile steps (all eight pass
      locally via `scripts/ci-skipped-steps.sh`).
- [x] `BASELINE` not raised — the count returned to 26, so `ci.yml` is
      unchanged and its comment still matches the constant beside it.
- [ ] A green CI run exists on `main`. *(Requires the DB-backed steps that
      cannot be checked locally: the API integration tests and the live
      schema-trust verification.)*
- [ ] `Security Audit` — still red, separately. See below.
