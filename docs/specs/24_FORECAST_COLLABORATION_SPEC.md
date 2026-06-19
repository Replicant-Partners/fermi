# 24 — Forecast Collaboration & Sharing Spec

**Status:** Draft, 2026-06-19
**Owner:** ilabra
**Surface:** ABW (Agentic Brier Workshop) — the unified auth/billing/forecast
backend. All routes here mount on the existing ABW Axum router under
`auth_middleware` / `optional_auth_middleware`. All notifications use
`notifications.source = 'abw'`. Console auth is the same ABW
`Authorization: Bearer <api_key>` flow already in
`crates/fermi-console/src/api/client.rs:658`. We do **not** introduce a
parallel surface; collab routes are siblings of `/api/forecasts` etc.
**Scope:** Allow operators to (a) share individual forecasts and portfolios with specific people, (b) form persistent teams that share many forecasts at once, (c) invite people who don't yet have accounts, and (d) see what's been shared with them — all from the fermi-console.

**Deployment reality:** ABW runs on Railway against a Neon Postgres. There is
no local DB in the loop — the `DATABASE_URL` in `.env` points at the live
deployed Neon instance, which is what every schema observation in §1 was
verified against on 2026-06-19. Local `cargo check` / `cargo test` is the
pre-merge gate; runtime verification only happens after Railway redeploys.
Migrations land on Neon when the next ABW process boots and runs the
hand-listed migration block at `src/api_server.rs:421-680`.

> This is not a greenfield design. The platform already ships most of the
> primitives (`teams`, `team_members`, `object_shares`, `fermi-auth::can_access`,
> `notifications`). They are partly broken and entirely unused for forecasts /
> portfolios, and the console has zero UI for any of them. This spec documents
> the gap, fixes the bugs, and adds the missing surfaces.

---

## 1. Inventory: what exists today

Verbatim findings from a code sweep on 2026-06-19. Paths/lines pinned so we don't redesign primitives we already have.

### 1.1 Visibility model on `fermi_forecasts`

- `migrations/094_fermi_forecasting.sql:67-117` declares
  `visibility TEXT CHECK ('private','shared','public')` (default `private`),
  `team_id UUID REFERENCES teams(id) ON DELETE SET NULL`, plus
  `owner_id TEXT REFERENCES users(user_id)` (subject to the schema-drift caveat
  in §1.7).
- `idx_forecasts_visibility` (partial: `shared|public`) and `idx_forecasts_team`
  (partial: non-null) already exist (`094:119-126`).
- The publish/list/get/update/delete handlers all live in
  `src/handlers/forecasts.rs`. ACL is hand-rolled on every endpoint:
  - `get_forecast_handler` chain (`forecasts.rs:329-352`): owner → not-private →
    team membership.
  - `list_forecasts_handler` (`forecasts.rs:454-457`): only
    `owner_id = $1 OR visibility IN ('shared','public')` — **team membership is
    NOT consulted**.
  - All write paths (`forecasts.rs:580-582, 711-714, 953-956, …`) gate on
    strict `owner_id == user_id`.
- `crates/fermi-console/src/api/client.rs:303-340` — `PortfolioForecast` (and
  `Forecast` higher up) carries `visibility: Option<String>`.
- The console commit-sheet UI (`crates/fermi-console/src/main.rs:5419-5706`)
  presents three tiles: **Private**, **Team**, **Public**.

### 1.2 Visibility model on `fermi_portfolios`

- `migrations/094_fermi_forecasting.sql:41-53` — same `visibility` and `team_id`
  shape as forecasts.
- `list_portfolios_handler` (`forecasts.rs:1099`) and
  `list_portfolio_forecasts_handler` (`forecasts.rs:1433-1436`) check
  `owner OR visibility != 'private'` — **team membership not checked at all**.
- `patch_portfolio_handler` (`forecasts.rs:1390-1402`) does **not** update
  `visibility` or `team_id` even though `UpdatePortfolioRequest`
  (`forecasts.rs:120-125`) declares those fields. This is dead code today.

### 1.3 Identity / auth model

- Users — `migrations/004_add_users_table.sql:6-30`. PK is `user_id TEXT`
  (Zitadel ID, ENS addr, etc.). Roles: `admin | developer | viewer`.
- API keys — `migrations/005_add_api_keys.sql` (Argon2-hashed, scoped).
- Auth principals + middleware — `fermi-auth/src/middleware.rs:21-112`.
  All `/api/forecasts/*`, `/api/portfolios/*`, `/api/teams/*`, `/api/shares/*`
  are mounted under `auth_middleware` (`api_server.rs:2905-2908`).

### 1.4 Teams system **already exists** and is wired

