#!/bin/bash
# Test script for Sprint S1 agents: style_transfer, watermark, delivery, video_analyst
#
# Usage:
#   ./scripts/test-new-agents.sh                    # tests against production
#   ./scripts/test-new-agents.sh http://localhost:3000  # tests against local
#
# Prerequisites:
#   - Authenticated session (cookie) OR API key
#   - Credits in wallet (use @faucet self 500 via Xaman Ek)
#   - jq installed
#
# Set auth:
#   export ABW_COOKIE="abw_session=eyJ..."
#   OR
#   export ABW_API_KEY="ferm_..."

set -euo pipefail

BASE="${1:-https://agent-bestiary.world}"
PASS=0
FAIL=0
SKIP=0

# ── Auth header ──────────────────────────────────────────────────────

auth_header() {
  if [ -n "${ABW_API_KEY:-}" ]; then
    echo "Authorization: Bearer $ABW_API_KEY"
  elif [ -n "${ABW_COOKIE:-}" ]; then
    echo "Cookie: $ABW_COOKIE"
  else
    echo ""
  fi
}

AUTH=$(auth_header)
if [ -z "$AUTH" ]; then
  echo "ERROR: Set ABW_COOKIE or ABW_API_KEY for authentication"
  echo "  export ABW_COOKIE=\"abw_session=eyJ...\""
  echo "  export ABW_API_KEY=\"ferm_...\""
  exit 1
fi

# ── Helpers ──────────────────────────────────────────────────────────

call_api() {
  local method="$1" url="$2" data="${3:-}"
  if [ "$method" = "GET" ]; then
    curl -s -w "\n%{http_code}" -H "$AUTH" "$BASE$url"
  else
    curl -s -w "\n%{http_code}" -X "$method" \
      -H "$AUTH" -H "Content-Type: application/json" \
      -d "$data" "$BASE$url"
  fi
}

parse_response() {
  local raw="$1"
  local body http_code
  http_code=$(echo "$raw" | tail -1)
  body=$(echo "$raw" | sed '$d')
  echo "$http_code"
  echo "$body"
}

execute_agent() {
  local agent_id="$1" query="$2"
  local raw body http_code
  echo "  Executing $agent_id..."
  raw=$(call_api POST "/api/agents/$agent_id/execute" "{\"query\": \"$query\"}")
  http_code=$(echo "$raw" | tail -1)
  body=$(echo "$raw" | sed '$d')

  if [ "$http_code" = "200" ]; then
    local status confidence tokens credits iterations tools
    status=$(echo "$body" | jq -r '.status // "unknown"')
    confidence=$(echo "$body" | jq -r '.confidence // "n/a"')
    tokens=$(echo "$body" | jq -r '.tokens_used // 0')
    credits=$(echo "$body" | jq -r '.credits_charged // 0')
    iterations=$(echo "$body" | jq -r '.loop_iterations // 0')
    tools=$(echo "$body" | jq -r '[.tool_invocations[]?.tool_name] | join(", ") // "none"')
    episode=$(echo "$body" | jq -r '.episode_id // "n/a"')

    local answer
    answer=$(echo "$body" | jq -r '(.answer // .result // .evidence[0].summary // "") | tostring' 2>/dev/null | head -c 600)

    echo "    Status:     $status"
    echo "    Confidence: $confidence"
    echo "    Tokens:     $tokens"
    echo "    Credits:    $credits"
    echo "    Iterations: $iterations (tool loop)"
    echo "    Tools used: $tools"
    echo "    Episode:    $episode"
    if [ -n "$answer" ] && [ "$answer" != "null" ] && [ "$answer" != "" ]; then
      echo "    ── Answer ──"
      echo "$answer" | sed 's/^/    /'
      echo "    ────────────"
    fi

    if [ "$status" = "Success" ]; then
      return 0
    else
      echo "    WARNING: status=$status (not Success)"
      return 1
    fi
  elif [ "$http_code" = "402" ]; then
    echo "    SKIP: Insufficient credits (402)"
    return 2
  elif [ "$http_code" = "429" ]; then
    echo "    SKIP: Rate limited (429) — wait and retry"
    return 2
  else
    echo "    FAIL: HTTP $http_code"
    echo "    Body: $(echo "$body" | head -c 300)"
    return 1
  fi
}

record() {
  local result=$1
  if [ "$result" -eq 0 ]; then
    PASS=$((PASS + 1))
  elif [ "$result" -eq 2 ]; then
    SKIP=$((SKIP + 1))
  else
    FAIL=$((FAIL + 1))
  fi
}

