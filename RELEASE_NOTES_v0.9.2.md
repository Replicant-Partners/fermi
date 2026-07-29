# Fermi Console v0.9.2 — Marketplace substrate cleanup

Unifies the platform's own architecture. Three changes, all in service
of one clean substrate story before Fermi Chat (v0.10.x) lands:

1. **Removed** v0.9.0's per-agent secret-management endpoints — they
   overlapped confusingly with ABW's profile page.
2. **Added** a single read-only funding-status endpoint that agrees by
   construction with the executor's key-resolution.
3. **Tightened** the executor: owner-owned agents no longer silently
   fall back to the platform env var. Hard-fail with the ABW profile URL
   in the error message.

## Architectural context

Per the review captured in
`docs/fermi/FERMI_CHAT_AND_AGENT_CREATION_DESIGN.md`, the platform's
model is:

- **ABW** (`agent-bestiary.world`) — where agents are created + funded.
  Profile page owns key upload (writes to `user_secrets` with
  `scope='*'`, i.e. owner-global).
- **Fermi Console** — where agents are hired + composed into forecasts.
  Will grow agent creation via Fermi Chat's design mode (v0.11.0),
  but that too will write to ABW's `agents` table.
- **One executor** — routes owner keys automatically via
  `resolve_agent_owner_secrets` (v0.9.0).

v0.9.0 shipped three Fermi-side endpoints for per-agent secret
management that:
- Duplicated ABW's profile page for the primary use case
- Scoped keys per-agent instead of globally (not the ABW model)
- Surfaced a management flow in a UI that can't yet create agents

**Verdict**: dead surface area. Remove and consolidate.

## What changed

### Removed

Three routes and their handler module:

```
PUT    /api/agents/:agent_id/secrets/:secret_name    ← gone
GET    /api/agents/:agent_id/secrets                 ← gone
DELETE /api/agents/:agent_id/secrets/:secret_name    ← gone
```

Files deleted:
- `src/handlers/agent_secrets.rs`
- `tests/agent_secrets_shapes.rs`

### Added

Single read-only endpoint that answers "is this agent runnable?":

```
GET /api/agents/:agent_id/funding
```

Two response shapes based on caller identity:

**Public / non-owner view** — just the boolean:
```json
{ "agent_id": "macro_forecaster", "funded": true }
```

**Owner or admin view** — adds providers + ABW profile URL:
```json
{
  "agent_id": "macro_forecaster",
  "tier": "community",
  "owner_id": "user-mario",
  "funded": true,
  "providers": ["anthropic", "openai"],
  "abw_profile_url": "https://agent-bestiary.world/profile"
}
```

**System-tier view** (Fermi, xaman_ek):
```json
{
  "agent_id": "fermi",
  "tier": "system",
  "funded": true,
  "funding_source": "platform",
  "providers": ["platform"],
  "abw_profile_url": null
}
```

Behavioural invariant: `funded == true` **iff** the executor could
actually run this agent. The endpoint calls the same
`get_secrets_for_agent` primitive the executor uses at hire time, so
"marketplace says funded" and "hire succeeds" cannot drift.

Files added:
- `src/handlers/agent_funding.rs`
- `tests/agent_funding_shapes.rs` — 8 wire-format tests

### Tightened

`src/agent_backend/tool_executor.rs::execute_anthropic_loop` used to
fall back to `ANTHROPIC_API_KEY` env var for owner-owned agents whose
owners hadn't uploaded a key. That soft-fallback recreated the
shared-pool bottleneck the marketplace architecture is designed to
avoid — it's how Mario's forecast composer kept working (badly) even
though his agents weren't funded.

Now the executor distinguishes based on `tool_context.user_secrets`:

- **`None`** → system-tier agent or unconfigured secrets subsystem.
  Use platform env var. Fermi keeps working on platform funding.
- **`Some(map)`** → owner-owned agent. Key comes from `map` only. If
  `map` doesn't contain `ANTHROPIC_API_KEY`, hard-fail with:

  ```
  Agent 'macro_forecaster' is not funded. Its owner has not set an
  ANTHROPIC_API_KEY on their ABW profile. Ask them to configure it at
  https://agent-bestiary.world/profile.
  ```

`resolve_agent_owner_secrets` (in `src/api_server.rs`) updated its
return semantics accordingly: it now returns `Some(empty_map)` for
owner-owned agents whose owners haven't funded them, rather than
collapsing that case into `None`. That's what unlocks the executor's
tightening.

### Configurable URL

New `ABW_PROFILE_URL` env var, defaults to
`https://agent-bestiary.world/profile`. Lets deploys point at local /
staging ABW instances without a code change. Used in the executor
error message and in the funding endpoint's owner-view response.

## Migration

- **No schema change**. All primitives (users, user_secrets, agents,
  encryptor) unchanged.
- **Client compatibility**. No console code called the removed
  routes; v0.9.0 shipped them backend-only for future use. Nothing
  in the shipped console binaries hits the deleted paths.
- **Behavioural change worth calling out**: any owner-owned agent
  whose owner hadn't uploaded an `ANTHROPIC_API_KEY` on ABW's profile
  page used to run silently on the platform's shared key. Now it
  refuses to run and directs the operator at the ABW profile page.
  This is the intended marketplace behaviour but it is a change from
  v0.9.0/v0.9.1 — plan the deploy with owners informed.

## What this unlocks

The substrate is now coherent:

- **ABW profile page** = owns key upload (single source of truth).
- **`user_secrets` table** = one shared store, one encryption path.
- **Executor** = routes owner keys automatically at hire time.
- **`GET /api/agents/:id/funding`** = the read side, agrees with the
  executor by construction.
- **Fermi Console** = renders `funded` badges (v0.9.2+); Fermi Chat
  (v0.10.x) will use the same signal; design-mode agent creation
  (v0.11.0) writes to the `agents` table and immediately shows
  unfunded until the owner sets a key on ABW.

No parallel data paths. No divergent schemas. No competing UIs.

## What's next

Per the roadmap laid out alongside this release:

- **v0.9.3** — credit flow (caller wallet → owner wallet at hire time).
- **v0.10.0** — Fermi Chat Slice 1 (drawer, RAM only).
- **v0.10.x** — scenario tree slices (independent track).
- **v0.11.0** — design mode (create-agent through conversation).

The chat + design-mode tracks can now proceed on top of a substrate
that has clear ownership boundaries — ABW owns creation + funding,
Fermi owns composition + hiring, both share one database.

## Files touched

- `src/api_server.rs` — `resolve_agent_owner_secrets` return semantics
  updated; new `abw_profile_url()` helper; route registration swapped
  from three secret routes to one funding route.
- `src/agent_backend/tool_executor.rs` — `execute_anthropic_loop`
  distinguishes system vs owner-owned; hard-fails with ABW URL on
  missing owner key.
- `src/handlers/agent_secrets.rs` — **deleted**.
- `src/handlers/agent_funding.rs` — **new** (single read-only handler).
- `src/handlers/mod.rs` — module registration swap.
- `tests/agent_secrets_shapes.rs` — **deleted**.
- `tests/agent_funding_shapes.rs` — **new** (8 wire-format tests).
- `crates/fermi-console/Cargo.toml` — version bump.

## Validation

- `cargo check --workspace` — clean.
- `cargo check --release --bin api-server` — clean (release build).
- `cargo check --release -p fermi-console` — clean.
- 52 shape tests pass (8 new funding + 44 pre-existing: 8 provenance +
  6 propagate + 6 mutex-math + 10 timeline + 9 bayesops + 5 posterior).
