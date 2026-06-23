# Spec 24 Handover — picking up at Sprint 2.4

**Status as of 2026-06-23:** Sprint 2 is 80% done. Five sub-sprints landed
clean, end-to-end tested against live Neon. Next up is **Sprint 2.4**, the
Wave-2 ACL switch — the final server-side piece before any console UI work
begins (Sprint 3+).

This document is everything a fresh session needs to keep going. It pairs
with `docs/specs/24_FORECAST_COLLABORATION_SPEC.md`, which is the authoritative
charter; this one is the operational state-of-play.

---

## 1. Where we are right now

### 1.1 Shipped (all on `origin/main`)

| Commit | Sub-sprint | What it landed |
| --- | --- | --- |
| `cb12b39` | 1 step 1 | `team_members.user_id`→`member_id` typo fix in `get_forecast_handler`. Replaced inline query with `fermi_auth::visibility::is_team_member`. |
| `41be99d` | 1 step 2 | `patch_portfolio_handler` now persists `visibility`/`team_id`/`domain`. Removed dead `UpdatePortfolioRequest` struct. |
| `b123f5d` | 1 step 3 | Four list endpoints (`list_forecasts_handler`, `list_portfolios_handler`, `portfolio_stats_handler`, `list_portfolio_forecasts_handler`) now honour team membership via the `team_id` column. |
| `4441a95` | 1 step 4 | `share_count` + `team_id` added to portfolio-list projection. Console `PortfolioForecast` extended. |
| `9c58a3f` | 2.1 | Migrations 151 (`forecast_invites`) + 152 (`object_shares.object_type` adds `'portfolio'`). Both registered in `src/api_server.rs:421-680` and applied + verified idempotent on Neon. |
| `00b336a` | 2.2 | Six per-target share routes + `GET /api/users/lookup`. New `src/handlers/shares.rs`. `ObjectType::Portfolio` added to `fermi-auth/src/types.rs`. |
| `d24a4ae` | 2.3a | Invite create / list / decline / revoke. New `src/handlers/invites.rs`. State machine `pending → {declined, revoked, expired}` (accept comes next). |
| `4e56770` | 2.3b | Invite **accept** + by-token landing. Three new routes; the by-token GET lives on the public router (`optional_auth_middleware`). Materialises grants in `object_shares` or `team_members` per target type. |
| `9523931` | 2.3c | Email-claim resolver. `fermi_auth::invites::claim_pending_for_email` wired into `oidc::sync_user` and `siwe_verify_handler` so first-time sign-in back-fills pending email invites. |

### 1.2 Test suite

Single integration test file: `tests/forecast_acl.rs`. **32 tests, all `#[ignore]`**, run with:

```bash
cargo test --test forecast_acl -- --ignored --test-threads=1
```

Requires `DATABASE_URL` from `.env` (the deployed Neon DB). Last full run:
**32 passed, 0 failed, in ~175s.**

Test sections (matching the file's module docstring):
1. `is_team_member_*` — the Sprint 1 typo fix.
2. `patch_portfolio_*` — the PATCH visibility/team_id repair.
3. `list_*_includes_team_private` — list endpoints honouring team membership.
4. `share_count_reflects_object_shares_rows` — projection field works.
5. `migration_15{1,2}_*` — schema presence assertions.
6. `forecast_shares_*` / `portfolio_shares_*` — Sprint 2.2 share routes.
7. `invite_*` — Sprint 2.3a state machine.
8. `accept_*` / `by_token_*` — Sprint 2.3b accept paths.
9. `email_claim_*` — Sprint 2.3c email resolver.

### 1.3 What works end-to-end on the API today

Without any console UI, the entire collab flow is exercisable via `curl`:

1. `POST /api/forecasts/:id/invites` with an email → `forecast_invites` row created, token minted, notification queued.
2. Recipient signs in (OIDC) → email-claim resolver back-fills `invitee_user_id` → invite appears in `GET /api/me/invites`.
3. `POST /api/invites/:id/accept` → `object_shares` row materialised with the requested permission, invite flips to `status='accepted'`, inviter gets `invite_accepted` notification.
4. Same forecast now appears in the new user's `GET /api/forecasts` because the list WHERE clause already honours `object_shares` team-shares (but **NOT** direct user-shares yet — that's what 2.4 fixes).

