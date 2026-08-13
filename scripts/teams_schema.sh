#!/usr/bin/env bash
# What's needed to insert a users row and a teams row referencing it. READ-ONLY.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a
psql "$DATABASE_URL" -X -q <<'SQL'
\pset pager off
\echo '=== teams.owner_id FK target column ==='
SELECT a.attname AS local_col, c.confrelid::regclass AS parent, af.attname AS parent_col
FROM pg_constraint c
JOIN unnest(c.conkey)  WITH ORDINALITY AS k(attnum, ord)  ON true
JOIN unnest(c.confkey) WITH ORDINALITY AS fk(attnum, ord) ON fk.ord = k.ord
JOIN pg_attribute a  ON a.attrelid  = c.conrelid  AND a.attnum  = k.attnum
JOIN pg_attribute af ON af.attrelid = c.confrelid AND af.attnum = fk.attnum
WHERE c.contype = 'f' AND c.conrelid = 'teams'::regclass;

\echo ''
\echo '=== users: NOT NULL columns without defaults ==='
SELECT column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'users' AND is_nullable = 'NO'
ORDER BY ordinal_position;
SQL
