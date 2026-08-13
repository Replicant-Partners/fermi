#!/usr/bin/env bash
#
# Stand up a CI-equivalent test database and print its DATABASE_URL.
#
# CI's `Test Suite` job runs several DB-backed targets that
# `scripts/ci-skipped-steps.sh` cannot cover — the agent-bestiary-memory unit
# tests, the API integration tests, the live schema-trust and rollup checks.
# Those are precisely the steps that had never run while the migration ratchet
# was tripped, so being able to run them locally is the difference between
# "CI should be green now" and knowing.
#
# Applies migrations in run_migrations() order, tolerating the 26 that are
# known to fail against an empty database (see docs/plans/CI_MIGRATION_RATCHET.md).
#
#   eval "$(./scripts/local-test-db.sh start)"
#   cargo test --lib -p agent-bestiary-memory -- --test-threads=1
#   ./scripts/local-test-db.sh stop

set -uo pipefail

CONTAINER=fermi-local-test-db
PORT="${PORT:-55433}"
cd "$(git rev-parse --show-toplevel)" || exit 1

case "${1:-start}" in
stop)
    docker rm -f "$CONTAINER" >/dev/null 2>&1
    echo "stopped $CONTAINER" >&2
    exit 0
    ;;
start) ;;
*)
    echo "usage: $0 [start|stop]" >&2
    exit 1
    ;;
esac

docker rm -f "$CONTAINER" >/dev/null 2>&1
echo "Starting $CONTAINER on :$PORT …" >&2
docker run -d --name "$CONTAINER" \
    -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=fermi_test \
    -p "$PORT":5432 pgvector/pgvector:pg16 >/dev/null || exit 1

for _ in $(seq 1 90); do
    docker exec "$CONTAINER" pg_isready -U postgres -q >/dev/null 2>&1 && break
    sleep 1
done

PSQL=(docker exec -i -e PGPASSWORD=postgres "$CONTAINER" psql -U postgres -d fermi_test)
"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "uuid-ossp";' >/dev/null 2>&1
"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "vector";' >/dev/null 2>&1

failed=0
while IFS= read -r f; do
    [ -z "$f" ] && continue
    [ ! -f "$f" ] && continue
    "${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f - < "$f" >/dev/null 2>&1 || failed=$((failed + 1))
done <<< "$(grep -oE '"migrations/[^"]+\.sql"' src/api_server.rs | tr -d '"')"

echo "Migrations applied with $failed known failure(s)." >&2
echo "export DATABASE_URL=postgres://postgres:postgres@localhost:$PORT/fermi_test"
