#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Is what our agents said actually true? — live database
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# The offline grounding contract asks whether every output field is
# *classified*: sourced, derived, inferred, narrative or unavailable. It
# passes without ever looking at something an agent produced.
#
# So it passed while `Antaxius beieri` — a bush-cricket, Orthoptera /
# Tettigoniidae — was profiled as a cerambycid beetle, Coleoptera /
# Cerambycidae, and described as a longhorn beetle. The field was present,
# non-null, correctly typed, and declared `Sourced`.
#
# `Sourced` asserts that a tool COULD supply the field. It never asserted
# that the value CAME from that tool. The GBIF-verified answer was on the
# creature row the entire time, one JOIN away, and nothing looked.
#
# This is the tier that looks.
#
# WHAT A FAILURE MEANS
#
#   * "N row(s) disagree with the independently-held source of truth"
#         An agent fabricated a field it declared sourced. Find the rows,
#         then decide whether the fix is `reconcile()` (make the canonical
#         record authoritative) or a prompt change.
#
#   * "cross-check could not run"
#         Not a pass. A query that errors reports healthy forever — the
#         same failure as `schema_trust`'s matview probe, which could never
#         return healthy and was therefore ignored for eight releases.
#
#   * "the JOIN matches nothing, so zero is inert rather than clean"
#         The agreement probe found no agreeing rows either. A comparison
#         that cannot succeed cannot meaningfully fail.
#
# LINEAGE
#
# Modelled on `scripts/rollup_contract_live.sh`, which exists because
# `agents.total_executions` was present, correctly typed, declared in
# SCHEMA_COLUMNS, and permanently zero. Every shape-based check passed the
# whole time. Same disease, one layer up: this one is about what agents
# say rather than what columns hold.
#
# Read-only: every query is a bare SELECT, and the offline tier asserts
# that at the unit level (`cross_check_queries_are_read_only_...`).

set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ] && [ -z "${DATABASE_URL:-}" ]; then
    echo "error: need DATABASE_URL in the environment or a .env file." >&2
    exit 1
fi
[ -f .env ] && { set -a; . ./.env; set +a; }

echo "▸ offline tier — is every sourced field verifiable, or does it say why not?"
cargo test --lib -p fermi grounding_trust
cargo test --lib -p fermi provenance_oracle
cargo test --test grounding_contract

echo
echo "▸ coverage — can any path write a rule without grading it?"
cargo test --test provenance_floor_coverage

echo
echo "▸ corpus — what is the state of the knowledge we inject into prompts?"
# Read-only. Two numbers matter and they mean different things:
#   ungradeable  — the evidence is GONE (pre-migration-199 episodes did not
#                  retain response_text). Not recoverable, and the remedy is
#                  time: new episodes accumulate graded.
#   dangling     — the rule cites episodes with no rows behind them. A
#                  citation that cannot be followed, which is a finding.
# A report that counted either as grounded would show the corpus getting
# cleaner as coverage got worse.
psql "$DATABASE_URL" -P pager=off -c "
WITH src AS (
  SELECT r.rule_id,
         cardinality(coalesce(r.source_episode_cluster,'{}')) AS declared,
         count(e.episode_id)     AS resolved,
         count(e.response_text)  AS retained
    FROM semantic_rules r
    LEFT JOIN unnest(coalesce(r.source_episode_cluster,'{}')) AS eid ON true
    LEFT JOIN episodes e ON e.episode_id = eid
   WHERE r.is_active
   GROUP BY 1,2
)
SELECT CASE
         WHEN declared = 0            THEN 'no sources declared'
         WHEN resolved = 0            THEN 'dangling: cited episodes do not exist'
         WHEN retained = 0            THEN 'ungradeable: evidence not retained (pre-199)'
         WHEN retained < resolved     THEN 'partially retained'
         ELSE                              'fully retained: gradeable'
       END AS state,
       count(*) AS rules
  FROM src GROUP BY 1 ORDER BY 2 DESC;"

# What the column actually holds. NULL is UNKNOWN and is listed explicitly
# rather than omitted, because an absent row in a report reads as zero and
# "zero unknowns" is the opposite of the truth.
#
# Guarded on the column existing. `run_migrations` applies 203 at boot, so a
# tree that is ahead of the deployed database is an ordinary state and must
# report as "not deployed" rather than aborting the run — an error here would
# skip the live tier below and look like a grounding failure.
has_floor=$(psql "$DATABASE_URL" -Atqc "
  SELECT count(*) FROM information_schema.columns
   WHERE table_name = 'semantic_rules' AND column_name = 'provenance_floor';")
if [ "$has_floor" = "0" ]; then
    echo "  migration 203 not yet deployed — no floors are recorded."
    echo "  Every active rule therefore reads as 'grounding unknown' in prompts."
else
    psql "$DATABASE_URL" -P pager=off -c "
    SELECT coalesce(provenance_floor, 'NULL (unknown — not a pass)') AS floor,
           count(*) AS rules
      FROM semantic_rules WHERE is_active GROUP BY 1 ORDER BY 2 DESC;"
fi

echo
echo "▸ live tier — does agent output agree with independently-held truth,"
echo "              and does the floor resolve against the real corpus?"
exec cargo test --test grounding_contract -- --ignored --nocapture --test-threads=1
