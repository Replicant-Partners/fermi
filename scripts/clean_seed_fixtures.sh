#!/usr/bin/env bash
#
# Remove stranded rows from the agent-bestiary-memory seed fixture.
#
# `SeedData` builds deterministic UUIDs via make_uuid(agent_idx, table_code,
# item_idx) = (agent_idx << 96) | (table_code << 64) | item_idx, so every
# seeded row has an id matching ^0000000[0-2]-. Because the ids are fixed
# rather than random, a run that dies before cleanup poisons every later run
# with a duplicate-key error on episodes_pkey.
#
# Deletes agents and lets ON DELETE CASCADE do the rest. Every seeded child
# table (episodes, entities, facts, semantic_rules, communities,
# consolidation_jobs) is CASCADE from agents, and Postgres orders the cascade
# correctly — which hand-rolled per-table deletes do not: episodes carry
# `consolidation_job_id` referencing consolidation_jobs, so deleting jobs
# first aborts the whole transaction.
#
# Pass --apply to delete. Default is a dry-run count.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

MODE="${1:-}"

psql "$DATABASE_URL" -X -q -v ON_ERROR_STOP=1 <<'SQL'
\pset pager off
\echo '=== Stranded seed-fixture rows ==='
SELECT 'agents'              AS relation, count(*) FROM agents             WHERE agent_id::text     ~ '^0000000[0-2]-'
UNION ALL SELECT 'episodes',            count(*) FROM episodes           WHERE episode_id::text   ~ '^0000000[0-2]-'
UNION ALL SELECT 'semantic_rules',      count(*) FROM semantic_rules     WHERE rule_id::text      ~ '^0000000[0-2]-'
UNION ALL SELECT 'entities',            count(*) FROM entities           WHERE entity_id::text    ~ '^0000000[0-2]-'
UNION ALL SELECT 'facts',               count(*) FROM facts              WHERE fact_id::text      ~ '^0000000[0-2]-'
UNION ALL SELECT 'communities',         count(*) FROM communities        WHERE community_id::text ~ '^0000000[0-2]-'
UNION ALL SELECT 'consolidation_jobs',  count(*) FROM consolidation_jobs WHERE job_id::text       ~ '^0000000[0-2]-'
ORDER BY 1;
SQL

if [ "$MODE" != "--apply" ]; then
  echo ""
  echo "Dry run. Re-run with --apply to delete these rows."
  exit 0
fi

echo ""
echo "── deleting agents (children follow via CASCADE)"
psql "$DATABASE_URL" -X -q -v ON_ERROR_STOP=1 \
  -c "DELETE FROM agents WHERE agent_id::text ~ '^0000000[0-2]-';"
echo "── done"
