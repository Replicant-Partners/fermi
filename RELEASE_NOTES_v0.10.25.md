# v0.10.25 — Clean up the test-fixture cruft, plug the leak

## Why

The v0.10.20 audit and v0.10.24 batching surfaced 574 "legacy" rows
in `agents`, of which **565 were `test_agent_<uuid>`** garbage from
6 unclosed test fixtures across `agent-bestiary/memory/src/`. Every
`cargo test` on that crate against the shared prod-shaped DB added
another handful of un-owned agents to production.

The rename tool (v0.10.20) would have preserved these — the correct
remedy is `DELETE`, gated by strong safety criteria. This release
ships that plus the test-side fix so the leak stops.

The read-side has been coping by filtering `WHERE agent_name NOT
LIKE 'test_agent_%'` in every listing endpoint (`admin_stats`,
`admin_list_agents`, `admin_agent_ownership_audit`, public
`list_agents`, `projector::collect_global_embeddings`). Fine as a
defensive layer, but the invariant should be "the DB doesn't
contain garbage" — not "every read remembers to filter garbage
out."

## Change

### 1. Migration 169 — CASCADE the mig-049 FKs

`migrations/169_akp_foundation_fks_cascade.sql`

Every FK on `agents(agent_id)` across the platform declares
`ON DELETE CASCADE` **except** the 4 in mig-049
(`agent_alignments.source_agent_id/target_agent_id`,
`pairwise_coherence.agent_a_id/agent_b_id`,
`knowledge_transfers.source_agent_id/target_agent_id`,
`agent_interaction_policies.agent_id`). Without CASCADE, a `DELETE
FROM agents` blocks with FK violation whenever a mig-049 row
references it. Semantically these SHOULD cascade — alignments and
coherence scores are derived data whose meaning collapses when the
agent goes away.

Migration probes `information_schema.referential_constraints` for
each constraint's current `delete_rule`, skips if already CASCADE,
otherwise `DROP CONSTRAINT` + `ADD CONSTRAINT ... ON DELETE CASCADE`
in per-constraint DO blocks with EXCEPTION handlers. Idempotent,
PgBouncer-safe, RAISE NOTICE observability. Registered in
`run_migrations()`.

### 2. Backend: `admin_cleanup_test_cruft_handler`

`src/handlers/admin.rs`

Route: `/api/admin/agents/cleanup-test-cruft`
  - `GET`                     — dry-run audit
  - `POST`                    — dry-run audit
  - `POST ?apply=true`        — execute DELETE (cascades to everything
                                 FK-linked; mig-169 covers mig-049)

Query params:
  - `apply` (bool, default false) — mutate vs. audit
  - `prefix` (str, default `test_agent_`) — name-prefix filter
  - `older_than_hours` (int, default 24) — grace period
  - `limit` (int) — batch cap

**Safety criteria — always enforced server-side:**

```
WHERE agent_name LIKE '<prefix>%'
  AND total_executions = 0                    -- never ran real work
  AND created_at < NOW() - <grace> hours       -- protects in-flight tests
  AND tier NOT IN ('curated', 'system')       -- protects platform agents
```

Deliberately **not** gated on `visibility` or `status` — the
leaking test fixtures set `visibility = 'public'` (either
explicitly in the `test_agent()` factory, or via mig-010's default
column value on raw INSERT). A visibility check would PROTECT the
exact rows we want to clean up.

Every deletion lands in `admin_bypass_events` with a full row
snapshot (`{agent_id, agent_name, tier, created_at, prefix,
older_than_hours, reason}`) so the audit trail is legible six
months from now. Audit rows are inserted BEFORE the DELETE in the
same transaction so the trail is guaranteed either both-land or
both-rollback.

Response body:

```json
{
  "prefix": "test_agent_",
  "older_than_hours": 24,
  "limit": 50,
  "apply": true,
  "total_matched": 565,
  "in_this_batch": 50,
  "truncated": true,
  "deleted": 50,
  "failures": [],
  "entries": [{ "agent_id": …, "agent_name": …, "tier": …, "created_at": …, "action_taken": "deleted" }, …]
}
```

Route wired for both `GET` and `POST` in `src/api_server.rs`.

### 3. CLI: `abw admin agents cleanup-test-cruft`

`crates/abw-cli/src/commands/admin.rs`

```
abw admin agents cleanup-test-cruft
    [--apply]                  # else dry-run
    [--prefix <str>]           # default: test_agent_
    [--older-than-hours <n>]   # default: 24
    [--limit <n>]              # cap batch
    [--json]                   # raw JSON
```

