#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Run the production integrity audit. Read-only. One command.
# ─────────────────────────────────────────────────────────────────────
#
#   ./scripts/run_integrity_audit.sh                 # uses DATABASE_URL from .env
#   ./scripts/run_integrity_audit.sh "postgres://…"  # or pass one explicitly
#
# Handles the two things that otherwise bite you:
#
#   1. Strips `-pooler` from the Neon host. The audit builds a TEMP table and
#      then selects from it; under PgBouncer transaction-mode pooling those two
#      statements can land on different backends and you get
#      "relation integrity_findings does not exist".
#
#   2. Saves a timestamped transcript, so the result is an artifact you can
#      diff after fixes rather than something scrolling past in a terminal.
#
# Exit codes:  0 = gate passed.  non-zero = gate failed (see the report).

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

command -v psql >/dev/null 2>&1 || {
    echo "error: psql not found. Install postgresql-client." >&2
    exit 1
}

# ── Resolve the connection string ─────────────────────────────────────
URL="${1:-}"
if [ -z "$URL" ] && [ -n "${DATABASE_URL:-}" ]; then
    URL="$DATABASE_URL"
    echo "▸ using DATABASE_URL from environment"
fi
if [ -z "$URL" ] && [ -f .env ]; then
    URL=$(grep -E '^[[:space:]]*DATABASE_URL[[:space:]]*=' .env | tail -1 |
          sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//; s/^'\''//; s/'\''$//')
    [ -n "$URL" ] && echo "▸ using DATABASE_URL from .env"
fi

if [ -z "$URL" ]; then
    cat >&2 <<'EOF'
error: no database URL.

  Pass one:      ./scripts/run_integrity_audit.sh "postgres://user:pass@host/db"
  Or set it:     export DATABASE_URL="postgres://…"
  Or put it in:  .env  (DATABASE_URL=…)
EOF
    exit 1
fi

# ── Force a direct (non-pooled) connection ────────────────────────────
DIRECT="$URL"
case "$URL" in
    *-pooler.*)
        DIRECT="${URL/-pooler./.}"
        echo "▸ rewrote Neon pooler host → direct host (required for TEMP tables)"
        ;;
esac

# Show the host only — never the credentials.
SAFE_HOST=$(printf '%s' "$DIRECT" | sed -E 's#^[a-z+]+://[^@]*@##; s#/.*$##')
echo "▸ target: $SAFE_HOST"

OUT="integrity-audit-$(date +%Y%m%d-%H%M%S).txt"

echo "▸ running 36 read-only checks…"
echo

# `script`-free tee: psql writes both streams, we capture everything.
psql "$DIRECT" -f scripts/integrity_audit.sql 2>&1 | tee "$OUT"
RC=${PIPESTATUS[0]}

echo
echo "════════════════════════════════════════════════════════════════"
echo "▸ transcript saved to: $OUT"
if [ "$RC" -eq 0 ]; then
    echo "▸ RESULT: gate PASSED — no critical violations, no missing objects."
else
    echo "▸ RESULT: gate FAILED (psql exit $RC)."
    echo "  Scroll up to the FINDINGS table. Anything marked VIOLATION is real;"
    echo "  anything marked SKIPPED is UNKNOWN, not passing."
fi
echo "════════════════════════════════════════════════════════════════"

exit "$RC"
