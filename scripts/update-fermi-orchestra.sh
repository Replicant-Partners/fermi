#!/usr/bin/env bash
set -euo pipefail

# ─────────────────────────────────────────────────────────────────
# update-fermi-orchestra.sh — Push updated system prompts, descriptions,
# tags, and metadata to EXISTING ABW agents.
#
# The sync script only CREATES missing agents. This script UPDATES
# existing ones — critically, the system_prompt field, which the
# ABW execution handler uses (db_agent.system_prompt overrides the
# registry card in resolve_agent_card).
#
# Without this, agents run with stale/empty system prompts on ABW,
# causing refusals, poor evidence quality, and missing CARDINAL RULES.
#
# Usage:
#   ABW_TOKEN="your-bearer-token" ./scripts/update-fermi-orchestra.sh
#
# Options:
#   --dry-run     Show what would be updated without making changes
#   --create-missing  Also create agents that don't exist on ABW yet
#   --agent ID    Only update a specific agent (e.g., --agent equity_analyst)
# ─────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

ABW_URL="${ABW_URL:-https://agent-bestiary.world}"
TOKEN="${ABW_TOKEN:-${FERMI_API_KEY:-}}"
DRY_RUN=false
CREATE_MISSING=false
ONLY_AGENT=""

# Parse args
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)    DRY_RUN=true; shift ;;
        --create-missing) CREATE_MISSING=true; shift ;;
        --agent)      ONLY_AGENT="$2"; shift 2 ;;
        *)            echo "Unknown option: $1"; exit 1 ;;
    esac
done

if [ -z "$TOKEN" ]; then
    echo "✗ No auth token found."
    echo ""
    echo "  Set ABW_TOKEN or FERMI_API_KEY:"
    echo "    ABW_TOKEN=\"your-token\" ./scripts/update-fermi-orchestra.sh"
    echo ""
    echo "  Options:"
    echo "    --dry-run          Show changes without applying"
    echo "    --create-missing   Also create agents not yet on ABW (e.g., equity_analyst)"
    echo "    --agent ID         Only update a specific agent"
    exit 1
fi

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Fermi Orchestra → ABW UPDATE                               ║"
echo "╠══════════════════════════════════════════════════════════════╣"
echo "║  API: $ABW_URL"
if $DRY_RUN; then
echo "║  MODE: DRY RUN (no changes will be made)                    ║"
fi
echo "╚══════════════════════════════════════════════════════════════╝"
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
import json
d = json.load(open('$card_path'))
tags = d.get('metadata', {}).get('tags', [])
print('yes' if 'fermi-orchestra' in tags else 'no')
" 2>/dev/null || echo "no")

    if [ "$IS_ORCHESTRA" = "yes" ]; then
        AGENT_ID=$(python3 -c "import json; print(json.load(open('$card_path'))['agent_id'])" 2>/dev/null)
        if [ -n "$ONLY_AGENT" ] && [ "$AGENT_ID" != "$ONLY_AGENT" ]; then
            continue
        fi
        ORCHESTRA_AGENTS+=("$AGENT_ID:$card_path")
        echo "  • $AGENT_ID"
    fi
done

