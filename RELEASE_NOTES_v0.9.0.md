# Fermi Console v0.9.0 — Agent-owner API key routing (marketplace unlock)

**Minor version bump.** This release closes the gap between the
platform's stated architecture — *"agents carry their own funding,
users hire agents with economics folded in"* — and what the code
actually did (one shared Anthropic account for every execution).

Before this release, every agent execution — Fermi, macro_forecaster,
someone's third-party analyst — read the same process-wide
`ANTHROPIC_API_KEY` env var. When that account ran out of credits,
the platform stopped working for everyone whose escape hatch wasn't
"I have my own key locally" (only the developer). That's the exact
outage Mario hit on his EPL forecasts.

**After this release:**

- **System agents** (Fermi, xaman_ek) continue to run on the
  platform's env-var key. Platform-funded, as intended.
- **Third-party agents** (anything Mario publishes) route through
  their **owner's** stored API key. The executor picks up the key
  from the *agent's* secrets, not the caller's env var.
- **The shared-account-depleted outage becomes structurally
  impossible** for owner-owned agents. Each agent has its own
  funding.

## What changed

### Executor rewiring

`src/agent_backend/tool_executor.rs::execute_anthropic_loop`
resolves the API key in a new order:

```rust
let api_key = self.tool_context.user_secrets
    .as_ref()
    .and_then(|s| s.get("ANTHROPIC_API_KEY").cloned())    // ← agent-owner secret
    .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())  // ← platform fallback
    .ok_or_else(|| ExecutionError::ExecutionFailed(
        "No ANTHROPIC_API_KEY: agent owner has not funded this agent, \
         and no platform fallback is set.".into()
    ))?;
```

`ToolContext.user_secrets` used to be hardcoded `None` at both
handler sites. Now `execute_agent_handler` and
`execute_agent_stream_handler` populate it via a new resolver.

### The resolver

`api_server.rs::resolve_agent_owner_secrets`:

```rust
pub(crate) async fn resolve_agent_owner_secrets(
    state: &AppState,
    agent: &Agent,
) -> Option<HashMap<String, String>> {
    // System-tier: platform funds via env var.
    if agent.tier.eq_ignore_ascii_case("system") { return None; }
    let encryptor = state.secret_encryptor.as_ref()?;
    let owner_id = agent.owner_id.as_ref()?;
    // Owner-owned agent: look up OWNER's secrets, scoped to the
    // agent's name (or "*" for global-across-owner-agents).
    fermi_auth::get_secrets_for_agent(
        &state.db, encryptor, owner_id, &agent.agent_name,
    ).await.ok().filter(|s| !s.is_empty())
}
```

`user_id` = the agent OWNER's id, not the caller's. `scope` = agent
name (a multi-agent owner can budget per-agent) or `*` (shared
across all their agents).

### New endpoints — owner secret management

Three routes, all owner-gated (only `agent.owner_id == caller` or
admin), all in `src/handlers/agent_secrets.rs`:

```
PUT    /api/agents/:agent_id/secrets/:secret_name
       body: { "value": "sk-...", "label": "personal", "description": "..." }

GET    /api/agents/:agent_id/secrets
       returns: { agent_name, count, has_anthropic_key, secrets: [...metadata only...] }

DELETE /api/agents/:agent_id/secrets/:secret_name
       returns: 204 No Content
```

Values are **never** returned by GET — only metadata (name, label,
scope, timestamps). The response's `has_anthropic_key: bool`
convenience field is the direct feed for the console's forthcoming
"is this agent funded?" marketplace badge.

**System agents return 403 on all three routes** — they're
platform-funded and don't have owner-managed secrets. The
`require_agent_owner_or_admin` guard catches this before touching
the secrets table.

### Backward compatibility

- **No schema migration.** `user_secrets` (migration 039) already
  exists. `Agent.owner_id` and `Agent.tier` were already columns.
