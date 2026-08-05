# v0.11.4 — Close the manager-effect loop: harness, viz, xaman_ek digest

v0.11.3 shipped the *substrate* for the football-manager metric — a
schema column, an admin inbox, and dynamic roster injection into
Fermi's system prompt. This release closes the loop by making the
metric flow end-to-end and giving Xaman Ek the same live-context
treatment Fermi got, at a fraction of the tokens.

Three items, sequenced by dependency:

1. **Harness computes and sends the counterfactual** — without this
   the v0.11.3 column stays NULL forever.
2. **Manager-effect readout on `/agent/fermi`** — needs data from #1
   to render anything meaningful.
3. **Xaman Ek digest injection** — independent of the Fermi loop;
   biggest design piece, so it lands last.

## Item 1: Fermi console populates `counterfactual_probability`

The v0.11.3 server accepted `counterfactual_probability` on create;
nothing sent it. Fixed.

- `crates/fermi-console/src/api/client.rs::CreateForecastRequest`
  gets a new `counterfactual_probability: Option<f64>` field
  (`#[serde(skip_serializing_if = "Option::is_none")]` so pre-v0.11.4
  server builds still work in the reverse direction).
- New helper `CockpitState::naive_counterfactual_probability(&self)`
  returns `question.base_rate.historical_frequency` clamped to
  `[0, 1]` via the existing `clamp_wire_probability`. This is the
  raw reference-class anchor — the classic Tetlock/Kahneman
  outside-view baseline. It's what a naive baseline model would have
  predicted absent Fermi's decomposition and specialist aggregation.
- Wired into both `persist_backend_save` (draft POST) and
  `publish_forecast` (publish POST). **POST-only.** The
  counterfactual is defined at forecast-creation-time and is not
  updated on PUT: later "Update outside rate" mutations don't
  retroactively change the naive baseline for a forecast that
  already exists.

### Why the raw base rate and not "naive average of specialist outputs"

Specialists in Fermi's decomposition produce driver *multipliers* on
the base rate (`p5`/`p50`/`p95` around 1.0), not stand-alone
probabilities. There is no naive arithmetic mean of multipliers that
maps back to a probability without reconstructing the FPL model —
which is the very thing the counterfactual is supposed to strip out.
The base rate is the honest "no manager, no team" reference; the
manager-effect delta then measures the value-add of the whole Fermi
apparatus.

If v0.11.5 wants a "team baseline" that includes specialist
adjustments but drops Fermi's structural choices, that's a client-only
change — the schema and the split of responsibility (client owns
formula, server persists + computes delta) don't move.

### Honest gap: drafts saved before `orchestrate`

If the operator saves a draft before running orchestrate, the base
rate isn't set yet and `counterfactual_probability` goes out as
`None`. The server stores NULL and `manager_effect` stays
unavailable for that row. This is an honest gap, not a bug — we do
NOT backfill on later PUTs because whatever base rate happened to be
in RAM at PUT time isn't the counterfactual "at forecast creation
time" that the metric is defined against.

## Item 2: Manager Effect on `/agent/fermi`

New endpoint + new UI section on the agent detail page.

### Endpoint

`GET /api/orchestras/:name/manager-effect?limit=N`

- `:name` must be a strategist orchestra (fermi). 400 with a clear
  message if the orchestra has no strategist.
- Anonymous-visible (leaderboards are public).
- Aggregates over resolved forecasts where
  `agents_used @> [{"agent_name": "<strategist>"}]` — the same
  containment predicate `eval_brier` uses, backed by the GIN index
  from v0.10.23 (mig-168), so this stays O(rows-matching) not
  O(table).
- Returns:
  - `n_resolved`, `n_with_counterfactual`
  - `mean_brier`, `mean_counterfactual`, `mean_manager_effect`
    (delta averaged only over rows where both fields exist)
  - `forecasts[]` — most-recent-first, capped by `limit` (default
    50, hard max 200). Each row carries `predicted_probability`,
    `counterfactual_probability`, `brier_score`,
    `counterfactual_brier`, `manager_effect = brier − cf_brier`,
    and `resolved_at`.

### UI

New "Manager Effect" section on the Overview tab of
`templates/agent_detail.html`. Rendered only for agents in the
strategist list (currently `{fermi: "fermi"}`) — non-strategists
never see the section.

- Stats-grid summary: five cards (Resolved, With counterfactual,
  Mean Brier, Mean counterfactual, Mean manager effect). The delta
  card is coloured green when negative (strategist beat baseline),
  red when positive.
- **Delta bar chart** (inline SVG, ~640×120): one vertical bar per
  paired forecast, chronologically ordered oldest → newest. Bars
  extend *downward* from a mid-line for negative deltas (green,
  wins) and *upward* for positive (red, losses). Hover for a
  tooltip with question, dates, and numeric values.
- **Recent-resolutions table**: last 10 rows with question link,
  Brier, counterfactual Brier, and colour-coded delta.
