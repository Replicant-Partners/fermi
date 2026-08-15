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
         197_claim_episode_correlation \
         198_episode_delegation_tree; do
    echo "→ migration $m"
    run_sql "$HERE/migrations/$m.sql" "$m"
done

# Idempotency: the platform re-runs migrations on every boot, so a second
# application must be a no-op rather than an error.
# Idempotency, tested the way production actually does it. `run_migrations()`
# keeps NO applied-state table: it re-runs every file, in list order, on every
# boot. So the test is not "is each file idempotent in isolation" but "does the
# whole sequence survive being replayed". Replaying in order is what catches a
# later migration widening a view that an earlier one then tries to shrink --
# which fails with "cannot drop columns from view" and takes the boot down.
echo "→ second boot: replaying the whole sequence in registration order"
for m in 193_route_provenance_outcomes \
         194_episode_cost_basis \
         197_claim_episode_correlation \
         198_episode_delegation_tree; do
    run_sql "$HERE/migrations/$m.sql" "$m-again"
done

echo "→ third boot: once more, to catch state that only diverges after two"
for m in 193_route_provenance_outcomes \
         194_episode_cost_basis \
         197_claim_episode_correlation \
         198_episode_delegation_tree; do
    run_sql "$HERE/migrations/$m.sql" "$m-again2"
done

echo "→ assertions"
docker cp "$HERE/scripts/sql/cost_attribution_assertions.sql" \
    "$CONTAINER:/tmp/assert.sql" >/dev/null
docker exec "$CONTAINER" psql -U "$PGUSER" -d "$DB" \
    -v ON_ERROR_STOP=1 -f /tmp/assert.sql

echo
echo "✓ cost attribution smoke passed"
