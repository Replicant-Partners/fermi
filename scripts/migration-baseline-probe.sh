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
#
# No published port: every query goes through `docker exec`, so binding one
# only created a race against a previous run's teardown (and an unnecessary
# collision with any local postgres).
echo "Starting postgres …"
if ! run_err=$(docker run -d --name "$CONTAINER" \
    -e POSTGRES_PASSWORD=postgres \
    -e POSTGRES_DB=fermi_test \
    pgvector/pgvector:pg16 2>&1); then
    echo "error: could not start postgres container" >&2
    echo "$run_err" >&2
    exit 1
fi

ready=0
for _ in $(seq 1 90); do
    if docker exec "$CONTAINER" pg_isready -U postgres -q >/dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 1
done
if [ "$ready" -ne 1 ]; then
    echo "error: postgres did not become ready in 90s" >&2
    echo "--- container status ---" >&2
    docker ps -a --filter "name=$CONTAINER" --format '{{.Status}}' >&2
    echo "--- container logs (tail) ---" >&2
    docker logs "$CONTAINER" 2>&1 | tail -20 >&2
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
baseline=$(grep -oE 'BASELINE=[0-9]+' .github/workflows/ci.yml | head -1 | cut -d= -f2)

# PASSES=2 re-applies the whole set to the same database.
#
# `run_migrations` re-executes every file on every boot, so "applies cleanly
# twice" is a correctness requirement here rather than a nicety, and it is
# the property CI does not check: its database is fresh every run, so a
# migration that succeeds once and fails on re-run looks green in CI and
# fails forever in production, silently, because run_migrations swallows the
# error. That is exactly how mig 171 went broken-on-every-boot unnoticed.
passes="${PASSES:-1}"

run_pass() {
    local pass_no="$1"
    local failed=0
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
    echo "$failed" > "/tmp/probe_failed_${pass_no}_$$"
}

pass=1
while [ "$pass" -le "$passes" ]; do
    if [ "$passes" -gt 1 ]; then
        echo "── pass $pass of $passes ──────────────────────────────────────"
    fi
    echo "Applying $total migrations in $order_label…"
    echo
    run_pass "$pass"
    echo
    echo "  pass $pass: $(cat "/tmp/probe_failed_${pass}_$$") failure(s)"
    echo
    pass=$((pass + 1))
done

failed=$(cat "/tmp/probe_failed_1_$$")
rm -f /tmp/probe_failed_*_$$

echo "RESULT: $failed migration(s) fail against an empty database"
echo "        (.github/workflows/ci.yml BASELINE=${baseline})"
