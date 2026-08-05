# Spec — Agent Credential & System-Agent Model

Status: **accepted** (2026-08-03) · Supersedes the ad-hoc "system-tier →
`ANTHROPIC_API_KEY` env var" behaviour.

## 1. Why

Credential resolution today conflates three orthogonal concerns into the
`tier` field and splits keys across two incompatible paths:

- **system-tier** → `std::env::var("ANTHROPIC_API_KEY")` (global to the
  container, Anthropic-only, no isolation, no observability, no rotation).
- **owner-owned** → `user_secrets` but `UNIQUE(user_id, secret_name)`
  allows only one key per provider per user.

This prevents per-agent funding isolation, per-agent failure containment,
ladder-driven provider switching, and management through ABW's
observability/eval surfaces. This spec re-models it precisely.

## 2. Three orthogonal axes

| Axis | Question | Representation |
|---|---|---|
| **Ownership (principal)** | Who administers this agent? | Every agent has an owning **principal** (a `users` row). |
| **Funding (economics)** | Whose budget bears cost / collects revenue? | A **funding principal**. Invoker pays credits → platform revenue → funding principal's key bears raw LLM cost. |
| **Credential (key)** | Which key powers provider P for this agent? | `(owning_principal, provider, scope)` in an encrypted store. **Never env vars.** |

"System" is **not** a key path. It is the *class of agent whose owning
principal is `abw-system`*.

## 3. Two classes of agent (the SOP)

| Class | Owning principal | Keys | Examples |
|---|---|---|---|
| **Platform-service** | `abw-system` | system keys (stored under `abw-system`) | `ontologist`, `dream_narrator`, `cohere_and_coordinate`, `fermi`, `xaman_ek` |
| **App / personal** | the developer's admin account (e.g. `ivan@axolotl.partners`) | that account's keys | the football factor agents, product agents |

Both classes fund to the platform economically; they differ in **owning
principal** (→ which credential store holds their keys) and in
administrative SOP. `abw-system` is a real principal, not a flag, so
system keys are isolated from personal keys for rotation and blast radius.

## 4. Credential store

Generalise `user_secrets` (or a new `agent_credentials` table):

```
credentials(
  credential_id UUID PK,
  principal_id  TEXT NOT NULL,     -- owning principal (users.user_id)
  provider      TEXT NOT NULL,     -- 'openai' | 'anthropic' | 'mistral' | …
  scope         TEXT NOT NULL,     -- '*' (principal default) | '<agent_name>'
  encrypted_value BYTEA NOT NULL,
  nonce         BYTEA NOT NULL,
  label TEXT, created_at, updated_at,
  UNIQUE(principal_id, provider, scope)   -- ← the key change vs mig-039
)
```

Per (principal, provider): **one `*` default + N per-agent keys**. This is
the funding-isolation primitive: fund a specific agent on its own
key/quota; everything else rides the default.

## 5. Resolution algorithm (single path)

```
resolve_credential(agent, provider):
    p = agent.owning_principal              # abw-system for platform agents
    return  store.get(p, provider, scope=agent.name)   # agent-specific
         ?? store.get(p, provider, scope='*')          # principal default
         ?? UNFUNDED(agent, provider)                  # loud, named error
```

- **No env branch for agent keys.** Env holds only (a) the store's master
  encryption key and (b) an optional break-glass bootstrap.
- Card declares *which providers* (via `capabilities.provider` and each
  `model_ladder` rung's `provider`); the store holds *the keys*, addressed
  by provider name (mirrors the existing `KNOWN_PROVIDER_SECRETS`
  convention). Keys are card-addressable but never sit in card JSON.

## 6. Ladder-driven provider switching & graceful degradation

`model_ladder` already encodes `(tier → provider → model)` with
`min_tier` / `min_provider_class` floors. On a rung's provider failing
(`401` / quota / `5xx` / timeout):

1. resolve the failing provider's credential from the store,
2. on failure, **step down** to the next rung's provider and resolve *its*
   credential,
3. refuse below `min_tier` / `min_provider_class` rather than degrade to
   unacceptable substrate,
4. emit a `provider_degradation` telemetry event (observable/evaluable).

Because credentials are per-agent, a bad key/quota is contained to **one
agent**, not the whole container.

## 7. Observability / eval

Platform-service agents execute through the normal executor → produce
`episodes` + `eval_signals`, and appear in the Loops / observability
surfaces like any agent. Provider-selection and degradation are logged as
signals. Learning/observability is thus a managed, billable ABW service.

## 8. Phased build

- **P0 — Foundation.** `abw-system` principal; `credentials` store keyed
  `(principal, provider, scope)`; migrate the platform OpenAI key into
  `(abw-system, openai, '*')`.
- **P1 — Unified resolution.** One `resolve_credential`; delete the env
  branch for agents (env → store master key only).
- **P2 — Executor multi-provider.** Wire OpenAI (min.) into the executor,
  resolving keys from the store (the code already anticipates this:
  *"when v0.9.3+ wires other providers"*).
- **P3 — `ontologist`.** System agent card (owner `abw-system`, provider
  `openai`, ladder with fallback rungs); consolidation routes extraction
  through `execute_agent(ontologist)` → card-configured, store-funded,
  observable. Register in `xaman_ek`.
- **P4 — Graceful degradation** across the ladder (fast-follow).
- **P5 — Migrate remaining platform-service agents** (`dream_narrator`,
  `cohere_and_coordinate`, `fermi`, `xaman_ek`, …) to `abw-system` owner.

## 9. Migration notes

- `abw-system` is seeded as a `users` row (non-login principal).
- mig-039 `user_secrets` → either ALTER the unique constraint and add
  `provider`, or introduce `agent_credentials` and backfill. Keep
  `user_secrets` for non-provider secrets (Instagram, Bluesky, Stripe) if
  cleaner; provider keys move to `agent_credentials`.
- Existing `scope='*'` owner keys carry over unchanged (default rung).
