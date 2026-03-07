#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────
# sync-fermi-orchestra.sh — Register fermi-orchestra agents on ABW
#
# All agents in the Fermi Console execute via ABW. This script
# ensures every fermi-orchestra agent exists on the platform.
#
# Usage:
#   ABW_TOKEN="your-bearer-token" ./scripts/sync-fermi-orchestra.sh
#
# To get a token: sign in at https://agent-bestiary.world, then
# grab the token from browser devtools or use the OAuth flow.
# ─────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

ABW_URL="${ABW_URL:-https://agent-bestiary.world}"
TOKEN="${ABW_TOKEN:-${FERMI_API_KEY:-}}"

if [ -z "$TOKEN" ]; then
    echo "✗ No auth token found."
    echo ""
    echo "  Set ABW_TOKEN or FERMI_API_KEY:"
    echo "    ABW_TOKEN=\"your-token\" ./scripts/sync-fermi-orchestra.sh"
    echo ""
    echo "  To get a token:"
    echo "    1. Sign in at ${ABW_URL}"
    echo "    2. Open browser devtools → Application → Local Storage"
    echo "    3. Copy the auth token"
    echo "    Or use: curl -s ${ABW_URL}/api/auth/me -H 'Authorization: Bearer TOKEN'"
    exit 1
fi

echo "╔═══════════════════════════════════════════════════════════╗"
echo "║  Fermi Orchestra → ABW Sync                              ║"
echo "╠═══════════════════════════════════════════════════════════╣"
echo "║  API: $ABW_URL"
echo "╚═══════════════════════════════════════════════════════════╝"
echo ""

# ── Verify auth ────────────────────────────────────────────────
echo "▸ Verifying authentication…"
AUTH_RESP=$(curl -sf -w "\n%{http_code}" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    "${ABW_URL}/api/auth/me" 2>/dev/null) || {
    echo "  ✗ Auth failed. Check your token."
    exit 1
}
HTTP_CODE=$(echo "$AUTH_RESP" | tail -1)
AUTH_BODY=$(echo "$AUTH_RESP" | sed '$d')
if [ "$HTTP_CODE" != "200" ]; then
    echo "  ✗ Auth returned HTTP $HTTP_CODE"
    echo "  $AUTH_BODY"
    exit 1
fi
USER_NAME=$(echo "$AUTH_BODY" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('display_name') or d.get('email','unknown'))" 2>/dev/null || echo "authenticated")
echo "  ✓ Signed in as: $USER_NAME"
echo ""