Pretty-table output shows a compact preview (first 10 + last 5 for
big batches) with columns `AGENT_NAME`, `CREATED_AT`, `TIER`, and
`STATUS`. Summary reports batched/matched/deleted/failures.
Truncation state surfaced with the resume tip when `--limit`
clipped the tail.

### 4. Test-side leak plugged

**`agent-bestiary/memory/src/locking.rs`** — 4 tests + a new
`cleanup_test_agent(pool, agent_id)` helper.

**`agent-bestiary/memory/src/store.rs`** — 6 tests + a
`cleanup_test_agent(store, agent_id)` helper. Also flipped the
`test_agent()` factory from `visibility: "public"` to `"private"`
so future fixtures come out clean regardless of teardown state.

**`agent-bestiary/memory/src/consolidation.rs`** — 1 test with
inline cleanup.

Every test that upserts a `test_agent_<uuid>` row now calls
`cleanup_test_agent` at the end, which does `DELETE FROM agents
WHERE agent_id = $1`. CASCADE takes care of episodes, entities,
semantic_rules, consolidation_jobs, workspace_agents, versions,
etc. so we don't need to know the full FK graph in the test code.

Panics in the middle of a test still leak — Rust doesn't run code
after a panic point in the test function. That's acceptable: the
server-side cleanup handler will sweep up leaked panics on demand.

## Suggested workflow

```bash
# 1. Audit — dry-run, see the scope.
abw admin agents cleanup-test-cruft
# → 565 orphan rows match cleanup criteria; 565 shown in this batch.

# 2. Chunk the actual delete under the client timeout.
abw admin agents cleanup-test-cruft --limit 100 --apply
# → 100 deleted, 465 remaining. Re-run.

# Repeat until:
abw admin agents cleanup-test-cruft --apply
# → summary: 0 matched. Clean.

# 3. Verify the DB is clean.
psql -c "SELECT COUNT(*) FROM agents WHERE agent_name LIKE 'test_agent_%';"
# → 0

# 4. Verify audit trail landed.
psql -c "SELECT COUNT(*), MIN(created_at), MAX(created_at)
         FROM admin_bypass_events
         WHERE action = 'delete_test_cruft';"
# → 565, and the timestamps span the batch runs.
```

## What this release does NOT do

**Retroactive test-fixture rewrite.** The 5 leaking tests will
still leak if they panic mid-execution — Rust's panic-unwind
doesn't run explicit cleanup code after the panic point. A more
robust fix would be a `Drop`-based guard or a `#[tokio::test]`
wrapper macro. That's real infrastructure and out of scope; the
server-side cleanup is the answer for the panic case.

**Other cruft prefixes.** This release specifically targets
`test_agent_*` rows because that's what's in the DB. If other
prefixes accumulate later (e.g. `bench_agent_*`), the `--prefix`
flag covers them without new code.

**Deletion of the read-side `NOT LIKE 'test_agent_%'` filters.**
Left in place as defense-in-depth. Removing them is a v0.10.26
candidate once the DB has been proven clean for a few days.

## Post-deploy verification

```bash
# mig-169 landed cleanly.
psql -c "SELECT tc.constraint_name, rc.delete_rule
         FROM information_schema.table_constraints tc
         JOIN information_schema.referential_constraints rc USING (constraint_name)
         WHERE tc.constraint_name IN (
           'agent_alignments_source_agent_id_fkey',
           'pairwise_coherence_agent_a_id_fkey',
           'knowledge_transfers_source_agent_id_fkey',
           'agent_interaction_policies_agent_id_fkey');"
# → all 4 (and the twins) show delete_rule = 'CASCADE'.

# Endpoint is live.
curl -s -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/admin/agents/cleanup-test-cruft" \
     | jq '.total_matched'
# → 565 (or whatever the current orphan count is)

# CLI works end-to-end.
abw admin agents cleanup-test-cruft
```

## Related

- v0.10.20 — legacy-slug audit + rename tool (the sibling for
  un-routable name data).
- v0.10.23 — perf fix that made the audit fast enough to actually
  use (mig-168 GIN index + N+1→1 rewrite).
- v0.10.24 — `--prefix` and `--limit` on legacy-slugs (same
  pattern applied here for cleanup).
- v0.10.26 (candidate) — remove the read-side `NOT LIKE
  'test_agent_%'` filters once prod is clean.
- v0.11.0 — trust-contract boot check. Would refuse deploy if
  `SELECT COUNT(*) FROM agents WHERE agent_name LIKE 'test_agent_%'`
  is non-zero, catching leaks at the deploy line.
