#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Port census — live tier
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# `scripts/port_census.py` is the offline tier: it reads the 100 agent
# cards on disk and asks what they *declare*. This tier asks the two
# questions that need real rows:
#
#   1. Which declared agents have ever actually run? An agent with no
#      episodes cannot have its output type induced from evidence, and
#      typing it from its prompt is how `output_contract` acquired seven
#      entries naming schemas that do not exist.
#
#   2. Is the raw output retained anywhere at all? The remediation plan
#      assumes types can be induced from response history. That
#      assumption is checkable, and the answer changes the plan.
#
# WHAT IT FOUND THE FIRST TIME (2026-08-15)
#
#   * `episodes` has a `query` column and NO response column. `context`
#     holds a parsed digest (reasoning / evidence / sources_consulted /
#     tool_invocations) produced by `parse_evidence_text`, not the
#     agent's own document. Induction from `episodes` is therefore not
#     possible for structured output.
#
#   * The card corpus is not the corpus. 748 agent rows exist; 107
#     agents have run and have no card on disk, so the offline tier
#     never examined them.
#
#   * Every one of the 51 `community`-tier agents has empty `accepts`
#     AND empty `produces` — the population `agent_contract.rs` was
#     written to gate, all of it predating the gate.
#
# READ-ONLY. Every statement here is a bare SELECT. Nothing in this
# script writes, and it is safe to point at production — which is the
# only place the answers exist.
#
# Usage:  scripts/port_census_live.sh            # all sections
#         scripts/port_census_live.sh runs       # one section
#
# Sections: retention | runs | corpus | ports | dbports | pilot

set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ] && [ -z "${DATABASE_URL:-}" ]; then
    echo "error: need DATABASE_URL in the environment or a .env file." >&2
    exit 1
fi
[ -f .env ] && { set -a; . ./.env; set +a; }

section="${1:-all}"

# Sections that describe the DB-only population. `port_census.py` globs
# `agents/*/*/agent_card.json` and is therefore blind to every agent admitted
# through `POST /api/agents`, which is the population with the worse ports:
# all 51 community-tier rows have empty `accepts` AND empty `produces`.
# Counting only the cards would report a burn-down over the healthier half of
# the corpus and call it the whole.
PSQL=(psql "$DATABASE_URL" -X -q -A -F' | ' --pset=footer=off)

run_if() {
    [ "$section" = "all" ] || [ "$section" = "$1" ]
}

# ─── 1. Where does agent output actually live? ────────────────────────
#
# The premise of evidence-based typing is that we kept the responses.
# Verify rather than assume: an induction corpus that does not exist is
# the same failure as a schema that is only a name.
if run_if retention; then
echo "▸ retention — is raw agent output kept anywhere?"
"${PSQL[@]}" -c "
SELECT 'episodes (rows)'                       AS store, count(*)::text AS n FROM episodes
UNION ALL SELECT 'episodes.source_text present',
       count(*)::text FROM episodes WHERE source_text IS NOT NULL AND source_text <> ''
UNION ALL SELECT 'workspace_messages (agent sender)',
       count(*)::text FROM workspace_messages WHERE sender_type = 'agent'
UNION ALL SELECT 'creature_conditions.genome_profile',
       count(*)::text FROM creature_conditions WHERE genome_profile IS NOT NULL;"
echo
echo "  episodes.context top-level keys (the digest that IS kept):"
"${PSQL[@]}" -c "
SELECT k AS key, count(*)::text AS n
  FROM episodes, LATERAL jsonb_object_keys(context) AS k
 WHERE context IS NOT NULL
 GROUP BY k ORDER BY count(*) DESC LIMIT 8;"
echo
fi

# ─── 2. Which agents have evidence to induce from? ────────────────────
if run_if runs; then
echo "▸ runs — per agent, newest first"
"${PSQL[@]}" -c "
SELECT a.agent_name AS agent,
       count(*)::text AS runs,
       count(*) FILTER (WHERE e.execution_status = 'success')::text AS ok,
       min(e.created_at)::date::text AS first_run,
       max(e.created_at)::date::text AS last_run
  FROM episodes e JOIN agents a ON a.agent_id = e.agent_id
 GROUP BY a.agent_name
 ORDER BY count(*) DESC LIMIT 20;"
echo
fi

