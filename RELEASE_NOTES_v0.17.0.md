# v0.17.0 — measured where it can be, labelled where it can't

Three unrelated surfaces turned out to be reporting a guess with the same
confidence as a measurement. A run's cost was derived from a rate table
that matched on the model string alone. A weather forecast's spread came
from a cited prior nobody had checked against the station it was settling
on. And the Trajectory pane, built to correlate events against rate
movements, printed each event's own name twice and no correlation at all.

None of the three was reported as broken, because none of them looked
broken. A number was present in each case. That is the theme of this
release: where the number can be measured, measure it; where it cannot,
say so in the row rather than in a comment.

## A DeepSeek run was priced at Sonnet's rate

`registry::calculate_cost` held its own rate table, matched on the model
string, and ended in `_ => 3.0`:

```rust
_ => 3.0    // $/Mtok, applied to *total* tokens
```

Two errors compounded. The fallback priced any unrecognised model at
roughly Anthropic Sonnet's rate — about **6.9× over** for DeepSeek. And it
applied a single rate to *total* tokens, when output tokens cost 3–5× input.
A run's price was wrong, and nothing recorded that it might be.

`rate_card.rs` replaces it with a `(provider, model)` table:

| | |
| --- | --- |
| separate `input_per_mtok` / `output_per_mtok` | output is not priced as input |
| longest-prefix matching | `claude-sonnet-4-20250514` resolves without a new row per date |
| `RATE_CARD_PATH` JSON override | rates change without a recompile |
| OpenRouter passthrough (`PROXY_UPLIFT = 1.055`) | upstream vendor inferred from the namespace |
| `FALLBACK_RATE` + `unknown_model` | an unknown pair is still priced, but flagged |

Twelve providers seeded — anthropic, openai, deepseek, glm/zhipu,
kimi/moonshot, qwen, mistral, gemini, ollama, openrouter. 16 unit tests.

## Every price now carries its own basis

A rate card is only as good as the token counts fed to it, so migration
194 adds four columns to `episodes`:

```
input_tokens   output_tokens   cost_basis   cost_rate_key
```

`cost_basis` is one of `measured_split`, `assumed_split`, `unknown_model`,
`no_charge`. Two rows both reading `$0.31` are no longer
indistinguishable when one measured the split and the other assumed 20%
output. `cost_rate_key` names the row that priced it.

`tool_executor` accumulates the split across loop iterations and maps
`(0, 0)` to `None`, so a provider that reports no usage is never read as a
free run. `provider_used` is now read from `AgentMetadata.provider` instead
of being guessed from the model name.

## Why the router chose an agent, not just how it did

An agent that lost as the generalist fallback is indistinguishable, in
outcome data, from a deliberately chosen domain specialist that
underperformed. A credit model trained on that data learns to distrust
whatever the router reaches for by default.

So `RouteReason` gains `slug()` and `is_deliberate()`,
`InvocationProvenance` gains `route_reason`, `route_deliberate`,
`route_overrode_suggestion` and `route_domain`, and migration 193 exposes
`route_outcomes` — provenance joined to `brier_score`, `shapley_value` and
`helped`. `route_overrode_suggestion` is retained only on disagreement, so
the field's presence *is* the "strategist was overruled" signal.

Migration 195 stamps `forecast_agent_claims` with the `episode_id` that
produced it, making that join exact rather than a ±10 minute window. The
claim hook races the episode write and usually lands first, so the column
deliberately carries no foreign key.

## A foreign analyst was inheriting World Cup drivers

`driver_prefix_for_agent` decided which parameters an agent owned by
substring:

```rust
n if n.contains("analyst") => …   // World Cup driver prefixes
```

`weather_market_analyst` contains "analyst". `resolve_driver_prefixes` now
reads `driver_refs` from the workspace's own FPL, with the legacy
substring rules scoped to the four agents that actually relied on them.

## The spread was inflated by a prior nobody had checked

The weather stack assumed SSR ≈ 0.85 — inflate the variance ~1.4×.
`weather_dispersion_fit` measures it instead, against the gauge the
contract settles on: per-lead bias, MAE and RMSE with standard errors from
up to 120 days of Open-Meteo previous-runs archive, and the residual SD
after bias correction.