# ── Find fermi-orchestra agents locally ────────────────────────
echo "▸ Scanning local agents/curated/ for fermi-orchestra tags…"
ORCHESTRA_AGENTS=()
for card_path in agents/curated/*/agent_card.json; do
    IS_ORCHESTRA=$(python3 -c "
import json, sys
d = json.load(open('$card_path'))
tags = d.get('metadata', {}).get('tags', [])
print('yes' if 'fermi-orchestra' in tags else 'no')
" 2>/dev/null || echo "no")

    if [ "$IS_ORCHESTRA" = "yes" ]; then
        AGENT_ID=$(python3 -c "import json; print(json.load(open('$card_path'))['agent_id'])" 2>/dev/null)
        ORCHESTRA_AGENTS+=("$AGENT_ID:$card_path")
        echo "  • $AGENT_ID"
    fi
done

if [ ${#ORCHESTRA_AGENTS[@]} -eq 0 ]; then
    echo "  ✗ No fermi-orchestra agents found in agents/curated/"
    exit 1
fi
echo "  Found ${#ORCHESTRA_AGENTS[@]} fermi-orchestra agents"
echo ""

# ── Check which already exist on ABW ──────────────────────────
echo "▸ Checking ABW for existing agents…"
CREATED=0
SKIPPED=0
FAILED=0

for entry in "${ORCHESTRA_AGENTS[@]}"; do
    AGENT_ID="${entry%%:*}"
    CARD_PATH="${entry#*:}"

    # Check if agent exists
    CHECK_CODE=$(curl -sf -o /dev/null -w "%{http_code}" \
        -H "Authorization: Bearer $TOKEN" \
        "${ABW_URL}/api/agents/${AGENT_ID}" 2>/dev/null || echo "000")

    if [ "$CHECK_CODE" = "200" ]; then
        echo "  ✓ $AGENT_ID — already on ABW (skipping)"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    # ── Register the agent ─────────────────────────────────────
    echo "  ⟳ $AGENT_ID — registering on ABW…"

    # Build the create request from the local agent card
    CREATE_JSON=$(python3 -c "
import json, sys

card = json.load(open('$CARD_PATH'))
meta = card.get('metadata', {})
caps = card.get('capabilities', {})

req = {
    'agent_name': card['agent_id'],
    'agent_type': card.get('agent_type', 'research'),
    'description': meta.get('description', ''),
    'system_prompt': card.get('system_prompt', ''),
    'model': caps.get('model', 'claude-3-haiku-20240307'),
    'temperature': caps.get('temperature', 0.3),
    'executor_type': caps.get('executor', 'llm'),
    'tags': meta.get('tags', []),
    'visibility': 'public',
    'llm_provider': caps.get('provider', 'anthropic'),
    'embedding_provider': 'anthropic',
    'embedding_model': 'voyage-2',
    'embedding_dimension': 1024,
    'accepts': card.get('accepts', []),
    'produces': card.get('produces', []),
    'education_budget_credits': 0,
}

# Include sample queries in description if present
samples = meta.get('sample_queries', [])
if samples:
    req['description'] += '\\n\\nSample queries:\\n' + '\\n'.join('- ' + q for q in samples)

print(json.dumps(req))
" 2>/dev/null)

    if [ -z "$CREATE_JSON" ]; then
        echo "    ✗ Failed to parse $CARD_PATH"
        FAILED=$((FAILED + 1))
        continue
    fi

    # POST to ABW
    CREATE_RESP=$(curl -sf -w "\n%{http_code}" \
        -X POST \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "$CREATE_JSON" \
        "${ABW_URL}/api/agents" 2>/dev/null) || {
        echo "    ✗ HTTP request failed"
        FAILED=$((FAILED + 1))
        continue
    }

    CREATE_CODE=$(echo "$CREATE_RESP" | tail -1)
    CREATE_BODY=$(echo "$CREATE_RESP" | sed '$d')

    if [ "$CREATE_CODE" = "200" ] || [ "$CREATE_CODE" = "201" ]; then
        NEW_ID=$(echo "$CREATE_BODY" | python3 -c "import sys,json; print(json.load(sys.stdin).get('agent_id','?'))" 2>/dev/null || echo "?")
        echo "    ✓ Created (id: $NEW_ID)"

        # Auto-publish (new agents are created as 'draft')
        curl -sf -X PUT \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d '{"status": "published"}' \
            "${ABW_URL}/api/agents/${AGENT_ID}" > /dev/null 2>&1 && \
            echo "    ✓ Published" || \
            echo "    ⚠ Created but failed to publish — run: curl -X PUT ${ABW_URL}/api/agents/${AGENT_ID} -d '{\"status\":\"published\"}'"

        CREATED=$((CREATED + 1))
    else
        echo "    ✗ HTTP $CREATE_CODE"
        # Truncate long error bodies
        echo "      $(echo "$CREATE_BODY" | head -c 200)"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo "═══════════════════════════════════════════════════════════"
echo "  Summary: ${#ORCHESTRA_AGENTS[@]} fermi-orchestra agents"
echo "    ✓ Created:  $CREATED"
echo "    ○ Skipped:  $SKIPPED (already on ABW)"
if [ $FAILED -gt 0 ]; then
echo "    ✗ Failed:   $FAILED"
fi
echo ""
echo "  Verify: curl -s ${ABW_URL}/api/agents | python3 -c \\"
echo "    \"import sys,json; [print(a['agent_id']) for a in json.load(sys.stdin) if 'fermi-orchestra' in a.get('tags',[])]\""
echo "═══════════════════════════════════════════════════════════"
