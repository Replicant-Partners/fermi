#!/usr/bin/env bash
# Does deleting an agent cascade to its episodes? READ-ONLY.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off
\echo '=== FK delete rules on seed-owned child tables ==='
SELECT c.conrelid::regclass AS child_table,
       a.attname            AS fk_column,
       c.confrelid::regclass AS parent_table,
       CASE c.confdeltype WHEN 'a' THEN 'NO ACTION' WHEN 'r' THEN 'RESTRICT'
                          WHEN 'c' THEN 'CASCADE'   WHEN 'n' THEN 'SET NULL'
                          WHEN 'd' THEN 'SET DEFAULT' END AS on_delete
FROM pg_constraint c
JOIN unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord) ON true
JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum
WHERE c.contype = 'f'
  AND c.confrelid = 'agents'::regclass
  AND c.conrelid::regclass::text IN
      ('episodes','entities','facts','semantic_rules','communities','consolidation_jobs')
ORDER BY 1;

\echo ''
\echo '=== Current stranded seed rows ==='
SELECT 'agents'   AS relation, count(*) FROM agents   WHERE agent_id::text   ~ '^0000000[0-2]-'
UNION ALL SELECT 'episodes', count(*) FROM episodes WHERE episode_id::text ~ '^0000000[0-2]-'
ORDER BY 1;
SQL
