#!/usr/bin/env bash
# How far did seed() get? READ-ONLY.
# seed() writes in a fixed order: agents, episodes, rules, entities, facts,
# communities, consolidation_jobs. The last non-zero relation is the last
# step that completed, which localises a mid-seed failure.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -qtA <<'SQL'
SELECT 'agents='             || count(*) FROM agents             WHERE agent_id::text     ~ '^0000000[0-2]-'
UNION ALL SELECT 'episodes='   || count(*) FROM episodes           WHERE episode_id::text   ~ '^0000000[0-2]-'
UNION ALL SELECT 'rules='      || count(*) FROM semantic_rules     WHERE rule_id::text      ~ '^0000000[0-2]-'
UNION ALL SELECT 'entities='   || count(*) FROM entities           WHERE entity_id::text    ~ '^0000000[0-2]-'
UNION ALL SELECT 'facts='      || count(*) FROM facts              WHERE fact_id::text      ~ '^0000000[0-2]-'
UNION ALL SELECT 'communities='|| count(*) FROM communities        WHERE community_id::text ~ '^0000000[0-2]-'
UNION ALL SELECT 'jobs='       || count(*) FROM consolidation_jobs WHERE agent_id::text     ~ '^0000000[0-2]-';
SQL
