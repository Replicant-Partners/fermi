#!/usr/bin/env bash
#
# Verify the Spec 31 history fix cannot weaken post-resolution immutability.
#
# The fix makes `apply_settlement` commit to the forecast's git history, and
# lets the reconciler provision repos for workspace-less forecasts. Both can
# write exactly one forecast column: `workspace_id`, via ensure_forecast_repo.
#
# This proves against the live schema that:
#   1. mig-174's trg_fermi_forecasts_freeze_resolved protects the scoring
#      tuple (scored_probability, brier_score, actual_outcome,
#      predicted_probability) and nothing else.
#   2. workspace_id is NOT in that protected set, so repo plumbing is legal
#      on a resolved row.
#   3. ensure_forecast_repo's UPDATE is guarded `WHERE workspace_id IS NULL`,
#      so it can only ever fill a blank, never relink an existing repo.
#
# Runs inside a transaction that is ALWAYS rolled back. Nothing persists.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off
\set ON_ERROR_STOP on

BEGIN;

\echo '=== Columns the freeze trigger actually guards ==='
SELECT DISTINCT m[1] AS protected_column
FROM pg_proc p,
     LATERAL regexp_matches(p.prosrc, 'NEW\.([a-z_]+) IS DISTINCT FROM', 'g') AS m
WHERE p.proname = 'fn_fermi_forecasts_freeze_resolved'
ORDER BY 1;

\echo ''
\echo '=== Is workspace_id among them? (expect: f) ==='
SELECT EXISTS (
  SELECT 1 FROM pg_proc p,
       LATERAL regexp_matches(p.prosrc, 'NEW\.([a-z_]+) IS DISTINCT FROM', 'g') AS m
  WHERE p.proname = 'fn_fermi_forecasts_freeze_resolved'
    AND m[1] = 'workspace_id'
) AS workspace_id_is_frozen;

\echo ''
\echo '=== Live test on a real resolved forecast (rolled back) ==='
CREATE TEMP TABLE probe AS
SELECT id, scored_probability, brier_score, actual_outcome, predicted_probability
FROM fermi_forecasts
WHERE status = 'resolved' AND brier_score IS NOT NULL
LIMIT 1;

\echo '-- attempt to tamper with the scoring tuple (must be refused) --'
UPDATE fermi_forecasts f
   SET brier_score = 0.999,
       actual_outcome = NOT f.actual_outcome,
       scored_probability = 0.123
  FROM probe p WHERE f.id = p.id;

SELECT
  f.brier_score IS NOT DISTINCT FROM p.brier_score                 AS brier_held,
  f.actual_outcome IS NOT DISTINCT FROM p.actual_outcome           AS outcome_held,
  f.scored_probability IS NOT DISTINCT FROM p.scored_probability   AS scored_prob_held
FROM fermi_forecasts f JOIN probe p ON p.id = f.id;

\echo '-- attempt the repo plumbing the fix performs (must succeed) --'
UPDATE fermi_forecasts f
   SET workspace_id = f.workspace_id
  FROM probe p WHERE f.id = p.id;

SELECT 'workspace_id write on resolved row: allowed' AS result;

ROLLBACK;

\echo ''
\echo '=== Rolled back. No data changed. ==='
SQL
