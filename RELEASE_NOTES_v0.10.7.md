# v0.10.7 — RBAC orphans: view-independent, deploy-resilient

Hotfix. On the deployed v0.10.5 backend,
`GET /api/admin/rbac/orphans` returned:

```
HTTP/2 500
error returned from database: relation "public.rbac_orphans" does not exist
```

Root cause: migration 163 (which defines the `rbac_orphans` view)
failed silently during deploy. The migration runner logs a warning
and continues on any error — a defensive posture that dates to
early ABW deploys — so downstream traffic hit an endpoint that
depends on a view that isn't there.

The specific failure inside mig 163 is a
column-reference-doesn't-exist-on-this-schema issue (Neon
schema-drift), which is exactly the class of problem the substrate
is *supposed* to expose, not be undone by.

## Fix

**Move the audit off the view and into the endpoint.**
`admin_rbac_orphans_handler` now:

1. Iterates a hard-coded `AUDIT_TARGETS` const array — 14 entries,
   one per resource, each carrying `(resource, table, pk_col,
   owner_col, label_expr, created_col)`.
2. For each target, composes and runs a one-table SELECT that finds
   rows whose owner reference isn't in `users.user_id`.
3. **On per-target error** (missing column, missing table,
   permission issue) — logs `tracing::warn!` with the resource
   name + underlying error, adds the resource to the response's
   `skipped_resources` list, and continues. Never fatal.
4. Aggregates + truncates to `limit`.

Response shape (unchanged from v0.10.4, plus one new field):

```json
{
  "total_orphans":     123,
  "returned":          123,
  "limit":             500,
  "by_resource":       { "agents": 1, "creatures": 2, ... },
  "skipped_resources": ["some_table_that_broke"],   // NEW
  "orphans":           [ ... ]
}
```

`skipped_resources` is the visibility fix — future schema drift on
one resource can never again silently zero out the whole audit.

## Why this is the right shape, not a workaround

The design intent of the substrate is that the audit is
**deploy-resilient**: whether or not a given tenant table exists on
a given deployment (Neon branches, feature flags, half-migrated
staging environments), the audit tells you what it can see. A view
is a fragile way to express that intent — one bad column reference
kills the whole thing. A per-target loop with graceful degradation
is what the intent actually requires.

The view (mig 163) is still there for anyone who wants to `SELECT
COUNT(*) FROM rbac_orphans` from psql / Grafana, but it's no longer
on the critical path. When mig 163 successfully rebuilds on a
future deploy, `SELECT ... FROM rbac_orphans` starts working again;
until then, `/api/admin/rbac/orphans` doesn't care.

## Deliberately NOT in this release

- **A fix for mig 163 itself.** The view definition should be
  reconciled against the actual deployed schema (identify which
  column reference broke), but that's a separate diagnostic task.
  The endpoint doesn't need the view to work.
- **A rebuild of mig 163 as separate per-table views.** Considered;
  rejected as complexity that doesn't add value now that the
  endpoint is view-independent.

## Files

- `src/handlers/admin_rbac.rs` — `admin_rbac_orphans_handler`
  rewritten around `AUDIT_TARGETS` + `fetch_orphans_for_target`.
  ~110 LOC added, ~40 removed. No other handlers changed.
- `crates/fermi-console/Cargo.toml` — 0.10.6 → 0.10.7.
- `RELEASE_NOTES_v0.10.7.md` — this file.

## Compatibility

- **No schema changes.** No new migration.
- **Response shape is a superset of v0.10.4's** — one new field
  (`skipped_resources`). Existing clients keep working.
- **Perf note:** 14 individual `SELECT`s instead of one UNION ALL.
  Each is bounded by `LIMIT $1` (max 1000 per resource); on a clean
  DB (no orphans), each returns zero rows. In production this
  should be well under 200ms round-trip. If it becomes a hot path,
  we add an in-memory cache with a 60-second TTL.

## Post-deploy check

```bash
TOKEN='<your admin jwt>'
curl -s -H "Authorization: Bearer $TOKEN" \
     https://agent-bestiary.world/api/admin/rbac/orphans \
     | jq '{total_orphans, by_resource, skipped_resources}'
```

Expected: `total_orphans` is a number (0 or more),
`by_resource` is a per-resource count, `skipped_resources` names
any tables the runner had to skip.

## Validation

- `cargo check --workspace` — clean.
- `cargo test --bin api-server` — 31 passed.
- `cargo test -p fermi-auth --lib` — 18 passed.

## What's next

- Once the endpoint returns real numbers, drill into whichever
  resource has orphans:
  ```bash
  curl -s -H "Authorization: Bearer $TOKEN" \
       "https://agent-bestiary.world/api/admin/rbac/orphans?resource=fermi_forecasts"
  ```
- For each orphan the operator can identify, use
  `POST /api/admin/rbac/reassign` to fix it.
- If Ilabra's Sunderland forecast appears here, we point that
  endpoint at her `user_id` and she's back in business.
