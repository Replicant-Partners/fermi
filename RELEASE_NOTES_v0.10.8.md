# v0.10.8 — Owner-column TEXT/UUID parity

Hotfix. v0.10.7 shipped the view-independent orphans endpoint and it
returned real data immediately:

```json
{
  "total_orphans": 25,
  "by_resource": {
    "agents": 0, "apps": 3, "creatures": 20, "teams": 2, ...
  },
  "skipped_resources": [
    "fermi_forecasts", "fermi_portfolios",
    "fermi_notebooks", "ar_beacons"
  ]
}
```

Four resources landed in `skipped_resources`. The pattern named
itself: those four tables are the ones documented in v0.9.1's
release notes as *"a compatibility artifact for a UUID-drifted
deployment"* — their `owner_id` / `creator_id` columns are stored
as **UUID** on this deploy, not `TEXT` (which is what
`migrations/094_fermi_forecasting.sql` declares).

## The bug

The orphans query on those tables was doing:

```sql
WHERE t.owner_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = t.owner_id)
```

`u.user_id` is `TEXT`, `t.owner_id` is `UUID`. Postgres refuses
`text = uuid` at parse time. v0.10.7's per-target error handler
caught the exception, logged a warning, and skipped the resource.
Working as designed — the substrate exposed the drift — but it
means we couldn't see Ilabra's Sunderland forecast, which is
exactly the drift-victim we're trying to unblock.

Same class of bug lurked in the reassign UPDATE (`SET owner_col =
$2` fails when the column is UUID and `$2` is a text user_id) and
the heal UPDATE (`SET t.col = u.user_id` — TEXT into UUID column
without cast).

## The fix

**Cast `t.{owner}::text` on both sides of every comparison, and
detect the target column's type via `information_schema` before
writes so the SET side gets the right cast.**

Three changes in `src/handlers/admin_rbac.rs`:

1. **Orphans query** — `fetch_orphans_for_target` now does
   `u.user_id = t.{owner}::text` in the WHERE. Works whether the
   owner column is TEXT or UUID.

2. **Reassign UPDATE** — new helper `owner_column_type(pool, table,
   col) → String` queries `information_schema.columns` once per
   request to detect the owner column's type. The composed SQL
   picks the right cast:

   ```rust
   let owner_type = owner_column_type(&state.db, table, owner_col).await;
   let owner_cast = if owner_type == "uuid" { "::uuid" } else { "" };
   let sql = format!(
       "UPDATE public.{} SET {} = $2{} WHERE {}::text = $1 …",
       table, owner_col, owner_cast, pk_col,
   );
   ```

3. **Heal UPDATE** — same `owner_column_type` lookup drives the
   `SET t.col = (u.user_id){cast}` clause. All comparisons switch
   to `t.col::text` for parity.

## Why "detect at runtime" instead of hardcoding

I could hardcode `owner_col_type: "uuid"` into each `AuditTarget`
entry that's known to be drifted. Rejected because:

- It couples the enum to a specific deploy's schema drift. A fresh
  deploy from `migrations/094` would have TEXT columns, and the
  hardcoded UUID would break.
- Adding a new tenant table means guessing its type at the
  migration author's desk instead of learning it at runtime.
- The extra query is one round-trip per endpoint call, well under
  1ms, and cacheable if it ever becomes hot.

Runtime detection Just Works across whatever mix of TEXT-native and
UUID-drifted deployments the platform lives on.

## Expected post-deploy behaviour

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
     https://agent-bestiary.world/api/admin/rbac/orphans \
     | jq '{total_orphans, by_resource, skipped_resources}'
```

Expected shape:

```json
{
  "total_orphans": <N + previously-skipped>,
  "by_resource": {
    ..., "fermi_forecasts": <n>, "fermi_portfolios": <n>,
    "fermi_notebooks": <n>, "ar_beacons": <n>
  },
  "skipped_resources": []   // ← empty; all 14 targets queried successfully
}
```

If `fermi_forecasts` has orphans, drill:

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
     "https://agent-bestiary.world/api/admin/rbac/orphans?resource=fermi_forecasts" \
     | jq '.orphans[] | select(.label | test("Sunderland"; "i"))'
```

That should surface Ilabra's Sunderland row with its current
`owner_ref`. Then:

```bash
curl -X POST -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"resource":"forecast","row_ids":["<sunderland_row_id>"],"new_owner_user_id":"<ilabras_actual_user_id>"}' \
     https://agent-bestiary.world/api/admin/rbac/reassign
```

Now that reassign handles UUID owner columns too, that call goes
through instead of silently failing.

## Files

- `src/handlers/admin_rbac.rs` — one WHERE cast, one new helper
  (`owner_column_type`), two write-path cast substitutions. ~40
  LOC net.
- `crates/fermi-console/Cargo.toml` — 0.10.7 → 0.10.8.
- `RELEASE_NOTES_v0.10.8.md` — this file.

## Compatibility

- No schema changes. No new migration.
- Response shape unchanged (still v0.10.7's superset with
  `skipped_resources`).
- Existing TEXT-owner tables (agents, teams, creatures, etc.)
  unaffected — `owner_column_type` returns `"text"` for them and
  the cast is a no-op empty string.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release --bin api-server` — clean.
- `cargo test -p fermi-auth --lib` — 18 passed.
- `cargo test --bin api-server` — 31 passed.

## Related: what to check next

Beyond the numeric confirmation:

- **20 orphaned creatures + 2 orphaned teams + 3 orphaned apps**
  are already visible. Once the deploy is verified, run
  `POST /api/admin/rbac/heal` (dry-run first) to see how many of
  these are the `owner_col = '' → NULL` or
  `owner_col = users.id::text → users.user_id` shapes. Those get
  fixed for free.
- **Anything remaining after heal** needs manual `reassign` calls.
  For each row, we need the intended owner's `user_id` — usually
  answered by "which email created this?" in whatever tenant
  UI/records still remember.
