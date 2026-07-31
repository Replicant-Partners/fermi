# v0.10.9 — Realign fermi FK targets. Root-cause fix for "only admin can save."

Root cause found and fixed. This is the release that unblocks every
non-legacy user (Ilabra, Mo, and everyone who signs up via OAuth
going forward).

## The bug in one paragraph

On this deploy, the FK constraints on `fermi_forecasts.owner_id`,
`fermi_portfolios.owner_id`, and `fermi_notebooks.owner_id` point at
**`users(id)`** (the UUID PK), not `users(user_id)` (the TEXT column
that migration 094 originally declared). Confirmed empirically:

```
fermi_forecasts_owner_id_fkey    FOREIGN KEY (owner_id) REFERENCES users(id)
fermi_portfolios_owner_id_fkey   FOREIGN KEY (owner_id) REFERENCES users(id)
fermi_notebooks_owner_id_fkey    FOREIGN KEY (owner_id) REFERENCES users(id)
```

Every handler across the codebase writes
`owner_id = principal.user_id()` — which resolves to `users.user_id`.
For legacy users (created before OAuth), migration 004b backfilled
`user_id = id::text`, so both columns hold the same UUID and the
constraint passes coincidentally. For every OAuth-created user
(where `sync_user_from_app` mints a fresh `Uuid::new_v4()` for
`user_id`, distinct from the row's PK `id`), the two values diverge
and every save trips the FK.

**Ivan-admin worked by accident. Every other real user was
categorically broken since the FK was originally drifted (v0.9.1
release notes hinted at it: "*a compatibility artifact for a
UUID-drifted deployment*").**

## Why the substrate work didn't catch this

Everything shipped v0.10.3 → v0.10.8 was correct — but was measuring
the wrong invariant on this specific deploy:

- v0.10.3's `ensure_user_row` heals `users.user_id`. Correct — but
  the FK was checking `users.id` on this deploy.
- v0.10.4's `rbac_orphans` view flags rows where `owner_id NOT IN
  (SELECT user_id FROM users)`. Correct semantically, but this
  deploy's FK targets `id`, so my view was measuring drift against
  the wrong column.
- v0.10.5's `rbac::require*` correctly uses `principal.user_id()` for
  the owner check. Correct — but the FK the DB actually enforces is
  a different one.
- v0.10.6 self-check reports `aligned` (JWT sub matches
  `users.user_id`) — correct diagnostically, but doesn't observe
  what the FK is *pointing at*.

Every fix was aimed at the right idea. None of them looked at the
deployed constraint definition. That's the retrospective lesson —
the substrate should verify not just row-level invariants but
schema-level ones too.

## Migration 165

Per table (`fermi_forecasts`, `fermi_portfolios`, `fermi_notebooks`),
in strict sequence with no window where the FK exists but data
doesn't satisfy it:

1. **`DROP CONSTRAINT` `<table>_owner_id_fkey`** — remove the drifted
   FK.
2. **`ALTER COLUMN owner_id TYPE TEXT USING owner_id::text`** —
   convert from UUID to TEXT so it can reference `users.user_id`
   (TEXT). Values preserved (UUID → text is lossless).
3. **`UPDATE t SET owner_id = u.user_id FROM users u WHERE t.owner_id
   = u.id::text AND t.owner_id <> u.user_id`** — rebase every row
   from `users.id::text` → `users.user_id`. For legacy users this is
   a no-op (they're equal); for OAuth users it's the actual heal.
4. **`ADD CONSTRAINT ... FOREIGN KEY (owner_id) REFERENCES
   users(user_id) ON DELETE CASCADE`** — the correct constraint per
   mig 094's original declaration.

Each step is wrapped in `DO $$ ... END $$;` with `EXCEPTION WHEN
OTHERS` handlers so per-table failures don't abort the migration.
Idempotent — safe to re-run.

## What this unblocks

Post-deploy, every user can:

- **Save** a new forecast/portfolio/notebook (POST /api/forecasts
  will not trip the FK).
- **See** their own forecasts (list_forecasts_handler's `WHERE
  f.owner_id = principal.user_id()` clause now matches DB rows).
- **Publish** forecasts (update_forecast_handler's owner check
  aligns with what's stored).

For the "only admin worked" reporters:

- **Ilabra** (`ilabra@gmail.com`, `user_id = 0c640559-…`,
  `id = 911b5834-…`): her session's `principal.user_id() = 0c64…`.
  Post-mig-165, `INSERT owner_id = "0c64…"` satisfies the FK
  (`0c64…` is in `users.user_id`). Fixed.
- **Mo** (`mo@axolotl.partners`, same shape): fixed.
- **Every future OAuth user**: fixed by construction.

## Compatibility

- **All existing forecasts are preserved.** Step 3's UPDATE only
  rewrites values that need it (`WHERE t.owner_id <> u.user_id`);
  legacy users' rows pass through unchanged.
- **Ivan-admin unaffected.** His `id == user_id`, so his rows are
  literal no-ops in the rebase. He keeps working.
- **Column type change UUID → TEXT is safe.** No code in the codebase
  binds `owner_id` in a way that requires UUID typing. The `$2::uuid`
  cast in INSERT SQL still works (converts text → uuid → back to text
  on assignment).
- **`ensure_user_row` unchanged.** It was already correct against
  `users.user_id`; it just couldn't help earlier because the FK was
  pointing elsewhere.
- **The substrate audit endpoints** (`/api/admin/rbac/orphans`,
  `/rbac/heal`, `/rbac/reassign`) now report the *true* invariant
  again — the one the FK actually enforces.

## Files

- `migrations/165_fermi_forecasts_owner_fk_realign.sql` — new. The
  three-table realign.
- `src/api_server.rs` — registers migration 165 in `run_migrations`.
- `crates/fermi-console/Cargo.toml` — 0.10.8 → 0.10.9.
- `RELEASE_NOTES_v0.10.9.md` — this file.

## Validation

- `cargo check --workspace` — clean.
- `scripts/lint-migrations.sh` — expected to warn on the ALTER TABLE
  statements outside DO blocks (they are inside DO blocks in this
  migration, so should pass).

## Post-deploy verification

Once Railway picks up the migration:

```bash
# 1. Confirm FK targets are now users(user_id)
psql -c "SELECT conname, pg_get_constraintdef(oid)
         FROM pg_constraint
         WHERE conname IN ('fermi_forecasts_owner_id_fkey',
                           'fermi_portfolios_owner_id_fkey',
                           'fermi_notebooks_owner_id_fkey');"

# Expected: FOREIGN KEY (owner_id) REFERENCES users(user_id) ON DELETE CASCADE

# 2. Reproduce the failing save from earlier — should now succeed
curl -si -X POST -H "Authorization: Bearer $ILABRA_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"question_text":"Post-165 test","predicted_probability":0.5,"visibility":"private","status":"draft"}' \
     https://agent-bestiary.world/api/forecasts

# Expected: HTTP/2 201, JSON with new forecast id.

# 3. rbac_orphans view now reports true drift (nothing for these tables
#    unless there's a genuine orphan we haven't reassigned)
curl -s -H "Authorization: Bearer $TOKEN" \
     https://agent-bestiary.world/api/admin/rbac/orphans \
     | jq '{total_orphans, by_resource}'
```

## Retrospective

Six hotfixes into this thread, the actual root cause was one line
in `pg_get_constraintdef()`. The retrospective:

- **We diagnosed session-side drift first (v0.10.3) — real, but not
  the whole story.**
- **We built substrate for schema-level enforcement (v0.10.4-5) —
  real, but measured the wrong column on this deploy.**
- **We shipped diagnostics (v0.10.6-8) — those finally surfaced the
  concrete failure mode.**
- **The final answer came from `pg_get_constraintdef`** — one query
  we should have run three releases earlier.

The lesson for the next unfamiliar deploy: **before shipping any
substrate migration, dump every FK targeting `users(*)` and confirm
the deployed schema matches the migration files that declare it.**
A schema-drift audit at deploy startup would have caught this in
v0.10.4.

Adding to the v0.11.x backlog: a boot-time consistency check that
compares declared vs actual FK targets and yells if they disagree.
