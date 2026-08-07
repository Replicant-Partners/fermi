#!/usr/bin/env bash
# ═══════════════════════════════════════════════════════════════════
# Lint — no LLM provider credentials from env at runtime
#
# Enforces SPEC_28 (docs/specs/SPEC_28_UNIFIED_CREDENTIAL_PATH.md) and
# AGENT_CREDENTIAL_MODEL.md §5: *"No env branch for agent keys."*
#
# WHY THIS LINT EXISTS
# ────────────────────
# Every regression in this class has had the same shape: someone adds a
# new execution path, forgets the credential store, reaches for
# `std::env::var("..._API_KEY")` because it's right there, and the path
# silently bills the platform for a user's agent. That is invisible in
# review and invisible in production until the bill arrives.
#
# The v0.9.2 hard-fail doctrine was written to close exactly this, and it
# was defeated within one release by a tool-loop bypass 700 lines away
# from the comment describing it. A prose invariant did not hold. A grep
# invariant does.
#
# WHAT IS AND ISN'T A VIOLATION
# ─────────────────────────────
# Violation: reading an *LLM provider* key from env in agent execution
# code. Which account pays for an LLM call has a per-agent answer that
# env cannot express.
#
# Not a violation:
#   * Third-party TOOL/service keys (Brave search, OpenWeather, Cartesia
#     voice, football data, …). These authenticate the *platform* to an
#     external service. They are platform infrastructure, not per-agent
#     billing, and env is the right home.
#   * `src/api_server.rs` boot bootstrap — env is the one-time seed that
#     populates the abw-system store. That is the design.
#   * `src/bin/*` single-tenant operator CLIs (e.g. agent-mcp-server):
#     one operator, own machine, own key. See `operator_credentials()`.
#
# Run:  ./scripts/lint-no-env-credentials.sh
# CI:   wire into .github/workflows/ci.yml beside lint-owner-columns.sh
# ═══════════════════════════════════════════════════════════════════

set -uo pipefail
cd "$(dirname "$0")/.."

# LLM provider keys — the ones that must come from the credential store.
LLM_KEYS='ANTHROPIC_API_KEY|OPENAI_API_KEY|MISTRAL_API_KEY|QWEN_API_KEY|OPENROUTER_API_KEY|GLM_API_KEY|DEEPSEEK_API_KEY|KIMI_API_KEY'

# Scanned: the multi-tenant execution surface.
SCAN_DIRS="src/agent_backend src/handlers"

# ── Known debt ─────────────────────────────────────────────────────
#
# Platform-service LLM calls that bypass the executor and read env
# directly. They bill the PLATFORM (not a user), so they are not a
# user-billing leak — but they are unauditable and unrotatable, and they
# will break when the platform moves fully off env.
#
# Each should move to a store-backed `resolve_platform_credential(state,
# provider)` reading `(abw-system, provider, '*')`. Behaviour-preserving:
# the bootstrap already seeds that row from the same env var.
#
# THIS LIST MUST ONLY EVER SHRINK. Adding an entry means adding an
# unauditable spend path — don't. Remove entries as they're migrated.
KNOWN_DEBT=(
  "src/handlers/eval_judge.rs"           # LLM-as-judge scoring
  "src/handlers/eval.rs"                 # LLM-backed evaluators
  "src/handlers/wizard.rs"               # agent-creation wizard (ER diagram gen)
  "src/handlers/creatures/agent_modules.rs" # creature episode consolidation
)

is_known_debt() {
  local file="$1"
  for d in "${KNOWN_DEBT[@]}"; do
    [[ "$file" == "$d" ]] && return 0
  done
  return 1
}

echo "── SPEC_28: LLM provider keys must come from the credential store ──"

violations=0
debt_hits=0

# `env::var("<LLM KEY>")` — the exact pattern. Matches both
# `std::env::var(...)` and a bare `env::var(...)`.
while IFS=: read -r file line _rest; do
  [[ -z "${file:-}" ]] && continue
  if is_known_debt "$file"; then
    debt_hits=$((debt_hits + 1))
  else
    echo "  VIOLATION  $file:$line"
    violations=$((violations + 1))
  fi
done < <(grep -rn -E "env::var\(\"($LLM_KEYS)\"\)" $SCAN_DIRS 2>/dev/null)

echo
if [[ $violations -gt 0 ]]; then
  cat <<'EOF'
FAIL: an LLM provider key is being read from the environment in agent
execution code.

Which account pays for an LLM call is a per-agent fact. Env is global to
the container, so an env-sourced key silently bills the platform for
whoever's agent happens to run — the failure mode SPEC_28 closed.

Fix: obtain the key from the execution's credentials instead.

    // in an executor
    let api_key = context.key_for(provider)?;

    // in a handler, before building ExecutionContext
    let credentials = crate::build_execution_credentials(&state, &db_agent, &card).await;

See docs/specs/SPEC_28_UNIFIED_CREDENTIAL_PATH.md §4.
EOF
  exit 1
fi

echo "PASS: no new env-sourced LLM provider credentials."
if [[ $debt_hits -gt 0 ]]; then
  echo "      ($debt_hits known-debt reference(s) in ${#KNOWN_DEBT[@]} file(s) — platform-funded,"
  echo "       tracked above. Migrate to the abw-system store; never add more.)"
fi
exit 0
