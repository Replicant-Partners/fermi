# v0.10.5 — Substrate applied: handlers on `rbac::require`

v0.10.4 shipped the RBAC substrate (`fermi_auth::rbac`, migrations 162-163,
the `rbac_orphans` view, the admin surface). v0.10.5 makes it real by
migrating every ownership check across the Fermi tenant off hand-rolled
patterns and onto the substrate helper.

## What changed

### Handlers migrated to `rbac::require*`

25 handlers across 8 modules now route ownership through
`fermi_auth::rbac`:

- **`agents.rs`** — `list_agents` (via `visible_sync`), `get_agent_handler`,
  `update_agent_handler`, `delete_agent_handler`,
  `restore_agent_version_handler`, `import_embeddings_handler`,
  `embeddings_export_consent_handler`, `embeddings_export_handler`.
  `agent_visible_to_caller` reduced to a two-line adapter around
  `rbac::visible_sync`.
- **`lifecycle.rs`** — `publish_checks_handler`, `publish_agent_handler`,
  `archive_agent_handler`, `restore_agent_handler`,
  `update_fork_pricing_handler`.
- **`agent_wallet.rs`** — all five wallet handlers via a new
  `require_admin_on_agent` helper that delegates to
  `rbac::require_admin_on`. Old bespoke `require_owner_or_admin`
  deleted.
- **`consolidation.rs`** — `topup_dreaming_budget_handler`,
  `get_consolidation_job_handler`.
- **`apps.rs`** — `update_app_handler_full`, `publish_app_handler`,
  `archive_app_handler`.
- **`composition.rs`** — `require_workspace_owner_or_admin` now
  routes through `rbac::require_admin_on` with `ObjectType::Team`.
- **`eval.rs`** — six hand-rolled `owner_id != user_id &&
  tier != "curated"` checks collapsed into one
  `require_eval_authority(state, principal, agent)` helper that
  short-circuits on curated agents and delegates to
  `rbac::require_admin_on` otherwise. 5 handler sites replaced;
  1 tool-context site (inside `EvalTrigger` impl) kept as-is
  because that path doesn't carry a full `AuthPrincipal`.
- **`observatory.rs`** — `require_owner_or_admin` (module-local)
  now uses `rbac::require_admin_on` with a curated-agent
  short-circuit.

### Uniform response shape

The substrate returns:

- **`404 NOT FOUND`** when the caller has no view whatsoever — don't
  leak existence of private resources through response codes.
- **`403 FORBIDDEN`** when they have View but not the permission
  they asked for. Existence was already visible, so signalling
  insufficient permission is fine.

Every migrated handler now emits this shape. Previously each
handler had its own text ("Not the owner of this agent", "You do
not own this agent", "Owner or admin access required" — a dozen
variants). Now: `"<permission> required on this <resource>"`.

### Admin force-publish + `admin_bypass_events` audit

