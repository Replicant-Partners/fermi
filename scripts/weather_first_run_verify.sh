#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Did the weather suite's first real run actually record anything?
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# `weather_oracle` has run 12 times. Every one returned `success`. Every one
# fired real tools. Every one recorded NOTHING checkable:
#
#     12 episodes, 0 with retained response_text
#      0 member episodes  (execute_agent appears in 4 of the 12)
#      0 claims, 0 attributions, 0 assertion_verifications
#      every episode digest: summary=null, key_findings=[]
#
# The cause was three format mismatches between what the card declared and what
# `extract_summary_from_json_contract` reads — no `summary` key, findings as
# objects rather than strings, and the multiplier as a JSON number where the
# orchestra parses `[MULTIPLIER] Suggested p50: X (p5: Y, p95: Z)` out of prose.
# None of them raised. `status = success` throughout.
#
# So the failure mode of this suite is SILENCE, and "it ran fine" is not
# evidence of anything. This script is the difference between *worked* and
# *silently didn't*.
#
# HOW TO READ THE STATUSES
#
#   OK        the link fired and carries what it should
#   SILENT    the upstream link fired and this one did not. A real finding:
#             broken or undeployed, same consequence, so it does not guess
#   INERT     nothing upstream to act on yet. NOT a pass — counted separately
#             so a suite that has proven nothing cannot look green
#   FAIL      it ran and produced the wrong thing
#   UNRUNNABLE a query errored. Never a pass
#
# The distinction between SILENT and INERT is the whole design. `count(*) = 0`
# is ambiguous on its own; each check below pairs its sink with the OPPORTUNITY
# that should have driven it, exactly as `liveness_contract_live.sh` does.
#
# USAGE
#
#   # after executing weather_oracle at least once:
#   bash scripts/weather_first_run_verify.sh
#
#   # only runs since a given moment (recommended — isolates THIS run):
#   SINCE='2026-08-18 12:00' bash scripts/weather_first_run_verify.sh
#
# Read-only. Every statement is a SELECT.

set -uo pipefail

if [[ -f .env ]]; then set -a; . ./.env; set +a; fi
: "${DATABASE_URL:?set DATABASE_URL (or put it in .env)}"

SINCE="${SINCE:-1970-01-01}"
AGENT="${AGENT:-weather_oracle}"

q() { psql "$DATABASE_URL" -tAqc "$1" 2>/dev/null; }

ok=0 silent=0 inert=0 fail=0 unrunnable=0

say() { # status label detail
  local s="$1" l="$2" d="${3:-}"
  case "$s" in
    OK)         printf '  \033[32mOK\033[0m         %-34s %s\n' "$l" "$d"; ok=$((ok+1)) ;;
    SILENT)     printf '  \033[31mSILENT\033[0m     %-34s %s\n' "$l" "$d"; silent=$((silent+1)) ;;
    INERT)      printf '  \033[33mINERT\033[0m      %-34s %s\n' "$l" "$d"; inert=$((inert+1)) ;;
    FAIL)       printf '  \033[31mFAIL\033[0m       %-34s %s\n' "$l" "$d"; fail=$((fail+1)) ;;
    UNRUNNABLE) printf '  \033[35mUNRUNNABLE\033[0m %-34s %s\n' "$l" "$d"; unrunnable=$((unrunnable+1)) ;;
  esac
}

echo
echo "─── weather first-run verification ──────────────────────────"
echo "  agent   $AGENT"
echo "  since   $SINCE"
echo