That's the gap 2.4 closes.

---

## 2. The Sprint 2.4 work, in order

### 2.4a — Migration 154 (data backfill, no handler changes)

**Goal:** every forecast/portfolio with `team_id` set today also gets an
`object_shares(object_type, object_id, share_type='team', share_target=team_id::text, permission='edit', granted_by=owner_id::text)` row.

After this, the team-share signal that's currently encoded *only* in the
denormalised `team_id` column is also visible to `can_access`/`can_view`
via the canonical `object_shares` path. The `team_id` column stays as a
"primary team share" pointer so `idx_forecasts_team`/`idx_portfolios_team`
remains useful.

**Migration text** (already specified in `24_FORECAST_COLLABORATION_SPEC.md`
§3.2 Wave 2 step 5, with the number updated to 154 in §3.1.4):

```sql
-- migrations/154_forecasts_object_shares_backfill.sql

-- ... header comment block per repo convention ...

INSERT INTO object_shares
    (object_type, object_id, share_type, share_target, permission, granted_by)
SELECT 'forecast', id::text, 'team', team_id::text, 'edit', owner_id::text
FROM fermi_forecasts WHERE team_id IS NOT NULL
ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING;

INSERT INTO object_shares
    (object_type, object_id, share_type, share_target, permission, granted_by)
SELECT 'portfolio', id::text, 'team', team_id::text, 'edit', owner_id::text
FROM fermi_portfolios WHERE team_id IS NOT NULL
ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING;
```

Note `ON CONFLICT DO NOTHING` so the migration is idempotent across restarts.
**Verify before merging:**
- Sanity check rowcount on Neon before applying:
  `SELECT COUNT(*) FROM fermi_forecasts WHERE team_id IS NOT NULL;` plus
  the portfolios analog.
- Apply, verify the same count of new `object_shares` rows appeared, run
  idempotently again, verify zero additional rows.

**Register in `src/api_server.rs`** after line 692 (right after where
`152_object_shares_portfolio.sql` is listed). Comment block should explain
the backfill rationale and reference Spec 24 §3.2 Wave 2 step 5.

**Tests:**
- `migration_154_backfill_idempotent` — pre/post rowcount probe.
- `migration_154_backfill_matches_team_id` — every forecast/portfolio with
  team_id has a corresponding `object_shares` row.

**Risk:** very low. Pure data backfill against existing rows; no row schema
changes; idempotent.

### 2.4b — Handler ACL switch via `can_access`

**Goal:** the same set of handlers from Sprint 1 step 3 (plus the four write
paths) now compute access using `fermi_auth::visibility::can_access`
exclusively. The inline `f.owner_id = $1::uuid OR f.visibility IN (...) OR EXISTS (team_members)` clauses get replaced.

**Read-path handlers to convert** (currently inline; replace with `can_view`):
- `get_forecast_handler` (`src/handlers/forecasts.rs:309`) — already uses `is_team_member`; convert to `can_view`.
- `portfolio_stats_handler` (`src/handlers/forecasts.rs:1149`).
- `list_portfolio_forecasts_handler` (`src/handlers/forecasts.rs:1469`).

**List-path handlers** are trickier — they don't fetch one row, they filter
many. `can_view` takes a single (object_type, object_id, owner_id,
visibility) tuple, so calling it per-row is N+1. Two options:

- (A) Keep the inline WHERE clause but **extend it with the `object_shares`
  user-share branch** (the team-share branch is already there post-Sprint 1):
  ```sql
  ... OR EXISTS (SELECT 1 FROM object_shares s
                 WHERE s.object_type = 'forecast'
                   AND s.object_id   = f.id::text
                   AND s.share_type  = 'user'
                   AND s.share_target = $1)
  ```
  Same shape as the Sprint 1 team-membership branch. **Recommended** — keeps
  list endpoints fast and the SQL matches what `can_view` computes row-by-row.
- (B) Use `can_view` per-row — clean but quadratic.

