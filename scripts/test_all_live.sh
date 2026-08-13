#!/usr/bin/env bash
#
# Run cargo test with DATABASE_URL supplied, so the DB-gated tests actually
# execute instead of self-skipping into a false green (see
# tests/schema_trust_contract.rs:259 for the pattern).
#
# Passes all arguments straight through to cargo test. With no arguments,
# runs the whole workspace.
#
#   bash scripts/test_all_live.sh                    # everything
#   bash scripts/test_all_live.sh -p fermi --lib     # one package
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

if [ "$#" -eq 0 ]; then
  exec cargo test --workspace
else
  exec cargo test "$@"
fi
