#!/usr/bin/env bash
# One real grounding run against production, end to end.
#
#   ABW_GROUND_KEY=ferm_... scripts/ground_smoke.sh [agent] [species]
#
# The key must belong to the agent's owner and carry the `ground:*` scope
# (Settings -> API Keys -> "Ground my agents"). Costs one credit per tool call.
#
# The submitted output deliberately invents a conservation status. The
# contract declares `conservation` as having no tool source, so the run passes
# only if that value comes back stripped.
set -euo pipefail

BASE="${ABW_BASE:-https://agent-bestiary.world}"
AGENT="${1:-genome_profiler}"
SPECIES="${2:-Lucanus cervus}"
KEY="${ABW_GROUND_KEY:?set ABW_GROUND_KEY to a ferm_ key with the ground:* scope}"
j() { python3 -c "import sys,json; print(json.load(sys.stdin)$1)"; }

echo "== 1. open a run for $AGENT"
OPEN=$(curl -sS -X POST "$BASE/v1/ground/runs" \
  -H "Authorization: Bearer $KEY" -H "Content-Type: application/json" \
  -d "{\"agent\":\"$AGENT\",\"query\":\"Profile $SPECIES\"}")
echo "$OPEN" | python3 -m json.tool
RUN=$(echo "$OPEN" | j "['run_id']")
TOK=$(echo "$OPEN" | j "['run_token']")

call() {
  echo "== 2. tool $1"
  curl -sS -X POST "$BASE/v1/ground/runs/$RUN/tools/$1" \
    -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" -d "$2" \
    | python3 -c "import sys,json; d=json.load(sys.stdin); o=d.get('output',''); print({k:v for k,v in d.items() if k!='output'}, '| output:', str(o)[:240])"
}
call gbif_taxonomy_tree "{\"scientific_name\":\"$SPECIES\"}"
call ncbi_genome_search "{\"scientific_name\":\"$SPECIES\"}"

echo "== 3. submit an output with an invented conservation status"
DOC=$(cat <<EOF
{"taxonomy":{"kingdom":"Animalia","phylum":"Arthropoda","class":"Insecta","order":"Coleoptera","family":"Lucanidae","genus":"Lucanus","species":"$SPECIES"},
 "genome":{"assembly_accession":null,"assembly_name":null,"chromosome_count":null,"estimated_size_mb":null,"ploidy":"diploid","notable_genes":[]},
 "phylogeny":{"superorder":null,"sister_taxa":["Dorcus parallelipipedus"],"divergence_mya":45.0,"defining_traits":["enlarged mandibles"]},
 "conservation":{"iucn_status":"Near Threatened","population_trend":"decreasing","genetic_diversity_notes":"invented for this test"},
 "summary":"A stag beetle of European oak woodland."}
EOF
)
OUT=$(curl -sS -X POST "$BASE/v1/ground/runs/$RUN/output" \
  -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" \
  -d "{\"document\": $DOC}")
echo "$OUT" | python3 -m json.tool

echo "== check"
echo "$OUT" | python3 -c "
import sys,json; d=json.load(sys.stdin)
doc=d.get('document') or {}
iucn=(doc.get('conservation') or {}).get('iucn_status')
print('reliance        :', d.get('reliance',{}).get('status'))
print('stripped        :', d.get('grounding',{}).get('stripped'))
print('tools_called    :', d.get('grounding',{}).get('tools_called'))
print('iucn delivered  :', iucn)
print('trace           :', d.get('trace_url'))
print('PASS' if iucn is None else 'FAIL: the invented conservation status reached the caller')
sys.exit(0 if iucn is None else 1)"

echo "== a second submit must be refused"
curl -sS -o /dev/null -w "%{http_code} (expect 409)\n" -X POST "$BASE/v1/ground/runs/$RUN/output" \
  -H "Authorization: Bearer $TOK" -H "Content-Type: application/json" -d '{"response":"again"}'
