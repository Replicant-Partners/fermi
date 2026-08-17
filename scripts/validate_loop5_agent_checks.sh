#!/bin/sh
# Validate the agent-scoped Loop 5 MECHANISM checks against the real schema.
#
# These eight queries were added by hand in `LOOP5_AGENT_CHECKS`
# (src/handlers/observatory.rs) and were shipped unvalidated: no offline
# Postgres parser was available, and a malformed check degrades quietly to
# `status: ERROR` -> verdict `inconclusive`, which is honest but tells you
# nothing about whether the SQL was ever right.
#
# The SQL is EXTRACTED FROM THE RUST SOURCE, not retyped here. A hand-copied
# duplicate would be a fourth copy of these checks to keep in step, and would
# validate itself rather than the code that runs.
#
# `PREPARE` rather than `EXPLAIN`: it parses, resolves every column, AND
# type-checks the `$1` parameter, which is exactly the surface a hand-written
# scoped query gets wrong. Nothing is executed and nothing is written; the
# transaction rolls back regardless.

set -eu

if [ -f .env.local ]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env.local
  set +a
fi

URL="${DATABASE_URL_UNPOOLED:-${DATABASE_URL:-}}"
if [ -z "$URL" ]; then
  echo "FAIL: no DATABASE_URL_UNPOOLED or DATABASE_URL in environment or .env.local" >&2
  exit 1
fi

OUT=$(mktemp -t loop5agent.XXXXXX.sql)
trap 'rm -f "$OUT"' EXIT

python3 scripts/emit_loop5_agent_sql.py > "$OUT"

echo "Prepared statements to validate:"
grep -c '^PREPARE' "$OUT"

psql "$URL" -v ON_ERROR_STOP=1 -X -q -f "$OUT"

echo ""
echo "OK: every agent-scoped Loop 5 check parses, resolves and type-checks."
