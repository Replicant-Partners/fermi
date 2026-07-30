# Fermi Console v0.10.3 — users.user_id alignment fix

Bug-fix release. Every account except the lucky INSERT-path original
(`ivan@axolotl.partners`) was hitting `foreign key violation` on
save/publish and `403 FORBIDDEN` on invite accept. Same underlying
mechanism, three cooperating fixes.

## Symptom

- **Save / publish** shows *"Backend save failed: this server is
  running an older version that can't auto-provision your account.
  Wait for the next deploy or contact support."* even on a fully
  up-to-date server.
- **Invite accept** returns `403 FORBIDDEN` with *"This invite was
  sent to a different user"* — even though the invite was sent to
  the email the caller is signed in with.
- **`ivan@axolotl.partners` (or whichever account first hit the
  INSERT branch) is unaffected** — its users row was set up
  cleanly.

## Root cause

`fermi-auth/src/oidc.rs::sync_user_from_app` had two branches:

- **INSERT branch** (brand-new email): explicit `Uuid::new_v4()` into
  `user_id`. Always clean.
- **UPDATE branch** (existing email matched by `google_id` OR `email`):
  updated every column except `user_id`. If that row's `user_id` was
  `NULL` or `''` (from a pre-mig-093 legacy row, a partial-provision,
  or an earlier code path that left it blank), it stayed broken.

The tail of the function minted the JWT `sub` claim from

```rust
record.user_id.clone().unwrap_or_else(|| record.id.to_string())
```

which has a nasty asymmetry: `Some("")` **skips** `unwrap_or_else`,
so the session carries an empty user_id string; `None` falls back to
`id::text`, a value that is *not* stored in the `user_id` column.

Downstream, every table with `owner_id REFERENCES users(user_id)`
(migration 094: `fermi_forecasts`, `fermi_portfolios`,
`fermi_notebooks`) tripped its FK on the first write.

`forecast_invites.invitee_user_id` was populated at claim time from
the same drifted `resolved_user_id`, so the invitee's *next* sign-in
minted a session with a different `sub`, and the strict user_id
check in `require_caller_is_invitee` locked them out permanently.

`ensure_user_row`'s v0.9.1 email-heal *would* have fixed most of
this, but its guard clause

```sql
WHERE ... AND (user_id IS NULL OR user_id = ''
               OR auth_provider = 'legacy' OR auth_provider IS NULL)
```

deliberately refuses to reparent rows that already have a live
`auth_provider` (e.g. `'google'`). For those rows the heal returned
`0 rows_affected` and cascaded to `PRECONDITION_FAILED` — but only
*if* `ensure_user_row` reached that branch at all. On the update
paths (`update_forecast_handler`, PUT-based autosave) it doesn't
even run, so the raw SQL FK error bubbled up.

## Fixes

### 1. `sync_user_from_app` backfills `user_id` in the UPDATE

`fermi-auth/src/oidc.rs`. The UPDATE clause now sets

```sql
user_id = COALESCE(NULLIF(user_id, ''), id::text)
```

alongside the existing columns. Idempotent — rows with a good
`user_id` are untouched. The `unwrap_or_else` fallback at the tail
also became a `match … { Some(s) if !s.is_empty() => …, _ => log! }`
so `Some("")` no longer silently produces drift; if it ever does,
the mismatch is logged loudly instead of being papered over.

### 2. Migration 161: backfill legacy rows

`migrations/161_backfill_users_user_id.sql` runs one-shot on the
next deploy:

```sql
UPDATE users SET user_id = id::text
WHERE user_id IS NULL OR user_id = '';

-- Normalise uppercase-UUID user_ids to lowercase so the
-- fermi_forecasts.owner_id::uuid round-trip cast produces a
-- value that FK-matches users.user_id.
UPDATE users SET user_id = LOWER(user_id)
WHERE user_id ~ '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-...'
  AND user_id <> LOWER(user_id);
```

Every legacy row heals; `users_user_id_unique` (mig 093)
guarantees `id::text` is a safe backfill target.

### 3. `require_caller_is_invitee` accepts email-match as fallback

`src/handlers/invites.rs`. When `invitee_user_id` is set but doesn't
match the caller's `user_id()`, we now check whether the caller's
email matches `invitee_email` (case-insensitively) and let them in
if it does. Mailbox ownership *is* the semantic invariant this
check exists to enforce — the strict `user_id` version was a
too-narrow implementation of it.

ApiKey callers still cannot use the fallback (they don't carry an
email claim), so this doesn't loosen anything in the machine-auth
path.

### 4. Console error rewriter no longer blames the deploy

`crates/fermi-console/src/cockpit.rs::friendly_backend_save_error`.
The old FK branch said *"this server is running an older version"*
whenever it saw `foreign key + (owner_id | users)` — which was a
lie any time the server actually was current but the users row was
drifted. The v0.10.3 branch says:

> *"Your users row and session don't line up (owner_id FK violation).
> If the server is on v0.10.3+ this should have been auto-healed at
> sign-in — sign out and back in, then retry."*

The v0.9.1 CONFLICT and PRECONDITION passthroughs still fire first,
so the informative server-side messages aren't clobbered.

## Compatibility

- **Migration 161 is idempotent.** Re-running the deploy re-executes
  the file; the UPDATE only touches NULL/'' rows.
- **`users_user_id_unique` (mig 093) is respected** — we backfill
  with `id::text` which is unique by PK.
- **No wire-format changes.** The console still POSTs / PUTs the same
  bodies; only the server's translation layer is different.
- **JWT-in-flight sessions** minted before this deploy carry the
  drifted `sub`. First write after the deploy hits `ensure_user_row`,
  which now heals via email-match (heal guard is unchanged, but
  migration 161 has already made the row heal-eligible by setting
  `user_id = id::text` — matching the drifted JWT sub exactly). No
  forced sign-out required.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release --bin api-server` — clean.
- Existing v0.9.1 tests continue to pass; new behavioural test not
  yet added (see "What's next" below).

## Manual verification

Against the deploy DB, run BEFORE and AFTER:

```sql
SELECT COUNT(*) FROM users WHERE user_id IS NULL OR user_id = '';
-- BEFORE: > 0 for affected deployments
-- AFTER: 0

SELECT id, user_id, email, auth_provider FROM users
WHERE email IN ('ilabra@gmail.com', 'mo@axolotl.partners');
-- AFTER: user_id = id::text; auth_provider unchanged.
```

Then from the affected account: forecast save/publish + invite
accept should succeed with no re-auth needed.

## What's next

- A shape test for `sync_user_from_app` UPDATE-branch that asserts
  `user_id` is non-empty on the returned `User` no matter what the
  input row looked like.
- Consider adding `/api/health` returning `{version: ...}` and
  displaying it beside the client version in the console footer so
  "up to date" isn't a half-truth.
- Consider dropping the `::uuid` cast in `fermi_forecasts` /
  `fermi_portfolios` INSERTs (the columns are TEXT). Kept for now
  because prod deployments with a UUID-drifted schema may rely on
  the coercion; migration 161's lowercase-normalisation defuses the
  worst case without needing the removal.