- Empty state when no forecasts have counterfactuals yet — the
  section still renders (so operators know the metric exists), but
  chart is replaced by an explanatory box.

## Item 3: Xaman Ek digest injection

v0.11.3 injected a full roster into Fermi's system prompt but
deferred Xaman Ek because its ~100+ members × ~40 tokens each would
have inflated every invocation by 4–5k tokens. This release ships
the compact digest.

### Two strategies, one entry point

`inject_orchestra_context` now dispatches on strategy per
strategist:

```rust
enum InjectionStrategy {
    FullRoster { view: &'static str },
    TierDigest { view: &'static str, exemplars_per_tier: usize },
}
```

- **Fermi** — `FullRoster` on `orchestra_fermi_members`. Small,
  curated, structural. One line per member.
- **Xaman Ek** — `TierDigest` on `orchestra_xaman_ek_members` with
  8 exemplars per tier.

### Digest format

```
## CATALOGUE DIGEST (dynamic, v0.11.3+)
You are the platform navigator. The Bestiary currently has N
published agents across the tiers below. Per-tier counts plus a
small alphabetical sample of names are shown so you can answer
catalogue-shape questions inline. For any specific agent or
capability not visible in this digest, use your `list_agents` tool —
don't guess or invent names.

- **system** (K agents): xaman_ek, ontologist, … (+X more)
- **curated** (K agents): fermi, guidance_tracker, macro_forecaster, … (+X more)
- **community** (K agents): agent_a, agent_b, … (+X more)
```

- **One SQL round-trip.** Aggregation uses
  `array_agg(agent_name ORDER BY agent_name) FILTER (WHERE rn <= $1)`
  over a `row_number() OVER (PARTITION BY tier ORDER BY agent_name)`
  windowed subselect, so exemplars are the first N alphabetical per
  tier without any post-processing in Rust.
- **Tier order is deterministic** (`system` → `curated` →
  `community`) so the digest reads the same shape on every
  invocation — helpful for LLM caching.
- **Bounded output** regardless of catalogue size: n_tiers ×
  (exemplars_per_tier + 2 lines) ≈ 500 tokens at 500+ members.
- **Never fails.** Same guardrails as `FullRoster` — DB error logs
  and returns the card unchanged; empty roster skips injection.

### Future refinement (called out, not shipped)

Ordering exemplars by `total_executions DESC NULLS LAST` (surface
the actively-used agents first) requires exposing that column in
`orchestra_xaman_ek_members`. Deferred as a low-risk mig-173. The
current alphabetical order is deterministic and legible.

## Post-deploy verification

```bash
# Item 1: create a Fermi forecast via cockpit with a base rate set,
# then check the row.
psql "$DATABASE_URL" -c "
  SELECT id, predicted_probability, counterfactual_probability
    FROM fermi_forecasts
   WHERE agents_used @> '[{\"agent_name\":\"fermi\"}]'::jsonb
   ORDER BY created_at DESC LIMIT 5;
"
# counterfactual_probability should be non-NULL for rows created
# after this deploy where orchestrate ran before save.

# Item 2: endpoint responds, page renders.
curl -s /api/orchestras/fermi/manager-effect | jq '.n_resolved, .mean_manager_effect'
# Then visit /agent/fermi and confirm the Manager Effect section
# appears on Overview.

# Item 3: xaman_ek gets the digest.
curl -X POST /api/agents/xaman_ek/execute \
  -d '{"task":"what agents are available?"}' \
  | jq '.execution_summary' | grep -c 'CATALOGUE DIGEST'
# Should print 1. Should NOT be a 5k-token full roster.
```

## Follow-ups (deliberately NOT in this release)

- **Team-baseline counterfactual** — a richer client-side
  counterfactual that keeps specialist adjustments but drops
  Fermi's FPL-specific aggregation. Schema unchanged; just a new
  formula in `naive_counterfactual_probability`.
- **Exemplar ordering by `total_executions`** — expose the column
  on `orchestra_xaman_ek_members` (mig-173) so the digest surfaces
  actively-used agents first.
- **Per-tag digest** — group by top tags in addition to tier, once
  we have data on which tags are load-bearing.

## Migrations

None. Item 1 is client-only. Item 2 uses existing schema (columns
from v0.11.3 + mig-172). Item 3 uses the existing
`orchestra_xaman_ek_members` view.

## Files changed

- `crates/fermi-console/src/api/client.rs` — new field on
  `CreateForecastRequest`.
- `crates/fermi-console/src/cockpit.rs` — helper +
  `persist_backend_save` + `publish_forecast` wiring.
- `src/handlers/orchestras.rs` — new `orchestra_manager_effect_handler`,
  refactored `inject_orchestra_context` with `FullRoster` +
  `TierDigest` strategies.
- `src/api_server.rs` — one route.
- `templates/agent_detail.html` — Manager Effect section on the
  Overview tab, loader, delta bar chart, recent-resolutions table.
