#!/usr/bin/env bash
#
# Run the schema-trust contract against a REAL database.
#
# `live_contract_is_satisfied_by_a_migrated_database` and
# `live_matviews_are_visible_to_the_probe` silently return early when
# DATABASE_URL is unset (tests/schema_trust_contract.rs:259), so a bare
# `cargo test` reports green without ever comparing the contract to a
# schema. This wrapper supplies the URL so those two actually run.
#
# Read-only: the contract probe only inspects the catalog.
set -euo pipefail
cd "$(dirname "$0")/.."
set -a; . ./.env; set +a

exec cargo test --test schema_trust_contract -- --nocapture
