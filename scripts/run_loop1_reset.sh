#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Apply scripts/loop1_reset_unlearned_episodes.sql — THIS ONE WRITES
# ═══════════════════════════════════════════════════════════════════════
#
# Deliberately a separate script from run_loop5_probe.sh. That one refuses to
# execute anything containing a mutating statement, and it should keep refusing;
# adding a "but allow writes sometimes" flag to it would hollow out the only
# guard protecting the read-only diagnostics.
#
# Dry run by default. Writing requires --apply, typed explicitly.
#
#   scripts/run_loop1_reset.sh            # show scope, change nothing
#   scripts/run_loop1_reset.sh --apply    # flip consolidated -> false
#
# Precondition: the extraction credential must resolve, or re-dreaming will
# consume the recovered episodes a second time. Check first with:
#   PROBE_FILE=scripts/loop1_extractor_readiness.sql scripts/run_loop5_probe.sh
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

cd "$(dirname "$0")/.."
SQL="scripts/loop1_reset_unlearned_episodes.sql"
[ -f "$SQL" ] || { echo "missing $SQL" >&2; exit 1; }

APPLY=0
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=1 ;;
    *) echo "unknown argument: $arg" >&2; exit 1 ;;
  esac
done

URL=""
for envfile in .env.local .env; do
  [ -f "$envfile" ] || continue
  for key in DATABASE_URL_UNPOOLED POSTGRES_URL_NON_POOLING DATABASE_URL; do
    val="$(grep -m1 "^${key}=" "$envfile" 2>/dev/null | cut -d= -f2- | tr -d '"'\''' || true)"
    if [ -n "${val:-}" ]; then URL="$val"; SRC="$envfile:$key"; break 2; fi
  done
done
[ -n "$URL" ] || { echo "No database URL found." >&2; exit 1; }

SAFE="$(printf '%s' "$URL" | sed -E 's#(://)[^@]*@#\1***@#')"
echo "Target : $SAFE"
echo "Source : ${SRC:-unknown}"
if [ "$APPLY" = "1" ]; then
  echo "Mode   : APPLY (this will write)"
else
  echo "Mode   : dry run (no writes; pass --apply to write)"
fi
echo

PSQL_BIN="${PSQL_BIN:-psql}"
if [ "$APPLY" = "1" ]; then
  "$PSQL_BIN" "$URL" -v ON_ERROR_STOP=1 -v apply=1 -f "$SQL"
else
  "$PSQL_BIN" "$URL" -v ON_ERROR_STOP=1 -f "$SQL"
fi