- `migrations/009_add_teams_and_sharing.sql`:
  - `teams(id, name, slug UNIQUE, description, owner_id, …)` with auto-trigger
    that inserts the owner as `team_members.role='owner'`.
  - `team_members(team_id, member_type CHECK ('user','agent'),
    member_id, role CHECK ('owner','admin','member','viewer'),
    invited_by, joined_at)`.
  - `object_shares(id, object_type CHECK ('agent','capability','forecast',
    'index','repo','file','rabble','workspace'), object_id, share_type
    CHECK ('team','user'), share_target, permission CHECK
    ('view','edit','admin'), granted_by, created_at)` with
    `UNIQUE(object_type, object_id, share_type, share_target)`.
- Helper API: `fermi-auth/src/teams.rs` — `create_team`, `add_team_member`,
  `remove_team_member`, `update_member_role`, `get_member_role`,
  `share_object`, `list_object_shares`, `revoke_share`.
- Generic ACL helper: `fermi-auth/src/visibility.rs:44-118` —
  `can_access(pool, principal, object_type, object_id, owner_id, visibility)
  -> AccessLevel` resolves system-admin → owner → public → user-share →
  team-share with highest-permission-wins. **Forecasts and portfolios do not
  call this** — they re-implement a thinner ACL inline.
- HTTP routes already mounted (`api_server.rs:2253-2290`):
  - `POST /api/teams`, `GET /api/teams`, `GET /api/teams/:id`,
    `POST /api/teams/:id/members`, `DELETE /api/teams/:id/members/:user_id`,
    `PUT /api/teams/:id/members/:user_id`.
  - `POST /api/shares`, `DELETE /api/shares/:id`.

### 1.5 Notifications

- `notifications(id, user_id, type, title, message, read, created_at,
  source DEFAULT 'abw')` — `migrations/021_notifications.sql` +
  `134_notifications_source.sql:21-35`.
- Routes: `GET/PUT /api/notifications` (`api_server.rs:2072-2083`).
- Helper: `create_notification` and `create_notification_for_surface`
  (`api_server.rs:3652-…`).
- `add_member_handler` (`teams.rs:259-269`) already emits a notification of type
  `"workspace_invite"` when a member is added directly.

### 1.6 Activity / audit

- `fermi_forecast_updates(id, forecast_id, previous_probability, new_probability,
  reason, agent_id, evidence_added, created_at, revision_trigger)` —
  immutable revision history (`migrations/094`, `149`).
- `forecast_spacetime` — tamper-evident commitments via AFTER-INSERT trigger
  (`api_server.rs:867-901`).
- No `audit_log`, no `comments`, no `webhooks`. `activity_events`
  (`migrations/090_social_layer.sql:104-142`) is Rabble-only — its CHECK
  constraint excludes any forecast event_type.

### 1.7 Known bugs and drift to clean up while we're here

1. **`forecasts.rs:338` queries `team_members.user_id`** but the column is
   `member_id` (009:40, verified in dev DB). The team-fallback branch in
   `get_forecast_handler` is silently dead today.
2. **`list_forecasts_handler` ignores team membership** — a private forecast
   with a `team_id` is invisible to its own team (`forecasts.rs:454-457`).
3. **`patch_portfolio_handler` doesn't write `visibility`/`team_id`**
   (`forecasts.rs:1390-1402`).
4. **Console commit sheet writes `visibility="team"`** (`main.rs:5589`) but the
   server CHECK constraint only accepts `'private' | 'shared' | 'public'`.
   The literal `"team"` is currently rejected by the DB. The "Team" tile also
   does not collect a `team_id`. Sanity check on dev DB: zero rows have
   `visibility NOT IN ('private','shared','public')` — all 48 existing rows
   are `shared`.
5. **Schema drift (verified 2026-06-19 against dev Neon DB):**
   - `migrations/094` declares `fermi_forecasts.owner_id TEXT`. Actual prod
     column type is `uuid`. Same for `team_id` and `workspace_id`.
   - `users.user_id` IS actually `text` (matching the migration).
   - `team_members.member_id` is `text`.
   - Therefore: handler casts `f.owner_id = $1::uuid` work (`$1` is a text
     user_id, owner_id is uuid in prod — the cast direction matters: it casts
     the parameter, not the column). The team-fallback path needs
     `member_id = $1` directly (both text, no cast).
   - Any new FK to `users(user_id)` must be `TEXT` (matches the actual users
     PK); any reference to `fermi_forecasts.id` is `UUID`.
6. **`object_shares.object_type` CHECK on dev DB is**
   `('agent','capability','forecast','index','repo','file','rabble')` —
   `'workspace'` is NOT in there despite migration 117's claim. Migration
   151 adds `'portfolio'`; we leave `'workspace'` for whoever needs it.
   `idx_object_shares_target(share_type, share_target)` already exists —
   the rewritten `list_forecasts` team-share branch will be index-fast.
7. **No `cargo sqlx prepare`** — every query is runtime
   `sqlx::query(...)`. No `_sqlx_migrations` table either; migrations are
   hand-listed in `src/api_server.rs:421-680`. Runtime tests are the only
   safety net.

### 1.8 Console reality check

`crates/fermi-console/src/api/client.rs:1073-1081` only wraps `list_teams` and
`get_team`. There is **no** wrapper for `create_team`, `add_member`,
`share_object`, `revoke_share`, or `list_object_shares`.

