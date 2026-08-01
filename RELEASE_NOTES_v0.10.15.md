# v0.10.15 — admin bypass on Publish, `agents.user_id` correctness, UUID-safe agent resolve

## Why

v0.10.13 closed the `text = uuid` sweep on fermi tables and its release
notes flagged two remaining items as v0.10.14 candidates:

- `eval_brier.rs:91,108` references `agents.owner_id` — a column that
  has never existed.
- (implicitly) the `resolve_agent` helper resolves only by
  `agent_name`, so any script hitting `/api/agents/<uuid>` gets 404.

v0.10.14 shipped the chip-publish UX unification instead. This release
picks up those two, and adds a third that surfaced under real
admin use:

- **Publish (as admin) refuses on failing checks** — the button label
  promises admin authority, but the frontend preflights
  `/publish-checks` and, when they fail, tells the admin to "ask the
  owner to fix these first." That's the exact bug Ivan hit trying to
  publish Mario's `efra_gorilla` from the admin view. The backend has
  supported `?force=true&reason=…` since v0.10.5 (audited to
  `admin_bypass_events`) — the frontend just wasn't using it.

## Changes

### 1. Admin force-publish path in the UI

**`templates/agent_detail.html`** — `adminPublishAgentDetail`
**`templates/admin.html`** — `adminPublishAgent`

Both flows now:

1. Preflight `/publish-checks` (unchanged).
2. If `can_publish` → POST `/publish` (unchanged).
3. If **not** `can_publish` → show the list of failing checks and
   `prompt(...)` for a short justification. Empty or cancelled →
   abort. Non-empty → POST
   `/publish?force=true&reason=<url_encoded>`.

The reason lands in `admin_bypass_events.details.reason` (mig-164,
wired in v0.10.5). Every force is auditable six months from now.

No backend change — this is a UI-layer fix over
`publish_agent_handler`'s existing contract.

### 2. `agents.user_id` in `eval_brier.rs`

`src/handlers/eval_brier.rs`

Two SQL sites referenced `agents.owner_id`; the column is
`agents.user_id` (mig-006, `AGENT_COLUMNS` in
`agent-bestiary/memory/src/store.rs` maps it to the Rust field
`owner_id` at read time).

- L91-93: subquery `SELECT owner_id FROM agents …` →
  `SELECT user_id FROM agents …`
- L108: `JOIN agents a ON a.owner_id = f.owner_id` →
  `JOIN agents a ON a.user_id = f.owner_id`

Type parity: both sides are TEXT after mig-006 (`agents.user_id`) and
mig-165 (`fermi_forecasts.owner_id`). No `::uuid` cast in either
site — clean.

Symptom this fixes: whenever the BrierLookup fell back to the
owner-JOIN branch (i.e. the agent's name wasn't found in any
`fermi_forecasts.agents_used` JSONB), the query 500'd with
`column "owner_id" does not exist`. Silently degraded to `None` for
callers who don't inspect the error, which is why we hadn't seen it
until non-admin users started resolving forecasts under
v0.10.9/v0.10.10.

### 3. `resolve_agent` accepts UUID as well as agent_name

`src/api_server.rs::resolve_agent`

Now tries `Uuid::parse_str(agent_id)` first; if it parses, looks the
agent up by UUID via `memory_store.get_agent(uuid)`. Falls back to
`get_agent_by_name` on parse failure.

No valid `agent_name` can also parse as a UUID — `slug::validate`
rejects `-`, which UUIDs require — so the two branches are disjoint.

Fixes 404s for admin scripts and audit tools that address by the
actual UUID emitted from `/api/admin/rbac/orphans` and similar
sources.

## Post-deploy verification

Force-publish path (as `ivan@axolotl.partners` on Mario's draft):

```bash
# Via UI: /agent/efra_gorilla → click "Publish (as admin)" → confirm
# → checks fail → prompt appears with the failing check list → enter
# "Sunday admin sweep, description backfilled inline" → submit.

# Or via curl:
curl -si -X POST \
     -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/agents/efra_gorilla/publish?force=true&reason=Sunday%20admin%20sweep"
# → HTTP/2 200
# → JSON: { transition: {from:"draft",to:"published"}, checks:[…],
#           published_by_admin: true, force_used: true }

# Audit trail:
psql -c "SELECT admin_user_id, action, details->>'reason', created_at
         FROM admin_bypass_events
         WHERE target_id = (SELECT agent_id::text FROM agents WHERE agent_name = 'efra_gorilla')
         ORDER BY created_at DESC LIMIT 1;"
```

UUID-addressed `GET /api/agents/:id`:

```bash
AGENT_UUID=$(psql -tA -c "SELECT agent_id FROM agents WHERE agent_name = 'efra_gorilla'")

curl -si -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/agents/$AGENT_UUID"
# → HTTP/2 200 (previously 404)
```

`eval_brier.rs` fallback branch:

```bash
# After a resolved forecast lands and Brier calibration runs, tail
# the server logs for the previous error message:
grep -i 'column "owner_id" does not exist' server.log
# → should be silent post-deploy
```

## Not in scope — deferred to v0.10.16 (design call required)

**Legacy agent names containing `-` or `/`** (Mario's
`efra-ai/04-forensic`, etc.) are unrouteable in the current URL
scheme regardless of what `resolve_agent` does. Axum's tree router
splits on `/`, so `/agent/efra-ai/04-forensic` matches
`/agent/:slug` with `slug = "efra-ai"` and fails to route the
remainder. `validate_http` (added later) rejects these characters
for new agents, but pre-existing data is stranded.

Three options, each with a different tradeoff:

1. **DB rename migration.** Replace `-` and `/` with `_` on the
   affected rows in `agents.agent_name`. Simple. Breaks any external
   bookmarks or documentation pointing to the old name. The
   `agent_name` UNIQUE index means we'd need a conflict resolution
   step; no FK downstream (`workspace_agents`, `agent_versions`
   reference `agent_id UUID`, and `fermi_forecasts.agents_used` is a
   JSONB blob we can leave alone or backfill separately).

2. **`GET /api/agents/by-name?name=…` fallback + catch-all page
   route** (`/agent/*name` with client-side re-normalisation).
   Preserves old URLs by redirect. Adds a second entry point per
   resource — mild substrate drift.

3. **301 redirect at the page layer only.** Server sees a slugged
   path with `-` or `/` in it, redirects to the sanitised form,
   client re-lands on the canonical page. Cheap; still requires
   option (1) in the DB.

Recommendation: (1) with a `abw-cli agents rename-legacy-slugs
--dry-run` step first, so Ivan can eyeball the list before it lands.
Blocks on Ivan's call on whether Mario should get pre-notified.

**Other still-deferred items** (unchanged from v0.10.14 notes):

- Duplicate FK on `fermi_market_observations` (cosmetic).
- Orphaned forecasts across dual-identity accounts (`ivan@axolotl`
  vs `ilabra@gmail`) — reassign UI TBD.
- v0.11.0 "trust contract": boot-time schema-consistency check that
  compares `pg_get_constraintdef()` against migration files.

## Related

- v0.10.5 — introduced `?force=true` on `publish` and the
  `admin_bypass_events` audit table (mig-164).
- v0.10.9 — realigned fermi FK targets → non-admins could save.
- v0.10.10 — `optional_auth_middleware` accepts API keys → Mo's
  `GET /api/agents/efra_thesis` stopped 404-ing.
- v0.10.13 — exhaustive `text = uuid` sweep; flagged
  `eval_brier.rs` as this release's target.
- v0.10.14 — chip-publish UX unified (UI-only).
