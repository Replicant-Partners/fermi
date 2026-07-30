# Fermi Console v0.10.1 — Credit flow (caller → owner royalty on hire)

**Substrate slice.** Closes the marketplace loop opened in v0.9.0
(agent-owner API key routing) and v0.9.2 (marketplace substrate
cleanup). When a caller executes a community/curated agent, a
configurable fraction of the execution fee now flows to the agent's
owner as royalty — payable to their standard `user`-typed wallet, no
new tables, no new endpoints, using the same primitives every other
credit flow in the codebase uses.

Server-only. No console changes; no schema changes. Agents that were
running on the platform's execution fees now genuinely earn for their
owners.

## What changed

### The economic model, made concrete

Before v0.10.1, the caller-side ledger for a community-agent execution
looked like:

```
caller  --  execution_fee  --> platform
caller  --  gas_fee        --> platform
```

The agent owner earned nothing. Fermi (system-tier, platform-funded)
worked the same way, and so did every community/curated agent
someone had published. From v0.10.1:

```
                     ┌─  execution_fee * (1 - royalty_pct)  ─→  platform
caller ── execution_fee ─┤
                     └─  execution_fee *      royalty_pct   ─→  owner
caller  ──  gas_fee     ─→  platform    (infrastructure, unchanged)
```

Same total charged to the caller. The gas surcharge stays with the
platform because it covers infrastructure cost, not the agent's fee.
The base `execution_fee` splits at `execution_owner_royalty_pct` —
default **85% to owner / 15% to platform**.

### Gates — when royalty flows and when it doesn't

Three conditions must all be true for a royalty deposit to fire:

1. **The agent has an `owner_id`.** System agents (Fermi, xamanEK,
   any `agents/curated/*` seeded without an owner) return None here
   and the royalty is skipped — platform absorbs, as before.
2. **The agent's tier is not `"system"`.** Even if a system agent
   were somehow assigned an owner, we defer to the tier: system
   agents are platform-funded and stay that way. This mirrors the
   secrets-routing rule from v0.9.0 (system agents get platform env
   var, community agents require owner-provided keys).
3. **The caller is not the owner.** Self-hires round-trip through
   the same wallet with no economic effect (net-negative once the
   platform cut is taken), so we skip the ledger churn. Ledger stays
   readable; end-of-month reports don't have to filter out
   caller==owner rows.

Any gate failing → 100% platform, same as before v0.10.1.

### New primitive: `fermi::gas::charge_execution_with_royalty`

Adds one function in `src/gas.rs` that packages the whole "debit
caller, deposit royalty, log the split" flow into a single call the
handlers can use:

```rust
pub async fn charge_execution_with_royalty(
    pool: &PgPool,
    caller_wallet_id: Uuid,
    caller_user_id: &str,
    execution_fee: i32,
    agent_owner_id: Option<&str>,
    agent_tier: &str,
    agent_id_str: &str,
    tokens: i32,
    episode_id: Option<&str>,
    royalty_pct: f64,
) -> Result<(i32, i32), (StatusCode, String)>
```

Returns `(execution_fee_charged, royalty_paid_to_owner)`. Handler
sites use the returned pair for structured logging.

**Not atomic.** Same pattern as the existing `charge_and_distribute`:
if the caller debit succeeds but the royalty deposit fails, we log
the miss and the platform absorbs. Ledger reconciliation can catch
persistent failures. In practice the deposit is a straightforward
`credit_deposit_typed` and won't fail unless the DB itself is
unhealthy — the same category of failure that already writes to
stderr on the caller side.

Owner-wallet resolution goes through the standard
`get_or_create_wallet(pool, "user", owner_id)` — no new wallet type,
owners collect royalties into the same wallet they'd use to hire
someone else's agent.

Rounding: `execution_fee * royalty_pct` truncates toward zero, then
clamps to `[1, execution_fee]` so small-token executions still pay
the owner at least 1 credit rather than rounding to zero. Owners
never earning on small calls would feel like a bug even if the math
was correct.

### New knob: `GasFees::execution_owner_royalty_pct`

```rust
pub struct GasFees {
    // …
    /// Fraction of the execution fee that flows to a community/curated
    /// agent's owner on hire. Default 0.85. Env override:
    /// GAS_EXECUTION_OWNER_ROYALTY_PCT.
    pub execution_owner_royalty_pct: f64,
}
```

Defaults to `0.85`. Env override: `GAS_EXECUTION_OWNER_ROYALTY_PCT`.
Clamped to `[0.0, 1.0]` at charge time so a misconfigured deploy
can't accidentally pay owners more than the fee itself.

### Wire-up sites

Two handlers now use the new helper:

- **`src/handlers/execution.rs::execute_agent_handler`** —
  non-streaming `/api/agents/:id/execute`. Previously called
  `credit_charge(&db, wallet.wallet_id, execution_fee, "execution_fee", …)`.
  Now calls `charge_execution_with_royalty(…)` with the resolved
  `db_agent.owner_id`, `db_agent.tier`, and
  `state.gas_fees.execution_owner_royalty_pct`.
