# Spec 33 — Admin "View as User" (read-only impersonation)

Status: **implemented (scaffold)** — 2026-08-12 · Read-only mode only;
`assist` (write) mode is specified but not issued.

## 1. Why

`can_admin()` short-circuits the two systems that decide what a user can
see:

- `fermi_auth::rbac::require` — returns early for platform admins
  (`rbac.rs:73`, `visible_sync`).
- `fermi_auth::visibility::can_access` — same bypass.

An admin therefore **cannot reproduce a user-visible bug**. The 404 the
user reports is a 200 for the admin, by construction. Every support
conversation degrades into asking the user to narrate their screen.

This spec adds a short-lived, audited session in which the platform
resolves the admin's requests as the *target user*, with the target's
privileges — including the absence of admin.

## 2. The contract

| Property | Rule |
|---|---|
| Effective identity | the target. `principal.user_id()` returns them |
| Privileges | the target's. **Never** the admin's |
| Methods | `GET` / `HEAD` / `OPTIONS` only in `read_only` |
| Reachability | credential, key-minting and money paths denied outright |
| Lifetime | 30 minutes, revocable, liveness checked per request |
| Audit | mandatory written reason; every request logged; visible to the target |
| Nesting | forbidden |

### 2.1 The load-bearing decision

`AuthPrincipal::can_admin()` returns the **effective** (target) role, not
the admin's. Returning the admin's role would:

1. defeat the feature — you would keep seeing the admin view of
   everything, which is exactly what you were trying to escape; and
2. turn a diagnostic tool into ambient privilege laundering.

Consequence, by design: while impersonating you cannot reach
`/api/admin/*`, including the endpoint that started the session. Exit is
therefore a dedicated, un-gated route (§4.3).

## 3. Data model

`migrations/189_impersonation_audit.sql`.

```
impersonation_sessions(
  session_id UUID PK, admin_user_id, target_user_id,
  reason TEXT NOT NULL, mode TEXT,          -- 'read_only' | 'assist'
  created_at, expires_at, ended_at, end_reason,
  CHECK (admin_user_id <> target_user_id)
)

impersonation_events(
  event_id UUID PK, session_id FK,
  method, path, status,
  blocked BOOLEAN, block_reason,            -- 'mutation_in_read_only' | 'denied_path'
  created_at
)
```

`impersonation_sessions` is **authoritative for liveness**. The JWT is
stateless, so without this the only way to end a session early would be
to wait for expiry. The guard treats "no live row" as refuse, which also
makes the insert-before-mint ordering fail closed.

Blocked attempts are recorded, not discarded — a blocked write is the
most security-relevant row in the table.

## 4. Mechanism

### 4.1 Token

`SessionClaims` gains an optional `imp` envelope (`fermi-auth/src/jwt.rs`).
All identity claims describe the **target**; `imp` carries `real_sub`,
`real_email`, `real_role`, `sid`, `mode`.

`#[serde(default, skip_serializing_if = "Option::is_none")]` keeps
ordinary session tokens byte-identical, so every JWT issued before this
change still validates.

An `imp` whose `sid` is not a UUID is **rejected**, not downgraded: it
could not be tied to an audit row, and "every impersonated request is
auditable" must be total.

### 4.2 Cookie

`abw_impersonation`, separate from `abw_session`. The admin's real
session stays intact underneath, so exiting is just dropping a cookie and
can never strand an admin logged out of their own account.

Precedence in `extract_token`: `Bearer` → `abw_impersonation` →
`abw_session` → `?token=`. Explicit beats implicit (matching the existing
header-over-cookie rule); between the two cookies the narrower identity
wins.

### 4.3 Guard

`fermi_auth::middleware::impersonation_guard`, layered **inside**
`auth_middleware` / `optional_auth_middleware` on both routers so the
principal is already resolved. Non-impersonated traffic pays one enum
check and returns early — no DB work.

Order: liveness → read-only contract → serve → record.

Denied prefixes (any method, any mode):

```
/api/secrets          /api/auth/api-keys    /api/auth/password
/api/wallet/transfer  /api/billing          /api/stripe
/api/agent-credentials                      /api/admin
```

The boundary: *view as* lets you see what the user sees; it must not let
you **become** them durably or extract anything outliving the session.
Everything on that list either mints a credential, reveals a secret, or
moves money.

Matching is segment-aware — `/api/secretsauce` is a different resource
from `/api/secrets`.

`/api/admin/impersonate/end` is exempt (§2.1).

## 5. Eligibility

Refused targets:

- **yourself** — produces audit records that look like impersonation but
  aren't;
- **any admin** — the session would carry admin rights under a second
  identity, since the effective role governs access;
- **service principals** (`abw-system`) — owns the platform's provider
  credentials and carries `role = 'admin'`.

Reason must be ≥ 10 characters. "test" in an audit log creates the
appearance of oversight without the substance.

## 6. Non-goal: administering service principals

Impersonation is **not** the way to manage `abw-system` or other
non-login principals. That belongs to the RBAC substrate, which already
honours `object_shares` and team grants for `ObjectType::Agent`:

1. resources owned by the service principal;
2. a human team (e.g. `platform-operators`);
3. `Permission::Admin` granted to that team on those resources.

That is attributable per person and revocable. A shared service identity
is neither.

## 7. Transparency

`GET /api/me/impersonation-history` — any user can see who viewed their
account, when, and why. A privileged capability the affected party cannot
inspect is indistinguishable from a backdoor.

## 8. Surfaces

| Route | Auth | Purpose |
|---|---|---|
| `POST /api/admin/impersonate` | admin | mint; sets cookie |
| `POST /api/admin/impersonate/end` | any | end + clear cookie |
| `GET /api/admin/impersonate/sessions` | admin | audit list |
| `GET /api/me/impersonation-history` | any | transparency |

UI: `static/js/impersonation-banner.js`, included from `base.html` and
`admin.html`. Renders a fixed, high-contrast bar naming the viewed
account with an Exit button. Load-bearing rather than cosmetic — while
impersonating, every surface renders the target's data, so without a
persistent marker there is nothing on screen distinguishing "this is a
user's broken workspace" from "this is my own".

"View as" buttons sit in the admin console's Users tab.

## 9. Deferred

- **`assist` mode.** Schema and enum exist; the mint endpoint never
  issues it. Needs a consent story and per-mutation attribution
  (stamping affected rows with the real admin) before it is safe.
- **Console support.** `crates/fermi-console` speaks the same API and
  will need to honour the mode and render its own banner.
- **Spend.** Agent execution during impersonation would burn the target's
  credits. Currently blocked implicitly (execution is `POST`); an
  explicit economic rule is needed before `assist` mode lands.