# ── PREFLIGHT: is the code under test actually running? ──────────────
#
# This exists because of twenty minutes wasted concluding the wrong thing.
#
# `fermi-console` is an HTTPS client for a Railway deployment, not a local
# server: its default base_url is https://agent-bestiary.world, and the
# `http://localhost:3000` in `api/client.rs` is inside a unit test. The card that
# steers an agent lives in the shared Neon database, so a card edit takes effect
# on the next deploy of ANYTHING — but a code change takes effect only on a
# deploy of ITSELF.
#
# That asymmetry is a trap. A prompt fix appears to work instantly while the
# check meant to confirm it reads SILENT, and SILENT is the status for a broken
# write path. Same symptom, completely different remedy: one needs a code fix,
# the other needs a deploy. Nothing distinguished them.
#
# `/api/health` already returns the running commit, so the comparison is one
# request. UNRUNNABLE rather than FAIL: when the deployment predates the change,
# the run is not evidence about it either way, and "reports healthy forever" is
# precisely what an unrunnable check does.
HEALTH_URL="${HEALTH_URL:-https://agent-bestiary.world/api/health}"
if [[ "${SKIP_DEPLOY_CHECK:-0}" == "1" ]]; then
  echo "  (deploy check skipped)"
elif ! command -v curl >/dev/null 2>&1; then
  say UNRUNNABLE "deploy freshness" "curl not available — cannot tell whether the running code contains what is being tested"
else
  LIVE_COMMIT=$(curl -fsS --max-time 10 "$HEALTH_URL" 2>/dev/null \
                | grep -oE '"commit"[[:space:]]*:[[:space:]]*"[0-9a-f]+"' \
                | grep -oE '[0-9a-f]{7,}')
  LOCAL_COMMIT=$(git rev-parse HEAD 2>/dev/null)
  if [[ -z "$LIVE_COMMIT" ]]; then
    say UNRUNNABLE "deploy freshness" "$HEALTH_URL unreachable or reported no commit — cannot tell what code produced the episodes below"
  elif [[ -z "$LOCAL_COMMIT" ]]; then
    say UNRUNNABLE "deploy freshness" "not a git checkout — nothing to compare the deployed commit against"
  elif [[ "$LOCAL_COMMIT" == "$LIVE_COMMIT"* ]]; then
    say OK "deploy freshness" "live commit ${LIVE_COMMIT} matches HEAD — the episodes below were produced by this code"
  else
    say UNRUNNABLE "deploy freshness" "live commit ${LIVE_COMMIT}, HEAD ${LOCAL_COMMIT:0:12} — the deployment does NOT contain your change. Anything below that depends on new code will read SILENT for that reason alone, not because the agent misbehaved"
    echo "                 → git --no-pager log --oneline ${LIVE_COMMIT}..HEAD   # what is not deployed"
  fi
fi
echo

# ── 0. Is there a run at all? ────────────────────────────────────────
EPISODES=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
              WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'")
if [[ -z "$EPISODES" ]]; then
  say UNRUNNABLE "episode probe" "query failed — check DATABASE_URL"
  exit 2
fi
if [[ "$EPISODES" == "0" ]]; then
  echo "  No $AGENT episodes since $SINCE. Nothing to verify."
  echo "  Execute the agent, then re-run:"
  echo "    POST /api/agents/$AGENT/execute  {\"query\": \"...\"}"
  echo
  exit 0
fi
say OK "0. episodes exist" "$EPISODES since $SINCE"

SUCCEEDED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
               WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                 AND e.execution_status='success'")
[[ "$SUCCEEDED" == "0" ]] \
  && say FAIL "0b. any succeeded" "0 of $EPISODES succeeded — fix the run before reading the rest" \
  || say OK "0b. succeeded" "$SUCCEEDED of $EPISODES"

# ── 1. Did the document survive retention? ───────────────────────────
# Migration 199 retains response_text. Before it, everything was discarded and
# permanently un-inducible, which is why the first 12 runs cannot be checked
# retrospectively no matter what else is fixed.
RETAINED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
              WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                AND e.response_text IS NOT NULL")
STRUCTURED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
                WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                  AND substring(e.response_text from '(?s)\{.*\}') IS JSON OBJECT")
if [[ "$RETAINED" == "0" ]]; then
  say SILENT "1. response_text retained" "0 of $EPISODES — migration 199 not applied, or the run predates it"
elif [[ "$STRUCTURED" == "0" ]]; then
  say FAIL "1. extractable JSON document" "$RETAINED retained, 0 containing an extractable JSON object — the response carries no document at all, only prose"
else
  say OK "1. extractable JSON document" "$STRUCTURED of $RETAINED retained carry an extractable JSON object"
fi

