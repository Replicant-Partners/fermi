#!/usr/bin/env bash
#
# Smoke-test the three Apps creation paths against a deployed ABW server.
#
#   1. CLI         — abw app new / validate / deploy / spawn
#   2. Conversational — POST /api/xaman/sessions (app_design) → create-app
#   3. Fork        — POST /api/workspaces/:id/fork-to-app → review → POST /api/apps
#
# Plus negative cases (reserved slug, invalid slug, unauth) and cleanup
# (archive every App created so reruns don't accumulate cruft).
#
# Usage:
#
#   ABW_BASE_URL=https://agent-bestiary.world \
#   ABW_API_TOKEN=<key> \
#       ./scripts/smoke-test-apps.sh
#
# Optional flags:
#   --cli-binary <path>   Path to abw binary (default: target/debug/abw)
#   --keep                Don't archive the Apps at the end (for inspection)
#   --skip-cli            Skip the CLI section (e.g. when binary isn't built)
#   --skip-fork           Skip the fork-from-workspace section
#   --skip-session        Skip the xamanEK app_design section
#   --skip-negatives      Skip the negative-case section
#
# Exits non-zero on first failure; pretty-prints a summary at the end either way.
#
# Dependencies: bash, curl, jq. No external test framework — this is a
# deliberately small script so failures show up as plain shell lines that
# anyone can read and fix.

set -uo pipefail

# ─── Configuration ──────────────────────────────────────────────────────────

ABW_BASE_URL="${ABW_BASE_URL:-https://agent-bestiary.world}"
ABW_API_TOKEN="${ABW_API_TOKEN:-}"
CLI_BINARY="${CLI_BINARY:-./target/debug/abw}"
KEEP_APPS=0
SKIP_CLI=0
SKIP_FORK=0
SKIP_SESSION=0
SKIP_NEGATIVES=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --cli-binary) CLI_BINARY="$2"; shift 2 ;;
        --keep)       KEEP_APPS=1; shift ;;
        --skip-cli)   SKIP_CLI=1; shift ;;
        --skip-fork)  SKIP_FORK=1; shift ;;
        --skip-session) SKIP_SESSION=1; shift ;;
        --skip-negatives) SKIP_NEGATIVES=1; shift ;;
        -h|--help)
            sed -n '2,30p' "$0"
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

if [[ -z "$ABW_API_TOKEN" ]]; then
    echo "error: ABW_API_TOKEN must be set (mint one at \$ABW_BASE_URL/settings/api-keys)" >&2
    exit 2
fi

for cmd in curl jq; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "error: required command '$cmd' not found" >&2
        exit 2
    fi
done

# ─── Pretty output ──────────────────────────────────────────────────────────

C_RESET=$'\033[0m'
C_BOLD=$'\033[1m'
C_DIM=$'\033[2m'
C_RED=$'\033[31m'
C_GREEN=$'\033[32m'
C_YELLOW=$'\033[33m'
C_CYAN=$'\033[36m'

PASSED=0
FAILED=0
SKIPPED=0
FAILURES=()
CREATED_APPS=()  # slugs to archive at the end
CREATED_WORKSPACES=()  # workspace ids to abandon (best effort)

section() {
    echo
    echo "${C_BOLD}${C_CYAN}── $* ──${C_RESET}"
}

pass() {
    echo "  ${C_GREEN}✓${C_RESET} $*"
    PASSED=$((PASSED + 1))
}

fail() {
    echo "  ${C_RED}✗${C_RESET} $*"
    FAILED=$((FAILED + 1))
    FAILURES+=("$1")
}

skip() {
    echo "  ${C_DIM}↷${C_RESET} ${C_DIM}$*${C_RESET}"
    SKIPPED=$((SKIPPED + 1))
}

info() {
    echo "  ${C_DIM}$*${C_RESET}"
}

# ─── HTTP helpers ───────────────────────────────────────────────────────────

