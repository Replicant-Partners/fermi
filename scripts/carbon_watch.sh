#!/usr/bin/env bash
# Watch the next calculate_carbon run land, from the outside.
#
#   bash scripts/carbon_watch.sh          # poll until it concludes
#   bash scripts/carbon_watch.sh --once   # print the current state and exit
#
# Read-only against the DATABASE_URL in .env, which is production. Nothing here
# writes.
#
# ## What it reads, and why these fields
#
# The run is detached from the request that starts it, so "did it work" is a
# question about stored state rather than about an HTTP response. Three rows
# answer it and they answer different things:
#
#   workspace_action_log  what the run CONCLUDED. `apply_result` is null while
#                         it is in flight, so its arrival is the terminal
#                         signal — the same signal the Studio polls. An
#                         `outcome` key means it ended without a statement.
#   episodes              what the run COST and how the tool loop ended.
#                         `error_details` is the field to read first: it
#                         carries `tool_calls=` and `iterations=`, which is
#                         what redirected the 2026-09-16 diagnosis from "the
#                         corpus is empty" to "the flush timed out". An episode
#                         still `running` after the action row is terminal
#                         means the finalising code did not run.
#   carbon_emission_factors  whether anything was BANKED. Until two runs
#                         resolve the same (material, geography, year,
#                         dataset), the cross-run factor check is INERT — an
#                         honest state, but not a pass.
#
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

ONCE=0
[ "${1:-}" = "--once" ] && ONCE=1

SQL=$(cat <<'EOSQL'
\pset pager off
\echo ''
\echo '── runs, newest first ─────────────────────────────────────────────────'
SELECT left(action_id::text, 8) AS action,
       to_char(created_at, 'MM-DD HH24:MI:SS') AS started,
       -- A run past the handler's own RUN_TIMEOUT_SECS (420s) that still has
       -- no `apply_result` cannot legitimately still be running, so the two
       -- cases are separated by age rather than guessed at. `abandoned` is the
       -- shape of the 2026-09-17 loss: the handler future was dropped before
       -- it could finalise anything, and nothing reconciles such a row. Once
       -- the detached-run change is deployed this should stop appearing; if it
       -- appears again, the container was restarted mid-run and a reaper is
       -- the missing piece (NOTE_CARBON_ZERO_TOKEN_EPISODE.md §7.9).
       CASE
         WHEN apply_result IS NULL AND applied IS NOT TRUE
              AND created_at < NOW() - INTERVAL '420 seconds' THEN 'abandoned?'
         WHEN apply_result IS NULL AND applied IS NOT TRUE THEN 'IN FLIGHT'
         WHEN apply_result ->> 'outcome' IS NOT NULL       THEN 'ENDED: ' || (apply_result ->> 'outcome')
         ELSE 'concluded'
       END AS state,
       apply_result ->> 'coverage'             AS coverage,
       apply_result ->> 'total_kg_co2e'        AS total_kg,
       apply_result ->> 'violations'           AS gate,
       apply_result ->> 'factors_recorded'     AS banked,
       apply_result ->> 'hints_offered'        AS hints,
       round((apply_result ->> 'duration_ms')::numeric / 1000, 1) AS secs
  FROM workspace_action_log
 WHERE action_type = 'calculate_carbon'
 ORDER BY created_at DESC
 LIMIT 6;

\echo '── episodes: cost, and how the tool loop ended ───────────────────────'
SELECT to_char(e.timestamp_ref, 'MM-DD HH24:MI:SS') AS at,
       e.execution_status AS status,
       e.tokens_used AS tokens,
       round(e.execution_time_ms / 1000.0, 1) AS secs,
       length(coalesce(e.response_text, '')) AS reply_chars,
       coalesce(jsonb_array_length(e.context -> 'tool_invocations'), 0) AS tool_calls,
       left(coalesce(e.error_details, ''), 96) AS error_details
  FROM episodes e
 WHERE e.agent_id = (SELECT agent_id FROM agents WHERE agent_name = 'carbon_accountant')
 ORDER BY e.timestamp_ref DESC
 LIMIT 6;

\echo '── the factor ledger: is the cross-run check live yet? ───────────────'
SELECT count(*) AS factors_banked,
       count(DISTINCT (material_key, geography, reference_year, dataset_key)) AS distinct_keys,
       count(*) - count(DISTINCT (material_key, geography, reference_year, dataset_key))
         AS comparable_pairs
  FROM carbon_emission_factors;
EOSQL
)

# Via a temp file rather than `-c`. psql's `-c` takes a single command and does
# not run backslash metacommands alongside several statements, so the first
# version of this printed nothing but "Pager usage is off." — a watcher that
# silently watched nothing.
SQLFILE=$(mktemp /tmp/carbon_watch.XXXXXX.sql)
trap 'rm -f "$SQLFILE"' EXIT
printf '%s\n' "$SQL" > "$SQLFILE"

run() {
    PGCONNECT_TIMEOUT=25 bash scripts/psql_direct.sh -f "$SQLFILE" 2>&1 \
      | grep -vE '^▸|^Pager usage is off\.$'
}

if [ "$ONCE" = 1 ]; then run; exit 0; fi

echo "Polling every 20s. Ctrl-C to stop."
while :; do
    out=$(run)
    clear 2>/dev/null || true
    echo "$out"
    if ! echo "$out" | grep -q "IN FLIGHT"; then
        echo
        echo "No run in flight. Latest state is above."
    fi
    sleep 20
done
