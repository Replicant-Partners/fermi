#!/usr/bin/env bash
# Forward exposure to the auto-resolve history gap. READ-ONLY.
# Which still-open forecasts would permanently lose their history if the
# Polymarket sweep resolved them right now?
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off

\echo '=== Open forecasts, by whether a repo already exists ==='
SELECT
  f.status,
  f.workspace_id IS NOT NULL                      AS has_workspace,
  COALESCE(t.git_commit_count,0) > 0              AS has_history,
  count(*)                                        AS forecasts,
  count(*) FILTER (WHERE f.metadata->'polymarket'->>'pm_market_id' IS NOT NULL)
                                                  AS pm_linked_at_risk
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
WHERE f.status IN ('active','draft')
GROUP BY 1,2,3
ORDER BY 1,2,3;
SQL