# api METHOD PATH [BODY_JSON]
# Prints "<status>:<body>" to stdout. Caller splits on the first colon.
api() {
    local method="$1"
    local path="$2"
    local body="${3:-}"
    local url="${ABW_BASE_URL%/}${path}"
    local args=(-sS -w "%{http_code}" -X "$method" -H "Authorization: Bearer ${ABW_API_TOKEN}")
    if [[ -n "$body" ]]; then
        args+=(-H "Content-Type: application/json" --data "$body")
    fi
    local response
    response=$(curl "${args[@]}" "$url")
    local status="${response: -3}"
    local body="${response::-3}"
    echo "${status}:${body}"
}

# Unique slug per run so we can rerun without clashes.
RUN_STAMP=$(date +%s)
slug_for() {
    echo "smoketest_${1}_${RUN_STAMP}"
}

# ─── Preflight: server reachable + auth works ───────────────────────────────

section "Preflight"

info "base url:      ${ABW_BASE_URL}"
info "api token:     $(echo "${ABW_API_TOKEN:0:6}…${ABW_API_TOKEN: -4}")"
info "cli binary:    ${CLI_BINARY}"

result=$(api GET "/api/auth/me")
status="${result%%:*}"
body="${result#*:}"
if [[ "$status" == "200" ]]; then
    user_id=$(echo "$body" | jq -r '.user_id // .ApiKey.user_id // "?"')
    pass "auth: server reachable, token valid (user=${user_id})"
else
    fail "auth: GET /api/auth/me returned ${status} — check ABW_BASE_URL and ABW_API_TOKEN"
    echo "    body: $(echo "$body" | head -c 200)"
    exit 1
fi

# ─── Path 1: CLI ────────────────────────────────────────────────────────────

if [[ $SKIP_CLI -eq 0 ]]; then
    section "Path 1 — CLI (abw)"

    if [[ ! -x "$CLI_BINARY" ]]; then
        skip "CLI binary not found at $CLI_BINARY (run: cargo build -p abw-cli)"
    else
        cli_slug=$(slug_for cli)
        tmpdir=$(mktemp -d)
        trap "rm -rf '$tmpdir'" EXIT

        # Absolute path to the CLI binary — subshells may cd around, and a
        # relative ./target/debug/abw would break.
        cli_abs=$(readlink -f "$CLI_BINARY" 2>/dev/null || realpath "$CLI_BINARY" 2>/dev/null || echo "$CLI_BINARY")

        # abw app new — scaffolds a directory + manifest.json
        # Capture stderr so a real error shows up in the failure message
        # instead of being swallowed by >/dev/null 2>&1.
        new_log="$tmpdir/abw-new.log"
        if (cd "$tmpdir" && \
            ABW_BASE_URL="$ABW_BASE_URL" ABW_API_TOKEN="$ABW_API_TOKEN" \
            "$cli_abs" --quiet app new "$cli_slug" \
                --tagline "smoke test ${RUN_STAMP}" \
                --description "Created by smoke-test-apps.sh" \
                >"$new_log" 2>&1) && [[ -d "$tmpdir/$cli_slug" ]]; then
            pass "abw app new ${cli_slug} — scaffold created"
        else
            fail "abw app new ${cli_slug} — scaffold failed (log: $(head -c 200 "$new_log" 2>/dev/null || echo none))"
            # The validate/deploy/spawn block below is guarded by `if [[ -d "$tmpdir/$cli_slug" ]]`
            # so we just fall through without those running.
        fi

        if [[ -d "$tmpdir/$cli_slug" ]]; then
            # abw app validate
            validate_log="$tmpdir/abw-validate.log"
            if (cd "$tmpdir/$cli_slug" && \
                ABW_BASE_URL="$ABW_BASE_URL" ABW_API_TOKEN="$ABW_API_TOKEN" \
                "$cli_abs" --quiet app validate >"$validate_log" 2>&1); then
                pass "abw app validate — passes on clean scaffold"
            else
                fail "abw app validate — failed (log: $(head -c 200 "$validate_log"))"
            fi

            # abw app deploy
            deploy_log="$tmpdir/abw-deploy.log"
            if (cd "$tmpdir/$cli_slug" && \
                ABW_BASE_URL="$ABW_BASE_URL" ABW_API_TOKEN="$ABW_API_TOKEN" \
                "$cli_abs" --quiet app deploy >"$deploy_log" 2>&1); then
                pass "abw app deploy — registered with server"
                CREATED_APPS+=("$cli_slug")
            else
                fail "abw app deploy — failed (log: $(head -c 300 "$deploy_log"))"
            fi

            # Server-side verification: App is fetchable
            result=$(api GET "/api/apps/${cli_slug}")
            status="${result%%:*}"
            if [[ "$status" == "200" ]]; then
                pass "GET /api/apps/${cli_slug} — App is fetchable"
            else
                fail "GET /api/apps/${cli_slug} — got ${status}, expected 200"
            fi

            # abw app spawn — creates a workspace from the App
            spawn_out=$(cd "$tmpdir/$cli_slug" && \
                ABW_BASE_URL="$ABW_BASE_URL" ABW_API_TOKEN="$ABW_API_TOKEN" \
                "$cli_abs" --quiet app spawn "$cli_slug" 2>&1)
            if [[ -n "$spawn_out" && "$spawn_out" == *"/workspace/"* ]]; then
                pass "abw app spawn — workspace URL returned ($(echo "$spawn_out" | head -c 60)…)"
                ws_id=$(echo "$spawn_out" | grep -oE '/workspace/[^/[:space:]]+' | head -1 | sed 's|/workspace/||')
                [[ -n "$ws_id" ]] && CREATED_WORKSPACES+=("$ws_id")
            else
                fail "abw app spawn — no workspace URL in output: ${spawn_out:0:200}"
            fi
        fi
    fi
