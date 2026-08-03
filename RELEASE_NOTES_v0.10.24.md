# v0.10.24 — legacy-slugs: `--prefix` and `--limit` for targeted, safe batches

## Why

Ivan ran the v0.10.23-perf-fixed audit tool and saw the surprise
in the report:

```
$ time abw admin agents legacy-slugs
  … 574 rows …
  summary: 574 legacy names, 574 would rename cleanly, 0 blocked by collisions.
  real  0m1.419s
```

The audit is fast now (v0.10.23 delivered), but the population is
**two very different sets** hiding under the same "legacy" bucket:

- **9 real legacy names** — Mario's `efra-ai/01-scout` through
  `efra-ai/09-lens`. Un-routable, needs renaming, valuable.
- **565 `test_agent_<uuid>` rows** — test-suite fixtures that
  never got cleaned up. Each is `test_agent_` + a UUID (which
  contains `-`, so they trip the slug rule). Zero forecast refs.
  Zero use.

Renaming 574 in one shot then hit a second problem — the
`--apply` timed out:

```
$ abw admin agents legacy-slugs --apply
error: operation timed out
```

Apply runs three statements per rename in one transaction
(`UPDATE agents` + JSONB rewrite of `fermi_forecasts.agents_used`
+ INSERT into `admin_bypass_events`). 574 × 3 = ~1,722 sequential
statements on the same connection. Even at 5-10 ms per statement
that's tight against the reqwest 60-second client timeout.

Correct fix: **give the operator scope + batch controls** so
targeted work (Mario's 9) doesn't have to wait behind a bulk
sweep, and so bulk work can be chunked into a series of
resumable batches.

## Change

### 1. Backend: `?prefix=…` and `?limit=…` query params

`src/handlers/admin.rs::LegacySlugsQuery`

```rust
pub struct LegacySlugsQuery {
    #[serde(default)] pub apply: bool,
    #[serde(default)] pub prefix: Option<String>,   // v0.10.24
    #[serde(default)] pub limit:  Option<usize>,    // v0.10.24
}
```

- **`prefix`** is pushed down to SQL via `WHERE agent_name LIKE
  $1` (with `%` appended by the handler, not the caller). Fast
  even without a functional index because it's a leading-anchor
  match. `--prefix efra-ai/` returns just the 9 rows.

- **`limit`** is applied **after** the slug-rule filter so the
  cap is meaningful ("first N legacy rows") rather than "first N
  agents alphabetically." Deterministic order (SQL is `ORDER BY
  agent_name`) so re-running with the same flags always picks the
  same batch of N. Safe to resume by re-invoking.

Response body gains three new fields:

```json
{
  ...
  "prefix": "efra-ai/",
  "limit": 50,
  "total_matched": 9,     // full slug-rule-failing set inside prefix
  "truncated": false      // did `limit` clip the tail?
}
```

Callers use `truncated` + `total_matched` to drive multi-batch
loops.

### 2. CLI: `--prefix` and `--limit`

`crates/abw-cli/src/commands/admin.rs`

```
abw admin agents legacy-slugs [--apply] [--json]
                              [--prefix <PREFIX>]
                              [--limit <N>]
```

Both threaded through as query params to the backend. The pretty
table now surfaces the active filter/batch controls up-front:

```
  ★ Legacy-slug audit — DRY RUN
  ↳ prefix filter: efra-ai/
  ↳ limit: 50 (this batch)
```

And when `limit` clipped the tail, the summary explicitly says so
and points at the resume path:

```
  batch: this batch shows 50 of 574 matching legacy names (remaining: 524).
  tip: re-run with the same flags to pick up the next batch (same
       deterministic order).
```

## Suggested workflow — the surgical path

For Mario's 9 real agents, exact-scoped:

```bash
# Audit — should return 9 rows only.
abw admin agents legacy-slugs --prefix efra-ai/

# Apply the same set — one batch, well under the timeout.
abw admin agents legacy-slugs --prefix efra-ai/ --apply
```

For the test-fixture backfill (if you decide to rename rather
than delete — see "Not in scope" below):

```bash
# Chunk to stay under the timeout — 50 at a time.
abw admin agents legacy-slugs --prefix test_agent_ --limit 50 --apply
# → renames 50, reports truncated=true, remaining count in the summary
# Re-run until truncated=false.
```

## Not in scope

**Test-fixture cleanup.** The 565 `test_agent_<uuid>` rows are
mostly garbage from unclosed test runs — they have zero forecast
refs, zero descriptions, zero everything. **Renaming them is
correct-but-wasteful.** A proper cleanup would `DELETE FROM
agents WHERE agent_name LIKE 'test_agent_%' AND total_executions
= 0 AND created_at < NOW() - INTERVAL '7 days'` (plus cascaded
cleanup of any orphan `agent_versions`, `episodes`, etc.). That
belongs in a separate release with its own dry-run tool — v0.10.25
candidate.

Until that lands, Ivan can either:

- Rename them (works with `--prefix test_agent_ --limit 50 --apply`,
  as above), or
- Ignore them (they don't hurt anything except surface area in the
  audit output), or
- Delete them manually via psql if you're confident about the
  criteria.

## Post-deploy verification

```bash
# Prefix filter narrows to Mario's real work.
abw admin agents legacy-slugs --prefix efra-ai/
# → 9 rows, all `efra-ai/*`

# Limit truncates and reports it.
abw admin agents legacy-slugs --limit 5
# → 5 rows, "batch: this batch shows 5 of NNN matching legacy names"

# Combined — apply Mario's 9 in one go.
abw admin agents legacy-slugs --prefix efra-ai/ --apply
# → 9 renames, well under the 60s timeout, admin_bypass_events populated.
```

Direct SQL smoke test:

```bash
psql -c "SELECT agent_name FROM agents
         WHERE agent_name LIKE 'efra-ai/%';"
# → 0 rows post-apply
psql -c "SELECT agent_name FROM agents
         WHERE agent_name LIKE 'efra_ai_%';"
# → 9 rows: efra_ai_01_scout … efra_ai_09_lens
```

## Related

- v0.10.20 — legacy-slug audit + rename tool (the endpoint).
- v0.10.23 — perf fix: mig-168 GIN index + N+1→1 aggregate query.
  Made the dry-run fast; this release makes `--apply` scoped and
  chunkable.
- v0.10.25 (candidate) — test-fixture DELETE-not-RENAME cleanup.
- v0.11.0 (still elevated) — trust-contract boot check.
