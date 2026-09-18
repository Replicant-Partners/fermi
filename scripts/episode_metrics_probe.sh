#!/usr/bin/env bash
# Prove the episode metrics aggregates do not count an unfinished run as a good
# one, without needing a credential.
#
#   bash scripts/episode_metrics_probe.sh
#
# Spins a throwaway Postgres cluster in /tmp, seeds three episodes for one agent
# — one `success`, one `failure`, one `running` — and executes the daily
# aggregates out of `src/handlers/metrics.rs`, reading the SQL from the source
# rather than restating it, so the probe cannot drift from the handler.
#
# ## The defect this exists for
#
# An episode is inserted in `running` when work is dispatched and transitioned
# to `success` or `failure` when it returns. If it never returns, nothing
# reconciles the row — a `tokio::time::timeout` or a client disconnect drops the
# handler future, so the finalising code never executes. Seven such rows exist.
#
# `COUNT(*) FILTER (WHERE execution_status = 'failure')` then reports those rows
# as though they were fine. `carbon_accountant` read as "2 executions, 1
# failure" — a 50% success rate for an agent that has never resolved a single
# emission factor. See docs/plans/NOTE_CARBON_ZERO_TOKEN_EPISODE.md.
#
# And `execution_time_ms` on such a row is a literal stored `0`, not a NULL, so
# averaging over all rows halves the reported latency. That average is the input
# to the iteration-budget decision, which is why it is asserted here separately
# from the counts: getting the count right and the duration wrong would look
# like a pass.
#
# ## Why not the database in .env
#
# That one is Neon and it is production. The probe writes. A fresh cluster
# answers the question actually being asked: given a `running` row, does this
# exact SQL report it honestly.
#
set -uo pipefail
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BIN=""
for v in 18 17 16 15; do
    [ -x "/usr/lib/postgresql/$v/bin/initdb" ] && BIN="/usr/lib/postgresql/$v/bin" && break
done
if [ -z "$BIN" ]; then
    echo "SKIPPED: no local postgres initdb found. This is an absence of a check," >&2
    echo "         not a passing one." >&2
    exit 0
fi

DATA=/tmp/em_pg_d; SOCK=/tmp/em_pg_s; PORT=55441
cleanup(){ "$BIN/pg_ctl" -D "$DATA" -m immediate stop >/dev/null 2>&1 || true
           rm -rf "$DATA" "$SOCK" /tmp/em_pg.log /tmp/em_sql.txt; }
trap cleanup EXIT
rm -rf "$DATA" "$SOCK"; mkdir -p "$SOCK"
"$BIN/initdb" -D "$DATA" -A trust -U postgres >/dev/null
"$BIN/pg_ctl" -D "$DATA" -o "-p $PORT -k $SOCK -c listen_addresses=''" -l /tmp/em_pg.log -w start >/dev/null
createdb -h "$SOCK" -p "$PORT" -U postgres probe
Q="psql -q -h $SOCK -p $PORT -U postgres -d probe"
A="psql -Atq -h $SOCK -p $PORT -U postgres -d probe"

AGENT=11111111-1111-1111-1111-111111111111
OTHER=22222222-2222-2222-2222-222222222222

# Only the columns the aggregates touch. A wider fixture would invite the
# suspicion that something else is carrying the result.
$Q -v ON_ERROR_STOP=1 -c "
  CREATE TABLE episodes (
    episode_id        uuid PRIMARY KEY,
    agent_id          uuid,
    timestamp_ref     timestamptz,
    execution_status  text,
    tokens_used       bigint,
    execution_time_ms bigint);
  INSERT INTO episodes VALUES
    ('aaaaaaaa-0000-0000-0000-000000000001','$AGENT', NOW(), 'success', 10,  100),
    ('aaaaaaaa-0000-0000-0000-000000000002','$AGENT', NOW(), 'failure', 20,  200),
    -- the row this probe is about: dispatched, never concluded. NULL tokens and
    -- a stored zero duration, exactly as ccf4ae90 carries them.
    ('aaaaaaaa-0000-0000-0000-000000000003','$AGENT', NOW(), 'running', NULL,  0),
    -- a second agent, so the agent-scoped query has something to exclude.
    ('bbbbbbbb-0000-0000-0000-000000000001','$OTHER', NOW(), 'success', 99,  999);"

