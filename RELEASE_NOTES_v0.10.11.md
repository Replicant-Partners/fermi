# v0.10.11 — users JOIN uses user_id after FK realign

Consequence of v0.10.9's `owner_id` type conversion. Mig 165 changed
`fermi_forecasts.owner_id` (and portfolio/notebook siblings) from UUID
to TEXT and rebased the values from `users.id::text` to
`users.user_id`. The FK constraint got fixed. **The `LEFT JOIN users`
clauses in read handlers didn't follow.**

## The bug

Four SQL queries in `src/handlers/forecasts.rs` still joined on
`u.id = f.owner_id`:

- `get_forecast_handler`  (line 536)
- `list_forecasts_handler` (line 761)
- `leaderboard_handler`   (line 2311)
- `public_forecasts_handler` (line 2496)

After mig 165:
- `f.owner_id` is TEXT holding `users.user_id` values.
- `u.id` is still UUID (the users PK — unchanged).
- Postgres refuses `uuid = text` at parse time → `operator does not
  exist: uuid = text`.

`update_forecast_handler` calls `get_forecast_handler` at the end to
return the updated row. So every PUT save 500'd with:

```
error returned from database: operator does not exist: uuid = text
```

Confirmed via curl:

```bash
curl -si -X PUT -H "Authorization: Bearer $MO_TOKEN" \
     -d '{"question_text":"…","predicted_probability":0.5, ...}' \
     https://agent-bestiary.world/api/forecasts/<id>
# → HTTP/2 500, error returned from database: operator does not exist: uuid = text
```

## The fix

`u.id = f.owner_id` → `u.user_id = f.owner_id` at all four sites.
Both TEXT, matching the values mig 165 rebased into place. One-line
sed, ~5 seconds of change.

The four handlers now render owner display names correctly again
after the type flip. `create_forecast_handler` and other
write-heavy paths were already fine — they don't JOIN on users.

## What broke, when, and why

Timeline:

- **Pre-v0.10.9:** `f.owner_id` (UUID) = `u.id` (UUID). JOINs worked.
  FK also targeted `u.id` (drifted from mig 094's declared target).
- **v0.10.9 mig 165:** realigned FK to `users(user_id)` per mig 094.
  Converted `f.owner_id` UUID → TEXT. Rebased values from
  `u.id::text` → `u.user_id`. **Correct for writes, correct for the
  FK, but the read-side JOIN was left on `u.id`.**
- **v0.10.10:** unrelated middleware fix (API keys on optional-auth
  routes).
- **v0.10.11:** JOINs realigned to `u.user_id`. Read path now agrees
  with write path.

## Files

- `src/handlers/forecasts.rs` — 4 JOIN clauses, `u.id` → `u.user_id`.
- `crates/fermi-console/Cargo.toml` — 0.10.10 → 0.10.11.
- `RELEASE_NOTES_v0.10.11.md` — this file.

## Compatibility

- **No schema changes. No migration.**
- **Additive semantics** — pre-mig-165 rows with `owner_id`
  containing `users.id::text` values were rebased by mig 165's step 3
  UPDATE, so the new JOIN condition matches the same set of rows.
- **No orphans introduced** — the FK enforces `owner_id ∈ users.user_id`.

## Validation

- `cargo check --workspace` — clean.
- Same curl that returned 500 before should now return 200:

  ```bash
  curl -si -X PUT -H "Authorization: Bearer $MO_TOKEN" \
       -H "Content-Type: application/json" \
       -d '{
         "question_text": "post-v0.10.11 test",
         "predicted_probability": 0.5,
         "simulation_results": {"mean":0.5,"median":0.5,"p5":0.1,"p95":0.9},
         "confidence_interval_low": 0.1,
         "confidence_interval_high": 0.9
       }' \
       https://agent-bestiary.world/api/forecasts/<any_existing_forecast_id>
  ```

  Expected: `HTTP/2 200`, updated JSON.

## The recurring lesson

We keep discovering that this deploy's schema has drift from what
migrations declared. mig 165 fixed the FK-target drift. This
release fixes the read-JOIN consequence. There may still be other
consequences we haven't hit yet — every handler that JOINs on
`users` and predates mig 165 assumes UUID-shaped owner_ids.

The v0.11.x work I keep flagging — boot-time schema-consistency
check — should also grep the code for `u.id = <text>.owner_id`
patterns (or the reverse) and warn. That's a lint layer above the
constraint check.

Adding to backlog. Not blocking on it.
