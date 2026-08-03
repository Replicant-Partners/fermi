# v0.10.23 — abw-cli legacy-slugs perf: N+1 → 1, plus GIN index

## Why

Ivan hit this running the v0.10.20 audit tool for the first time:

```
$ abw admin agents legacy-slugs
error: GET https://agent-bestiary.world/api/admin/agents/legacy-slugs
  caused by: operation timed out
```

The v0.10.20 handler was correct but slow. Two compounding
problems:

1. **N+1 JSONB queries.** The audit loop ran one
   `SELECT COUNT(*) FROM fermi_forecasts WHERE agents_used @> …`
   per legacy name. With ~43 legacy names, that's 43 sequential
   containment queries.

2. **No GIN index on `agents_used`.** `fermi_forecasts.agents_used`
   is JSONB and `@>` (containment) is the standard lookup, but the
   column had no supporting index (mig-094 shipped it un-indexed).
   Every containment query seq-scanned the whole table.

`43 sequential seq-scans` >> `60s client timeout`. The abw-cli
reqwest client's default is 60 seconds; the request never got to
send a response body.

(v0.10.21 and v0.10.22 were parallel work on the forecast-save
path: v0.10.21 persisted simulations that were being discarded;
v0.10.22 clamped the whole `[p5, p95]` confidence interval into
`[0,1]` after v0.10.21 fixed only the point estimate. Both
unrelated to this hotfix.)

## Change

### 1. Migration 168 — GIN index on `fermi_forecasts.agents_used`

`migrations/168_fermi_forecasts_agents_used_gin.sql`

```sql
CREATE INDEX idx_forecasts_agents_used_gin
    ON public.fermi_forecasts
    USING gin (agents_used);
```

Idempotent, PgBouncer-safe DO blocks, RAISE NOTICE observability.
No `CONCURRENTLY` — can't run inside a tx, and current
`fermi_forecasts` size is small enough that the ACCESS EXCLUSIVE
lock is sub-second. If the table ever gets big enough to matter,
migrate to an out-of-band rebuild.

Other read sites that get faster for free:

- `handlers/eval_brier.rs::latest_for_agent` — Brier lookup by
  agent_name via `agents_used @> $2::jsonb`.
- `handlers/agents.rs::get_agent_calibration_handler` — same
  containment shape.
- `handlers/forecasts.rs::loop_health_handler` — future JSONB
  containment queries against `agents_used`.

### 2. Handler rewrite — one query instead of N

`src/handlers/admin.rs::admin_legacy_agent_slugs_handler`

Was: 43 separate `SELECT COUNT(*)` round-trips.
Now: one `unnest($1::text[])` LEFT JOIN + GROUP BY that returns
all counts in a single request:

```sql
SELECT ln.name AS name, COUNT(f.id)::int8 AS refs
  FROM unnest($1::text[]) AS ln(name)
  LEFT JOIN fermi_forecasts f
    ON f.agents_used @> jsonb_build_array(
         jsonb_build_object('agent_name', ln.name))
 GROUP BY ln.name
```

Bound `$1` = the full array of legacy names collected during the
audit loop. LEFT JOIN so names with zero references still appear
in the result set (with count 0) — matches the v0.10.20 report
shape exactly.

Combined effect: **one round trip, one seq-scan** (or one GIN
lookup per name when mig-168 is present). Endpoint returns in
milliseconds instead of timing out.

## What the fix looks like from Ivan's terminal

```bash
$ abw admin agents legacy-slugs
  ★ Legacy-slug audit — DRY RUN

    OLD_NAME                    →   PROPOSED_NEW_NAME             REFS   STATUS
    ──────────────────────────────────────────────────────────────────────
    efra-ai/04-forensic         →   efra_ai_04_forensic              2   audit
    efra-ai/05-valuation        →   efra_ai_05_valuation             1   audit
    …

  summary: 43 legacy names, 40 would rename cleanly, 3 blocked by collisions.
  tip: run with `--apply` to execute the rename in a transaction.
```

Returned in <1s post-deploy.

## Post-deploy verification

```bash
# Index exists and is populated.
psql -c "\d public.fermi_forecasts" | grep agents_used_gin
# → idx_forecasts_agents_used_gin | gin (agents_used)

# EXPLAIN the containment query uses the index.
psql -c "EXPLAIN SELECT COUNT(*) FROM fermi_forecasts
         WHERE agents_used @> jsonb_build_array(
           jsonb_build_object('agent_name', 'efra-ai/04-forensic'));"
# → Bitmap Index Scan on idx_forecasts_agents_used_gin

# Endpoint returns quickly.
time curl -s -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/admin/agents/legacy-slugs" \
     | jq '.total_legacy'
# → real  0m0.4xxs (was: timeout at 60s)

# And the CLI Just Works.
abw admin agents legacy-slugs
# → renders the pretty table
```

## Notes

- The client-side `reqwest::Client::builder().timeout(Duration::from_secs(60))`
  in `crates/abw-cli/src/commands/mod.rs::Ctx::http()` is unchanged.
  Bumping it wouldn't have been the right fix — a request that
  can't complete in 60s over a well-connected link is a server
  perf issue, not a client-timeout issue.

- Server-side there's no explicit query timeout on this endpoint;
  the request would eventually complete given enough time. The
  60s timeout was reqwest deciding to give up.

- The v0.10.20 audit is still correct — this release only
  changes the performance characteristics. Numbers reported are
  identical.

## Related

- v0.10.20 — legacy-slug audit + rename tool (the endpoint).
- v0.10.21, v0.10.22 — parallel forecast-save fixes, unrelated.
- mig-094 — created `fermi_forecasts` with `agents_used JSONB`
  but no GIN index (this release fills the gap).
- v0.11.0 (still elevated) — trust-contract boot check would
  flag "hot JSONB column with no GIN index" as a review item.