# The SQL, read out of the handler. Restating it here would only prove this
# probe agrees with itself.
python3 - <<'PY' > /tmp/em_sql.txt
import re
src = open("src/handlers/metrics.rs", encoding="utf-8").read()
found = re.findall(r'"(SELECT[^"]*DATE\(timestamp_ref\) AS day[^"]*)"', src)
if len(found) != 2:
    raise SystemExit(
        f"expected 2 daily aggregates in src/handlers/metrics.rs, found {len(found)}. "
        "The extraction is broken, which would make this probe vacuous."
    )
# Labelled by content, not by position. The first version indexed them 1 and 2
# and got them backwards, because platform_metrics_handler precedes
# agent_metrics_handler in the file — so the agent assertions ran against the
# platform query and three of them failed for the wrong reason. Position in a
# file is not a property of the thing being tested.
for sql in found:
    one = sql.replace("\n", " ").replace(
        "$1", "'11111111-1111-1111-1111-111111111111'::uuid"
    )
    label = "agent" if "WHERE agent_id" in sql else "platform"
    print(f"{label}\t{one}")
PY

# awk rather than grep, because a tab inside single quotes reaches grep as a
# literal backslash-t and matched nothing.
AGENT_SQL=$(awk -F'\t' '$1=="agent"{print $2; exit}' /tmp/em_sql.txt)
PLATFORM_SQL=$(awk -F'\t' '$1=="platform"{print $2; exit}' /tmp/em_sql.txt)
if [ -z "$AGENT_SQL" ] || [ -z "$PLATFORM_SQL" ]; then
    echo "could not label both daily aggregates; extraction is broken" >&2; exit 1
fi

fail=0
check() { # name expected actual
    if [ "$2" = "$3" ]; then printf '  PASS  %-46s %s\n' "$1" "$3"
    else printf '  FAIL  %-46s expected %s, got %s\n' "$1" "$2" "$3"; fail=1; fi
}

echo "=== A. the agent-scoped daily aggregate ==="
row=$($A -c "$AGENT_SQL" 2>&1)
if ! echo "$row" | grep -q '|'; then
    echo "  DID NOT RUN: $row"; exit 1
fi
echo "  row: $row"
check "executions (all three rows counted)" 3 "$(echo "$row" | cut -d'|' -f2)"
check "failures (only the 'failure' row)"   1 "$(echo "$row" | cut -d'|' -f3)"
check "unfinished (the 'running' row)"      1 "$(echo "$row" | cut -d'|' -f4)"
check "tokens (NULL skipped by SUM)"       30 "$(echo "$row" | cut -d'|' -f5)"
# The sharp one. 150 is the mean of the two runs that concluded. 100 is the mean
# including the unfinished row's stored zero, which is what the handler reported
# before this and is how 157s of real latency read as ~78s.
check "avg_time_ms over CONCLUDED only"   150 "$(echo "$row" | cut -d'|' -f6)"

echo
echo "=== B. the platform-wide daily aggregate ==="
prow=$($A -c "$PLATFORM_SQL" 2>&1)
if ! echo "$prow" | grep -q '|'; then
    echo "  DID NOT RUN: $prow"; exit 1
fi
echo "  row: $prow"
check "executions (both agents)"            4 "$(echo "$prow" | cut -d'|' -f2)"
check "failures"                            1 "$(echo "$prow" | cut -d'|' -f3)"
check "unfinished"                          1 "$(echo "$prow" | cut -d'|' -f4)"
check "tokens"                            129 "$(echo "$prow" | cut -d'|' -f5)"

echo
echo "=== C. a day with nothing but an unfinished run ==="
# The shape carbon_accountant actually presented. avg_time_ms must be absent
# rather than 0: there is no measured duration, and a zero reads as an instant
# run. This is the case where COALESCE(SUM(...), 0) legitimately yields 0 for
# tokens while the duration must stay null.
$Q -v ON_ERROR_STOP=1 -c "
  DELETE FROM episodes WHERE agent_id = '$AGENT';
  INSERT INTO episodes VALUES
    ('cccccccc-0000-0000-0000-000000000001','$AGENT', NOW(), 'running', NULL, 0);"
row=$($A -c "$AGENT_SQL")
echo "  row: $row"
check "executions"                          1 "$(echo "$row" | cut -d'|' -f2)"
check "failures (NOT counted as one)"       0 "$(echo "$row" | cut -d'|' -f3)"
check "unfinished"                          1 "$(echo "$row" | cut -d'|' -f4)"
check "tokens (coalesced to zero)"          0 "$(echo "$row" | cut -d'|' -f5)"
check "avg_time_ms is NULL, not 0"         "" "$(echo "$row" | cut -d'|' -f6)"

echo
if [ "$fail" = 0 ]; then echo "all assertions held."; else echo "FAILURES ABOVE."; fi
exit "$fail"
