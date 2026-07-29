# Fermi Console v0.9.1 — `ensure_user_row` self-heals stale provisioning

Patch release. Fixes the provisioning race that left Mario
(mo@axolotl.partners) with a broken account state: forecast save
returning "your account isn't fully provisioned", portfolio create
returning a raw FK violation, and no OAuth loop that could actually
fix it.

The v0.9.0 executor rewiring stays as shipped — this release is
purely about the `users` table backfill path.

## Root cause

`ensure_user_row` (added in commit `70651ed`) guarded forecast + portfolio
writes against a missing `users(user_id)` row by INSERTing on the fly.
Its ON CONFLICT clause targeted `user_id` — which meant when a *stale*
row existed with the operator's **email** but a different (or NULL /
legacy) `user_id`, the INSERT hit `users.email` UNIQUE and failed.

The old code interpreted every INSERT failure as "not provisioned"
and told the operator to sign out and sign in — which cannot repair a
DB row that already exists. Mario looped through OAuth several times
with no effect, then got a raw FK error from `create_portfolio_handler`
(same underlying missing row, but no friendly rewrite).

Meanwhile team creation kept working because `teams.owner_id` is
plain TEXT with **no FK** (mig 009); it accepts any string, so
Mario's writes never touched the users constraint.

## The fix

`ensure_user_row` now has a two-phase backfill:

1. **INSERT** attempts the fresh-row path as before (unchanged when
   the users table has no row for this email).
2. **On email-UNIQUE conflict** — detected by inspecting the error
   string for `users_email` / `"unique" + "email"` — runs a healing
   UPDATE:

   ```sql
   UPDATE users
      SET user_id       = $1,
          auth_provider = COALESCE(NULLIF(auth_provider, ''), $2, auth_provider),
          display_name  = COALESCE(display_name, $3),
          last_login_at = NOW(),
          updated_at    = NOW()
    WHERE email = $4
      AND (user_id IS NULL
           OR user_id = ''
           OR auth_provider = 'legacy'
           OR auth_provider IS NULL)
   ```

   The `WHERE` guard is the safety net: we **only** re-parent rows
   that look legacy / orphaned. A row already provisioned under a
   different, live `user_id` is left alone, and the endpoint returns
   `409 CONFLICT` with:

   > *"Your email (…) is already registered under a different
   > account. Contact support to merge or use a different email."*

   That's an account-takeover refusal, not a bug — the operator
   should hear it named clearly.

3. **Only if the heal is inapplicable** do we return
   `PRECONDITION_FAILED` — and the message is honest now:
   *"Your account isn't fully provisioned and we couldn't auto-heal
   it. Please contact support."* No more "sign out and sign in"
   suggestion, because that path never helped and misled several
   operators.

## Why teams worked and portfolios/forecasts didn't

Three FK cardinalities, three behaviours:

| Table | Column | FK | Mario's outcome |
|---|---|---|---|
| `teams` | `owner_id TEXT` | none (mig 009) | Create works (no check) |
| `fermi_forecasts` | `owner_id TEXT` | `REFERENCES users(user_id)` (mig 094) | Blocked by `ensure_user_row`'s bad error |
| `fermi_portfolios` | `owner_id TEXT` | `REFERENCES users(user_id)` (mig 094) | Same as forecasts, or raw FK on older deploy |

The v0.9.1 heal fixes both fermi paths; teams are unchanged (no
FK, still accepts any string).

## Console changes

`friendly_backend_save_error` in `crates/fermi-console/src/cockpit.rs`
now rewrites two error classes correctly:

1. **Stale FK from an old server deploy** — no longer suggests OAuth
   loop. Says: *"this server is running an older version that can't
   auto-provision your account. Wait for the next deploy or contact
   support."*
2. **v0.9.1 CONFLICT on email taken** — passes through so the
   operator sees the specific "email already registered under a
   different account" message the server sent.
3. **"couldn't auto-heal"** — new phrase the server uses when the
   INSERT failed for a reason other than email conflict; the friendly
   rewriter now catches it and passes it through.

## Compatibility

- **No migration.** Zero schema changes.
- **Idempotent.** Any prior row that ended up correctly provisioned
  (matching `user_id`) hits the fast-path SELECT and returns Ok
  before touching the INSERT / heal path.
- **Backward compatible clients** still work — they just see the
  updated error messages when things go wrong.

## What Mario should do

1. Update his server to v0.9.1 (or whatever deploy carries this
   commit).
2. Try any write action from the console (save a forecast, create
   a portfolio).
3. If the write succeeds: his stale users row got healed. `mo@axolotl.partners`
   now has `user_id = <his current OAuth id>`. All future writes
   Just Work.
4. If he gets `409 CONFLICT: "email already registered under a
   different account"` — his email is on a fully-provisioned row
   with a different user_id (probably a test user, or a prior
   Google account). Contact support to merge or use a different
   email. This is deliberately unhealable at the handler layer.

## What this does NOT fix

- **Server deploy propagation.** If Mario's server isn't running
  this commit yet, none of the above helps — he'll keep seeing the
  old messages. Deploy this and the fix activates automatically.
- **Existing bad data outside Mario's row.** If other users are in
  the same stale state, they self-heal on their next write attempt
  after the deploy. No batch migration needed.
- **The v0.9.0 agent-owner API key work** is unchanged and unaffected.
- **The `$4::uuid` casts** in the fermi_portfolios / fermi_forecasts
  INSERTs — those looked suspicious during the investigation
  (schema says TEXT) but appear to be a compatibility artifact for
  a UUID-drifted deployment. Left alone; the heal fix operates one
  layer up.

## Files touched

- `src/handlers/forecasts.rs` — `ensure_user_row` gains the
  email-conflict heal path. Function visibility changed to
  `pub(crate)` so future write handlers in other modules can call it
  without re-implementing the guard.
- `crates/fermi-console/src/cockpit.rs` — `friendly_backend_save_error`
  updated to rewrite the new v0.9.1 error phrases and stop
  suggesting the useless "sign out and sign in" flow.
- `crates/fermi-console/Cargo.toml` — version bump.

## Validation

- `cargo check --workspace` — clean
- `cargo check --release --bin api-server` — clean
- 37 pre-existing shape tests pass (7 agent-secrets + 8 provenance
  + 6 propagate + 6 mutex-math + 10 timeline)
- **Manual repro test to run against a live DB before / after the
  fix:**
  1. Insert a `users` row with `email='test@example.com'`,
     `user_id=NULL`, `auth_provider='legacy'`.
  2. Have a fresh OAuth session for `test@example.com` with a new
     Zitadel/Google `user_id`.
  3. Hit `POST /api/forecasts` with valid body.
  4. Before v0.9.1: `PRECONDITION_FAILED` on the first attempt,
     forever.
  5. After v0.9.1: first attempt succeeds. The stale row now has
     `user_id = <new session id>` and `auth_provider = google` (or
     whichever provider). Subsequent writes go straight through.

## What's next

Back to the v0.9.2 track — the console UI for agent owners to
upload their API keys, and the follow-on tightening ("hard fail
when owner hasn't funded") from v0.9.0's deferred list.