separator() {
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# ── Preflight ────────────────────────────────────────────────────────

echo "Agent Test Suite — Sprint S1"
echo "Base: $BASE"
separator

echo "Preflight: checking auth + wallet..."
raw=$(call_api GET "/api/auth/me")
http_code=$(echo "$raw" | tail -1)
body=$(echo "$raw" | sed '$d')

if [ "$http_code" != "200" ]; then
  echo "  AUTH FAILED (HTTP $http_code) — check your token"
  exit 1
fi

user_id=$(echo "$body" | jq -r '.user_id')
role=$(echo "$body" | jq -r '.role // "unknown"')
echo "  User: $user_id (role: $role)"

raw=$(call_api GET "/api/wallet")
http_code=$(echo "$raw" | tail -1)
body=$(echo "$raw" | sed '$d')

if [ "$http_code" = "200" ]; then
  balance=$(echo "$body" | jq -r '.balance // 0')
  echo "  Balance: $balance credits"
  if [ "$balance" -lt 20 ]; then
    echo "  WARNING: Low balance — some tests may fail (need ~20 credits)"
  fi
else
  echo "  Could not check wallet (HTTP $http_code) — continuing anyway"
fi

# ── Test 1: Agent cards exist ────────────────────────────────────────

separator
echo "Test 1: Agent cards accessible"

for agent in style_transfer watermark delivery video_analyst; do
  raw=$(call_api GET "/api/agents/$agent")
  http_code=$(echo "$raw" | tail -1)
  body=$(echo "$raw" | sed '$d')

  if [ "$http_code" = "200" ]; then
    name=$(echo "$body" | jq -r '.display_alias // .agent_name // .name // "?"')
    tools=$(echo "$body" | jq -r '[.capabilities.tools[]?.name] | join(", ") // "none"' 2>/dev/null || echo "n/a")
    echo "  OK: $agent ($name) — tools: $tools"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $agent — HTTP $http_code"
    FAIL=$((FAIL + 1))
  fi
done

# ── Test 2: style_transfer ───────────────────────────────────────────

separator
echo "Test 2: style_transfer — text-only prompt (no source image)"
echo "  Expects: agent uses generate_image tool to create from text"

execute_agent "style_transfer" \
  "Generate a small watercolor painting of a Japanese garden with a red bridge"
record $?

# ── Test 3: watermark ────────────────────────────────────────────────

separator
echo "Test 3: watermark — text instruction (no source image)"
echo "  Expects: agent attempts edit_image or explains it needs a source"

execute_agent "watermark" \
  "Describe how you would add an authentication watermark to an artwork"
record $?

# ── Test 4: delivery ─────────────────────────────────────────────────

separator
echo "Test 4: delivery — list workspace contents"
echo "  Expects: agent uses workspace tools or explains the workflow"

execute_agent "delivery" \
  "What pieces are available in the current collection? List what you can find."
record $?

# ── Test 5: video_analyst — discovery ────────────────────────────────

separator
echo "Test 5: video_analyst — project discovery"
echo "  Expects: reduct_list_projects → reduct_get_project"

execute_agent "video_analyst" \
  "List all available video projects and show me the recordings in each one"
record $?

# ── Test 6: video_analyst — full reel pipeline ───────────────────────

separator
echo "Test 6: video_analyst — AI analysis → reel block candidates"
echo "  Expects: full pipeline: get transcript → analyze → create reel → add blocks"
echo "  This is the core use case: identify meaningful clips with timestamps"

execute_agent "video_analyst" \
  "Pick a project, pull the transcripts, and create a highlight reel. Identify the 3-5 most insightful moments — look for strong opinions, key decisions, or surprising insights. For each clip, use the exact start and end timestamps from the JSON transcript. Create the reel with a title card, then add each clip as a doc-range block."
record $?

# ── Test 7: gas schedule endpoint ────────────────────────────────────

separator
echo "Test 7: Gas schedule endpoint (public, no auth needed)"

raw=$(curl -s -w "\n%{http_code}" "$BASE/api/gas-schedule" 2>/dev/null)
http_code=$(echo "$raw" | tail -1)
body=$(echo "$raw" | sed '$d')

if [ "$http_code" = "200" ]; then
  echo "  OK: gas schedule returned"
  echo "  $(echo "$body" | jq -c '.' 2>/dev/null || echo "$body" | head -c 200)"
  PASS=$((PASS + 1))
elif [ "$http_code" = "404" ]; then
  echo "  SKIP: /api/gas-schedule not deployed yet (Sprint R)"
  SKIP=$((SKIP + 1))
else
  echo "  FAIL: HTTP $http_code"
  FAIL=$((FAIL + 1))
fi

# ── Summary ──────────────────────────────────────────────────────────

separator
echo ""
echo "Results: $PASS passed, $FAIL failed, $SKIP skipped"
echo ""

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
