#!/usr/bin/env bash
#
# Reproduce CI's migration ratchet locally and name the migrations that fail.
#
# The CI job (`.github/workflows/ci.yml`, "Set up database") applies every
# migration in `api_server::run_migrations()` order against an empty database
# and counts failures, refusing to let the count grow past `BASELINE`. When
# that ratchet trips, CI tells you the count and nothing else — you get
# "27 migrations fail to apply, baseline is 26" and no way to see which one
# is new without reading raw job logs.
#
# This script prints the list, so the count can be attributed.
#
# Usage:
#   ./scripts/migration-baseline-probe.sh                 # probe HEAD
#   ./scripts/migration-baseline-probe.sh <git-ref>       # probe another ref
#
# Requires docker. Leaves nothing behind.

set -uo pipefail

REF="${1:-}"
PORT="${PORT:-55432}"
CONTAINER="fermi-migration-probe-$$"
WORKTREE=""

cleanup() {
    docker rm -f "$CONTAINER" >/dev/null 2>&1
    if [ -n "$WORKTREE" ] && [ -d "$WORKTREE" ]; then
        git worktree remove --force "$WORKTREE" >/dev/null 2>&1
    fi
}
trap cleanup EXIT

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root" || exit 1

# When probing a ref, materialise it in a throwaway worktree so the working
# tree is never touched. This repo frequently has uncommitted work in it.
if [ -n "$REF" ]; then
    WORKTREE=$(mktemp -d -t fermi-probe-XXXXXX)
    if ! git worktree add --detach "$WORKTREE" "$REF" >/dev/null 2>&1; then
        echo "error: cannot create a worktree for '$REF'" >&2
        exit 1
    fi
    cd "$WORKTREE" || exit 1
    echo "Probing ref: $REF ($(git rev-parse --short HEAD))"
else
    echo "Probing: working tree ($(git rev-parse --short HEAD))"
fi

# pgvector image, matching CI's service container.
echo "Starting postgres on :$PORT …"
docker run -d --name "$CONTAINER" \
    -e POSTGRES_PASSWORD=postgres \
    -e POSTGRES_DB=fermi_test \
    -p "$PORT":5432 \
    pgvector/pgvector:pg16 >/dev/null 2>&1 || {
        echo "error: could not start postgres container" >&2
        exit 1
    }

for _ in $(seq 1 60); do
    if docker exec "$CONTAINER" pg_isready -U postgres >/dev/null 2>&1; then break; fi
    sleep 1
done
if ! docker exec "$CONTAINER" pg_isready -U postgres >/dev/null 2>&1; then
    echo "error: postgres did not become ready" >&2
    exit 1
fi

PSQL=(docker exec -i -e PGPASSWORD=postgres "$CONTAINER" psql -U postgres -d fermi_test)

"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "uuid-ossp";' >/dev/null 2>&1
"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "vector";'    >/dev/null 2>&1

# Runner order, not filename order — the distinction CI's own comment calls
# out as having bitten them before.
#
# `ORDER=filename` reproduces the *pre*-b51d5909 loop (`ls migrations/*.sql |
# sort`), which is how the frozen BASELINE appears to have been measured.
# Kept because "which order was the baseline counted in" turned out to be
# the whole question.
if [ "${ORDER:-runner}" = "filename" ]; then
    order=$(ls migrations/*.sql | sort)
    order_label="filename order (pre-b51d5909 loop)"
else
    order=$(grep -oE '"migrations/[^"]+\.sql"' src/api_server.rs | tr -d '"')
    order_label="runner order"
fi
total=$(echo "$order" | grep -c .)
echo "Applying $total migrations in $order_label…"
echo

failed=0
while IFS= read -r f; do
    [ -z "$f" ] && continue
    if [ ! -f "$f" ]; then
        echo "  MISSING  $f"
        failed=$((failed + 1))
        continue
    fi
    if ! out=$("${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f - < "$f" 2>&1); then
        reason=$(echo "$out" | grep -oE '(ERROR|FATAL):.*' | head -1)
        printf '  FAIL  %-52s %s\n' "$f" "${reason:-unknown error}"
        failed=$((failed + 1))
    fi
done <<< "$order"

echo
echo "RESULT: $failed migration(s) fail against an empty database"
echo "        (.github/workflows/ci.yml BASELINE=$(grep -oE 'BASELINE=[0-9]+' .github/workflows/ci.yml | head -1 | cut -d= -f2))"
