#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Run the Loop 5a mechanical probe against a real database — READ ONLY
# ═══════════════════════════════════════════════════════════════════════
#
# Wraps scripts/loop5_brier_mechanical_check.sql with the two things that
# are easy to get wrong when running it by hand:
#
#   1. DIRECT connection, not the pooler. The probe builds pg_temp tables and
#      reads them back in a later statement. Under PgBouncer transaction-mode
#      pooling those statements can land on different backends and you get
#      "relation does not exist". On Neon the direct host is the one WITHOUT
#      `-pooler`, exposed as DATABASE_URL_UNPOOLED / POSTGRES_URL_NON_POOLING.
#
#   2. Proof of read-only intent. The probe issues no INSERT/UPDATE/DELETE/DDL
#      against application tables — it creates only pg_temp objects, which
#      vanish on disconnect.
#
#      Note we do NOT use `default_transaction_read_only`: Postgres disallows
#      *all* CREATE in a read-only transaction, including CREATE TEMP TABLE,
#      so it would block the probe's own scratch objects. Instead the wrapper
#      statically scans the SQL for mutating statements against non-temp
#      objects and refuses to run if it finds any. That is a weaker guarantee
#      than server-side enforcement — it checks the file, not the session — but
#      it is the one compatible with how the probe works, and it fails closed.
#
# Usage:
#   scripts/run_loop5_probe.sh                 # uses .env.local, then .env
#   scripts/run_loop5_probe.sh <psql-url>      # explicit target
#
# Prints the target host/database (never the credentials) before connecting so
# you can confirm which database you are about to read.
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

cd "$(dirname "$0")/.."

# Override to run a different read-only diagnostic through the same guarded
# path (direct connection + mutation scan). Defaults to the Loop 5 probe.
PROBE="${PROBE_FILE:-scripts/loop5_brier_mechanical_check.sql}"
[ -f "$PROBE" ] || { echo "missing $PROBE" >&2; exit 1; }

URL="${1:-}"

if [ -z "$URL" ]; then
  for envfile in .env.local .env; do
    [ -f "$envfile" ] || continue
    # Prefer the unpooled/direct URLs; fall back to DATABASE_URL last.
    for key in DATABASE_URL_UNPOOLED POSTGRES_URL_NON_POOLING POSTGRES_URL_NO_SSL DATABASE_URL; do
      val="$(grep -m1 "^${key}=" "$envfile" 2>/dev/null | cut -d= -f2- | tr -d '"'\''' || true)"
      if [ -n "${val:-}" ]; then
        URL="$val"
        SRC="$envfile:$key"
        break 2
      fi
    done
  done
fi

[ -n "$URL" ] || { echo "No database URL found. Pass one explicitly." >&2; exit 1; }

# Show where we are pointing, with credentials stripped.
SAFE="$(printf '%s' "$URL" | sed -E 's#(://)[^@]*@#\1***@#')"
echo "Target : $SAFE"
echo "Source : ${SRC:-argument}"
case "$SAFE" in
  *-pooler*) echo "WARNING: this looks like a POOLED host. The probe's temp tables may not"
             echo "         survive; use the -unpooled/non-pooling URL instead." ;;
esac
echo

PSQL_BIN="${PSQL_BIN:-psql}"
command -v "$PSQL_BIN" >/dev/null 2>&1 || {
  echo "psql not found; set PSQL_BIN to a client binary" >&2; exit 1; }

# ── Fail closed: refuse to run anything that could mutate the database ──
# Comments are stripped first so prose describing a write is not mistaken for
# one. `loop5_findings` is the probe's own pg_temp table, so inserts into it are
# expected and allowed.
# Anchored to statement starts. An earlier unanchored version flagged the word
# "drop" inside a diagnostic message ("...this is a genuine drop..."), which is
# prose inside a string literal, not a statement. Anchoring keeps the guard
# honest without teaching it to ignore the keyword outright.
OFFENDING="$(sed 's/--.*//' "$PROBE" \
  | grep -inE '(^|;)[[:space:]]*(UPDATE|DELETE[[:space:]]+FROM|TRUNCATE|ALTER|DROP|GRANT|REVOKE|INSERT[[:space:]]+INTO)\b' \
  | grep -viE 'INSERT[[:space:]]+INTO[[:space:]]+loop5_findings' || true)"

if [ -n "$OFFENDING" ]; then
  echo "REFUSING TO RUN: $PROBE contains statements that could mutate data:" >&2
  echo "$OFFENDING" >&2
  exit 1
fi
echo "Read-only check: no mutating statements found in $PROBE"
echo

"$PSQL_BIN" "$URL" -v ON_ERROR_STOP=1 -f "$PROBE"