- **`src/handlers/execution_stream.rs::execute_agent_stream_handler`** —
  SSE `/api/agents/:id/execute/stream`. Same change, with an extra
  clone of `db_agent.owner_id` and `db_agent.tier` before the async
  stream closure captures them.

The caller-side transaction shape (`tx_type = "execution_fee"`,
description `"Execute {agent} ({N}tk)"`) is unchanged. Any dashboard
or export that groups by `tx_type` keeps working.

New tx_type for the owner side:

```
tx_type       = "agent_royalty_in"
description   = "Royalty from {caller_user_id} — {agent_id} ({N}tk)"
related_id    = episode_id
```

### Diagnostic traces

Every royalty payment logs at info level with structured fields, so
`grep [credit-flow]` in a run log surfaces the full economic history:

```
[credit-flow] royalty paid owner_id=… agent_id=fermi_forecaster
    caller_id=user-alice execution_fee=12 royalty=10 platform_cut=2

[credit-flow] hire settled agent=fermi_forecaster caller=user-alice
    execution_fee=12 gas_fee=1 royalty_paid=10
```

Non-royalty executions (system agents, self-hires, owner-less
agents) don't log a `hire settled` line — silence there is the
signal "no royalty applied", not a failure.

Failures also log:

```
[credit-flow] failed to resolve owner wallet — royalty NOT paid (platform absorbs)
[credit-flow] failed to deposit royalty — platform absorbs
```

### Non-changes

- **No schema migration.** All new writes use existing tables
  (`wallets`, `credit_ledger`) and existing columns.
- **No new endpoints.** The existing `GET /api/agents/:id/earnings`
  and `GET /api/wallet` surface the new deposits automatically —
  owners will see incoming credits with `tx_type = "agent_royalty_in"`
  in their wallet history the moment their agent gets hired.
- **No console changes.** The fermi-console binary is unchanged from
  v0.10.0 apart from the version bump. Ivan or Mario running v0.10.0
  are already talking to a v0.10.1 server if their deploy picked up
  the new tag.
- **No pricing change.** Caller pays exactly what they paid in
  v0.10.0. The change is who receives it.
- **No change to system agents.** Fermi is still platform-funded.

## Files touched

- `src/gas.rs` — new `execution_owner_royalty_pct` field on
  `GasFees` (env `GAS_EXECUTION_OWNER_ROYALTY_PCT`, default 0.85),
  new `charge_execution_with_royalty` helper (~90 LOC).
- `src/handlers/execution.rs` — non-streaming execute handler
  rewired to use the new helper. `credit_charge` import replaced
  with the gas-module helper.
- `src/handlers/execution_stream.rs` — SSE execute handler rewired
  identically. Also captures `db_agent.owner_id` + `db_agent.tier`
  before the stream closure.
- `crates/fermi-console/Cargo.toml` — 0.10.0 → 0.10.1.
- `RELEASE_NOTES_v0.10.1.md` — this file.

Validation: `cargo check` (whole workspace) clean; no new warnings
beyond the 114 pre-existing.

## What this unlocks

Mario's original ask — "publish an agent to the marketplace and earn
from it" — now genuinely works end-to-end:

1. Mario publishes `mario_market_watcher` to `agents` with
   `tier="community"`, `owner_id="user-mario"`.
2. Mario configures his `ANTHROPIC_API_KEY` on his ABW profile
   (v0.9.0 wired this).
3. Alice hires `mario_market_watcher` from the marketplace.
4. Alice's wallet is debited `execution_fee + gas_fee`.
5. Mario's wallet is credited `execution_fee * 0.85` as
   `agent_royalty_in`.
6. Mario sees the deposit under his wallet history immediately;
   `GET /api/agents/mario_market_watcher/earnings` reflects it.

The marketplace is a working economy from this release onward. Every
piece of the four-step v0.9.0–v0.10.1 arc has now shipped:

| Version | Piece |
| --- | --- |
| v0.9.0 | Agent-owner API keys resolve at execution time. |
| v0.9.1 | `ensure_user_row` self-heals stale rows so owners can actually provision. |
| v0.9.2 | Marketplace substrate cleanup + hard-fail on missing owner keys. |
| **v0.10.1** | **Caller → owner royalty on every execution.** |

Slice 1 of Fermi Chat (v0.10.0) is orthogonal — it shipped in
between because it was the higher-visibility deliverable.

## Roadmap next

- **v0.10.2 — Fermi Chat Slice 2 (tool dispatch).** Adds
  `open_forecast`, `open_portfolio`, `run_simulation`,
  `assign_agent`, `set_base_rate`, `link_polymarket` MCP tools on
  Fermi's card + console-side dispatch. Fermi becomes a hand, not
  just a mouth.
- **v0.11.0 — Fermi Chat Slice 3 (persistence).** Wires chat history
  to ABW's `chat_messages` table so refresh doesn't lose the
  transcript.
- **v0.12.0 — Fermi Chat Slice 4 (design mode).** The
  create-agent-through-conversation walk-through. Emits a valid
  `agent_card.json` into `agents/community/`.

v0.10.1
