# v0.10.4 — RBAC substrate: one invariant, every tenant app

Platform-level bug fix. Establishes a single ownership invariant
across ABW (Fermi, Rabble, simOps, SOSA, AR, apps directory, plus
future tenants) so that the class of drift v0.10.3 exposed on Fermi
can never accumulate again on any tenant.

## The invariant

> For every session, `principal.user_id() ∈ users.user_id`, and for
> every row in every resource table, `row.owner_column ∈ users.user_id`.
> The two sets are always in the same namespace.

v0.10.3 fixed the first half (sessions). v0.10.4 fixes the second
half — resource rows — and makes both halves verifiable in one SQL
query.

## Why this matters for tenant apps

Rabble creatures, simOps swarm sessions, SOSA platforms, AR beacons,
and the apps directory all store `owner_id TEXT` / `creator_id TEXT`
/ `user_id TEXT` with **no FK to `users(user_id)`**. They "worked"
because their read paths are mostly public — no ownership gate — so
the drift was silent. The moment a new feature gates a read on
ownership (agent-creation, workspace admin, wallet actions), any
account whose owner reference drifted pre-v0.10.3 would 404 or 403.

You cannot ship those features on top of `n × m` per-tenant per-drift
workarounds. You need substrate.

## What ships in v0.10.4

### 1. Migration 162: FK NOT VALID across every resource-owner column

`migrations/162_rbac_substrate_fk.sql`. Adds

```sql
ALTER TABLE <resource>
    ADD CONSTRAINT <resource>_<col>_fk
    FOREIGN KEY (<col>) REFERENCES users(user_id)
    ON DELETE SET NULL
    NOT VALID;
```

on 22 (table, column) pairs spanning every tenant app currently in
the tree: `agents`, `teams`, `apps`, `creatures`, `creature_*`,
`swarm_events`, `swarm_sessions`, `sosa_platforms`,
`observation_sessions`, `forage_observations`, `ar_beacons`,
`ar_grid_maps`, `shopping_profiles`, `rabble_co_presence`,
`forecast_relationships`, `forecast_relationship_groups`,
`pending_cascades`, `fermi_market_observations`.

`NOT VALID` means:
- Existing pre-v0.10.3 drift **is not checked** on migration.
- **All new writes must resolve to a real `users.user_id`**.
- A future `VALIDATE CONSTRAINT` migration promotes to enforced once
  orphans are cleared.

Each ADD CONSTRAINT is wrapped in `IF NOT EXISTS` + `EXCEPTION WHEN
OTHERS` so a single-table hiccup (missing table on this deploy, type
mismatch) doesn't abort the whole migration.

Same file heals two recoverable drift classes on the same 22 tables:
- **`owner_col = ''`** → `NULL` (system-orphan, admin reassignable)
- **`owner_col = users.id::text`** → `users.user_id` (belt-and-suspenders
  after mig 161)

Unrecoverable drift (e.g. an old Zitadel sub with no matching
`users` row) is left alone and surfaced via view 163 for manual
reassignment.

### 2. Migration 163: `rbac_orphans` view — one query, whole platform

`migrations/163_rbac_orphans_view.sql`. `CREATE OR REPLACE VIEW
public.rbac_orphans AS SELECT … UNION ALL SELECT …` across every
resource table. Columns: `(resource, row_id, owner_col, owner_ref,
label, created_at)`.

The trust signal is one number:

```sql
SELECT COUNT(*) FROM public.rbac_orphans;
-- 0  → invariant holds; every owner reference resolves.
-- >0 → someone has orphaned rows; drill in via admin endpoint.
```

Extending: when a new tenant app adds a resource table with an owner
column, add one `SELECT` block to the view. That's the whole tax.

### 3. `fermi_auth::rbac` module — one entry point for ownership checks

`fermi/fermi-auth/src/rbac.rs`. Exports:

```rust
pub async fn require(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
    needed: Permission,
) -> Result<AccessLevel, (StatusCode, String)>;

pub async fn require_view(…);
pub async fn require_edit(…);
pub async fn require_admin_on(…);
pub fn      require_platform_admin(…);
```

Every handler routes through this. Delegates to
`fermi_auth::visibility::can_access` (the existing 6-step ladder:
platform-admin → owner → public → user-share → team-share → deny).
The rbac helper adds:

- **Uniform HTTP shape**: `404` when caller has no view (don't leak
  existence); `403` when they have View but asked for more.
- **Ord-based permission comparison** so `Admin >= Edit >= View` is
  a single check.
- **Platform-admin bypass** happens once, in one place.

### 4. `ObjectType` extended with tenant-owned resources

`fermi-auth/src/types.rs`. New variants: `Creature`, `Team`,
`SwarmEvent`, `SwarmSession`, `SosaPlatform`, `ObservationSession`,
`ArBeacon`, `App`. Plus a new method `ObjectType::owner_table()`
returning `(table, pk_col, owner_col)` for admin reassign flows —
enum-driven so there's no format-string SQL injection surface.

Note: these variants are **not yet in the `object_shares.object_type`
CHECK constraint**. That means they resolve to "owner + platform-admin
only, no share/team ACL" until a follow-up mig extends the CHECK.
Handler migrations in v0.10.5 will decide per-resource whether shares
are wanted, and add mig entries at that time.

### 5. Admin surface — one endpoint set, all tenants

Three new routes replace `n × m` per-resource admin handlers:

```
GET  /api/admin/rbac/orphans[?resource=creatures&limit=500]
POST /api/admin/rbac/reassign
     { resource: "creature", row_ids: ["…"], new_owner_user_id: "…" | null }
POST /api/admin/rbac/heal
     { dry_run: true|false, resource: "agents" | null }
```