else
    skip "Path 1 (CLI) skipped via --skip-cli"
fi

# ─── Path 2: Conversational (xamanEK app_design session) ────────────────────

if [[ $SKIP_SESSION -eq 0 ]]; then
    section "Path 2 — Conversational (xamanEK app_design)"

    session_slug=$(slug_for session)

    # Create an app_design session with in_progress already populated.
    # The session-create endpoint accepts an in_progress object on creation —
    # there's no separate update endpoint (sessions evolve via xaman_ek's
    # __UPDATE__ blocks emitted from the message handler). For a smoke test
    # that doesn't talk to the LLM, we just hand it the finished manifest
    # directly and call create-app.
    create_body=$(jq -nc \
        --arg type "app_design" \
        --arg title "smoke test session" \
        --arg slug "$session_slug" \
        --arg name "Smoke session $RUN_STAMP" \
        --arg tagline "exercises POST /api/xaman/sessions/:id/create-app" \
        '{
            session_type: $type,
            title: $title,
            in_progress: {
                slug: $slug,
                name: $name,
                tagline: $tagline,
                description: "Created by smoke-test-apps.sh via the conversational path",
                visibility: "private",
                status: "ready_to_create"
            }
        }')
    result=$(api POST "/api/xaman/sessions" "$create_body")
    status="${result%%:*}"
    body="${result#*:}"

    if [[ "$status" == "200" || "$status" == "201" ]]; then
        session_id=$(echo "$body" | jq -r '.session_id')
        if [[ -n "$session_id" && "$session_id" != "null" ]]; then
            pass "POST /api/xaman/sessions (type=app_design, in_progress prefilled) — created ${session_id}"
        else
            fail "POST /api/xaman/sessions — no session_id in response: ${body:0:200}"
            session_id=""
        fi
    else
        fail "POST /api/xaman/sessions — got ${status}: ${body:0:200}"
        session_id=""
    fi

    if [[ -n "$session_id" ]]; then
        # Fire create-app
        result=$(api POST "/api/xaman/sessions/${session_id}/create-app")
        status="${result%%:*}"
        body="${result#*:}"
        if [[ "$status" == "200" || "$status" == "201" ]]; then
            created_slug=$(echo "$body" | jq -r '.slug')
            if [[ "$created_slug" == "$session_slug" ]]; then
                pass "POST /api/xaman/sessions/${session_id}/create-app — App '${created_slug}' registered"
                CREATED_APPS+=("$created_slug")
            else
                fail "create-app returned slug '${created_slug}', expected '${session_slug}'"
            fi
        else
            fail "POST /api/xaman/sessions/${session_id}/create-app — got ${status}: ${body:0:200}"
        fi

        # Verify the App is fetchable
        result=$(api GET "/api/apps/${session_slug}")
        status="${result%%:*}"
        if [[ "$status" == "200" ]]; then
            pass "GET /api/apps/${session_slug} — App registered via session path is fetchable"
        else
            fail "GET /api/apps/${session_slug} — got ${status}"
        fi
    fi