The cockpit/main UI has **zero** surfaces for: creating a team, listing/managing
members, inviting users, choosing per-forecast share targets, viewing who has
access, accepting/declining invites, or pending-invite indicators.

---

## 2. Goals & non-goals

### Goals

1. **Per-forecast and per-portfolio sharing** with named users *or* a team, at
   `view | edit | admin` granularity, surfaced in the cockpit and portfolio
   detail.
2. **Persistent teams** with member management, used by both forecasts and
   portfolios; one team selection at publish time means "share with everyone
   in this team."
3. **Invitations** that work for users who don't yet have an account
   (email-pending) and for known users (instant), with accept/decline.
4. **Notifications** when something is shared with you, when you're invited to
   a team, or when an invite is accepted.
5. **A console UI** for all of the above, replacing the dishonest "Team" tile.
6. **Repair the existing ACL drift** (the bugs in §1.7) so that when we ship
   collaboration the underlying code paths are correct.

### Non-goals (this spec)

- Comments / discussion threads on forecasts (separate spec, follow-on work).
- Public profile pages, follower graphs, social-style activity feed (Rabble
  already covers that surface).
- Cross-organisation tenancy / SSO-scoped orgs (`zitadel_org_id` exists on
  users but no team-level enforcement; we explicitly defer this).
- Real-time collaborative editing (CRDT, OT, presence).
- Webhooks / outbound integrations.

### Explicit decisions

- **Reuse `object_shares` and `can_access`** rather than inventing per-object
  ACL tables. They already cover everything we need and are used elsewhere
  (Rabble, workspaces).
- **Reuse the `teams` table.** No new "org" abstraction. A "team" *is* the
  collab unit. (Rabble's "swarm" is a separate world; we don't try to unify.)
- **The legacy `team_id` column on `fermi_forecasts` / `fermi_portfolios`
  becomes the "primary team"** (the first team-level share, materialised for
  cheap filtering and indexing). All other shares live in `object_shares`.
  This avoids a destructive migration and keeps the partial index useful.
- **Visibility values stay `private | shared | public`** with the meaning:
  - `private` — only owner + entries in `object_shares` see it.
  - `shared` — anyone with the link / id can read (the current "unlisted"
    semantics that the migration glossary at
    `fermi-auth/src/types.rs:122-129` already maps to).
  - `public` — listed in `/api/forecasts/public` and the leaderboard surface.
  The "team" UI tile in the commit sheet is **not** a fourth visibility — it
  becomes "private + share with team X at view/edit/admin," materialised via
  `object_shares` and the `team_id` column.

---

## 3. Specification

### 3.1 Data model

#### 3.1.1 `forecast_invites` (NEW)

Generic invite primitive scoped to forecasts/portfolios *and* teams. We use
one table with a polymorphic target so the console has one notification list.

```sql
-- migrations/150_forecast_invites.sql
CREATE TABLE IF NOT EXISTS forecast_invites (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- What the invite grants access to.
    target_type     TEXT NOT NULL CHECK (target_type IN
                        ('forecast','portfolio','team')),
    target_id       TEXT NOT NULL,
    -- Permission to grant on accept. For target_type='team' this is the
    -- team_members.role (owner/admin/member/viewer).
    permission      TEXT NOT NULL CHECK (permission IN
                        ('view','edit','admin','owner','member','viewer')),
    -- Recipient. EXACTLY ONE of (invitee_user_id, invitee_email) is non-null.
    invitee_user_id TEXT REFERENCES users(user_id) ON DELETE CASCADE,
    invitee_email   TEXT,
    -- For email invites and shareable links we generate a token. NULL for
    -- direct user-id invites that don't need a link.
    token           TEXT UNIQUE,
    inviter_id      TEXT NOT NULL,    -- users.user_id; matches owner_id type
    message         TEXT,             -- optional human note
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending','accepted','declined',
                                       'revoked','expired')),
    expires_at      TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '14 days',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at     TIMESTAMPTZ,
    CHECK ((invitee_user_id IS NOT NULL) <> (invitee_email IS NOT NULL))
);

CREATE INDEX idx_invites_recipient_user
    ON forecast_invites(invitee_user_id) WHERE invitee_user_id IS NOT NULL;
CREATE INDEX idx_invites_recipient_email
    ON forecast_invites(LOWER(invitee_email)) WHERE invitee_email IS NOT NULL;
CREATE INDEX idx_invites_target
    ON forecast_invites(target_type, target_id) WHERE status = 'pending';
CREATE INDEX idx_invites_token ON forecast_invites(token) WHERE token IS NOT NULL;
```

Rationale:
- Unifies the three flows (share-forecast, share-portfolio, join-team) so the
  console has one inbox.
- Decouples invite state from grant materialisation: accepting writes the
  appropriate row in `object_shares` or `team_members`, then sets
  `status='accepted'`. We never read the invite when computing access.