- **`orphans`** returns `total_orphans` (the trust signal), per-resource
  counts, and paginated rows. Same shape whether the resource is Fermi,
  Rabble, or a tenant we haven't built yet.
- **`reassign`** validates the new owner exists (prevents "fix orphan
  by creating orphan"), uses `ObjectType::owner_table()` for the
  SQL identifiers (no injection surface from `req.resource`), and
  returns per-row `{updated | not_found | error}`.
- **`heal`** re-runs mig 162's empty-string + id::text drift heal on
  demand. Dry-run first — reports how many rows *would* heal without
  writing. Useful when new orphans appear between deploys.

Old `/api/admin/agent-ownership-audit` + `/agent-ownership-reassign`
remain wired for backwards compat but are effectively subsumed by
the new endpoints.

### 6. CI enforcement — `scripts/lint-owner-columns.sh`

Grep-based lint runs in the pre-commit hook. For any migration with
number **>= 162**, flags user-reference columns declared without
`FOREIGN KEY REFERENCES users(user_id)`. Migrations < 162 are
grandfathered (mig 162 covers them via NOT VALID).

Allowlist mechanism for legitimate audit columns (moderation
records, invite history, provenance metadata) — 14 entries currently,
each with a one-line justification.

Consequence: **the next migration that lands with an un-FK'd owner
column fails CI at commit time.** Regression prevention that's not
aspirational.

## What did NOT change in v0.10.4

Deliberately scoped out (arrives in v0.10.5):

- **Handler migration to `rbac::require`.** The old `agents`
  `agent_visible_to_caller` inline check still exists. Same for
  hand-rolled ownership checks in creatures / swarm / SOSA
  handlers. Those get moved in a follow-up so v0.10.4 stays
  substrate-only and reviewable in one sitting.
- **`VALIDATE CONSTRAINT`** to promote NOT VALID FKs to enforced.
  Requires orphans = 0 first. Ship after operators have run
  `POST /api/admin/rbac/heal` + reviewed remaining orphans.
- **`object_shares` CHECK extension** for `creature`, `team`,
  `swarm_event`, etc. Ship per-resource when a share/team ACL
  feature actually needs it.
- **Admin force-publish** (`?force=true` on
  `POST /api/agents/:id/publish`). Separate concern (workflow gates,
  not RBAC); tracked for v0.10.5 alongside handler migration.

## Compatibility

- **All new migrations are idempotent** (IF NOT EXISTS everywhere).
  Re-deploys are safe.
- **PgBouncer-safe**: every write is wrapped in `DO $$ … END $$;`.
- **No API breaking changes.** Old handlers still work; new
  endpoints are additive.
- **The FK NOT VALID additions cannot break existing writes.** Any
  new write that would violate the constraint was already broken
  (the row would fail on the FK check anyway once VALIDATED); NOT
  VALID surfaces this at INSERT time now instead of never.
- **Sessions issued pre-v0.10.4 keep working** — the invariant they
  care about (JWT sub in users.user_id) was fixed in v0.10.3.

## Verification

Immediately after deploying v0.10.4:

```bash
# 1. Invariant holds?
curl -H "Authorization: Bearer <admin_key>" \
     https://<api>/api/admin/rbac/orphans | jq .total_orphans

# 2. Per-resource breakdown
curl -H "Authorization: Bearer <admin_key>" \
     https://<api>/api/admin/rbac/orphans | jq .by_resource

# 3. Dry-run heal (see how many the on-demand heal would catch)
curl -X POST -H "Authorization: Bearer <admin_key>" \
     -H "Content-Type: application/json" \
     -d '{"dry_run": true}' \
     https://<api>/api/admin/rbac/heal

# 4. If .total_orphans > 0, drill into a specific resource
curl -H "Authorization: Bearer <admin_key>" \
     "https://<api>/api/admin/rbac/orphans?resource=creatures"

# 5. Reassign the ones you can attribute
curl -X POST -H "Authorization: Bearer <admin_key>" \
     -H "Content-Type: application/json" \
     -d '{"resource":"creature","row_ids":["…"],"new_owner_user_id":"…"}' \
     https://<api>/api/admin/rbac/reassign
```

## Validation performed

- `cargo check --workspace` — clean.
- `cargo test -p fermi-auth --lib` — 18 passed (16 pre-existing + 2
  new for `rbac::require_platform_admin`).
- `bash scripts/lint-owner-columns.sh` — clean.
- Migration lint (`scripts/lint-migrations.sh`) — clean on 162 + 163.

## Rolling forward — the tenant-app contract

Every new tenant app now must satisfy, at review time:

1. Every resource table with a user reference has an inline
   `REFERENCES users(user_id)`. CI enforces this.
2. Every handler that gates on ownership calls `rbac::require*`. Never
   hand-roll `if owner != user && !can_admin() { 403 }`.
3. Every new resource added to the audit surface — one `SELECT`
   block in `rbac_orphans`, one variant in `ObjectType`, one row in
   `ObjectType::owner_table()`.
4. Ship. If you did steps 1-3, the tenant app inherits ownership
   drift detection, admin reassign, and health monitoring for free.

## v0.10.5 preview

- `agents` + `creatures` + `swarm_*` handlers → `rbac::require`.
- Admin force-publish flag with audit trail (`admin_bypass_events`
  table).
- Deprecate `agent_visible_to_caller` and `agents.owner_id`
  hand-rolled checks.
- `VALIDATE CONSTRAINT` migration once orphans reach zero on prod.
