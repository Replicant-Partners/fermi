# v0.10.16 — close the creation-time bypasses

## Why

Ivan asked the sharp question after v0.10.15: **if `slug::validate`
already rejects bad names, how did `efra-ai/04-forensic` get into
the DB in the first place?** The audit surfaced three problems in
the write-side of the agent lifecycle — two live bypasses and one
broken feature. All three explain how Mario's un-routable EFRA
agents came to exist, and closing them prevents the next wave.

The rule (`fermi::slug::validate_http`) landed 2026-05-23 in
commit d0f94e8 and was applied to `POST /api/agents` and
`POST /api/agents/import` at the same time. Everything before
that date was unchecked. The rule was **not** back-propagated to
every agent-creation surface — that oversight is what this release
fixes.

## The audit

Every write path into the `agents` table, re-checked:

| Path | Slug check | Column names | Status before v0.10.16 |
|---|---|---|---|
| `POST /api/agents` → `create_agent_handler` | ✅ | ✅ `user_id` | OK |
| `POST /api/agents/import` → `import_agent_handler` | ✅ | ✅ `user_id` | OK |
| Curated seed → `seed_agents_to_database` | trusted names | ✅ `user_id` | OK |
| `POST /api/workspaces/:id/agents` → `create_workspace_agent_handler` | ❌ **none** | ✅ `user_id` | **Active bypass** |
| `POST /api/agents/:id/fork` → `fork::fork_agent` | ❌ **none** | ❌ **`owner_id` — column doesn't exist** | **Fork 500'd for everyone** |
| `api/agents.rs` (Vercel-function file, unbuilt) | ❌ | ❌ (wrong `Agent` shape) | Dead code |

## Changes

### 1. `create_workspace_agent_handler` — close active bypass

`src/handlers/workspace/core.rs`

Any workspace member could `POST /api/workspaces/:ws_id/agents`
with `{"agent_name": "efra-ai/whatever"}` and land an un-routable
agent in the DB — the sibling handlers had `slug::validate_http`
but this one never did. Added the same one-line guard at the top
of the handler:

```rust
fermi::slug::validate_http("agent_name", &req.agent_name)?;
```

This is the live bypass that most likely produced Mario's legacy
`efra-ai/*` names.

### 2. `fork::fork_agent` — column bug + fork-name slug check

`src/workflows/fork.rs`

Two bugs stacked in one function.

**Bug A (broken feature):** the SELECT and INSERT both referenced
`agents.owner_id` — a column that has never existed. `agents.user_id`
is the real owner column (mig-006). Every fork attempt 500'd at
the SELECT with `column "owner_id" of relation "agents" does not
exist`. Nobody has successfully forked an agent post-mig-006.

- SELECT: `SELECT ..., user_id AS owner_id, ...` — aliased so the
  Rust `SourceAgent` struct field name stays stable and the
  downstream royalty code (`source.owner_id`) is unchanged.
- INSERT: `INSERT INTO agents (..., user_id, ...)` (was `owner_id`).

**Bug B (would-be perpetuator):** the derived fork name is
`{source.agent_name}_fork_{n}`. If the source has a legacy name
like `efra-ai/04-forensic`, the fork inherits the un-routable
shape (`efra-ai/04-forensic_fork_1`). With Bug A fixed, this
would immediately start creating new bad-shape agents.

Fix: validate the derived name against `slug::validate` before
INSERT. On failure, refuse with **400 + detailed error** that
tells the forker exactly why and how to unblock:

```
Cannot fork `efra-ai/04-forensic`: the derived fork name
`efra-ai/04-forensic_fork_1` fails the platform slug rule (slug must
contain only lowercase letters, digits, and underscores). This
happens when the source agent has a legacy name that predates the
URL-safety rule enforced since 2026-05-23 (commit d0f94e8). Legacy
names contain characters (`-` or `/`) that would produce un-routable
URLs on the fork. Ask an admin to rename `efra-ai/04-forensic` to a
snake_case name first, then retry the fork.
```