# ─── 3. How much of the corpus does the offline tier not see? ─────────
#
# `port_census.py` globs `agents/*/*/agent_card.json`. Anything admitted
# through the API path has no card, so the offline census is blind to it.
if run_if corpus; then
echo "▸ corpus — agent rows by tier, and how many the card glob misses"
"${PSQL[@]}" -c "
SELECT tier,
       count(*)::text AS agents,
       count(*) FILTER (WHERE coalesce(array_length(accepts,1),0) = 0)::text AS no_accepts,
       count(*) FILTER (WHERE coalesce(array_length(produces,1),0) = 0)::text AS no_produces
  FROM agents GROUP BY tier ORDER BY count(*) DESC;"
echo
fi

# ─── 4. Portless agents that are nonetheless visible ──────────────────
if run_if ports; then
echo "▸ ports — portless agents by visibility"
"${PSQL[@]}" -c "
SELECT tier, is_public::text, visibility,
       count(*)::text AS agents,
       count(*) FILTER (WHERE coalesce(array_length(accepts,1),0) = 0)::text AS portless
  FROM agents
 WHERE tier IN ('community','curated','system')
 GROUP BY tier, is_public, visibility
 ORDER BY tier, count(*) DESC;"
echo
fi

# ─── 5. The DB-only population, by burn-down status ───────────────────
#
# The offline census's counters, recomputed over agents that have no card.
# Deliberately reported separately rather than merged: the two populations
# need opposite fixes. A card agent has ports that are FAKE (513 free-text
# labels asserting composability nothing verified); a community agent has no
# ports at ALL. Averaging them would hide both.
if run_if dbports; then
echo "▸ dbports — the population port_census.py cannot see"
"${PSQL[@]}" -c "
WITH scope AS (
    SELECT a.agent_id, a.agent_name, a.tier,
           coalesce(array_length(a.accepts,1),0)  AS n_accepts,
           coalesce(array_length(a.produces,1),0) AS n_produces,
           EXISTS (SELECT 1 FROM episodes e WHERE e.agent_id = a.agent_id) AS has_run
      FROM agents a
     WHERE a.tier <> 'test'
)
SELECT tier,
       count(*)::text                                              AS agents,
       count(*) FILTER (WHERE n_accepts = 0 AND n_produces = 0)::text AS portless,
       count(*) FILTER (WHERE n_accepts = 0 AND n_produces = 0 AND has_run)::text
                                                                   AS portless_and_used,
       count(*) FILTER (WHERE n_accepts > 0 OR n_produces > 0)::text  AS declares_something
  FROM scope GROUP BY tier ORDER BY count(*) DESC;"
echo
echo "  portless agents that have actually run (the ones worth fixing first):"
"${PSQL[@]}" -c "
SELECT a.agent_name, a.tier, a.visibility, count(e.episode_id)::text AS runs
  FROM agents a JOIN episodes e ON e.agent_id = a.agent_id
 WHERE a.tier <> 'test'
   AND coalesce(array_length(a.accepts,1),0) = 0
   AND coalesce(array_length(a.produces,1),0) = 0
 GROUP BY a.agent_name, a.tier, a.visibility
 ORDER BY count(e.episode_id) DESC LIMIT 15;"
echo
fi

# ─── 6. The pilot: what genome_profiler actually served ───────────────
#
# The remediation prompt says 56 episodes need backfilling. True, but the
# copy a user reads is the cache, and these are the rows that are wrong
# on screen right now. GBIF supplies taxonomy only: every value in the
# last four columns was produced without a source.
if run_if pilot; then
echo "▸ pilot — genome_profiler's served output"
"${PSQL[@]}" -c "
SELECT count(*)::text AS cached_profiles,
       count(*) FILTER (WHERE genome_profile->'genome'       <> '{}'::jsonb)::text AS with_genome,
       count(*) FILTER (WHERE genome_profile->'phylogeny'    <> '{}'::jsonb)::text AS with_phylogeny,
       count(*) FILTER (WHERE genome_profile->'conservation' <> '{}'::jsonb)::text AS with_conservation
  FROM creature_conditions WHERE genome_profile IS NOT NULL;"
echo
echo "  ungrounded values currently served (GBIF supplies none of these):"
"${PSQL[@]}" -c "
SELECT genome_profile->'taxonomy'->>'species'          AS species,
       genome_profile->'genome'->>'estimated_size_mb'  AS genome_mb,
       genome_profile->'genome'->>'chromosome_count'   AS chromosomes,
       genome_profile->'conservation'->>'iucn_status'  AS iucn
  FROM creature_conditions
 WHERE genome_profile IS NOT NULL
   AND genome_profile->'taxonomy'->>'species' IS NOT NULL
 ORDER BY 1;"
echo
fi
