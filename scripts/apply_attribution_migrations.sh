#!/usr/bin/env bash
#
# Apply the combinatorial credit assignment migrations (mig-187, mig-188).
#
# These are already registered in api_server.rs's boot migration list, so a
# server restart would run them too; this just does it without a boot. Both
# files are CREATE TABLE/INDEX IF NOT EXISTS throughout — additive and
# idempotent, safe to re-run.
#
# See docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

for f in migrations/187_forecast_agent_claims.sql \
         migrations/188_forecast_attributions.sql; do
  echo "── applying $f"
  psql "$DATABASE_URL" -X -q -v ON_ERROR_STOP=1 -f "$f"
done

echo ""
echo "── resulting relations"
psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off
SELECT t.name AS table_name,
       to_regclass('public.' || t.name) IS NOT NULL AS exists_in_db
FROM (VALUES
  ('forecast_agent_claims'),
  ('forecast_attributions'),
  ('forecast_agent_credit'),
  ('forecast_agent_interactions')
) AS t(name)
ORDER BY 1;
SQL