The screenshot scenario ("Cannot publish: failing checks. Ask the
owner to fix these first.") — resolved.

**New endpoint shape:**

```
POST /api/agents/:agent_id/publish?force=true&reason=<url_encoded_text>
```

- **`force=true` is platform-admin-only.** Owners can't
  force-publish their own agent — the checks exist to protect the
  platform from junk publishes, and owner-side bypass defeats that.
  Response is `403` with an actionable message pointing at either
  fixing the checks or asking an admin.
- **When force is used and checks would have blocked**, the
  underlying `publish_pipeline::publish_agent` gets a new `force:
  bool` parameter that skips `can_publish`. The response body
  reports every check (including the failed ones) plus a
  `force_used: true` flag so the operator sees exactly what was
  bypassed.
- **Every force-publish writes one row to `admin_bypass_events`**
  (migration 164). Row includes: `admin_user_id`, `target_type =
  'agent'`, `target_id`, `action = 'force_publish'`, and
  `details.reason` + `details.failing` (the specific checks that
  were bypassed). Best-effort — a failure to write the audit row
  does NOT block the underlying publish (paper trail is nice-to-have
  after the fact; the action itself is the primary intent).

**Deliberately narrow scope:**

Only admin bypass of *workflow quality gates* land in
`admin_bypass_events`. Ownership bypass by a platform admin is
implicit in the role and not logged. This keeps the audit trail
focused on the interesting delta: not "admin did something as
admin" but "admin overrode a gate that would have blocked the
owner."

### Migration 164 — `admin_bypass_events`

`migrations/164_admin_bypass_events.sql`. `event_id UUID PRIMARY
KEY`, `admin_user_id TEXT NOT NULL REFERENCES users(user_id)`,
`target_type TEXT`, `target_id TEXT`, `action TEXT`,
`details JSONB`, `created_at TIMESTAMPTZ`. Two indexes:
`(target_type, target_id)` and `(admin_user_id, created_at DESC)`.
PgBouncer-safe, idempotent.

### `publish_pipeline::publish_agent` signature change

**Breaking for direct callers** (not for HTTP clients):

```rust
// Before
pub async fn publish_agent(
    pool: &PgPool,
    agent: &Agent,
    user_id: &str,
    gas_fees: &GasFees,
) -> Result<(TransitionResult, Vec<PublishCheck>), String>

// After
pub async fn publish_agent(
    pool: &PgPool,
    agent: &Agent,
    user_id: &str,
    gas_fees: &GasFees,
    force: bool,
) -> Result<(TransitionResult, Vec<PublishCheck>), String>
```

Only one caller in-tree; updated in this release. External callers
(if any) get a clear compile error and can pass `false` to preserve
old behaviour.

## Uniform ObjectType visibility mapping

`handlers::agents::agent_effective_visibility` (and its twin
`handlers::lifecycle::agent_visibility`) map an `Agent`'s persisted
`(visibility, status)` to substrate `Visibility`:

- `status='published' AND visibility='public'` → `Visibility::Public`
- `visibility='unlisted'`                       → `Visibility::Shared`
- everything else                               → `Visibility::Private`

This bakes the "a draft with visibility=public is still author-only"
rule in one place so every handler (list, detail, execute, wallet,
funding) computes the same answer.

## What did NOT change in v0.10.5

Deliberately deferred:

- **`creatures/*` handlers.** The Rabble tenant's owner checks live
  inside more complex logic (join_swarm_handler mixes ownership with
  share checks + walk-in payment flows). Migrating them cleanly needs
  a focused review — v0.10.6 pass. Existing checks continue to work.
- **`swarm_*` / `sosa_*` handlers.** Same reasoning — safer to review
  as a unit in v0.10.6.
- **`VALIDATE CONSTRAINT`** on the NOT VALID FKs. Requires
  `rbac_orphans` to be zero on prod first. Ship after operators have
  run `POST /api/admin/rbac/heal` and reviewed remaining orphans.
- **`object_shares.object_type` CHECK constraint extension** for
  `Creature`, `Team`, `SwarmEvent`, etc. Add per-resource when a
  share/team ACL feature actually wants them.

## Compatibility

- **All existing HTTP clients keep working.** The migrated handlers
  return the same success shapes. The failure shapes are tighter
  (uniform 404/403), which some clients that were parsing error
  strings might notice — but that's a bad pattern anyway.
- **New `?force=true&reason=…` query params on publish** are
  additive; existing clients that don't send them get the same
  no-force path they got in v0.10.4.
- **Sessions in flight** across the v0.10.4 → v0.10.5 boundary
  keep working. Nothing about session shape changed.
- **`resolved_user_id`, `users.user_id`, migration 161/162/163** —
  all unchanged. v0.10.5 only touches handler code + one workflow
  signature + one new migration.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release --bin api-server` — clean.
- `cargo test -p fermi-auth --lib` — 18 passed (16 pre-existing +
  2 for `rbac::require_platform_admin` shape).
- `cargo test --bin api-server` — 31 passed.
- `scripts/lint-owner-columns.sh` — clean.
- `scripts/lint-migrations.sh` — clean on migration 164.

## Post-deploy verification

1. Force-publish the screenshot's agent as admin:

   ```
   POST /api/agents/efra_thesis/publish?force=true&reason=<url_encoded>
   ```

   Should return 200 with `force_used: true` and the failing checks
   in the response body.

2. Confirm the audit row landed:

   ```sql
   SELECT * FROM admin_bypass_events
    WHERE target_type = 'agent' AND action = 'force_publish'
    ORDER BY created_at DESC LIMIT 5;
   ```

3. Any migrated handler now returns 404 (not 403) when the caller
   has zero visibility, and 403 when the caller has View but asked
   for Edit/Admin. Spot-check by hitting `GET /api/agents/:priv_id`
   as a non-owner non-admin — should be 404. As the owner — 200. As
   admin — 200.

## Handler migration cheat-sheet

For anyone porting the remaining `creatures/*` handlers in v0.10.6:

```rust
// Old (deleted)
if agent.owner_id.as_deref() != Some(&user_id) && !principal.can_admin() {
    return Err((StatusCode::FORBIDDEN, "Not the owner".into()));
}

// New (v0.10.5 pattern)
rbac::require_admin_on(
    &state.db,
    &principal,
    ObjectType::Agent,      // or ::Creature / ::Team / …
    &row.primary_key.to_string(),
    row.owner_column.as_deref().unwrap_or(""),
    map_to_substrate_visibility(&row),
)
.await?;
```

For list filters (O(N) rows, no share/team ACL affordable):

```rust
db_rows.iter().filter(|r|
    rbac::visible_sync(&principal, r.owner.as_deref(), r.visibility)
)
```

For anon endpoints:

```rust
if !rbac::visible_sync_anon(visibility) {
    return Err((StatusCode::NOT_FOUND, "…".into()));
}
```

## What's next

- **v0.10.6** — `creatures/*`, `swarm_*`, `sosa_*` handler migration
  (Rabble/simOps tenants).
- **v0.10.7** — `VALIDATE CONSTRAINT` on the NOT VALID FKs, once
  `rbac_orphans` reaches zero on prod.
- **v0.11.x** — extend `object_shares.object_type` CHECK to include
  the new `ObjectType` variants when a specific share/team feature
  wants them per-resource.
