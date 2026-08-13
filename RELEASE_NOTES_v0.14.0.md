# v0.14.0 — Forecasts that are actually probabilities

Two forecasts went wrong in ways that looked like confidence. A Premier
League question handed all five drivers to `macro_forecaster` despite
`football_analyst` being hired into the workspace. A London temperature
question returned **99%** against a market price of 14.5%.

Neither was a modelling disagreement. Both were defects.

## The 99% was `min(6.4, 0.99)`

The base rate never entered the model.

`process_macro_forecaster_result` printed Fermi's suggested
`base_rate * driver_a * driver_b …` as a chat message and then **deleted
the model statement**. `generate_fpl_text` regenerated a bare multiplier
chain from the driver names alone. Drivers are probability multipliers
centred on 1.0, so their product was 6.4 — not a probability — and
`run_simulation` clamped it into `[0.01, 0.99]` and displayed it.

Both log lines were telling the truth about different things:

```
11:58:49  Model: base_rate * climate_trend_adjustment * ...   ← a printed string
11:58:49  No model expression defined.                        ← the actual AST
```

Verified against the executor with the same drivers:

| model | mean | p5 | p95 | displayed |
|---|---|---|---|---|
| `climate * seasonal * synoptic * uhi` | 6.66 | 2.97 | 11.28 | 99.00% |
| `0.003 * climate * seasonal * synoptic * uhi` | 0.0200 | 0.0089 | 0.0338 | 2.0% |

Auto-generated models now carry the base-rate anchor. This satisfies the
contract `run_simulation` already documented — that the model expression
*is* the forecast quantity and the cockpit never re-multiplies the anchor
in afterwards. Hand-authored FPL always did this; auto-decomposed
forecasts now do too.

**A probability forecast evaluating above 1.0 is no longer clamped.** It
raises an error naming the likely fix and refuses to persist, rather than
saturating to 0.99 and writing a fake forecast to your trajectory.

## Every driver went to the generalist

Three independent failures, all firing at once:

1. **Availability was checked against the wrong registry.** Routing
   probed `registry.get()` — the local `agents/curated/` directory,
   resolved relative to CWD at startup. Agents execute *server-side*, so
   a missing directory says nothing about whether an agent can run. It
   demoted every specialist anyway.
2. **The fallback chain was dead code.** An unavailable suggestion fell
   back to the domain agent — which, for a football question, is the
   `football_analyst` that just failed the check. It could not succeed,
   so it always landed on a hardcoded `macro_forecaster`.
3. **Fermi's suggestion was honoured unconditionally**, letting a
   generalist displace the resident expert.

Routing now lives in `fermi_console::routing`, is checked against
orchestra ∪ local cards ∪ server roster, and degrades to the next-best
expert rather than to the generalist. The same EPL decomposition now
routes four drivers to `football_analyst` and the FFP driver to
`entity_investigator`.

Three copies of the ladder existed and disagreed — one mapped
`sports_football` to `market_research`. There is now one.

## Substring matching was routing on accidents

`"pre-industrial"` contains `"trial"`, so a climate driver was
recommended `biotech_analyst`. `"development"` contains `"elo"`.
`"warming"` contains `"war"`, so climate questions classified as
politics. Keyword matching is now whole-word with plural absorption.

The new tests caught `"home_court_advantage"` matching `"court"` — a
collision this release introduced — before it shipped.

`detect_domain` also gains weather vocabulary; a temperature question
previously classified as `general`.

## Base rates are now checked

The base rate is the number every driver multiplies, and nothing
inspected it. New `fermi_console::calibration` reports:

```
⚠ Base rate is 60% from only n=10. The 95% interval on that frequency
  alone is 31%–83% (52pp wide) — before any driver is applied.

⚠ Reference class "Manchester City EPL title wins in Guardiola era" is
  about Manchester City — the subject of the question — and has only
  n=10. That is the inside view, not a base rate.
```

Wilson score intervals, not the normal approximation, which collapses to
a single point at p=0 and would let a degenerate anchor pass as
certainty.

Circularity detection requires the class to name the subject **and** be
small. "Days in London reaching 32°C during August" (n=744) names London
and is a perfectly good class — the subject is one draw from a large
unselected population. Both cases are pinned as tests.

Companion change in `agents/curated/fermi/agent_card.json` (v1.2.0): the
card told Fermi to anchor on "the most specific applicable reference
class", which has no floor and converges on a class of one.

## Smaller things

- **Sobol influence percentages summed past 100%** (a real run printed
  110%). Those were total-order indices, which count each interaction
  once per participating driver and are not shares. The narrative now
  reports first-order variance shares with the interaction remainder
  named; the tornado bars keep total-order and say so in the header.
- **Scheduled-research descriptions wrapped one letter per line.** The
  description was a shrinkable flex item with `min_w(0)` beside the agent
  name; the solver squeezed it to a one-character column.
- Agent descriptions fall back to the server roster, so they no longer
  read "Research agent" on installs without a local card directory.
- `AgentRegistry::load_from_directory` no longer aborts the whole walk on
  one duplicate id, which left an arbitrary (unstable) prefix loaded.

## Testing

`cockpit.rs` is a module of the binary target, where rustc segfaults
expanding the GPUI element tree under `--test`. Logic worth testing now
lives in the lib target: `routing` and `calibration` add 33 tests that
run in about two seconds.

Console lib: 196 passing.
