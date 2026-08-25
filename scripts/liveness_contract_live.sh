#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Does the write path ever actually run? — live database
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# Every other contract examines data that EXISTS. None of them can see a
# table that is empty because nothing ever wrote to it:
#
#   schema_trust     — does the column exist?
#   rollup_trust     — is the cached value true?
#   grounding_trust  — could the value have been known?
#   port_trust       — is the caller sending what the agent takes?
#   liveness_trust   — does the writer ever run?          ← this one
#
# That blind spot produced five findings in a single afternoon:
#
#   * credit_ledger_tx_type_check — declared by SEVENTEEN migrations, applied
#     by none. Each early migration dropped it and failed to re-add it, and
#     `run_migrations` logs failures and continues, so every repair DELETED it.
#   * the provenance oracle — three ConsolidationWorker construction sites,
#     one wired. The missed one is the highest-volume rule writer.
#   * forecast_agent_claims — coded, wired, exhaustively commented, 0 rows.
#   * anomaly_events — CHECK extended for a new kind, never fired.
#   * semantic_rules.application_count — declared in migration 010, never
#     incremented.
#
# All five look correct in the source. Reading the code proves nothing.
#
# WHY NOBODY WRITES THIS CHECK
#
# `count(*) = 0` is ambiguous: unused and broken are indistinguishable, so
# the check looks unactionable and gets skipped. The disambiguator is the
# OPPORTUNITY count — how many times the path should have fired. Zero claims
# beside fourteen multiplier-bearing episodes is broken. Zero beside zero is
# merely unused. Same number, opposite meanings.
#
# HOW TO READ THE STATUSES
#
#   OK            opportunities exist, sink has rows. The path runs.
#   SILENT        opportunities exist, sink empty. The signal does not exist.
#                 Broken or undeployed — same consequence, so the status does
#                 not guess.
#   INERT         no opportunities yet. NOT a pass. Counted separately so a
#                 suite that has proven nothing cannot look green.
#   NOT DEPLOYED  the sink's column does not exist yet.
#   UNRUNNABLE    a query errored. Never a pass.
#
# Two contracts are positive controls. A suite with no known-good case cannot
# distinguish "every path is broken" from "the runner is broken", and the
# final assertion refuses to pass while nothing has been demonstrated to work.
#
# Read-only: every query is a bare SELECT, asserted at the unit level by
# `every_query_is_read_only`.

set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ] && [ -z "${DATABASE_URL:-}" ]; then
    echo "error: need DATABASE_URL in the environment or a .env file." >&2
    exit 1
fi
[ -f .env ] && { set -a; . ./.env; set +a; }

echo "▸ offline tier — are the contracts well-formed and read-only?"
cargo test --lib -p fermi liveness_trust
cargo test --lib -p fermi anomaly_vocabulary

echo
echo "▸ live tier — has each declared write path ever run?"
cargo test --test liveness_contract -- --ignored --nocapture --test-threads=1

# The Conditional sinks cannot be settled by a row count: a detector that finds
# nothing may be right. What CAN be settled is whether it would be recorded if
# it found something, and that half was false — Loop 2's grounding seed wrote a
# severity the CHECK constraint rejects, in a spawned task, with the error only
# logged. Without this tier the sink's zero is unfalsifiable, which is the same
# standing as a scan that cannot go red.
echo
echo "▸ firing tier — could the anomaly detectors record anything if they fired?"
cargo test --test anomaly_firing_probe -- --ignored --nocapture --test-threads=1

# The chain view. Every stage of Loop 2 is empty; read stage by stage that is
# five findings, read as a chain it is one, and only the first is actionable.
echo
echo "▸ chain tier — where does each feedback loop stop, and why?"
cargo test --test loop_model_contract -- --ignored --nocapture --test-threads=1

# Do Rust and Postgres still agree about what each column will accept? This is
# the tier that would have caught `severity = "L1"` the day it was written.
echo
echo "▸ seam tier — do the vocabularies still agree with the schema?"
cargo test --test seam_vocabulary_contract -- --ignored --nocapture --test-threads=1

# And the machinery scoring itself. Last, because it reads what the tiers above
# produce: counters, gate decisions, chains and the liveness report.
echo
echo "▸ native tier — what do the evaluators make of all of the above?"
exec cargo test --test native_evaluator_contract -- --ignored --nocapture --test-threads=1
