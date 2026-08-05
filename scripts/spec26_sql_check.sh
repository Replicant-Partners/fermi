#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Spec 26 SQL validation — offline, throwaway cluster
# ─────────────────────────────────────────────────────────────────────
#
# The collaboration feature added ~20 runtime SQL strings (sqlx::query,
# not query!), so nothing about them is checked at compile time. This
# script stands up a disposable Postgres cluster in /tmp, applies a
# fixture schema that mirrors production's real column TYPES, runs
# migration 176 twice (idempotency), then:
#
#   PART A — PREPAREs every query verbatim. Catches typos, bad `::text`
#            casts, and UNION arity/type mismatches — the whole class of
#            bug that otherwise only shows up as a 500 on first call.
#   PART B — asserts the inheritance rule and its leak guard behave as
#            Spec 26 §2.1 specifies. This is the safety-relevant part:
#            the guard is what stops "add a colleague's private forecast
#            to my portfolio, then share the portfolio" from being a
#            privilege-escalation primitive.
#
# Requires only initdb/pg_ctl/psql on PATH — no server, no credentials,
# and it never touches DATABASE_URL. Safe to run anywhere.
#
# Usage:  scripts/spec26_sql_check.sh
# Exit:   0 = all checks passed

set -euo pipefail

PGBIN="${PGBIN:-/usr/lib/postgresql/17/bin}"
if [ -d "$PGBIN" ]; then
    export PATH="$PGBIN:$PATH"
fi

for tool in initdb pg_ctl psql; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: $tool not on PATH. Set PGBIN to your Postgres bin dir." >&2
        exit 1
    }
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DATA_DIR="$(mktemp -d /tmp/spec26-pg.XXXXXX)"
PORT="${SPEC26_PGPORT:-55432}"
SOCK_DIR="$DATA_DIR/sock"
mkdir -p "$SOCK_DIR"

cleanup() {
    pg_ctl -D "$DATA_DIR/data" stop -m immediate >/dev/null 2>&1 || true
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

echo "▸ initdb ($DATA_DIR)"
initdb -D "$DATA_DIR/data" -U validator --auth=trust -E UTF8 >"$DATA_DIR/initdb.log" 2>&1

echo "▸ starting cluster on port $PORT (unix socket only)"
pg_ctl -D "$DATA_DIR/data" \
    -o "-p $PORT -k $SOCK_DIR -c listen_addresses=''" \
    -l "$DATA_DIR/server.log" start >/dev/null

PSQL="psql -h $SOCK_DIR -p $PORT -U validator -v ON_ERROR_STOP=1 -q"

$PSQL -d postgres -c "CREATE DATABASE spec26"

echo "▸ fixture schema + migration 176 (twice, for idempotency)"
$PSQL -d spec26 -f scripts/spec26_sql_check.sql >"$DATA_DIR/schema.log" 2>&1 || {
    cat "$DATA_DIR/schema.log" >&2
    exit 1
}

echo "▸ PART A: PREPARE every query · PART B: inheritance + leak guard"
# Not -q here: PART B's output IS the assertion record, and a human
# reading CI logs should be able to see the leak guard doing its job.
psql -h "$SOCK_DIR" -p "$PORT" -U validator -d spec26 -v ON_ERROR_STOP=1 \
    -f scripts/spec26_queries_check.sql

echo
echo "✓ Spec 26 SQL checks passed"
