#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Are the denormalised columns telling the truth? — live database
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# `scripts/schema_contract_live.sh` asks whether every schema object the
# code depends on is PRESENT. This asks whether the ones that cache a
# derivable value are CORRECT.
#
# The distinction is the whole point. `agents.total_executions` was
# present, correctly typed, declared in SCHEMA_COLUMNS, and permanently
# zero — because nothing ever wrote it. Six user-facing surfaces read it
# and served zeros: marketplace pricing, the Dashboard Research card, both
# profile endpoints, the orchestra inbox, and the ecology lens. A deletion
# guard in `admin_cleanup_test_cruft_handler` gated on `total_executions =
# 0`, a predicate that was therefore always true.
#
# Every shape-based check passed the entire time. Boot probes, the
# migration lint, the satisfiability check, the type system, the unit
# tests — all green, all blind, because none of them look at values.
#
# WHAT A FAILURE MEANS
#
#   * "declared WriteOrphaned but AGREE with their source of truth"
#         Either someone wired up a writer (good — promote the column to
#         `Maintained` in src/rollup_trust.rs so the agreement is
#         asserted rather than assumed), or this database has too little
#         data to distinguish them (run against one that has more).
#
#   * "declared Maintained but disagrees ... on N row(s)"
#         The writer is buggy or incomplete. A counter that is right
#         sometimes is read as right always.
#
#   * "`agent_execution_rollup` must exist"
#         migrations/192 hasn't been applied to this database. Six
#         surfaces will 500 — which is the intended failure mode, and
#         strictly better than the silent zeros they used to serve.
#
# The offline tier (the tripwire that stops a seventh handler reaching for
# a dead column) runs in a bare `cargo test` and in the pre-commit hook.
# This script adds the tier that needs real rows.
#
# Read-only: every query in the contract is a bare SELECT, and
# `tests/rollup_contract.rs` asserts that at the unit level.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ] && [ -z "${DATABASE_URL:-}" ]; then
    echo "error: need DATABASE_URL in the environment or a .env file." >&2
    exit 1
fi
[ -f .env ] && { set -a; . ./.env; set +a; }

echo "▸ offline tier (tripwire + contract self-checks)"
cargo test --test rollup_contract

echo
echo "▸ live tier (content verification against a real database)"
exec cargo test --test rollup_contract -- --ignored --nocapture --test-threads=1
