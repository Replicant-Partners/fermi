# v0.10.12 — owner_display_name COALESCE

Cosmetic follow-up to v0.10.11. Owner display names were rendering as
`null` on `fermi_forecasts` reads because the SELECT referenced
`u.name` (a pre-mig-004b legacy column that's `NULL` on
OAuth-created users) instead of `u.display_name` (added by mig 004b
and populated by `sync_user_from_app`).

## The fix

Four SELECTs in `src/handlers/forecasts.rs` now use:

```sql
COALESCE(u.display_name, u.name, u.email, u.user_id) AS owner_display_name
```

Falls through in preference order:

1. **`display_name`** — canonical, populated by every OIDC callback since v0.10.3.
2. **`name`** — legacy pre-OIDC column; kept as a fallback for
   accounts that predate the OAuth flow.
3. **`email`** — universally populated; fallback when display is
   unset.
4. **`user_id`** — the absolute last resort so the field is never null.

Sites updated:

- `get_forecast_handler`
- `list_forecasts_handler`
- `leaderboard_handler` (also expanded the `GROUP BY` to match)
- `public_forecasts_handler`

## Compatibility

- **No schema changes; no migration.**
- **No API surface change.** Same field name (`owner_display_name` /
  `display_name`), same JSON shape — just no more nulls where the
  data was actually present under a different column.

## Files

- `src/handlers/forecasts.rs` — 4 SELECT clauses + 1 GROUP BY.
- `crates/fermi-console/Cargo.toml` — 0.10.11 → 0.10.12.
- `RELEASE_NOTES_v0.10.12.md` — this file.

## Post-deploy verification

Same PUT that returned `"owner_display_name": null` before should
now show the actual display name:

```bash
curl -s -H "Authorization: Bearer $MO_TOKEN" \
     https://agent-bestiary.world/api/forecasts/<some_id> \
     | jq '.owner_display_name'
```

Expected: `"Mario Orellana"` (or whatever the row's user has as
display).

## Validation

- `cargo check --workspace` — clean.
