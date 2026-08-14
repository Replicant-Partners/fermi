# CI has never been green: the migration ratchet was born tripped

**Status:** RESOLVED — CI green at `1b818ee5`, [run 31749678602][run]
**Affects:** `main` 2026-08-07 → 2026-08-13 (`CI / Test Suite`)
**Reproduce:** `./scripts/migration-baseline-probe.sh`

[run]: https://github.com/Replicant-Partners/fermi/actions/runs/31749678602

First green `CI` run on `main` in the queryable history — the previous 100
runs, back to 2026-07-27, all failed. `Test Suite` went from stopping at
step 9 of 25 (1m09s) to completing all 25 (16m20s):

```
✓ Set up database                    ✓ Run agent-bestiary-memory unit tests
✓ Lint — env-sourced credentials     ✓ Run API integration tests
✓ Lint — SQL column refs             ✓ Schema trust contract — hygiene
✓ Lint — user-reference columns FK   ✓ Schema trust contract — live (advisory)
✓ Lint — agent taxonomy              ✓ Rollup contract — tripwire
✓ Check all binaries compile         ✓ Rollup contract — live (advisory)
✓ Run unit tests (non-DB crates)     ✓ Run api-server binary unit tests
```

Four stale assertions were hiding behind the tripped ratchet, none of them a
production defect and all four introduced after 2026-08-07 — entirely inside
the window where the gate could not report. See "What the skipped steps were
hiding" below.

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
weeks ran for the first time — and four assertions were failing. Every one
was a deliberate, documented behaviour change whose test was never updated;
none was a production defect. Two of the four demanded the return of a bug
that had been deliberately fixed:

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

3. **`agent-bestiary-memory` — embeddings without provenance.**
   `test_vector_similarity_search` and `test_consolidation_workflow` both
   wrote episodes via `store_episode`, which is `#[deprecated]` precisely
   because it "writes NULL provenance even when an embedding is present" —
   and migration 136 added `episodes_embedding_has_provenance` to make that
   row illegal. Moved to `store_episode_with_provenance`.

4. **`test_consolidation_workflow` demanded the data-loss bug back.**
   Underneath (3): it asserted a run leaves zero unconsolidated episodes.
   But the worker is built with `ConsolidationWorker::new`, so it has no
   extraction model, and step 7 of `consolidate_agent` deliberately does
   *not* consume the episodes in that case — a gate added because marking
   them anyway "turned a recoverable outage into permanent data loss": 62
   agents, 1,035 episodes consumed, empty ontology, nothing eligible for
   retry. The assertion now checks the guard, making it a regression test
   for the fix rather than a demand for its reversal.

None was detectable while CI stopped at the database step. All were
introduced after 2026-08-07, i.e. entirely within the window where the gate
could not report. That is the cost of a permanently-red check.

Run them yourself:

```bash
./scripts/ci-skipped-steps.sh            # the eight needing no database

eval "$(./scripts/local-test-db.sh start)"   # CI-equivalent database
cargo test --lib -p agent-bestiary-memory -- --test-threads=1
cargo test --test api_tests -- --test-threads=1
./scripts/local-test-db.sh stop
```

Note `schema_trust_contract` must be run **without** `DATABASE_URL` to match
CI's blocking "hygiene" step; its live tier self-skips when the variable is
unset and is `continue-on-error` when it is set. Exporting the variable makes
it look like a regression when it is the documented advisory tier.

### Then the schema itself: 26 → 6, and the trust contract is satisfied

Follow-up work in `81dac4f2` fixed the rebuild rather than just the signal.

| | before | after |
|---|---|---|
| from-empty failures (pass 1) | **26** | **6** |
| from-empty failures (pass 2) | 15 | 7 |
| `BASELINE` in `ci.yml` | 26 | **6** |
| schema-trust live contract | **13 issues** — 6 missing tables, 4 missing columns, 3 missing functions | **satisfied** |

`live_contract_is_satisfied_by_a_migrated_database` now passes. Per
`ci.yml`'s own note, that is *"the point at which `SCHEMA_STRICT=1` becomes
safe to enable in production"* — so that decision, and flipping the live
tier from `continue-on-error` to blocking, are now open rather than blocked.
The note ties the flip to the baseline reaching 0; the contract being
satisfied at 6 suggests those are separate thresholds.