- Email invites can land before the user exists; we resolve `invitee_user_id`
  on first sign-in by the email-claim flow already used in
  `fermi-auth/src/oidc.rs`.

#### 3.1.2 No-op `object_shares` extension

`object_shares.object_type` already includes `'forecast'`. We add `'portfolio'`:

```sql
-- migrations/151_object_shares_portfolio.sql
ALTER TABLE object_shares
    DROP CONSTRAINT IF EXISTS object_shares_object_type_check;
ALTER TABLE object_shares
    ADD CONSTRAINT object_shares_object_type_check CHECK (object_type IN
        ('agent','capability','forecast','portfolio','index','repo','file',
         'rabble','workspace'));
```

#### 3.1.3 Notification types

Add typed string constants in code (`fermi-auth/src/notifications.rs` or
`src/handlers/notifications.rs`):
- `forecast_shared` — someone shared a forecast with you.
- `portfolio_shared` — someone shared a portfolio with you.
- `team_invite` — you've been invited to a team. (Replaces the existing
  `workspace_invite` for the team-management flow; `workspace_invite` stays
  for the bestiary workspace surface.)
- `invite_accepted` — your invite was accepted.

No schema change; `notifications.type` is already free-form `TEXT`.

#### 3.1.4 Migrations to ship

| File | Purpose |
| --- | --- |
| `150_forecast_invites.sql` | New invite table. |
| `151_object_shares_portfolio.sql` | Add `'portfolio'` to `object_type` CHECK. |
| `152_forecasts_team_id_index_fix.sql` | Confirm partial index exists; backfill any missing. |