if [ ${#ORCHESTRA_AGENTS[@]} -eq 0 ]; then
    echo "  ✗ No matching fermi-orchestra agents found"
    exit 1
fi
echo "  Found ${#ORCHESTRA_AGENTS[@]} agents to process"
echo ""

# ── Process each agent ─────────────────────────────────────────
UPDATED=0
CREATED=0
SKIPPED=0
FAILED=0

for entry in "${ORCHESTRA_AGENTS[@]}"; do
    AGENT_ID="${entry%%:*}"
    CARD_PATH="${entry#*:}"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Processing: $AGENT_ID"
    echo "  Source:     $CARD_PATH"

    # Read local card details
    LOCAL_INFO=$(python3 -c "
import json
card = json.load(open('$CARD_PATH'))
meta = card.get('metadata', {})
caps = card.get('capabilities', {})
sp = card.get('system_prompt', '')
desc = meta.get('description', '')
tags = meta.get('tags', [])
samples = meta.get('sample_queries', [])
skills = caps.get('skills', [])

print(f'PROMPT_LEN={len(sp)}')
print(f'HAS_CARDINAL={\"CARDINAL\" in sp}')
print(f'DESC_LEN={len(desc)}')
print(f'TAGS={len(tags)}')
print(f'SKILLS={len(skills)}')
print(f'SAMPLES={len(samples)}')
" 2>/dev/null)
    echo "  Local:      $LOCAL_INFO"

    # Check if agent exists on ABW
    CHECK_RESP=$(curl -sf -w "\n%{http_code}" \
        -H "Authorization: Bearer $TOKEN" \
        "${ABW_URL}/api/agents/${AGENT_ID}" 2>/dev/null || echo -e "\n000")
    CHECK_CODE=$(echo "$CHECK_RESP" | tail -1)
    CHECK_BODY=$(echo "$CHECK_RESP" | sed '$d')

    if [ "$CHECK_CODE" != "200" ]; then
        echo "  ⚠ Agent not found on ABW (HTTP $CHECK_CODE)"
        if $CREATE_MISSING; then
            echo "  ⟳ Creating new agent on ABW…"

            CREATE_JSON=$(python3 -c "
import json
card = json.load(open('$CARD_PATH'))
meta = card.get('metadata', {})
caps = card.get('capabilities', {})

req = {
    'agent_name': card['agent_id'],
    'agent_type': card.get('agent_type', 'research'),
    'description': meta.get('description', ''),
    'system_prompt': card.get('system_prompt', ''),
    'model': caps.get('model', 'claude-sonnet-4-5-20250929'),
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
    'sample_queries': meta.get('sample_queries', []),
    'education_budget_credits': 0,
}
print(json.dumps(req))
" 2>/dev/null)

            if $DRY_RUN; then
                echo "  [DRY RUN] Would create $AGENT_ID"
                SKIPPED=$((SKIPPED + 1))
            else
                CREATE_RESULT=$(curl -sf -w "\n%{http_code}" \
                    -X POST \
                    -H "Authorization: Bearer $TOKEN" \
                    -H "Content-Type: application/json" \
                    -d "$CREATE_JSON" \
                    "${ABW_URL}/api/agents" 2>/dev/null || echo -e "\n000")
                CR_CODE=$(echo "$CREATE_RESULT" | tail -1)
                if [ "$CR_CODE" = "200" ] || [ "$CR_CODE" = "201" ]; then
                    echo "  ✓ Created successfully"
                    # Publish it
                    curl -sf -X PUT \
                        -H "Authorization: Bearer $TOKEN" \
                        -H "Content-Type: application/json" \
                        -d '{"status": "published"}' \
                        "${ABW_URL}/api/agents/${AGENT_ID}" > /dev/null 2>&1 && \
                        echo "  ✓ Published" || echo "  ⚠ Created but publish failed"
                    CREATED=$((CREATED + 1))
                else
                    CR_BODY=$(echo "$CREATE_RESULT" | sed '$d')
                    echo "  ✗ Create failed (HTTP $CR_CODE): $(echo "$CR_BODY" | head -c 200)"
                    FAILED=$((FAILED + 1))
                fi
            fi
        else
            echo "  ○ Skipping (use --create-missing to create)"
            SKIPPED=$((SKIPPED + 1))
        fi
        continue
    fi

    # ── Agent exists — check what needs updating ───────────────
    ABW_INFO=$(echo "$CHECK_BODY" | python3 -c "
import sys, json
d = json.load(sys.stdin)
sp = d.get('system_prompt', '') or ''
desc = d.get('description', '') or ''
tags = d.get('tags', []) or []
print(f'PROMPT_LEN={len(sp)}')
print(f'HAS_CARDINAL={\"CARDINAL\" in sp}')
print(f'DESC_LEN={len(desc)}')
print(f'TAGS={len(tags)}')
" 2>/dev/null || echo "PARSE_ERROR")
    echo "  ABW:        $ABW_INFO"

    # Build the update payload from local card
    UPDATE_JSON=$(python3 -c "
import json, sys

card = json.load(open('$CARD_PATH'))
meta = card.get('metadata', {})
caps = card.get('capabilities', {})

# Read current ABW state to compare
abw = json.load(sys.stdin)
abw_prompt = abw.get('system_prompt', '') or ''
local_prompt = card.get('system_prompt', '')

changes = []

update = {}

# Always update system_prompt — this is the critical field
if local_prompt != abw_prompt:
    update['system_prompt'] = local_prompt
    old_len = len(abw_prompt)
    new_len = len(local_prompt)
    has_cardinal_old = 'CARDINAL' in abw_prompt
    has_cardinal_new = 'CARDINAL' in local_prompt
    changes.append(f'system_prompt: {old_len}→{new_len} chars, cardinal: {has_cardinal_old}→{has_cardinal_new}')

# Update description
local_desc = meta.get('description', '')
abw_desc = abw.get('description', '') or ''
if local_desc and local_desc != abw_desc:
    update['description'] = local_desc
    changes.append(f'description: {len(abw_desc)}→{len(local_desc)} chars')

# Update tags
local_tags = meta.get('tags', [])
abw_tags = abw.get('tags', []) or []
if set(local_tags) != set(abw_tags):
    update['tags'] = local_tags
    added = set(local_tags) - set(abw_tags)
    removed = set(abw_tags) - set(local_tags)
    changes.append(f'tags: +{list(added)} -{list(removed)}')

# Update agent_type if changed
local_type = card.get('agent_type', 'research')
abw_type = abw.get('agent_type', '')
if local_type != abw_type:
    update['agent_type'] = local_type
    changes.append(f'agent_type: {abw_type}→{local_type}')

# Update model if different
local_model = caps.get('model', '')
abw_model = abw.get('model', '')
if local_model and local_model != abw_model:
    update['model'] = local_model
    changes.append(f'model: {abw_model}→{local_model}')

# Update temperature if different
local_temp = caps.get('temperature', 0.3)
abw_temp = abw.get('temperature', 0.3)
if abs(local_temp - abw_temp) > 0.01:
    update['temperature'] = local_temp
    changes.append(f'temperature: {abw_temp}→{local_temp}')

# Update accepts/produces
local_accepts = card.get('accepts', [])
abw_accepts = abw.get('accepts', []) or []
if set(local_accepts) != set(abw_accepts) and local_accepts:
    update['accepts'] = local_accepts
    changes.append(f'accepts: {abw_accepts}→{local_accepts}')

local_produces = card.get('produces', [])
abw_produces = abw.get('produces', []) or []
if set(local_produces) != set(abw_produces) and local_produces:
    update['produces'] = local_produces
    changes.append(f'produces updated')

# Update sample_queries
local_samples = meta.get('sample_queries', [])
abw_samples = abw.get('sample_queries', []) or []
if local_samples and local_samples != abw_samples:
    update['sample_queries'] = local_samples
    changes.append(f'sample_queries: {len(abw_samples)}→{len(local_samples)}')

if not changes:
    print('NO_CHANGES')
else:
    # Output changes summary to stderr, JSON to stdout
    for c in changes:
        print(f'CHANGE: {c}', file=sys.stderr)
    print(json.dumps(update))
" <<< "$CHECK_BODY" 2>&1)

    # Separate stderr (CHANGE lines) from stdout (JSON)
    CHANGES=$(echo "$UPDATE_JSON" | grep "^CHANGE:" || true)
    JSON_PAYLOAD=$(echo "$UPDATE_JSON" | grep -v "^CHANGE:" | grep -v "^NO_CHANGES" || true)
    NO_CHANGES=$(echo "$UPDATE_JSON" | grep "^NO_CHANGES" || true)

    if [ -n "$NO_CHANGES" ]; then
        echo "  ✓ Already up to date"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    # Show what's changing
    echo "$CHANGES" | sed 's/^/  /'

    if $DRY_RUN; then
        echo "  [DRY RUN] Would update $AGENT_ID"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    # ── Send PUT to ABW ────────────────────────────────────────
    if [ -z "$JSON_PAYLOAD" ]; then
        echo "  ⚠ Empty update payload — skipping"
        SKIPPED=$((SKIPPED + 1))
        continue
    fi

    UPDATE_RESP=$(curl -sf -w "\n%{http_code}" \
        -X PUT \
        -H "Authorization: Bearer $TOKEN" \
        -H "Content-Type: application/json" \
        -d "$JSON_PAYLOAD" \
        "${ABW_URL}/api/agents/${AGENT_ID}" 2>/dev/null || echo -e "\n000")
    UPDATE_CODE=$(echo "$UPDATE_RESP" | tail -1)
    UPDATE_BODY=$(echo "$UPDATE_RESP" | sed '$d')

    if [ "$UPDATE_CODE" = "200" ]; then
        echo "  ✓ Updated successfully"
        UPDATED=$((UPDATED + 1))
    else
        echo "  ✗ Update failed (HTTP $UPDATE_CODE)"
        echo "    $(echo "$UPDATE_BODY" | head -c 300)"

        # If PUT doesn't work, try PATCH
        echo "  ⟳ Retrying with PATCH…"
        PATCH_RESP=$(curl -sf -w "\n%{http_code}" \
            -X PATCH \
            -H "Authorization: Bearer $TOKEN" \
            -H "Content-Type: application/json" \
            -d "$JSON_PAYLOAD" \
            "${ABW_URL}/api/agents/${AGENT_ID}" 2>/dev/null || echo -e "\n000")
        PATCH_CODE=$(echo "$PATCH_RESP" | tail -1)
        PATCH_BODY=$(echo "$PATCH_RESP" | sed '$d')

        if [ "$PATCH_CODE" = "200" ]; then
            echo "  ✓ Updated via PATCH"
            UPDATED=$((UPDATED + 1))
        else
            echo "  ✗ PATCH also failed (HTTP $PATCH_CODE)"
            echo "    $(echo "$PATCH_BODY" | head -c 300)"
            FAILED=$((FAILED + 1))
        fi
    fi
done

echo ""
echo "══════════════════════════════════════════════════════════════"
echo ""
echo "  Summary: ${#ORCHESTRA_AGENTS[@]} fermi-orchestra agents processed"
echo ""
echo "    ✓ Updated:  $UPDATED"
if [ $CREATED -gt 0 ]; then
echo "    + Created:  $CREATED"
fi
echo "    ○ Skipped:  $SKIPPED"
if [ $FAILED -gt 0 ]; then
echo "    ✗ Failed:   $FAILED"
fi
echo ""

# ── Verification ───────────────────────────────────────────────
if [ $UPDATED -gt 0 ] || [ $CREATED -gt 0 ]; then
    echo "▸ Verifying updates…"
    echo ""
    for entry in "${ORCHESTRA_AGENTS[@]}"; do
        AGENT_ID="${entry%%:*}"
        VERIFY=$(curl -sf \
            -H "Authorization: Bearer $TOKEN" \
            "${ABW_URL}/api/agents/${AGENT_ID}" 2>/dev/null | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    sp = d.get('system_prompt', '') or ''
    has_cardinal = 'CARDINAL' in sp
    desc_len = len(d.get('description', '') or '')
    tags = d.get('tags', []) or []
    tag_count = len(tags)
    status = '✓' if has_cardinal or 'fermi' == '${AGENT_ID}' else '⚠'
    print(f'  {status} ${AGENT_ID:25s} prompt={len(sp):5d}  cardinal={has_cardinal}  desc={desc_len}  tags={tag_count}')
except:
    print(f'  ? ${AGENT_ID:25s} (verification failed)')
" 2>/dev/null || echo "  ? $AGENT_ID (unreachable)")
        echo "$VERIFY"
    done
    echo ""
fi

echo "══════════════════════════════════════════════════════════════"
if $DRY_RUN; then
    echo "  This was a DRY RUN. Re-run without --dry-run to apply changes."
fi
echo ""
echo "  Agents should now use updated system prompts on their next execution."
echo "  The DB system_prompt field takes priority over registry cards"
echo "  (see resolve_agent_card in api_server.rs line 2232)."
echo ""