(If you see `DRIFT DETECTED — 1 issue(s)` in that step's log, it is the
`public.nope` negative control from `b51d5909` proving the detector can fail,
not real drift.)

**Every cause was one shape**: `IF NOT EXISTS` silently skipping a schema
change, with `run_migrations` swallowing whatever failed next. Four
instances, and the third is worth eight migrations on its own:

1. **004 edited in place after being applied** — nothing creates `users.id`,
   `name`, `password_hash` or `password_salt`, yet fermi-auth JOINs on `id`
   and 005 declares an FK to it.
2. **`users_auth_provider_check` omits `'legacy'`** — 004b tries to widen it
   by re-declaring the column with `ADD COLUMN IF NOT EXISTS`, a no-op once
   the column exists. A CHECK cannot be widened by adding a column that is
   already there.
3. **048 creates `fermi_forecasts` with 13 columns; 094 does
   `CREATE TABLE IF NOT EXISTS` declaring 28**, so the statement is skipped
   and its 15 extra columns never appear. 094 aborts on an index over the
   `status` column it believes it created, never reaches its own
   `fermi_forecast_updates`, and takes 140/149/150/156/174/176 with it — plus
   175, because 140 is what creates `forecast_spacetime`. **Eight migrations
   from one skipped statement.**
4. **`migrations_log`** is written by 089 and 090 and created by nobody, so
   090 aborts on its final bookkeeping line having already built its schema
   correctly — the most misleading failure in the set.

`195_declare_the_ghost_schema.sql` and `196_reconcile_fermi_forecasts.sql`
declare those objects. Additive, guarded, no-ops against production, and
**positioned rather than sorted**: 195 immediately after 004 because
004b/005/161/165/171 depend on it, 196 between 048 and 094. Numbered high so
the directory reads honestly about when they were written.

Five files were edited **in place**, which is normally forbidden here —
justified because each contained SQL that has never executed anywhere, so
there is no applied state to diverge from:

| file | defect |
|---|---|
| 113 | `\|\|` inside `COMMENT ON ... IS`, which takes a literal. Three of them, one on `composition_versions` itself — which is why 120 failed too |
| 097 | `NOW()` in a partial index predicate. Illegal, and wrong even if allowed: the predicate freezes at build time, so rows silently leave the index as cooldowns elapse |
| 142 | `CREATE INDEX CONCURRENTLY` inside `DO` blocks, four times. CONCURRENTLY cannot run in a function body |
| 166 | a plpgsql local named `is_nullable` colliding with the same-named `information_schema` column, selected unqualified |
| 091 | `ON CONFLICT ON CONSTRAINT` naming a partial unique *index*. That form takes a constraint, and a partial unique constraint cannot exist — so its backfill had never run |

### The remaining 6

None is a one-liner, and one should not be "fixed" at all:

- **006, 007** — ordering. They `ALTER` `agents`/`episodes`, which the runner
  creates later in the list. Fixing means reordering, which deserves its own
  change.
- **089, 095** — PostGIS is absent from the `pgvector` service image. Either
  add the extension or guard the two files. Possibly an environment artefact
  rather than a defect.
- **163** — needs `ar_beacons.location_name`, which 089 adds.
- **096** — indexes `activity_events(creature_id)`. That column exists in no
  migration and nothing writes it; the table has `actor_creature_id` and
  `target_creature_id`. Declaring `creature_id` would manufacture exactly the
  write-orphaned column that the rollup tripwire exists to catch, so this
  wants an owner's decision about which column was meant, not a guess.

Pass 2's seven are a different problem: pre-existing non-idempotency (bare
`CREATE INDEX` / `CREATE TRIGGER` / `ADD CONSTRAINT` in 004, 008, 091, 143)
which `run_migrations` hits on every boot and swallows. CI measures a single
pass, so they are invisible there. `PASSES=2` on the probe surfaces them.

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
- [x] A green CI run exists on `main` — [31749678602][run] at `1b818ee5`.
- [ ] `Security Audit` — still red, separately. Does not fail the run:
      that job carries `continue-on-error: true` (ci.yml:395), which is
      why the run above is green despite it. See below.
