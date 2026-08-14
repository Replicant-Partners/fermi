#!/usr/bin/env bash
#
# Smoke test for migrations 194 (episode cost basis) + 195 (claim/episode
# correlation + forecast_cost_attribution).
#
# Spins up a throwaway Postgres in Docker, applies the fixture schema at its
# PRE-194 shape, runs migrations 193 → 194 → 195 in order, then asserts the
# arithmetic. Never reads DATABASE_URL and has no flag to point it at another
# database, so it cannot touch production. Same doctrine as
# scripts/smoke_economics.sh.
#
# Also checks idempotency by applying 194 and 195 twice.
#
# Usage:  ./scripts/smoke_cost_attribution.sh
# Exit 0 = all assertions passed.

set -euo pipefail

CONTAINER="abw-cost-attribution-smoke"
DB="migcheck"
PGUSER="postgres"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cleanup() {
    docker rm -f "$CONTAINER" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "→ starting throwaway Postgres ($CONTAINER)"
cleanup
docker run -d --name "$CONTAINER" \
    -e POSTGRES_PASSWORD=smoke \
    -e "POSTGRES_DB=$DB" \
    postgres:16 >/dev/null

echo -n "→ waiting for readiness"
for _ in $(seq 1 40); do
    if docker exec "$CONTAINER" pg_isready -U "$PGUSER" >/dev/null 2>&1; then
        echo " ok"
        break
    fi
    echo -n "."
    sleep 1
done

run_sql() {
    # $1 = local path, $2 = label
    docker cp "$1" "$CONTAINER:/tmp/in.sql" >/dev/null
    docker exec "$CONTAINER" psql -U "$PGUSER" -d "$DB" \
        -v ON_ERROR_STOP=1 -q -f /tmp/in.sql
}

echo "→ fixture schema (pre-194 shape)"
run_sql "$HERE/scripts/sql/cost_attribution_fixture.sql" fixture

for m in 193_route_provenance_outcomes \
         194_episode_cost_basis \
         195_claim_episode_correlation; do
    echo "→ migration $m"
    run_sql "$HERE/migrations/$m.sql" "$m"
done

# Idempotency: the platform re-runs migrations on every boot, so a second
# application must be a no-op rather than an error.
echo "→ re-applying 194 + 195 (idempotency)"
run_sql "$HERE/migrations/194_episode_cost_basis.sql" 194-again
run_sql "$HERE/migrations/195_claim_episode_correlation.sql" 195-again

echo "→ assertions"
docker cp "$HERE/scripts/sql/cost_attribution_assertions.sql" \
    "$CONTAINER:/tmp/assert.sql" >/dev/null
docker exec "$CONTAINER" psql -U "$PGUSER" -d "$DB" \
    -v ON_ERROR_STOP=1 -f /tmp/assert.sql

echo
echo "✓ cost attribution smoke passed"
