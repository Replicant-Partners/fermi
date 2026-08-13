#!/usr/bin/env bash
#
# Spec 31 forecast-history coverage check. READ-ONLY (SELECT only).
#
# Answers "does the History tab actually show the evolution of every
# forecast?" from the data rather than from the code. Spec 31 shipped
# 2026-08-06 (c8073ed9); lazy provisioning means forecasts untouched
# since before then legitimately have no repo, so coverage is only
# meaningful for the post-ship cohort.
#
# This originally surfaced a gap since fixed: `apply_settlement` in
# src/handlers/polymarket.rs resolved forecasts with raw SQL and never
# called the Spec 31 commit hook, while `reconcile_once` in
# src/handlers/forecast_git.rs INNER JOINed teams and so could never pick
# up a forecast with no workspace_id. Auto-resolved forecasts recorded
# nothing for the one event that is terminal and unrevertible. Both are
# now closed; section 6 is the regression check and should stay empty.
#
# Usage: bash scripts/history_verify.sh
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off

\echo '=== 1. Overall coverage ==='
SELECT
  count(*)                                              AS forecasts,
  count(f.workspace_id)                                 AS with_workspace,
  count(*) FILTER (WHERE COALESCE(t.git_commit_count,0) > 0) AS versioned,
  round(100.0 * count(*) FILTER (WHERE COALESCE(t.git_commit_count,0) > 0)
        / NULLIF(count(*),0), 1)                        AS pct_versioned
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id;

\echo ''
\echo '=== 2. Split by Spec 31 ship date (pre-ship gaps are by design) ==='
SELECT
  CASE WHEN f.updated_at >= DATE '2026-08-06' THEN 'touched post-ship'
       ELSE 'untouched since pre-ship' END              AS cohort,
  count(*)                                              AS forecasts,
  count(*) FILTER (WHERE COALESCE(t.git_commit_count,0) > 0) AS versioned,
  count(*) FILTER (WHERE f.workspace_id IS NULL)        AS no_workspace
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
GROUP BY 1 ORDER BY 1;

\echo ''
\echo '=== 3. The invariant: no workspace => never versioned ==='
SELECT
  f.workspace_id IS NOT NULL                            AS has_workspace,
  count(*)                                              AS forecasts,
  count(*) FILTER (WHERE COALESCE(t.git_commit_count,0) > 0) AS versioned
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
GROUP BY 1 ORDER BY 1;

\echo ''
\echo '=== 4. Coverage by resolution path (auto-resolve bypasses the hook) ==='
SELECT
  COALESCE(f.resolution_source, '(unresolved)')         AS resolution_source,
  count(*)                                              AS forecasts,
  count(*) FILTER (WHERE COALESCE(t.git_commit_count,0) > 0) AS versioned
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
GROUP BY 1 ORDER BY 2 DESC;

\echo ''
\echo '=== 5. Post-ship cohort, forecast by forecast ==='
SELECT
  left(f.id, 8)                                         AS forecast,
  left(f.question_text, 30)                             AS question,
  f.status,
  COALESCE(f.resolution_source, '-')                    AS resolved_via,
  f.workspace_id IS NOT NULL                            AS has_ws,
  COALESCE(t.git_commit_count, 0)                       AS commits,
  to_char(f.updated_at, 'MM-DD HH24:MI')                AS updated
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
WHERE f.updated_at >= DATE '2026-08-06'
ORDER BY f.updated_at DESC;

\echo ''
\echo '=== 6. Resolved with no commit: pre-fix casualties vs regressions ==='
-- The hook in apply_settlement landed 2026-08-12. Anything resolved before
-- then with an empty log is a casualty of the old bypass and cannot be
-- repaired (resolution is terminal; there is no later write to hook, and
-- fabricating a commit would assert a state we never observed).
-- Anything resolved AFTER that date with an empty log is a real regression.
SELECT
  CASE WHEN f.resolved_at >= DATE '2026-08-12'
       THEN 'REGRESSION - investigate'
       ELSE 'pre-fix casualty (expected)' END           AS verdict,
  left(f.id, 8)                                         AS forecast,
  COALESCE(f.resolution_source, '-')                    AS resolved_via,
  to_char(f.resolved_at, 'MM-DD HH24:MI')               AS resolved
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
WHERE f.resolved_at >= DATE '2026-08-06'
  AND COALESCE(t.git_commit_count,0) = 0
ORDER BY f.resolved_at DESC;

\echo ''
\echo '=== 7. Forward exposure: open forecasts one sweep from resolution ==='
SELECT
  f.status,
  f.workspace_id IS NOT NULL                            AS has_workspace,
  COALESCE(t.git_commit_count,0) > 0                    AS has_history,
  count(*)                                              AS forecasts,
  count(*) FILTER (WHERE f.metadata->'polymarket'->>'pm_market_id' IS NOT NULL)
                                                        AS pm_linked
FROM fermi_forecasts f
LEFT JOIN teams t ON t.id = f.workspace_id
WHERE f.status IN ('active','draft')
GROUP BY 1,2,3
ORDER BY 1,2,3;
SQL