Compared against today's ensemble spread — pooled across models, and for a
single reference model — those two point in **opposite directions at short
lead**. The cited prior was widening a distribution that was already too
wide.

`weather_portfolio_risk` separates two sizing problems that were being
conflated. Across stations: per-station error series, a correlation matrix
on the common-day intersection, `N_eff = N² / Σρ`, and the implied Kelly
haircut. Within a ladder: multi-outcome Kelly over mutually exclusive
buckets by projected gradient ascent, contrasted against summed per-bucket
Kelly, which overstakes because the buckets cannot all win. Three guards
reject the misuse cases rather than returning a number — bucket labels read
as one-sided thresholds, an incomplete ladder, an under-round market.

`templates/weather/bucket_ladder.fpl` is the model this implies: a bucket
indicator over a composed predictive temperature, not a multiplicative
`base_rate × d1^a × …` chain. The regression test reproduces the live EGLC
ladder within 8pp — the multiplicative form had produced 0.3%.

## `market_observation · market_observation`, seventeen times

The Trajectory pane exists to correlate system events against rate
movements. It rendered rows like this:

```
· market_observation · market_observation   2026-08-14T04:09:26.138076+00:00
```

`render_trajectory_event` had arms for `rate_revision`, `bayesops_fit`,
`agent_run` and `upstream_resolved` — but none for market ticks, the most
frequent event on the timeline. They fell to the `_` fallback, which
formatted `kind · content` with `content` defaulting to `kind`: the name
twice, then 26 characters of microsecond noise.

The correlation was absent from the data as well. `build_phase_summary`
reported crowd drift by reading `market_price`, which the timeline endpoint
never emitted on market events — the clause was dead code, and every phase
reported a bare count.

The endpoint now sends what a tick is worth (price, tick-to-tick delta,
bid/ask, spread, 24h volume, liquidity, sampling method, confidence
signal) and annotates *every* event with the state of both worms at that
instant and the revision that followed. A row now reads:

```
▼ Crowd 5.0% (-0.3pp)                                   5h ago · 04:09
  scheduled poll · low confidence · bid 4.0% / ask 6.0% · $642 liquidity
  ↳ 12m before the +4.6pp manual revision · model 3.4% vs crowd 5.3% (model -1.9pp)
```

Adjacency is reported as adjacency — "before" is a statement about clocks,
not mechanism. When no revision followed, that absence is stated, because
an event that did *not* move the rate is as informative as one that did.
The series lookup is a step function, and returns nothing before a series
begins rather than back-filling: dating a crowd quote to before it existed
is worse than a missing clause.

## Known gaps

Stated here rather than discovered later.

- **The write path shipped; the read path did not.** No handler or console
  surface selects `cost_basis`, `cost_rate_key`, or any of the six new
  views yet. `economics.rs` still serves the `RATE_CARD_WIRED_ON` date
  heuristic that `cost_basis` is meant to replace.
- **`forecast_cost_attribution` is workspace-grain, not forecast-grain.**
  It joins claims to forecasts on `workspace_id`, which is not unique, so a
  workspace backing two forecasts attributes every episode's cost to both.
  Per-execution de-duplication is correct; forecast attribution is not yet.
- **Historical rows stay wrong**, deliberately. There is no backfill —
  `cost_basis IS NULL` marks every row priced before this release, and the
  token split they would need was never recorded.
- **`weather_portfolio_risk` has no `BuiltinToolDef`.** It is reachable
  from the examples and unit tests, but no agent card can declare it.
- **The bucket-ladder template's `base_rate` is a placeholder** that the
  spawn path does not fill, so Brier skill denominators on spawned
  workspaces are only right for the station the literal was written for.
- **The six new views are not in `schema_trust`.** A silent failure to
  apply migration 193 or 195 would not trip the ratchet.
- **BYOK is not in this release.** `SPEC_34` is filed as investigation, not
  a proposal, and no BYOK or gateway credential exists in code. What ships
  is that spec's C1 row — make the substrate measurable — and nothing past
  it.