# ── 2. Did the digest populate? ──────────────────────────────────────
# The specific defect. `extract_summary_from_json_contract` reads a fixed
# vocabulary; the old card matched none of it, so summary was null for 12 runs
# and the multiplier hook had no string to scan.
WITH_SUMMARY=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
                  WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                    AND nullif(trim(e.context->'evidence'->0->>'summary'),'') IS NOT NULL")
WITH_FINDINGS=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
                   WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                     AND jsonb_array_length(coalesce(e.context->'evidence'->0->'key_findings','[]'::jsonb)) > 0")
if [[ "$STRUCTURED" == "0" ]]; then
  say INERT "2. episode digest" "no structured document upstream"
elif [[ "$WITH_SUMMARY" == "0" ]]; then
  say SILENT "2. digest carries a summary" "$STRUCTURED structured, 0 with a summary — the document is missing its top-level \`summary\` key, exactly as before"
else
  say OK "2. digest carries a summary" "$WITH_SUMMARY with summary, $WITH_FINDINGS with findings"
fi

# ── 3. Did the multiplier line parse into a claim? ───────────────────
# The orchestra's wire format is a regex over prose:
#   [MULTIPLIER] Suggested p50: X (p5: Y, p95: Z)
# A `"multiplier": 1.15` JSON field is invisible to it. Both halves of this are
# unit-tested; this is the live confirmation.
HAS_LINE=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
              WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                AND e.context->'evidence'->0->>'summary' ~* 'Suggested\s+p50:\s*[0-9.]+\s*\(p5:'")
CLAIMS=$(q "SELECT count(*) FROM forecast_agent_claims WHERE agent_name='$AGENT' AND claimed_at >= '$SINCE'")
if [[ "$WITH_SUMMARY" == "0" ]]; then
  say INERT "3. multiplier line present" "no summary upstream to carry it"
elif [[ "$HAS_LINE" == "0" ]]; then
  say SILENT "3. multiplier line present" "$WITH_SUMMARY summaries, none matching the orchestra regex — the model emitted the number but not the LINE"
else
  say OK "3. multiplier line present" "$HAS_LINE summaries carry it"
  # A claim additionally needs a workspace: forecast_agent_claims.workspace_id
  # is NOT NULL, so a standalone evaluation genuinely cannot produce one and
  # must not be counted as a missed opportunity.
  IN_WS=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
             WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
               AND e.context ? 'workspace_id'")
  if [[ "${IN_WS:-0}" == "0" ]]; then
    say INERT "3b. claim recorded" "0 runs had a workspace; claims require one, so this is unused not broken. Re-run inside a forecast workspace to exercise it"
  elif [[ "${CLAIMS:-0}" == "0" ]]; then
    say SILENT "3b. claim recorded" "$IN_WS run(s) in a workspace with a multiplier line and 0 claims — check driver_refs resolve (see resolve_driver_prefixes)"
  else
    say OK "3b. claim recorded" "$CLAIMS claim(s)"
  fi

  # ── 3c. Did the judgement survive validation? ──────────────────────
  #
  # A multiplier line can be present, correctly formatted, and still discarded:
  # `assertions.rs` drops any spread whose p5 is below the declared floor, whose
  # p95 is above the ceiling, or that is not ordered. Six of this agent's first
  # ten lines went that way, because a bucket on an eleven-way ladder honestly
  # needs an adjustment near 0.03 against a floor of 0.1.
  #
  # This was invisible before the `assertion:rejected` tag: the drop was a
  # `tracing::warn!` and nothing else, so the episode looked identical to one
  # from an agent that quantified nothing. `status = success`, empty
  # `assertions`, no finding.
  #
  # Note which way the statuses run here, because it is the opposite of every
  # other check in this file: a REJECTION is the bad outcome, so finding the tag
  # is a FAIL rather than a pass.
  #
  # And the check RECONCILES rather than just looking for the tag, which is the
  # difference between this passing honestly and passing falsely. The first draft
  # asked "any rejections? no — any assertions? yes — OK" and reported green on a
  # corpus where 10 lines produced 4 assertions. The other six were dropped
  # before the tag existed, so they carry no tag, and a check that only looks for
  # the tag cannot see them: the exact false-green this file exists to prevent,
  # written into the file that exists to prevent it.
  #
  # Every parsed line must end up EITHER recorded or tagged. A shortfall means
  # judgements left no trace at all, which is a finding about the platform's
  # bookkeeping and not a pending state.
  RECORDED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
                WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                  AND jsonb_array_length(coalesce(e.assertions,'[]'::jsonb)) > 0")
  REJECTED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
                WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                  AND 'assertion:rejected' = ANY(e.tags)")
  UNACCOUNTED=$(( HAS_LINE - ${RECORDED:-0} - ${REJECTED:-0} ))
  if [[ "${REJECTED:-0}" != "0" ]]; then
    say FAIL "3c. judgement survived validation" "$REJECTED run(s) had a multiplier DROPPED — out of the card's declared range, or an unordered spread. The claim is gone and the run still reported success"
  elif (( UNACCOUNTED > 0 )); then
    say SILENT "3c. judgement survived validation" "$HAS_LINE line(s) parsed, $RECORDED recorded, $REJECTED tagged — $UNACCOUNTED left NO trace. Runs predating the assertion:rejected tag: the judgement was dropped and nothing recorded that it had been"
  else
    say OK "3c. judgement survived validation" "$HAS_LINE line(s), all $RECORDED recorded, 0 dropped"
  fi