Slug-compliant sources are unaffected — `efra_thesis_fork_1` etc.
still pass the check.

### 3. Delete dead Vercel-function files

Removed:

- `api/agents.rs`
- `api/execute.rs`
- `api/health.rs`
- `api/` directory itself

These were skeletons from a February 2026 Vercel deploy attempt
that was abandoned in favor of the axum server. `vercel.json`
already builds `src/api_server.rs` — the `api/*.rs` files were
not registered as Vercel functions, would not compile against the
current `Agent` struct shape (references removed fields like
`created_at`), and had no auth or validation. Kept only as a trap
for anyone who thought they were live. Gone now.

## What this release does NOT do

**Legacy-name rename migration.** The un-routable data already in
the DB is still there. This release closes the write paths so no
new bad-shape names can be created, but Mario's existing
`efra-ai/*` agents are still un-routable. That's v0.10.17:

1. `abw-cli agents legacy-slugs --dry-run` — audits every
   `agents.agent_name` against `slug::validate`, prints the
   would-be sanitised name, flags collisions, counts
   `fermi_forecasts.agents_used` JSONB rows that need backfill.
2. `abw-cli agents legacy-slugs --apply` — transactional rename +
   JSONB backfill, prints the final mapping table.

## Post-deploy verification

Bypass A — workspace-scoped creation now rejects bad names:

```bash
WS_ID=$(psql -tA -c "SELECT id FROM teams WHERE slug = 'some_workspace'")

curl -si -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"agent_name":"efra-ai/whatever","agent_type":"research","executor_type":"llm","model":"claude-sonnet-4-5","temperature":0.3}' \
     "https://agent-bestiary.world/api/workspaces/$WS_ID/agents"
# → HTTP/2 400
# → "Invalid agent_name: slug must contain only lowercase letters, digits, and underscores"
```

Bug A — fork on a slug-compliant source now succeeds:

```bash
SRC_UUID=$(psql -tA -c "SELECT agent_id FROM agents WHERE agent_name = 'biotech_analyst'")

curl -si -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"include_ontology":false,"include_embeddings":false}' \
     "https://agent-bestiary.world/api/agents/$SRC_UUID/fork"
# → HTTP/2 200 (previously 500: column "owner_id" does not exist)
# → { agent_id, agent_name: "biotech_analyst_fork_1", total_cost, author_royalty }
```

Bug B — fork on a legacy-name source refuses with detailed 400:

```bash
LEGACY_UUID=$(psql -tA -c "SELECT agent_id FROM agents WHERE agent_name = 'efra-ai/04-forensic'")

curl -si -X POST \
     -H "Authorization: Bearer $TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"include_ontology":false,"include_embeddings":false}' \
     "https://agent-bestiary.world/api/agents/$LEGACY_UUID/fork"
# → HTTP/2 400
# → "Cannot fork `efra-ai/04-forensic`: the derived fork name
#    `efra-ai/04-forensic_fork_1` fails the platform slug rule ...
#    Ask an admin to rename ... first, then retry the fork."
```

Dead code:

```bash
ls fermi/api 2>&1
# → ls: cannot access 'fermi/api': No such file or directory
```

## Related

- 2026-05-23 (d0f94e8) — `slug::validate` introduced,
  `create_agent_handler` + `import_agent_handler` locked down.
- v0.10.5 — RBAC substrate; publish handler on `rbac::require_admin_on`.
- v0.10.9 — realigned fermi FK targets → non-admin save unblocked.
- v0.10.10 — `optional_auth_middleware` accepts API keys.
- v0.10.13 — exhaustive `text = uuid` sweep on fermi tables.
- v0.10.15 — admin force-publish path, `eval_brier.rs` column fix,
  UUID-safe `resolve_agent`.

Sibling column-name bugs found in the same audit: `eval_brier.rs`
(fixed in v0.10.15) and this one. That is now the complete set of
`agents.owner_id` references in src/ — verified by:

```
grep -rn 'agents\.owner_id\|agents (.*owner_id' src/
# → only comments explaining the historical bug remain
```
