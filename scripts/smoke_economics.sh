#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════════
# Pre-deploy smoke test for migration 189 (impersonation audit) and the
# platform-economics queries.
#
#   ./scripts/smoke_economics.sh
#
# Spins up a throwaway Postgres in Docker, applies a minimal fixture
# schema + migration 189, loads known data, and asserts exact results.
#
# SAFETY: this script never reads DATABASE_URL and never connects to
# anything but the container it just created on a random localhost port.
# There is no flag to point it at another database — if you want that,
# you want a different script.
#
# What it proves:
#   * migration 189 applies cleanly and its constraints/cascades work
#   * the three economics queries parse, type-check (via PREPARE) and
#     return correct numbers over known data
#   * the ledger sign convention, window filtering and LEFT JOIN
#     behaviour are all right
#
# What it does NOT prove:
#   * that production's schema matches the fixture shapes. That is the
#     boot-time trust contract's job (/api/admin/schema-health).
# ═══════════════════════════════════════════════════════════════════════
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PG_IMAGE="${PG_IMAGE:-postgres:16-alpine}"
CONTAINER="fermi-smoke-econ-$$"
PORT="$(shuf -i 15432-25432 -n 1)"
PGPASSWORD_VALUE="smoke"
export PGPASSWORD="$PGPASSWORD_VALUE"
PSQL=(psql -h 127.0.0.1 -p "$PORT" -U postgres -d postgres -v ON_ERROR_STOP=1 -q)

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
bold()  { printf '\033[1m%s\033[0m\n'  "$*"; }

cleanup() {
    local code=$?
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
    rm -f "$PREPARED_SQL" 2>/dev/null || true
    if [ "$code" -ne 0 ]; then
        red "✗ smoke test FAILED (exit $code)"
    fi
    exit "$code"
}
PREPARED_SQL=""
trap cleanup EXIT INT TERM

command -v docker >/dev/null || { red "docker is required"; exit 1; }
command -v psql   >/dev/null || { red "psql is required";   exit 1; }

bold "▸ Starting throwaway Postgres ($PG_IMAGE) on port $PORT"
docker run --rm -d \
    --name "$CONTAINER" \
    -e POSTGRES_PASSWORD="$PGPASSWORD_VALUE" \
    -p "127.0.0.1:$PORT:5432" \
    "$PG_IMAGE" >/dev/null

printf '  waiting for readiness'
for i in $(seq 1 60); do
    if docker exec "$CONTAINER" pg_isready -U postgres -q 2>/dev/null; then
        # pg_isready can report ready before the socket accepts TCP.
        if "${PSQL[@]}" -c 'SELECT 1' >/dev/null 2>&1; then
            printf ' ready\n'
            break
        fi
    fi
    printf '.'
    sleep 1
    if [ "$i" -eq 60 ]; then printf '\n'; red "database never became ready"; exit 1; fi
done

bold "▸ Applying fixture schema"
"${PSQL[@]}" -f scripts/sql/smoke_fixture_schema.sql

bold "▸ Applying migration 189 (impersonation audit)"
"${PSQL[@]}" -f migrations/189_impersonation_audit.sql

bold "▸ Re-applying migration 189 (idempotency check)"
"${PSQL[@]}" -f migrations/189_impersonation_audit.sql

bold "▸ Loading fixture data"
"${PSQL[@]}" -f scripts/sql/smoke_fixture_data.sql

# PREPARE the *actual* query files the handler include_str!s. This is
# itself a test: PREPARE fails loudly if the SQL doesn't parse or the
# $n placeholders don't type-resolve against the real column types.
bold "▸ Preparing the three economics queries (parse + type check)"
PREPARED_SQL="$(mktemp)"
{
    printf 'PREPARE by_principal(text, text) AS\n'
    cat src/handlers/sql/economics_by_principal.sql
    printf ';\n\nPREPARE by_agent(text, text) AS\n'
    cat src/handlers/sql/economics_by_agent.sql
    printf ';\n\nPREPARE royalties(text) AS\n'
    cat src/handlers/sql/economics_royalties.sql
    printf ';\n\n'
    cat scripts/sql/smoke_assertions.sql
} > "$PREPARED_SQL"

bold "▸ Running assertions"
"${PSQL[@]}" -f "$PREPARED_SQL"

echo
green "✓ All checks passed — migration 189 and the economics queries are sound."
echo "  (Schema parity with production is a separate concern:"
echo "   GET /api/admin/schema-health)"