fi

# ── 4. Did the composition actually compose? ─────────────────────────
# Members have never run: 0 episodes across all three, despite
# tool:execute_agent in 4 of the first 12. This is the per-member credit chain,
# so without it Shapley attribution has nothing to attribute.
DELEGATED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
               WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                 AND 'tool:execute_agent' = ANY(e.tags)")
MEMBERS=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
             WHERE a.agent_name IN ('weather_ensemble_forecaster','weather_calibrator','weather_market_analyst')
               AND e.created_at >= '$SINCE'")
CHILDREN=$(q "SELECT count(*) FROM episodes WHERE parent_episode_id IS NOT NULL AND created_at >= '$SINCE'")
if [[ "${DELEGATED:-0}" == "0" ]]; then
  say INERT "4. delegation attempted" "no run called execute_agent"
elif [[ "${MEMBERS:-0}" == "0" ]]; then
  say SILENT "4. member episodes" "$DELEGATED run(s) called execute_agent and 0 member episodes exist — the composition is not composing"
else
  say OK "4. member episodes" "$MEMBERS member run(s)"
  [[ "${CHILDREN:-0}" == "0" ]] \
    && say SILENT "4b. parent_episode_id set" "$MEMBERS member episode(s), none linked to a parent — cost and credit cannot be reassembled into a tree" \
    || say OK "4b. parent_episode_id set" "$CHILDREN linked episode(s)"
fi

# ── 5. Are the cross-checks live rather than inert? ──────────────────
# All nine report zero mismatches. Until a structured document exists that
# means "nothing to look at", NOT "clean" — the failure this whole tier exists
# to avoid. Named the station because it is the field the two production
# forecasts both omitted.
WITH_STATION=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id,
                    LATERAL (SELECT CASE WHEN substring(e.response_text from '(?s)\{.*\}') IS JSON OBJECT
                                         THEN substring(e.response_text from '(?s)\{.*\}')::jsonb END AS doc) j
                  WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                    AND j.doc #>> '{settlement_target,station}' IS NOT NULL")
if [[ "$STRUCTURED" == "0" ]]; then
  say INERT "5. cross-checks live" "no structured document to compare"
elif [[ "${WITH_STATION:-0}" == "0" ]]; then
  say FAIL "5. settlement station named" "$STRUCTURED structured document(s), none naming a station — the largest error source in these markets, and unverifiable if never recorded"
else
  say OK "5. cross-checks live" "$WITH_STATION document(s) name a settlement station"
  echo "                 → confirm with: bash scripts/grounding_contract_live.sh"
fi

