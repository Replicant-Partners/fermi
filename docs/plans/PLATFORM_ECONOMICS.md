# Platform Economics — cost vs. revenue

Status: **shipped (v1)** — 2026-08-12 · Surface:
`GET /api/admin/economics/platform`, admin console → **Economics** tab

## 1. The question

Platform-service agents run on `abw-system`'s provider keys. Somebody
pays real dollars for every `xaman_ek` answer and every consolidation
cycle. Until now that number was unavailable outside a psql session, so
the platform's own margin was invisible — and about to get more so,
since the P5 ownership migration redirects 13 agents' royalties into a
wallet nobody can see.

## 2. What is measured vs. modelled

The distinction the whole design turns on:

| Figure | Source | Nature |
|---|---|---|
| funding principal | `episodes.context->>'funding_principal'` | **measured** — stamped at execution (SPEC_28) |
| tokens | `episodes.tokens_used` | measured |
| credit revenue | `credit_ledger` `execution_fee`, joined on `related_id = episode_id` | measured |
| royalties out | `credit_ledger` `agent_royalty_in` | measured |
| USD cost | `tokens × per-model rate` | **modelled** |
| USD revenue / margin | `credits × assumed rate` | **modelled** |

Credits and dollars are *different currencies*. Collapsing them into one
"margin" number would read as authoritative and be false, so the
endpoint returns measured and modelled quantities separately, and every
response carries a `cost_basis` block stating its assumptions. The UI
renders that as a banner that cannot be collapsed.

### 2.1 Attribution is historical, not current

The view groups by the funding principal **recorded on the episode**,
not one re-derived from the agent's present tier. When the ownership
migration runs, history stays attributed to whoever actually paid rather
than silently retconning itself.

## 3. Fix shipped alongside: cost was flat-rated

`src/api_server.rs` priced every episode at a hardcoded **$3 per million
tokens**, regardless of provider or model:

```rust
cost_usd: output.tokens_used.map(|t| (t as f64 / 1_000_000.0) * 3.0)
```

Meanwhile `agent_backend::registry::calculate_cost(provider, model,
tokens)` already held a per-model rate card — Opus $15, Sonnet $3, Haiku
$0.25, Ollama $0 — and was simply never wired to the persistence path.

Error introduced by the flat rate:

| Model | Flat rate said | Actual | Error |
|---|---|---|---|
| `claude-opus-4-6` | $3/Mtok | $15/Mtok | **5× understated** |
| `claude-haiku-4-5` | $3/Mtok | $0.25/Mtok | **12× overstated** |
| Ollama (local) | $3/Mtok | $0 | charged for free compute |

Now wired. **Episodes written before 2026-08-12 retain the flat-rate
estimate**, so any window spanning that date mixes two cost bases —
flagged as `cost_basis.mixed_history_before` in the response and called
out in the UI banner.

## 4. Remaining error bars

1. **No input/output split.** `tokens_used` is one number; real pricing
   differs 3–5× between input and output tokens. This is now the largest
   source of error and the highest-leverage next fix.
2. **Rate card drift.** Hardcoded in `registry.rs:364`; provider price
   changes require a code edit. Unknown models fall back to $3/Mtok.
3. **Credit → USD is a scenario.** Credits sell at 2.0¢ (250-credit
   tier) down to 1.0¢ (5000-credit tier). The view defaults to a blended
   **1.5¢**, deliberately round because it is an assumption. Override
   with `?credit_usd=` or the input in the UI.
4. **Unattributed episodes.** Runs predating SPEC_28 have no funding
   principal; they are bucketed as `unattributed` rather than dropped —
   they are real spend, and hiding them would understate cost.

## 5. Reading it

- **Cost (real USD)** — hard money out. The only figure that is
  approximately real today.
- **Revenue (credits)** — measured exactly, but soft currency.
- **Margin (modelled)** — a scenario at the stated credit rate. Move the
  rate input to see how sensitive the conclusion is; if the sign flips
  between 1.0¢ and 2.0¢, the answer is "we don't know yet".
- **`by_funding_principal`** — the top-line "what does the platform cost
  me". Filter with `?principal=abw-system`.
- **`episodes_missing_cost`** — a non-zero count means executions landed
  without token accounting; investigate before trusting the totals.

## 6. Verification

```bash
./scripts/smoke_economics.sh
```

Spins up a throwaway Postgres in Docker, applies a fixture schema plus
migration 189, loads known data and asserts exact results. It never
reads `DATABASE_URL` and has no flag to point it at another database,
so it cannot touch production.

The three queries live in `src/handlers/sql/*.sql` and are
`include_str!`d by the handler *and* executed verbatim by the script —
so the smoke test cannot drift from the code it is testing. Loading them
via `PREPARE` also type-checks the `$n` placeholders against real column
types before anything runs.

Covered: window filtering, the negative-`execution_fee` sign convention,
`LEFT JOIN` retention of zero-revenue agents, `unattributed` bucketing
of pre-SPEC_28 episodes, and for migration 189 the
mint → live → log → end → revoked lifecycle, both CHECK constraints,
cascade delete, and re-application idempotency.

Mutation-tested — each of these was introduced deliberately and the
suite caught all three:

| Mutation | Caught by |
|---|---|
| `SUM(-l.amount)` → `SUM(l.amount)` | `xaman_ek revenue: expected 50, got -50` |
| `LEFT JOIN fees` → `JOIN fees` | `expected 4 (agent, principal) groups, got 2` |
| fee CTE window filter deleted | `xaman_ek revenue: expected 50, got 827` |

The third initially *survived*: the episode window already constrains
the join, so the fee filter looked redundant. A backdated ledger row
pointing at an in-window episode was added to the fixtures, which is the
one case where the filter actually bites. Worth knowing that the filter
earns its place rather than assuming it.

Not covered: schema parity with production. That is the boot-time trust
contract's job (`/api/admin/schema-health`).

## 7. Not answered here

Cash-out. There is no credits→fiat path for any principal, so
`abw-system`'s wallet accumulates value that cannot currently be
realised. Tracked in `ROADMAP.md` §6b.