else
    skip "Path 2 (conversational) skipped via --skip-session"
fi

# ─── Path 3: Fork from workspace ────────────────────────────────────────────

if [[ $SKIP_FORK -eq 0 ]]; then
    section "Path 3 — Fork from workspace"

    fork_slug=$(slug_for fork)
    # Team slug must be unique-per-server and slug-formatted. Reuse the
    # timestamp so reruns don't collide and the slug remains predictable.
    source_ws_slug="smoke_fork_source_${RUN_STAMP}"

    # Create a workspace to fork from. Workspaces ARE teams on the platform —
    # the canonical creation endpoint is POST /api/teams. CreateTeamRequest
    # requires name + slug, optional description + origin + mission +
    # coordination_strategist_id. Origin defaults to "bestiary_workspace"
    # which is exactly what we want (a personal workspace, not an app-spawned
    # one — apps spawn workspaces via POST /api/apps/:slug/workspaces, which
    # is a different code path).
    ws_body=$(jq -nc \
        --arg name "smoke-fork-source-${RUN_STAMP}" \
        --arg slug "$source_ws_slug" \
        '{
            name: $name,
            slug: $slug,
            description: "smoke test fork source — safe to delete",
            origin: "bestiary_workspace"
        }')
    result=$(api POST "/api/teams" "$ws_body")
    status="${result%%:*}"
    body="${result#*:}"
    if [[ "$status" == "200" || "$status" == "201" ]]; then
        ws_id=$(echo "$body" | jq -r '.id // .team_id // .workspace_id')
        if [[ -n "$ws_id" && "$ws_id" != "null" ]]; then
            pass "POST /api/teams — created source workspace ${ws_id}"
            CREATED_WORKSPACES+=("$ws_id")
        else
            fail "POST /api/teams — no id in response: ${body:0:200}"
            ws_id=""
        fi
    else
        fail "POST /api/teams — got ${status}: ${body:0:200}"
        ws_id=""
    fi

    if [[ -n "$ws_id" ]]; then
        # Fork it
        result=$(api POST "/api/workspaces/${ws_id}/fork-to-app")
        status="${result%%:*}"
        body="${result#*:}"
        if [[ "$status" == "200" || "$status" == "201" ]]; then
            has_manifest=$(echo "$body" | jq -r '.draft_manifest != null')
            issue_count=$(echo "$body" | jq -r '.issues | length // 0')
            if [[ "$has_manifest" == "true" ]]; then
                pass "POST /api/workspaces/${ws_id}/fork-to-app — draft manifest returned (${issue_count} issues/suggestions)"
            else
                fail "fork-to-app returned no draft_manifest: ${body:0:200}"
            fi
        else
            fail "POST /api/workspaces/${ws_id}/fork-to-app — got ${status}: ${body:0:200}"
            body=""
        fi

        if [[ -n "$body" ]]; then
            # Take the draft and publish it via POST /api/apps with our own slug
            publish_body=$(echo "$body" | jq -c --arg slug "$fork_slug" \
                '.draft_manifest | .slug = $slug | .visibility = "private"')
            result=$(api POST "/api/apps" "$publish_body")
            status="${result%%:*}"
            published_body="${result#*:}"
            if [[ "$status" == "201" || "$status" == "200" ]]; then
                pass "POST /api/apps — published forked manifest as '${fork_slug}'"
                CREATED_APPS+=("$fork_slug")
            else
                fail "POST /api/apps — got ${status}: ${published_body:0:200}"
            fi
        fi
    fi
else
    skip "Path 3 (fork) skipped via --skip-fork"
fi

# ─── Negative cases ─────────────────────────────────────────────────────────

