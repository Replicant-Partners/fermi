#!/usr/bin/env bash
# Prove carbon_accountant's SQL works, without needing a credential.
#
#   bash scripts/carbon_sql_probe.sh
#
# Spins a throwaway Postgres cluster in /tmp, applies mig-238, seeds a stub
# `episodes`/`agents` pair, and executes every cross-check the agent declares —
# reading the SQL out of src/grounding_trust.rs rather than restating it, so the
# probe cannot drift from the contract.
#
# ## Why not the database in .env
#
# That one is Neon and it is production. The probe writes, and
# `carbon_emission_factors` does not exist there until mig-238 deploys, so a run
# against it would report a missing relation and prove nothing about the SQL. A
# fresh cluster answers the question actually being asked: is this valid
# Postgres, and does each check fire on the rows it claims to.
#
# The live tier is still the only thing that can say whether PRODUCTION data
# disagrees with itself — `scripts/grounding_contract_live.sh` does that. This
# says the queries are sound before they get there.
#
set -uo pipefail
BIN=/usr/lib/postgresql/18/bin
DATA=/tmp/ca_pg_d3; SOCK=/tmp/ca_pg_s3; PORT=55434
cleanup(){ "$BIN/pg_ctl" -D "$DATA" -m immediate stop >/dev/null 2>&1 || true
           rm -rf "$DATA" "$SOCK" /tmp/ca_pg3.log /tmp/ca_sql.txt; }
trap cleanup EXIT
rm -rf "$DATA" "$SOCK"; mkdir -p "$SOCK"
"$BIN/initdb" -D "$DATA" -A trust -U postgres >/dev/null
"$BIN/pg_ctl" -D "$DATA" -o "-p $PORT -k $SOCK -c listen_addresses=''" -l /tmp/ca_pg3.log -w start >/dev/null
createdb -h "$SOCK" -p "$PORT" -U postgres probe
Q="psql -q -h $SOCK -p $PORT -U postgres -d probe"
A="psql -Atq -h $SOCK -p $PORT -U postgres -d probe"

$Q -v ON_ERROR_STOP=1 -f migrations/238_carbon_emission_factors.sql
$Q -v ON_ERROR_STOP=1 -c "
  CREATE EXTENSION IF NOT EXISTS pgcrypto;
  CREATE TABLE agents   (agent_id uuid PRIMARY KEY, agent_name text, system_prompt text);
  CREATE TABLE episodes (episode_id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                         agent_id uuid, response_text text, context jsonb);
  INSERT INTO agents VALUES (gen_random_uuid(), 'carbon_accountant', 'PROMPT v1');"

CLEAN='{"inventory":{"items":[
  {"item_id":"hibiscus_infusion","activity_qty_kg":0.02805,"factor_kg_co2e_per_kg":2.1,
   "kg_co2e":0.058905,"dataset":"ecoinvent 3.9.1","corroborating_dataset":"Agribalyse 3.1"}]}}'
FAULTY='{"inventory":{"items":[
  {"item_id":"hibiscus_infusion","activity_qty_kg":0.02805,"factor_kg_co2e_per_kg":2.1,
   "kg_co2e":0.589,"dataset":"ecoinvent 3.9.1","corroborating_dataset":"Agribalyse 3.1"},
  {"item_id":"black_tea","activity_qty_kg":0.00264,"factor_kg_co2e_per_kg":8.0,
   "kg_co2e":0.02112,"dataset":"ecoinvent 3.9.1","corroborating_dataset":" Ecoinvent 3.9.1 "}]}}'

seed() { $Q -v ON_ERROR_STOP=1 -c "
  INSERT INTO episodes (agent_id, response_text, context)
  SELECT a.agent_id, \$reply\$$1\$reply\$,
         jsonb_build_object('card_prompt_hash',
           encode(sha256(convert_to(a.system_prompt,'UTF8')),'hex'))
  FROM agents a WHERE a.agent_name='carbon_accountant';"; }

python3 "$(dirname "$0")/carbon_extract_sql.py" > /tmp/ca_sql.txt

runall() {
  while IFS=$'\t' read -r path sql; do
    scoped=${sql//\{\{COHORT\}\}/AND e.context->>\'card_prompt_hash\' = encode(sha256(convert_to(a.system_prompt, \'UTF8\')), \'hex\')}
    out=$($A -c "$scoped" 2>&1 | tr -d '[:space:]')
    if [[ "$out" =~ ^[0-9]+$ ]]; then printf '  %-42s %s\n' "${path:0:42}" "$out"
    else printf '  %-42s DID NOT RUN: %s\n' "${path:0:42}" "$(echo "$out" | head -c 120)"; fi
  done < /tmp/ca_sql.txt
}

echo "=== A. an honest reply: every check runs, every count 0 ==="
seed "$CLEAN"
runall

echo
echo "=== B. a reply with one bad product and one same-dataset 'corroboration' ==="
seed "$FAULTY"
runall
echo "  (expect: arithmetic 1, independence 1 — the ledger pair stays 0, nothing wrote to it)"

echo
echo "=== C. the ledger, once two runs resolve the same key ==="
ins() { $Q -v ON_ERROR_STOP=1 -c "
  INSERT INTO carbon_emission_factors
    (material_key,geography,reference_year,dataset_key,value_kg_co2e_per_kg,material,agent_name)
  VALUES ('dried hibiscus calyces','EG',2021,'ecoinvent 3.9.1',$1,'Dried hibiscus calyces','carbon_accountant');"; }
ins 2.1;   echo "  one reading:"          ; runall | grep -E "factor_kg|COVERAGE"
ins 2.104; echo "  a second that agrees:" ; runall | grep -E "factor_kg|COVERAGE"
ins 4.9;   echo "  a third that does not:"; runall | grep -E "factor_kg|COVERAGE"
