#!/usr/bin/env bash
#
# Smoke-test the attribution schema against the queries the design doc and
# the live read path actually issue. READ-ONLY.
#
# Proves the mig-187/188 tables are not just present but shaped correctly:
# the gated credit query from
# docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md ("Reading the
# results") must plan and run. Empty results are expected until a forecast
# resolves; a column/type error is not.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -q -v ON_ERROR_STOP=1 <<'SQL'
\pset pager off

\echo '=== Per-agent credit, gated on both validity checks ==='
SELECT c.agent_name,
       count(*)                                AS n_forecasts,
       round(avg(c.shapley_value)::numeric, 5) AS mean_credit
  FROM forecast_agent_credit c
  JOIN forecast_attributions a
    ON a.forecast_id = c.forecast_id
   AND a.neutralisation = c.neutralisation
 WHERE c.neutralisation = 'identity'
   AND a.efficiency_residual < 1e-6
   AND (a.reconstruction_error IS NULL OR a.reconstruction_error < 0.01)
 GROUP BY c.agent_name
 ORDER BY mean_credit DESC;

\echo ''
\echo '=== Loop 4 pairwise interactions ==='
SELECT agent_a, agent_b,
       round(avg(interaction_index)::numeric, 5) AS mean_interaction
  FROM forecast_agent_interactions
 WHERE neutralisation = 'identity'
 GROUP BY agent_a, agent_b
 ORDER BY abs(avg(interaction_index)) DESC;

\echo ''
\echo '=== Claims ledger ==='
SELECT count(*) AS claims_recorded FROM forecast_agent_claims;

\echo ''
\echo '=== All queries planned and ran. Schema shape is correct. ==='
SQL