if [[ $SKIP_NEGATIVES -eq 0 ]]; then
    section "Negative cases"

    # Reserved slug → 409 CONFLICT
    body='{"slug": "rabble_swarm", "name": "Reserved Test"}'
    result=$(api POST "/api/apps" "$body")
    status="${result%%:*}"
    if [[ "$status" == "409" ]]; then
        pass "POST /api/apps slug='rabble_swarm' → 409 (reserved)"
    else
        fail "reserved slug test — expected 409, got ${status}: ${result#*:}"
    fi

    # Invalid slug (uppercase) → 400 BAD REQUEST
    body='{"slug": "BadSlug", "name": "Invalid Test"}'
    result=$(api POST "/api/apps" "$body")
    status="${result%%:*}"
    if [[ "$status" == "400" ]]; then
        pass "POST /api/apps slug='BadSlug' → 400 (invalid format)"
    else
        fail "invalid slug test — expected 400, got ${status}: ${result#*:}"
    fi

    # Duplicate slug (same slug twice in quick succession) → 409 CONFLICT
    dup_slug=$(slug_for dup)
    body=$(jq -nc --arg slug "$dup_slug" --arg name "Dup test" \
        '{slug: $slug, name: $name}')
    result=$(api POST "/api/apps" "$body")
    status="${result%%:*}"
    if [[ "$status" == "201" || "$status" == "200" ]]; then
        CREATED_APPS+=("$dup_slug")
        # Second time should conflict
        result=$(api POST "/api/apps" "$body")
        status="${result%%:*}"
        if [[ "$status" == "409" ]]; then
            pass "POST /api/apps (duplicate slug) → 409 (already exists)"
        else
            fail "duplicate slug test — expected 409 on 2nd POST, got ${status}: ${result#*:}"
        fi
    else
        info "(skipping duplicate-slug check — initial register failed)"
    fi

    # Unauthenticated → 401
    bad_token_response=$(curl -sS -w "%{http_code}" \
        -H "Authorization: Bearer not-a-real-token-${RUN_STAMP}" \
        "${ABW_BASE_URL%/}/api/apps" -X POST \
        -H "Content-Type: application/json" \
        --data '{"slug":"unauthtest","name":"u"}')
    status="${bad_token_response: -3}"
    if [[ "$status" == "401" || "$status" == "403" ]]; then
        pass "POST /api/apps (bad token) → ${status} (auth refused)"
    else
        fail "unauth test — expected 401/403, got ${status}"
    fi
else
    skip "Negative cases skipped via --skip-negatives"
fi

# ─── Cleanup ────────────────────────────────────────────────────────────────

section "Cleanup"

if [[ $KEEP_APPS -eq 1 ]]; then
    info "--keep set — leaving ${#CREATED_APPS[@]} apps + ${#CREATED_WORKSPACES[@]} workspaces in place"
    for slug in "${CREATED_APPS[@]}"; do
        info "  app: ${slug}"
    done
    for ws in "${CREATED_WORKSPACES[@]}"; do
        info "  workspace: ${ws}"
    done
else
    for slug in "${CREATED_APPS[@]}"; do
        result=$(api POST "/api/apps/${slug}/archive")
        status="${result%%:*}"
        if [[ "$status" == "200" || "$status" == "204" ]]; then
            info "archived app: ${slug}"
        else
            info "archive failed for ${slug} (${status}) — clean up manually"
        fi
    done
    # Workspaces don't have a generic delete endpoint; leave them alone.
    if [[ ${#CREATED_WORKSPACES[@]} -gt 0 ]]; then
        info "${#CREATED_WORKSPACES[@]} workspaces created during the run remain (no public delete endpoint)"
    fi
fi

# ─── Summary ────────────────────────────────────────────────────────────────

section "Summary"

echo "  passed:  ${C_GREEN}${PASSED}${C_RESET}"
echo "  failed:  $(if [[ $FAILED -gt 0 ]]; then echo "${C_RED}${FAILED}${C_RESET}"; else echo "$FAILED"; fi)"
if [[ $SKIPPED -gt 0 ]]; then
    echo "  skipped: ${C_DIM}${SKIPPED}${C_RESET}"
fi
echo

if [[ $FAILED -gt 0 ]]; then
    echo "${C_RED}${C_BOLD}FAILED${C_RESET}"
    for f in "${FAILURES[@]}"; do
        echo "  - $f"
    done
    exit 1
fi

echo "${C_GREEN}${C_BOLD}OK${C_RESET}"