Each must also be added to the explicit migration list at
`src/api_server.rs:421-680` (per project convention — they don't auto-run).

### 3.2 Server: ACL repair

The repair happens in two waves to avoid behaviour-change surprises.

#### Wave 1 (Sprint 1): pure bug fixes, no new code paths

`fermi_auth::visibility::can_access` (`fermi-auth/src/visibility.rs:44-118`)
already implements the chain we want — owner → public → user-share →
team-share-via-`object_shares`. **However, it does NOT consult the legacy
`fermi_forecasts.team_id` column.** Today, prod has zero `object_shares` rows
for `object_type='forecast'` (no UI ever wrote them) but does have
`fermi_forecasts.team_id` set on some rows. So a naive flip from inline ACL
to `can_view` would regress whatever team-membership signal `team_id` is
carrying today — which, due to the `team_members.user_id` typo at
`forecasts.rs:338`, currently grants access to **nobody**. The two wrongs
happen to cancel out.

Wave 1 therefore preserves shape, just fixes the typo:

1. **`get_forecast_handler` (`forecasts.rs:329-352`)** — change
   `team_members.user_id = $X` to `team_members.member_id = $X` (one-token
   fix). Add a regression test that creates a team, makes user B a member,
   creates a `private` forecast owned by A with that `team_id`, and asserts B
   gets 200 (today: silently 404).
2. **`patch_portfolio_handler` (`forecasts.rs:1390-1402`)** — actually persist
   `visibility` and `team_id` from `UpdatePortfolioRequest`. Test: owner
   PATCHes `visibility='public'`, GET reflects it.
3. **`list_forecasts_handler` (`forecasts.rs:454-457`)** — extend the WHERE
   to honour `team_id`-based membership *only*, matching what
   `get_forecast_handler` does (so list and detail agree):
   `f.owner_id = $1::uuid
    OR f.visibility IN ('shared','public')
    OR (f.team_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM team_members m
                    WHERE m.team_id = f.team_id AND m.member_id = $1))`.
   Same shape for `list_portfolios_handler`, `portfolio_stats_handler`,
   `list_portfolio_forecasts_handler`. No `object_shares` join yet — that
   comes in Wave 2 once Sprint 2 starts producing `object_shares` rows.
4. **Add `share_count` to the enriched projection** at
   `forecasts.rs:1436-1500` — `LEFT JOIN object_shares s ON s.object_type =
   'forecast' AND s.object_id = f.id::text`, `COUNT(s.id) AS share_count`.
   Always 0 today; non-zero starting Sprint 2. Lets the UI render correct
   badges in Sprint 4 without a second migration.

Wave 1 ships **no migration** and **no schema change**. Pure logic fixes
plus one cheap projection field.

#### Wave 2 (folded into Sprint 2): switch to `can_access`

Once Sprint 2's invite/share endpoints exist and start writing
`object_shares` rows, we can switch the handlers to
`fermi_auth::visibility::can_access`. To preserve the `team_id` signal
during cutover:

5. One-shot data migration `152_forecasts_object_shares_backfill.sql`:
   `INSERT INTO object_shares (object_type, object_id, share_type,
   share_target, permission, granted_by) SELECT 'forecast', id::text, 'team',
   team_id::text, 'edit', owner_id::text FROM fermi_forecasts WHERE team_id
   IS NOT NULL ON CONFLICT DO NOTHING;` — and the same for `fermi_portfolios`.
   After this, `team_id` becomes a denormalised pointer to the "primary team
   share" rather than the source of truth.
6. Replace the inline ACL queries from Wave 1 step 3 with calls to
   `fermi_auth::visibility::can_view` / `can_edit`, and the `list_*` WHERE
   clauses with the full union including `object_shares` (per the original
   draft of this section).
7. Audit every "owner-only" write path (`forecasts.rs:580-582, 711-714,
   953-956, 1273-1277, 1346-1350, 1384-1388, 1842-1845, 1903-1906,
   1975-1978, 2013-2016`). Replace with `can_edit` / `can_admin` so a
   collaborator with `permission='edit'` can update probability and
   evidence; only `admin` can delete or change visibility.

Why split: Wave 1 is provably side-effect-free (the only people gaining
access are users in a team that *already* has `team_id` set on a forecast —
i.e. the access the owner already intended). Wave 2 introduces real
behaviour change (collaborators can now write) and depends on data that
doesn't yet exist; doing it after Sprint 2 means we can verify on real
`object_shares` rows.

### 3.3 Server: new endpoints

All under `auth_middleware` unless noted.

#### Sharing on forecasts and portfolios

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/forecasts/:id/shares` | List `object_shares` for this forecast + the resolved user/team display data. Caller must `can_view`. |
| `POST` | `/api/forecasts/:id/shares` | Create a share. Body: `{share_type:'user'|'team', share_target, permission}`. Caller must `can_admin` (= owner today). On `share_type='team'`, also write `fermi_forecasts.team_id` if NULL (materialise the "primary team" so `idx_forecasts_team` keeps working). |
| `DELETE` | `/api/forecasts/:id/shares/:share_id` | Revoke. Caller must `can_admin`. |
| `GET` | `/api/portfolios/:id/shares` | Same shape for portfolios. |
| `POST` | `/api/portfolios/:id/shares` | Same. |
| `DELETE` | `/api/portfolios/:id/shares/:share_id` | Same. |

Implementation reuses `fermi_auth::teams::share_object` /
`list_object_shares` / `revoke_share` (already exists, just not routed for
forecasts/portfolios).

The existing generic `POST /api/shares` (`api_server.rs:2286-2290`) stays for
back-compat but the dedicated routes give us a place to enforce
forecast/portfolio-specific business rules (e.g. "you cannot share a forecast
your team did not author" once we have org tenancy — out of scope here, but
the route shape is forward-compatible).

#### Invites

| Method | Path | Purpose |
| --- | --- | --- |
| `POST` | `/api/forecasts/:id/invites` | Body: `{invitee: {user_id?, email?}, permission, message?}`. Caller must `can_admin`. If user exists → instant share + `invite_accepted` notification skipped (we don't queue it). If only email → row in `forecast_invites` with token, send email (via existing email path used by SIWE / billing — TBD: confirm SMTP plumbing exists, or stub for v1 with token URL on response). |
| `POST` | `/api/portfolios/:id/invites` | Same. |
| `POST` | `/api/teams/:id/invites` | Body: `{invitee, role, message?}`. Caller must be `owner`/`admin` of the team. **Replaces** the current direct-add path's email-less behaviour: if invitee is by `user_id`, behave as today (instant add) for back-compat; if by `email` and unknown, queue invite. |
| `GET` | `/api/me/invites` | List pending invites for the current user (`invitee_user_id = me OR invitee_email = me.email`). Used by the console's "Inbox" panel. |
| `POST` | `/api/invites/:id/accept` | Caller must match invitee. Materialises a `team_members` row or `object_shares` row in a transaction with `status='accepted'`. Emits `invite_accepted` notification to the inviter. |
| `POST` | `/api/invites/:id/decline` | Caller must match invitee. Sets `status='declined'`. |
| `DELETE` | `/api/invites/:id` | Inviter or any team admin can revoke. Sets `status='revoked'`. |
| `GET` | `/api/invites/by-token/:token` | Public-via-optional-auth router — used by the email-link landing page. Returns minimal target metadata so the recipient can preview before signing in. |
| `POST` | `/api/invites/by-token/:token/accept` | Auth required. Same body-less semantics as `/accept`. |

Mount the by-token routes on `optional_auth_middleware` so the email link works
in a fresh browser; the actual accept happens after sign-in.

#### Lookup helper

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/users/lookup?email=…` | Caller must be authenticated. Returns `{user_id, display_name, avatar_url}` if user exists; 404 otherwise. Used by the "share with…" autocomplete to decide instant-share vs email-invite. Does NOT enumerate (single exact match only). |

### 3.4 Console: API client wrappers

Add to `crates/fermi-console/src/api/client.rs`:

```rust
// Forecast & portfolio sharing
pub async fn list_forecast_shares(&self, fid: &str) -> Result<Vec<ShareEntry>>;
pub async fn add_forecast_share(&self, fid: &str, body: &ShareRequest) -> …;
pub async fn revoke_forecast_share(&self, fid: &str, sid: &str) -> …;
pub async fn list_portfolio_shares(&self, pid: &str) -> …;
pub async fn add_portfolio_share(&self, pid: &str, body: &ShareRequest) -> …;
pub async fn revoke_portfolio_share(&self, pid: &str, sid: &str) -> …;

// Teams (existing list_teams/get_team, plus:)
pub async fn create_team(&self, body: &CreateTeamRequest) -> Result<Team>;
pub async fn add_team_member(&self, tid: &str, body: &AddMemberRequest) -> …;
pub async fn remove_team_member(&self, tid: &str, uid: &str) -> …;
pub async fn update_team_member_role(&self, tid: &str, uid: &str,
                                     role: &str) -> …;

// Invites
pub async fn invite_to_forecast(&self, fid: &str, body: &InviteRequest) -> …;
pub async fn invite_to_portfolio(&self, pid: &str, body: &InviteRequest) -> …;
pub async fn invite_to_team(&self, tid: &str, body: &InviteRequest) -> …;
pub async fn list_my_invites(&self) -> Result<Vec<Invite>>;
pub async fn accept_invite(&self, iid: &str) -> …;
pub async fn decline_invite(&self, iid: &str) -> …;
pub async fn revoke_invite(&self, iid: &str) -> …;

// User lookup for the share autocomplete
pub async fn lookup_user(&self, email: &str) -> Result<Option<UserSummary>>;
```

Types live in the same module beside `PortfolioForecast`.

### 3.5 Console: UI surfaces

Five new surfaces; one rewrite.

#### 3.5.1 Rewrite the commit sheet (`main.rs:5419-5706`)

Replace the broken Private/Team/Public radio with a richer panel:

```
┌─ Commit forecast ─────────────────────────────┐
│ Question: …                                   │
│ Probability: 0.42                             │
│                                               │
│ Visibility:  ( ) Private (only invited)       │
│              ( ) Public  (listed globally)    │
│                                               │
│ Share with:                                   │
│   [+ Add person or team…]                     │
│   👥 World-Cup-Sims        view  ✕            │
│   🧑 alice@example.com    edit  (pending)    │
│   🧑 bob_user_id          view  ✕            │
│                                               │
│ [Cancel]                          [Commit]    │
└───────────────────────────────────────────────┘
```

- The radio is now binary (`private | public`). `shared` is implicit when the
  Share-with list is non-empty and visibility is `private`.
- "Add person or team" is a typeahead. On `enter` for an `@-handle` or
  `user_id`, instantly add as user share. On enter for an email, call
  `lookup_user`; if hit, instant share, else show as `(pending invite)` and
  defer the actual API call until **Commit** — that way one Commit click
  produces one transaction (forecast write + share writes + invite writes).
- Permission picker per row: `view | edit | admin` (tab to cycle). Default
  `view`.
- The legacy `"team"` literal is GONE. Sharing-with-a-team uses an entry of
  kind `team` in the list (rendered with a 👥 icon).

#### 3.5.2 Forecast detail "Access" tab (cockpit)

New side panel in the cockpit, behind a small `🔗 Share` chip in the cockpit
header. Lists current shares, lets the owner add/revoke, and shows pending
invites. No new conceptual surface — just a renderer for `list_forecast_shares`
+ `list_invites_for(forecast)`.

#### 3.5.3 Portfolio detail "Access" tab

Same as above, scoped to the portfolio.

#### 3.5.4 Teams panel (top-level)

Add `Panel::Teams` to the existing `Panel` enum (`main.rs:185-200`). Two-pane
layout:

- Left: list of teams I'm in (with role chip).
- Right: members of the selected team, with `+ Invite` button, role pickers,
  and a "Forecasts shared with this team" sub-list (re-uses
  `list_forecasts?team_id=…` once we ship the team-membership branch in
  `list_forecasts_handler`).

Activate from the existing left sidebar (`main.rs:185-200`) with a 👥 icon.

#### 3.5.5 Inbox (notifications + invites)

A new Inbox panel that merges:
- Pending invites (`/api/me/invites`) with **Accept** / **Decline** buttons.
- Recent notifications (`/api/notifications`, source filter
  `forecast_shared|portfolio_shared|team_invite|invite_accepted`).

Drives a small unread badge in the top bar.

#### 3.5.6 Visibility badge on forecast rows

Replace the lying badge map at `main.rs:4899-4902` with one that reads
`visibility + share count`:

- 🔒 if `private` and `share_count == 0`.
- 🔗 if `private` and `share_count > 0` (shared with N).
- 👥 if `team_id` is set and `team_id` is one of mine (small chip with team
  initials).
- 🌐 if `public`.

This requires the list-forecasts handler to also return `share_count` (cheap
LEFT JOIN COUNT — add to the existing enriched projection at
`src/handlers/forecasts.rs:1436-1500` we already touched in the portfolio
detail PR).

### 3.6 Authorization summary

| Action | Who can do it |
| --- | --- |
| Read forecast/portfolio | `can_view` from `fermi-auth::visibility` |
| Update probability / evidence / agents | `can_edit` (owner OR `permission ≥ edit`) |
| Change visibility / share / unshare / delete | owner (= `can_admin` today) |
| Add/remove team member | team `owner` or `admin` |
| Send invite for forecast/portfolio | `can_admin` on the target |
| Send invite for team | team `owner` or `admin` |
| Accept/decline own invite | invitee only |
| Revoke invite | inviter OR team admin (for team invites) |

### 3.7 Notifications matrix

| Trigger | Recipient | Type | Source |
| --- | --- | --- | --- |
| Direct share created | the new sharee (if user) | `forecast_shared` / `portfolio_shared` | `abw` |
| Invite created (user) | invitee | `forecast_shared` / `portfolio_shared` / `team_invite` | `abw` |
| Invite created (email) | none in app; email goes out instead | — | — |
| Invite accepted | inviter | `invite_accepted` | `abw` |
| Member added directly to team | invitee (back-compat with `add_member_handler`) | `team_invite` (renamed from `workspace_invite` for the teams flow) | `abw` |

### 3.8 Email plumbing

ABW already has user-touching email moments (Stripe receipts, possibly OIDC
flows). **Action item before sprint 2**: audit `src/handlers/billing.rs` and
`fermi-auth/src/oidc.rs` for an existing SMTP/SES helper inside ABW. If none,
v1 ships with the invite token URL returned in the API response and surfaced
in the console as a "📋 Copy invite link" button. Email delivery becomes a
fast-follow once we know whether ABW already has a sender configured on
Railway.

### 3.8.1 Email-claim resolution at sign-in

When an invite is created with `invitee_email` for a user who does not yet
exist, the user must end up bound to that invite on first ABW sign-in. The
hook is in ABW's existing OIDC callback (`fermi-auth/src/oidc.rs`) and SIWE
verify path: after creating the `users` row, call a new helper
`fermi_auth::invites::claim_pending_for_email(pool, &user.user_id, &email)`
that runs an UPDATE setting `invitee_user_id` on every pending invite where
`LOWER(invitee_email) = LOWER($email) AND status = 'pending'`. The user's
inbox at `/api/me/invites` then sees them as if they had been invited
directly. We do not auto-accept; the user still chooses.

### 3.9 Backwards compatibility & migration

- Existing forecasts with `visibility='private'` and `team_id` set keep working
  — the rewritten `list_forecasts_handler` now actually returns them to
  teammates (this is a *bug fix*, not a behaviour change from the user POV).
- Existing forecasts with `visibility='shared'` keep "anyone with link"
  semantics. We do not migrate them.
- The dishonest `visibility="team"` string from the old commit sheet was never
  accepted by the DB CHECK constraint, so there are zero rows with that value
  to clean up. A `SELECT count(*) FROM fermi_forecasts WHERE visibility NOT IN
  ('private','shared','public')` should return 0 as a sanity check before the
  migration ships.
- The legacy `POST /api/shares` route stays. New console code uses the
  `/api/forecasts/:id/shares` endpoints exclusively.
- `add_member_handler`'s direct `POST /api/teams/:id/members` stays for
  back-compat; the new flow is `POST /api/teams/:id/invites`. Direct add
  is retained for tooling/admin scripts.

---

## 4. Roadmap

Five sprints. Each sprint merges as a normal increment — **no feature flag**.
Bug fixes land flag-free (they restore intended behaviour). New endpoints
gate at route-mount in `api_server.rs` if a sprint slips, which means no
orphaned `if FLAG { … }` branches inside handlers. Partial state between
sprints means "endpoint exists but no console UI yet," which is the repo's
normal mode.

### Sprint 1 — Repair & test (server only, no new UI, no migrations) [≈ 1–2 days]

Wave 1 only (per §3.2). All changes are logic-only against the existing
schema:

- Fix `team_members.user_id` → `member_id` typo in `get_forecast_handler`.
- Fix `patch_portfolio_handler` to write `visibility` / `team_id`.
- Extend `list_forecasts_handler` (and the three portfolio analogues) WHERE
  clauses with the `team_id`-based membership branch — matching detail-view
  semantics.
- Add `share_count` to the enriched projection (always 0 until Sprint 2;
  ships the wire format ahead of need).
- New test file `tests/forecast_acl.rs` following the `tests/api_tests.rs`
  pattern (live `DATABASE_URL` against deployed Neon, `--test-threads=1`,
  unique test_user_ids, explicit cleanup, `try_pool` early-return on missing
  DATABASE_URL). Tests:
  - Owner can GET own private forecast.
  - Stranger cannot GET private forecast.
  - Team member CAN GET private forecast with `team_id` set (the bug fix).
  - Anyone authed can GET `shared` / `public`.
  - Same matrix for `list_forecasts`.

**Exit criterion:** All tests pass. `grep -n 'team_members.user_id'
src/handlers/` returns zero hits. `cargo check --workspace` clean. No new
migrations to deploy. Existing console behaviour unchanged for owners and
strangers; team members start seeing private forecasts shared with their
team (which was the documented intent all along).

### Sprint 2 — Server: shares + invites endpoints [≈ 3–4 days]

- Migrations 150–152 (forecast_invites, object_shares CHECK extension,
  team_id → object_shares backfill per §3.2 step 5).
- Wave 2 ACL switch: replace inline ACL with
  `fermi_auth::visibility::can_view` / `can_edit` / `can_admin`. List
  WHERE-clauses extended with the `object_shares` union.
- Wave 2 write-path audit per §3.2 step 7.
- New routes from §3.3.
- `forecast_invites` handlers.
- Notifications wired per §3.7.
- `lookup_user` route.
- Token-link landing route under `optional_auth_middleware`.
- Tests for invite lifecycle (pending → accepted → access reflected in
  `list_forecasts`).

**Exit criterion:** End-to-end `curl` flow: invite by email → another user
signs up with that email → `GET /api/me/invites` shows it → accept → forecast
appears in their list.

### Sprint 3 — Console: API client + Teams panel [≈ 3 days]

- All client wrappers from §3.4.
- New `Panel::Teams` view with member listing, `+ Invite`, role editing.
- A minimal "Create team" modal.

**Exit criterion:** A user can form a team and invite collaborators from the
console without ever touching `curl`.

### Sprint 4 — Console: per-forecast / per-portfolio sharing UI [≈ 3 days]

- Rewrite the commit sheet (§3.5.1) — the dishonest "Team" tile is gone.
- Forecast cockpit "Access" panel (§3.5.2).
- Portfolio detail "Access" panel (§3.5.3).
- Visibility badge fix on rows (§3.5.6).

**Exit criterion:** From the cockpit, an operator can click 🔗 Share on a
forecast, type an email, hit Enter, and an invite is sent (or instant share
if the user exists).

### Sprint 5 — Inbox & polish [≈ 2 days]

- Inbox panel (§3.5.5) with accept/decline.
- Unread badge in top bar.
- Email delivery (or finalise the "copy link" fallback) — see §3.8.
- Update `docs/specs/24_FORECAST_COLLABORATION_SPEC.md` to "Shipped".

**Exit criterion:** New user receives an email link → clicks → signs in → the
forecast is in their portfolio. Inviter sees `invite_accepted` notification.

---

## 5. Open questions

1. **Email plumbing:** confirmed-or-stubbed by sprint 1 kickoff (§3.8).
2. **Public profile URLs:** Do we want `app.fermi.ai/u/<display_name>` so
   share UIs can show real names? Adjacent but out of scope.
3. **Org tenancy:** Should a team belong to a `users.zitadel_org_id`? Today
   teams are flat / personal. Defer until we have a paying multi-seat
   customer asking for org-scoped sharing.
4. **Read receipts on forecasts:** "X saw your forecast at T" — easy with the
   existing notification infra, but is it desired? Not in v1.
5. **Anonymous public-link reads** for `visibility='shared'` rows — current
   behaviour is auth-required. Should the share-with-link semantics drop the
   auth requirement? Probably yes for v2 ("send a link to a journalist"), but
   skipped here to avoid widening the optional-auth surface.

---

## 6. Out of scope (will be its own spec)

- **Comments** on forecasts. Will live in a `forecast_comments` table and
  follow the same `can_view` / `can_edit` ACL.
- **Activity feed** for forecasts (separate from Rabble's `activity_events`).
- **Forecast forking** ("clone this forecast into my workspace").
- **Org/SSO tenancy.**

---

## 7. Risks

- **PgBouncer transaction-mode hazards** on multi-statement migrations
  (callouts at `migrations/119_teams_mission_defensive.sql:8-12`). We adopt
  the defensive single-`DO`-block pattern for migration 151.
- **Schema drift on `owner_id`**: any new FK we declare must match prod's
  actual UUID-shaped column even though `094` says `TEXT`. Migration 150
  declares `inviter_id TEXT REFERENCES users(user_id)`; prod schema is
  effectively `users.user_id` already. Confirm before merge.
- **No compile-time SQL checks** — runtime is the only safety net. Sprint 1
  ships ACL tests *first* so subsequent sprints have a regression baseline.
- **The `add_member_handler` already exists** and emits a notification of type
  `workspace_invite`. We rename to `team_invite` for the teams flow but the
  current callers (bestiary workspaces) continue to emit `workspace_invite`.
  Console Inbox must show both types.

---

## 8. Definition of done

- All five sprints land with feature flag enabled by default.
- `grep -n 'visibility = "team"' crates/fermi-console/src/` returns zero.
- `grep -n 'team_members\.user_id' src/handlers/` returns zero.
- A single demo session: I publish a forecast → invite a teammate by email →
  they sign up → accept → modify probability → I see their update in the
  revision history with their `user_id` (requires an `updated_by` column —
  added in §3.2 step 4 audit, captured in `fermi_forecast_updates` extension
  if needed; if not strictly needed for v1, deferred to a follow-on with a
  one-line migration).
