# v0.10.6 — Diagnostic surface: `/api/rbac/self-check` + auto-diagnose

Bug-fix + observability release. Ilabra's account was still seeing
"Backend save failed: your users row and session don't line up
(owner_id FK violation)" after v0.10.5 shipped — but there was no
way for the operator to tell whether that meant (a) the deployed
backend was older than v0.10.3, (b) the JWT was minted before
v0.10.3's backfill, or (c) something else entirely.

v0.10.6 makes the answer definitive and machine-readable.

## New: `GET /api/rbac/self-check`

Authenticated, one-shot diagnostic. Any signed-in user hits this
endpoint and gets:

```json
{
  "ok":                      true|false,
  "invariant_holds":         true|false,
  "diagnosis":               "aligned"
                             | "stale_jwt"
                             | "users_row_needs_backfill"
                             | "users_row_missing",
  "remediation":             "human-readable next step",
  "principal_user_id":       "<jwt sub>",
  "principal_email":         "<jwt email>",
  "principal_auth_provider": "google" | "github" | "email" | ...,
  "server_commit":           "<git sha of the deployed backend>",
  "server_version":          "0.10.6",
  "users_row": {
    "found":         true|false,
    "matched_by":    "user_id" | "email",
    "id":            "<users.id UUID>",
    "user_id":       "<users.user_id>",
    "email":         "<users.email>",
    "auth_provider": "<...>"
  }
}
```

**The four diagnoses:**

- **`aligned`** — invariant holds. No action needed. `principal.user_id() ∈ users.user_id`.
- **`stale_jwt`** — a users row for this email exists with a good
  `user_id`, but it doesn't match the caller's JWT `sub`. Session
  was minted before v0.10.3's `sync_user_from_app` UPDATE-clause
  backfill. **Sign out and back in** — the new JWT will carry the
  healed value.
- **`users_row_needs_backfill`** — users row exists but `user_id`
  is NULL or empty. Deployed backend is pre-mig-161. **Redeploy
  from main** or run `POST /api/admin/rbac/heal`.
- **`users_row_missing`** — no row for this session's user_id or
  email. Shouldn't happen post-v0.10.3; suggests the OIDC callback
  is failing silently. Sign out + back in first; if unchanged,
  check server logs.

The `server_commit` field is the same git SHA `/api/health` returns,
so operators can verify deploy freshness in the same call.

## Composer auto-diagnose

`persist_backend_save` in `cockpit.rs` now calls
`/api/rbac/self-check` when it gets a FK-shaped save error and
inlines the diagnosis in the warning toast:

Before (v0.10.5):

> ⚠️ Backend save failed: your users row and session don't line up
> (owner_id FK violation). Two likely causes: (1) your session JWT
> was minted before the v0.10.3 backfill — sign out and back in
> first; (2) the deployed backend is older than v0.10.3 — check
> GET /api/rbac/self-check for a definitive answer + remediation.
> Raw error: …

After (v0.10.6):

> ⚠️ Backend save failed: your users row and session don't line up
> (owner_id FK violation). …
>
> → Diagnosis: [stale_jwt] Your session was minted before v0.10.3's
> user_id backfill. Sign out of the console and sign back in — the
> new JWT will carry the healed user_id. (server v0.10.6 @ c351a37…)

The self-check call is best-effort: if the deployed backend doesn't
have the endpoint yet (pre-v0.10.6), the composer falls back to the
generic message. No new error path.

## Concrete fix path for the screenshot

For anyone hitting the FK error today:

```bash
# 1. Confirm deployed backend version
curl https://<backend>/api/health | jq
# → look at .commit — should match a v0.10.3+ git sha from main

# 2. As the affected user (session cookie required), ask the diagnosis
curl -H "Cookie: session=<session_token>" \
     https://<backend>/api/rbac/self-check | jq
# → .diagnosis and .remediation tell you exactly what to do

# 3. If diagnosis is `stale_jwt`: sign out + back in in the console.
# 4. If diagnosis is `users_row_needs_backfill`: redeploy backend.
# 5. If diagnosis is `aligned`: the FK error is coming from a different
#    code path than we thought — file an issue with the response body.
```

## Files

- `src/handlers/rbac_self_check.rs` — new, ~150 LOC.
- `src/handlers/mod.rs` — module registration.
- `src/api_server.rs` — `/api/rbac/self-check` route.
- `crates/fermi-console/src/api/client.rs` — `ApiClient::rbac_self_check`.
- `crates/fermi-console/src/cockpit.rs` — auto-diagnose in
  `persist_backend_save`; `format_self_check_diagnosis` helper;
  updated `friendly_backend_save_error` message.
- `crates/fermi-console/Cargo.toml` — 0.10.5 → 0.10.6.
- `RELEASE_NOTES_v0.10.6.md` — this file.

## Compatibility

- **New endpoint is authenticated.** Anonymous callers get 401 (via
  the auth middleware). Any signed-in user can hit their own
  diagnostic — no leaked data because the endpoint returns the
  caller's own row, keyed off the JWT.
- **Client is backward-compatible with pre-v0.10.6 backends.** The
  self-check fetch is wrapped in a `match … Err(_) => None`; a 404
  from an older backend degrades to the v0.10.5 generic message.
- **No schema changes.** No new migration.

## Validation

- `cargo check --workspace` — clean.
- `cargo test -p fermi-auth --lib` — 18 passed.
- `cargo test --bin api-server` — 31 passed.

## What's next (v0.10.7+)

- Console footer chip showing `server v0.10.x @ <sha>` alongside the
  client version, populated from `/api/health` on startup. Never
  again "up to date" as a half-truth.
- Rabble tenant handler migration to `rbac::require` (deferred from
  v0.10.5 to keep the batch reviewable).
- `VALIDATE CONSTRAINT` on the mig 162 FK NOT VALIDs once
  `rbac_orphans = 0` on prod.
