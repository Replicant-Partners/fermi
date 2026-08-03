# v0.10.27 — `agents.updated_at`, actually this time (via ensure_critical_schema)

## Why

Ivan tried to force-publish Mario's `key_metrics` agent and hit the
exact same error we thought v0.10.18 had fixed:

```
Publish failed: 400 DB error: error returned from database:
column "updated_at" of relation "agents" does not exist
```

**mig-166 didn't apply to prod.** The migration wraps its
`ADD COLUMN` inside `DO $$ … $$` blocks (for the idempotent probe
+ RAISE NOTICE observability). PgBouncer in transaction mode can
split multi-statement DDL at dollar-quoted body boundaries — and
`sqlx::raw_sql` is a single `execute()` on a shared pool. Result:
mig-166 executed against a PgBouncer connection that ate the
`DO $$` block silently. The migration runner logged "Migration
completed" but the column was never added.

This is a **known failure mode** on this deploy — `api_server.rs`
even has a function called `ensure_critical_schema` with this exact
warning in its header:

> "Each ALTER is its own single-statement sqlx::query — bypasses any
> interaction between raw_sql, DO blocks, and PgBouncer in
> transaction mode that has eaten multi-statement DDL in the past."

mig-166 should have been landed via `ensure_critical_schema` from
the start. Fixing that now.

## Change

### `src/api_server.rs::ensure_critical_schema`

Adds two single-statement entries to the `alters` slice:

```rust
("agents.updated_at",
 "ALTER TABLE public.agents \
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()"),
("agents.updated_at.backfill",
 "UPDATE public.agents SET updated_at = created_at WHERE updated_at IS NULL"),
```

Each is a **single statement**, so PgBouncer can't split it at a
`DO $$` boundary. Runs after `run_migrations()` at every boot;
idempotent via `IF NOT EXISTS` on the ALTER and a `WHERE
updated_at IS NULL` gate on the backfill.

The `NOT NULL DEFAULT NOW()` on `ADD COLUMN` means:
- Existing rows get `updated_at = <migration time>` (PG ≥ 11) or
  `NULL` on older versions
- The follow-up `UPDATE ... WHERE updated_at IS NULL` catches the
  older-PG case and backfills from `created_at` (which every row
  has since mig-010).
- Future INSERTs that don't set `updated_at` get `NOW()`
  automatically.

mig-166 itself is not removed. On environments where it did apply
successfully (or on a fresh DB), it's a no-op idempotent probe.
On environments where PgBouncer ate it, `ensure_critical_schema`
now backstops.

## Post-deploy verification

```bash
# Column exists.
psql -c "\d public.agents" | grep updated_at
# → updated_at | timestamp with time zone | not null default now()

# Every row has it.
psql -c "SELECT COUNT(*) FROM public.agents WHERE updated_at IS NULL;"
# → 0

# Force-publish now succeeds — Mario's agents publishable end-to-end.
curl -si -X POST \
     -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/agents/key_metrics/publish?force=true&reason=post-v0.10.27"
# → HTTP/2 200 (was: 400 column does not exist)

# UI: catalogue → any of Mario's agents → Publish (as admin) →
# give reason → confirm → succeeds → agent appears in public catalogue.
```

## What Mario gets

After deploy, all ~34 of Mario's un-publishable agents can be
force-published via the admin panel (or `abw` CLI, once that
subcommand exists). The publish-checks are strict on purpose
(name/description/system_prompt/tags), so most will require a
`?force=true&reason=…` from Ivan — but that path is now unblocked.

The `Efrain AI` App itself has been public since day one (App
publishing goes through `handlers/apps.rs`, not the agent lifecycle
that hit this bug). The composition-workspace path already works.
This release unblocks the **individual agent** publish surface so
the catalogue shows Mario's work alongside the App.

## The three sightings, closed

- **v0.10.18** — mig-166 shipped, thought this was fixed.
- **v0.10.27** — Ivan re-hit same error, root cause = PgBouncer
  ate the DO $$ block. Fixed via `ensure_critical_schema` on a
  single-statement path.

Third time's the charm because `ensure_critical_schema` uses
single-statement `sqlx::query` per ALTER — PgBouncer can't split
one statement.

## Note on numbering

Both my fix and Ivan's parallel embedder fix were in the working
tree at the same time. Ivan's `d5901a88` (ci-console version-stamp
fix) accidentally swept up my `ensure_critical_schema` edits and
this file's earlier draft during commit, which is why the code is
already at HEAD on `origin/main` before this tag. Cleanest history:

- **v0.10.26** — Ivan's embedder fix (commit `03edd0d6`).
- **v0.10.27** — this release: `agents.updated_at` + the ci-console
  version-stamp fix (both landed in `d5901a88`).

## Follow-up (still elevated)

This is exactly the class of failure the v0.11.0 trust-contract
substrate is designed to catch. At every boot, walk every code
site that references `agents.<column>` and confirm the column
exists. If not, refuse to serve. The bug would have surfaced at
deploy time with a clear "column missing after migration" error
instead of at first user click.

**Migration authoring guideline** for the interim: any
production-critical column ADD/ALTER should ALSO be listed in
`ensure_critical_schema` as a single-statement ALTER. mig-166's
`DO $$` block was fine for auditing and observability, but the
actual `ADD COLUMN` should have been a bare single-statement
fallback. Consider adding this as a note in the migration
template.

## Related

- v0.10.18 — mig-166 first attempt (eaten by PgBouncer).
- v0.10.15 — the admin force-publish path that surfaced this bug.
- v0.10.25 — test-fixture cleanup (previous release).
- v0.10.26 — parallel embedder fix (unrelated but same push window).
- v0.11.0 — trust-contract boot check. **Fourth in a row of
  "single-statement schema drift" issues. Unambiguously the next
  release.**
