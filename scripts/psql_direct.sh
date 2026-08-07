#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# psql against the DIRECT (non-pooled) database, with args passed through.
# ─────────────────────────────────────────────────────────────────────
#
#   ./scripts/psql_direct.sh -f scripts/integrity_triage.sql
#   ./scripts/psql_direct.sh -c 'SELECT count(*) FROM users'
#
# Resolves DATABASE_URL from the environment or .env, rewrites the Neon
# `-pooler` host to the direct host, and never prints the credentials.
#
# Use this for anything involving TEMP tables, session state, or advisory
# locks — all of which break under PgBouncer transaction-mode pooling.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

command -v psql >/dev/null 2>&1 || { echo "error: psql not found." >&2; exit 1; }

URL="${DATABASE_URL:-}"
if [ -z "$URL" ] && [ -f .env ]; then
    URL=$(grep -E '^[[:space:]]*DATABASE_URL[[:space:]]*=' .env | tail -1 |
          sed -E 's/^[^=]*=[[:space:]]*//; s/^"//; s/"$//; s/^'\''//; s/'\''$//')
fi
[ -z "$URL" ] && { echo "error: no DATABASE_URL in env or .env" >&2; exit 1; }

DIRECT="$URL"
case "$URL" in
    *-pooler.*) DIRECT="${URL/-pooler./.}" ;;
esac

SAFE_HOST=$(printf '%s' "$DIRECT" | sed -E 's#^[a-z+]+://[^@]*@##; s#/.*$##')
echo "▸ direct connection: $SAFE_HOST" >&2

exec psql "$DIRECT" "$@"