- **Env-var fallback is preserved** for owner-owned agents whose
  owners haven't uploaded a key yet. This is deliberately soft so
  v0.9.0 doesn't hard-break the current state. v0.9.1 will tighten
  the fallback into a `"agent owner has not funded this agent"`
  hard error once owners have started uploading.
- **All existing agent-execution behaviour is unchanged** when no
  owner secret is set — same env-var path.

### Security invariants (test-enforced)

`tests/agent_secrets_shapes.rs` — 7 assertions:

- PUT response never contains `value` / `plaintext`
- GET response never contains `value` / `plaintext` / `encrypted_value`
- Every list row carries the full metadata set the console expects
- `count == secrets.len()`
- `has_anthropic_key` iff a row has that secret_name
- Every row's scope matches the agent name OR `"*"`
  (matches the executor's WHERE clause exactly)
- No response body anywhere contains `"sk-"` or `"Bearer "`
  (belt-and-braces regression net for any future plaintext leak)

### Out of scope (documented, deferred)

- **Console UI** for owners to upload keys (v0.9.1 — small; server
  contract is now stable). Mario can already fund an agent today
  by hitting the endpoints via curl / any HTTP client:

  ```bash
  curl -X PUT https://.../api/agents/<his-agent-id>/secrets/ANTHROPIC_API_KEY \
    -H "Authorization: Bearer $ABW_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"value":"sk-ant-...","label":"personal"}'
  ```

- **Credit flow** (v0.9.2). Caller pays owner in platform credits at
  hire time. Primitives exist (`get_or_create_wallet`,
  `credit_charge`); the executor needs to invoke them between the
  wallet check and the LLM call.

- **Hard-fail when owner has no key** (v0.9.1). Currently soft-falls
  through to env; will tighten once owners have funded.

- **OpenAI / Mistral / other-provider secret routing.** v0.9.0
  focuses on Anthropic because that's the only path currently in
  active use. The `execute_openai_loop` and multi-model executor
  paths still read from env. Extending them is a small, isolated
  follow-up.

## Migration

None. No schema change. Env-var fallback preserves current behavior
for agents whose owners haven't funded them.

## What this unlocks for Mario

Once he uploads a key against his own published agent:

1. `PUT /api/agents/<mario-agent-id>/secrets/ANTHROPIC_API_KEY` with
   his own key.
2. When Ivan (or anyone) hires Mario's agent, the executor picks up
   Mario's key from `ToolContext.user_secrets`. Anthropic bills
   Mario's account.
3. Mario's marketplace card shows "funded" (`has_anthropic_key:
   true`) — buyers know the agent is runnable.
4. The shared platform pool being depleted doesn't stop Mario's
   agent from running.

For **Fermi** and other system agents, nothing changes — they still
run on the platform's env-var key. Which means: **top up the platform
account** and Fermi works again for everyone, but no user has to
share that pool for their own hired agents.

## Files touched

- `src/api_server.rs`
  - New `resolve_agent_owner_secrets` helper alongside `resolve_agent`
  - Two new routes registered under `/api/agents/:agent_id/secrets`
- `src/agent_backend/tool_executor.rs`
  - `execute_anthropic_loop` prefers `tool_context.user_secrets["ANTHROPIC_API_KEY"]`
- `src/handlers/execution.rs`
  - `user_secrets: resolve_agent_owner_secrets(&state, &db_agent).await`
- `src/handlers/execution_stream.rs`
  - Same treatment for the SSE handler
- `src/handlers/agent_secrets.rs` — new module, three handlers
- `src/handlers/mod.rs` — module registration
- `tests/agent_secrets_shapes.rs` — new, 7 wire-format tests
- `crates/fermi-console/Cargo.toml` — version bump 0.8.13 → 0.9.0

## Validation

- `cargo check --workspace` — clean
- `cargo check --release --bin api-server` — clean (release build ~24s)
- 7 new shape tests pass
- 44 pre-existing shape tests unchanged (provenance, cascade,
  bayesops, mutex-math, timeline, posterior-fpl)