Go with (A). The handlers to touch with the new branch:
- `list_forecasts_handler` (`src/handlers/forecasts.rs:454`)
- `list_portfolios_handler` (`src/handlers/forecasts.rs:1099`)

**Write-path audit** — replace `if owner_id != user_id` with `can_edit` /
`can_admin`. Call sites:
- `update_forecast_handler` (`forecasts.rs:580-582`) → `can_edit`
- `update_probability_handler` (`forecasts.rs:711-714`) → `can_edit`
- `delete_forecast_handler` (`forecasts.rs:953-956`) → `can_admin`
- `resolve_forecast_handler` (`forecasts.rs:1273-1277`) → `can_edit`
- `void_forecast_handler` (`forecasts.rs:1346-1350`) → `can_admin`
- `patch_portfolio_handler` (`forecasts.rs:1384-1388`) → `can_admin` (visibility change is admin-level)
- `delete_portfolio_handler` (`forecasts.rs:1842-1845`) → `can_admin`
- `add_forecast_to_portfolio_handler` (`forecasts.rs:1903-1906`) → `can_edit`
- `remove_forecast_from_portfolio_handler` (`forecasts.rs:1975-1978`) → `can_edit`
- The portfolio-share `create`/`revoke` in `shares.rs` (currently owner-only) → `can_admin`
- The forecast-share `create`/`revoke` in `shares.rs` (currently owner-only) → `can_admin`
- The invite `create` handlers in `invites.rs` (currently owner-only for forecast/portfolio) → `can_admin`

The widening of the share/invite endpoints is the user-visible behaviour
change: a collaborator with `permission='admin'` can now invite further
people. That's the spec's intent.