# ── 5b. Was this run produced by the card the agent has NOW? ─────────
#
# The cross-checks read all of history, so a defect found once was found
# forever: after the card was corrected, four consecutive clean runs still sat
# beside nine failures written by the superseded prompt. A suite that cannot go
# green after a fix gets ignored, and an ignored suite is worse than none —
# the next real regression arrives as one more line in a list already red.
#
# So each episode records the SHA-256 of the card's declared system_prompt, and
# a check can scope itself to rows whose hash matches the prompt in force now.
# Edit a card and its cohort empties itself, which is correct: nothing is yet
# known about the new prompt.
#
# No backfill. Which prompt produced an older episode is not a fact this
# platform holds, and inferring it from a deploy time is exactly the
# fabrication the contract exists to catch.
STAMPED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
              WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                AND e.context ? 'card_prompt_hash'")
COHORT=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
             WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
               AND e.context->>'card_prompt_hash'
                   = encode(sha256(convert_to(a.system_prompt,'UTF8')),'hex')")
if [[ "${STAMPED:-0}" == "0" ]]; then
  say SILENT "5b. prompt hash recorded" "$EPISODES run(s), none carrying card_prompt_hash — the binary predates the stamp, or this path does not build episodes through agent_output_to_episode"
elif [[ "${COHORT:-0}" == "0" ]]; then
  say FAIL "5b. run matches current card" "$STAMPED run(s) stamped, 0 matching the prompt the card carries now — the card changed after these ran, or the Rust and SQL hashes disagree (see the_prompt_hash_means_the_same_thing_in_rust_and_in_sql)"
else
  say OK "5b. run matches current card" "$COHORT of $STAMPED stamped run(s) produced by the current prompt — cross-checks on them are LIVE, not inert"
fi

# ── 6. Did routing reach the weather agent? ──────────────────────────
# `domain_specialist` is a match over four domains and climate is not one, so
# every driver fell to macro_forecaster. The DeclaredSpecialist rung fixes it;
# this confirms the fix is the reason the agent ran, rather than luck.
ROUTED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
            WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
              AND e.context->'invocation'->>'route_reason' IS NOT NULL")
DECLARED=$(q "SELECT count(*) FROM episodes e JOIN agents a ON a.agent_id=e.agent_id
              WHERE a.agent_name='$AGENT' AND e.created_at >= '$SINCE'
                AND e.context->'invocation'->>'route_reason' = 'declared_specialist'")
if [[ "${ROUTED:-0}" == "0" ]]; then
  say INERT "6. route provenance" "no run carried a route_reason — direct execution, not a console decomposition. Expected for a manual test"
elif [[ "${DECLARED:-0}" == "0" ]]; then
  say SILENT "6. routed as declared" "$ROUTED routed run(s), none via declared_specialist — check metadata.domains on the card"
else
  say OK "6. routed as declared" "$DECLARED of $ROUTED via declared_specialist"
fi

# ── summary ──────────────────────────────────────────────────────────
echo
echo "─── result ──────────────────────────────────────────────────────"
printf '  %d ok   %d silent   %d inert   %d fail   %d unrunnable\n' \
  "$ok" "$silent" "$inert" "$fail" "$unrunnable"
echo

if (( unrunnable > 0 )); then
  echo "  UNRUNNABLE present. A query that cannot run reports healthy forever."
  exit 2
fi
if (( fail > 0 )); then
  echo "  FAIL present: a link ran and produced the wrong thing. Fix before"
  echo "  reading anything downstream of it."
  exit 1
fi
if (( silent > 0 )); then
  echo "  SILENT present: an upstream link fired and this one did not. This is"
  echo "  the shape of the original defect — 12 successful runs that recorded"
  echo "  nothing — so it is a finding, not a pending state."
  exit 1
fi
if (( inert > 0 && ok < 4 )); then
  echo "  Mostly INERT. The run has not exercised enough of the chain to"
  echo "  conclude anything. INERT is not a pass."
  exit 0
fi

echo "  Chain intact. Next:"
echo "    bash scripts/grounding_contract_live.sh   # 9 cross-checks, now live"
echo "    bash scripts/liveness_contract_live.sh    # is the claim sink writing?"
exit 0
