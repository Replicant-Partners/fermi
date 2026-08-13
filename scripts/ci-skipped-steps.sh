#!/usr/bin/env bash
#
# Run the CI "Test Suite" steps that the tripped migration ratchet has been
# skipping since 2026-08-07 (see docs/plans/CI_MIGRATION_RATCHET.md).
#
# These are `- name:` steps ordered after "Set up database" in
# .github/workflows/ci.yml. When that step exits non-zero the whole job stops,
# so none of the following have executed on any push for weeks. They report as
# skipped, not passing, and the distinction was invisible.
#
# Database-dependent steps are excluded — they need the service container and
# a DATABASE_URL. This covers the four lints, the compile check, and the
# non-DB test targets.

set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

fail=0
run_step() {
    local name="$1"; shift
    printf '\n═══ %s\n' "$name"
    if "$@" >/tmp/ci_step.out 2>&1; then
        echo "    PASS"
    else
        echo "    FAIL (exit $?)"
        grep -E '(^error|ERROR|error\[|FAILED|panicked|✗|MISSING)' /tmp/ci_step.out \
            | head -12 | sed 's/^/      /'
        tail -4 /tmp/ci_step.out | sed 's/^/      | /'
        fail=1
    fi
}

run_step "Lint — no env-sourced LLM provider credentials" \
    ./scripts/lint-no-env-credentials.sh

# shellcheck disable=SC2046
run_step "Lint — SQL column refs resolve to a migration" \
    python3 scripts/lint-schema-consistency.py $(git ls-files '*.rs')

run_step "Lint — user-reference columns FK to users(user_id)" \
    ./scripts/lint-owner-columns.sh

run_step "Lint — agent taxonomy conformance" \
    ./scripts/taxonomy.py audit --gate derived

run_step "Check all binaries compile" \
    cargo check --bins --workspace

run_step "Unit tests (non-DB crates)" \
    cargo test --lib --workspace --exclude agent-bestiary-memory

run_step "api-server binary unit tests" \
    cargo test --bin api-server

run_step "Schema trust contract — hygiene" \
    cargo test --test schema_trust_contract

printf '\n'
if [ "$fail" -eq 0 ]; then
    echo "All previously-skipped steps pass."
else
    echo "At least one previously-skipped step FAILS — see above."
fi
exit $fail