**Tests:**
- `can_view_grants_via_user_share` — user-share in `object_shares` grants list+detail access.
- `can_edit_collaborator_can_update_probability` — `permission='edit'` user can mutate probability; `permission='view'` cannot (403).
- `can_admin_collaborator_can_share` — `permission='admin'` user can create further shares; non-admin gets 403.
- `non_owner_cannot_delete` — even `admin` cannot delete unless target_type-specific rule allows it (our spec puts delete under can_admin, so admin-share-holder CAN delete; verify that's the intended behaviour and adjust the test).

**Schema-drift caveat to preserve:** the owner-check still uses the
`f.owner_id = $1::uuid` cast (text user_id parsed as UUID). That brittleness
stays — it only fails for users whose `users.user_id` doesn't parse as a UUID,
which is a known prod issue (see Spec 24 §1.7 #5). The new `object_shares`
branches sidestep the drift entirely (`share_target` is text, no cast).

**Risk:** this is the first sprint that **changes user-visible behaviour for
existing rows**. The mitigation is the backfill in 2.4a — by the time 2.4b
ships, the `object_shares` rows for existing team-shared content already
exist, so list+detail+write behaviour is preserved for current users.

---

## 3. Recurring operational notes

### 3.1 Parallel agent on the same checkout

Throughout this session, another agent has been pushing forecast revisions
and a separate `forecast_relationships` feature to the same `main`. Symptoms:

- Random untracked files (`docs/VALENCE_*.md`, `tests/wc_arg_scenario.rs`,
  `scripts/world_cup/__pycache__/`) that aren't mine.
- Occasional uncommitted edits to `crates/fermi-console/src/main.rs`,
  `src/api_server.rs`, `src/handlers/mod.rs` that I didn't make.

**Standard procedure before commit:**
```bash
git status --short                        # survey
git stash push -m "..." -- <unrelated>    # if necessary
git add <my files only>
git commit -m "..."
git push
git stash pop                             # restore unrelated work
```

`git stash pop` may fail-merge if the parallel agent has re-touched the same
file; in that case just `git stash drop` (the unrelated change usually lands
via the parallel agent's own commit shortly after). Don't try to reconcile.

### 3.2 Migration policy

Migrations are run by hand-listed `sqlx::raw_sql` calls in
`src/api_server.rs::run_migrations` (≈ line 421-695). New migrations:
1. Add the `.sql` file in `migrations/`.
2. Add a string entry in the explicit list with a comment explaining why.
3. **Verify idempotency** by applying twice on Neon — re-runs must produce
   only `NOTICE: ... skipping` lines, no errors.
4. Migration 150 (`forecast_relationships`) is owned by another feature and
   lives in `ensure_critical_schema` rather than this list. Do not duplicate.

### 3.3 Schema-consistency lint

A pre-commit hook (`scripts/lint-schema-consistency.py`) scans staged Rust
files for qualified SQL column references and verifies each resolves to a
declared migration column. Catches typos like the original
`team_members.user_id` bug before they ship. It auto-discovers new tables
from added migrations on the next commit; "scanning N Rust file(s) against
M known columns" — M grew from 855 → 866 over Sprint 2.

A migration lint (`scripts/lint-migrations.sh`) also runs pre-commit. Warns
on >1 top-level statement (PgBouncer hazard); only escalates to ERROR for
specific dangerous patterns like DROP+ADD CONSTRAINT outside a DO block.
Warnings are tolerated for ordinary CREATE TABLE migrations — see
migrations 148-150 which all warn similarly.

### 3.4 Schema drift to remember

Verified against the live Neon DB on 2026-06-19:

| Column | Schema says | Prod is |
| --- | --- | --- |
| `users.user_id` | TEXT NULLABLE | text (matches; populated for all 18 users today) |
| `users.id` | not in migration | uuid PK (legacy) |
| `fermi_forecasts.owner_id` | TEXT (migration 094) | **uuid** with FK to `users(id)` |
| `fermi_portfolios.owner_id` | TEXT | **uuid** with FK to `users(id)` |
| `team_members.member_id` | TEXT | text |
| `object_shares.share_target` | TEXT | text |

So: collab-side tables (`team_members`, `object_shares`, `forecast_invites`,
`forecast_relationships`) all use TEXT for user identifiers and have no FK.
Content tables (`fermi_forecasts`, `fermi_portfolios`) use UUID with FK to
`users(id)`. The handler casts `f.owner_id = $1::uuid` work only because
**for the 1 user where `users.user_id == users.id::text`**, the cast lines
up. The other 17 users only see their forecasts via non-owner paths
(shared/public/team-member/object-share). This is a real prod bug; Spec 24
doesn't fix it. **Do not introduce more `::uuid` casts on principal user_ids.**

### 3.5 Test pattern

`tests/forecast_acl.rs` follows the `tests/api_tests.rs` / `tests/bayesops_refit.rs`
shape:
- Live `DATABASE_URL` against Neon.
- `try_pool()` early-returns `None` so missing env doesn't fail the suite.
- `#[ignore]` by default; explicit `--ignored` to run.
- `--test-threads=1` because we share one Neon instance.
- Each test minted a unique suffix and cleans up its rows.
- **SQL queries are verbatim lifts from the handler.** If a handler's SQL
  drifts, the test must drift with it — that's the intended pressure.
- Tests call the actual `fermi_auth::teams` / `fermi_auth::invites` helpers
  where possible. `AppState` is `pub(crate)` so HTTP-layer tests aren't
  feasible without a major fixture investment.

### 3.6 ABW surface

Every route lands on the ABW Axum app under `auth_middleware` (or
`optional_auth_middleware` for one route — the by-token GET preview). All
notifications use `source='abw'`. The console authenticates via
`Authorization: Bearer <api_key>` already. Don't introduce a parallel
surface; collab routes are siblings of `/api/forecasts` etc.

---

## 4. Open issues / known gaps (for after 2.4)

These are noted in `docs/specs/24_FORECAST_COLLABORATION_SPEC.md` §5 and §6
already, but worth surfacing:

1. **Email delivery is not wired.** Invite creation mints a token and stores
   it; nothing actually sends the email. Per spec §3.8: audit
   `src/handlers/billing.rs` and `fermi-auth/src/oidc.rs` for an existing
   SMTP/SES helper before Sprint 3 starts; if none, ship a "Copy invite link"
   console button and add email delivery as fast-follow.
2. **Console UI is zero.** Sprint 3 starts the console-side work:
   - `Panel::Teams` for team management
   - "Access" tab on the cockpit/portfolio detail
   - Rewrite of the commit sheet's dishonest "Team" tile
   - Inbox panel with accept/decline
   - Badge fixes on forecast rows
3. **`ObjectType::Workspace`** exists in the Rust enum but is not in the DB
   CHECK constraint. Migration 117 didn't take. Unrelated to Spec 24 but
   somebody should fix it.
4. **The `users.user_id` vs `users.id` drift** is still painted around, not
   fixed (see §3.4). A proper fix is a multi-handler audit + a backfill that
   harmonises `users.user_id` with `users.id::text` for the 17 mismatched
   rows. Out of Spec 24 scope.

---

## 5. Mechanical resume instructions

When you start the next session:

```bash
cd /home/ilabra/fermi
git fetch && git status -sb
# Should be "## main...origin/main" with at most a few stray untracked files
# from the parallel agent. Don't worry about them.

# Confirm the test suite is still green against current Neon:
cargo test --test forecast_acl -- --ignored --test-threads=1
# Expect: 32 passed, 0 failed.
```

Then start Sprint 2.4a:

1. Sanity-check the row counts you'll be backfilling:
   ```sql
   SELECT 'forecasts'  AS t, COUNT(*) FROM fermi_forecasts  WHERE team_id IS NOT NULL
   UNION ALL
   SELECT 'portfolios' AS t, COUNT(*) FROM fermi_portfolios WHERE team_id IS NOT NULL
   UNION ALL
   SELECT 'existing_team_shares' AS t, COUNT(*) FROM object_shares
   WHERE share_type = 'team' AND object_type IN ('forecast','portfolio');
   ```
2. Draft `migrations/154_forecasts_object_shares_backfill.sql` following the
   shape in §2.4a above.
3. Register in `src/api_server.rs` after the `152_*` entry.
4. Apply manually on Neon, verify counts, re-apply for idempotency.
5. Add the two presence tests in `tests/forecast_acl.rs`.
6. `cargo check --workspace` and the full ACL suite green.
7. Commit + push.

Then 2.4b — the handler conversions per §2.4b above. That commit will be
larger (5-10 handlers + a dozen new tests). Consider splitting into
**read-path conversion** and **write-path conversion** if it gets unwieldy.

---

## 6. Reference: file map

Files added or substantially modified during Sprint 1-2.3c:

- `docs/specs/24_FORECAST_COLLABORATION_SPEC.md` — authoritative charter (Sprint 1 step 1)
- `docs/specs/24_HANDOVER_2.4.md` — this file
- `migrations/151_forecast_invites.sql` — Sprint 2.1
- `migrations/152_object_shares_portfolio.sql` — Sprint 2.1
- `fermi-auth/src/types.rs` — `ObjectType::Portfolio` added (2.2)
- `fermi-auth/src/oidc.rs` — `sync_user` calls `claim_pending_for_email` (2.3c)
- `fermi-auth/src/invites.rs` — `claim_pending_for_email` (2.3c, new file)
- `fermi-auth/src/lib.rs` — `pub mod invites` registered (2.3c)
- `src/api_server.rs` — migration list + ~12 new route registrations
- `src/handlers/auth.rs` — `siwe_verify_handler` calls `claim_pending_for_email` (2.3c)
- `src/handlers/forecasts.rs` — Sprint 1 ACL repairs throughout
- `src/handlers/invites.rs` — invite handlers (new file in 2.3a; accept added 2.3b)
- `src/handlers/mod.rs` — `pub mod invites; pub mod shares;`
- `src/handlers/shares.rs` — per-target share handlers (new file in 2.2)
- `src/handlers/users.rs` — `lookup_user_by_email_handler` (2.2)
- `tests/forecast_acl.rs` — 32 regression tests across sprints 1-2.3c
- `crates/fermi-console/src/api/client.rs` — `PortfolioForecast.share_count` + `team_id` (Sprint 1 step 4)

Good luck. The hard part — getting the data model right and the ACL chain
correct — is done. 2.4 is mostly mechanical replacement of inline ACL with
the canonical helpers, plus one data migration.
