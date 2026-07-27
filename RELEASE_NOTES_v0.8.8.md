# Fermi Console v0.8.8 — Cascade provenance (Phase 2.5)

Ships the first UI surface for the generalized **cascade** primitive:
the redistribution waterfall. Explains, per forecast, exactly where its
current probability came from in terms of upstream resolutions that
cascaded mass onto (or off) it.

Background: during the WC 2026 sim the cascade engine was the only
loop that actually moved forecasts — Spain's 55.9% was the raw model's
11.9% plus the mass of 40+ eliminated teams redistributed through the
mutex group. The engine worked, but the movement was invisible in the
UI: the probability drifted with no explanation. This release makes it
legible.

See `docs/fermi/WORLD_CUP_ROADMAP.md` for the roadmap positioning
(Phase 2.5, sits between the DAG in Phase 2 and batch spawn in Phase
3).

## What's new

### Provenance right-tab in the composer

New `Provenance` tab in the composer's right panel, next to Trajectory.
Reads `GET /api/forecasts/:id/cascade-provenance` and renders a
top-down waterfall:

```
Current 55.9%  =  baseline 11.9%  +  +44.0 pp cascades
42 cascade events — sorted by |Δpp| descending

▲ Curaçao      +2.0   50.0% → 52.0%   2026-06-30 20:00
▲ Panama       +1.5   52.0% → 53.5%   2026-07-04 22:00
▲ Jordan       +1.3   53.5% → 54.8%   2026-07-05 22:00
▲ Türkiye      +1.2   54.8% → 56.0%   2026-07-08 22:00
↺ Cascade undo −0.1   56.0% → 55.9%   2026-07-09 22:00
…
```

- Rows are sorted by `|delta_pp|` descending — biggest movers first
  regardless of sign, so a large `cascade_undo` sorts above a small
  positive cascade.
- Trigger labels come from the trigger forecast's `question_text`,
  best-effort shortened (strip `"Will "` prefix, `"?"` suffix, cut at
  ` win ` / ` happen ` / ` occur `). Falls back to the trigger's short
  uuid, then to `"Upstream update"` for pathologically-formed reasons.
- `cascade_undo` rows render with `↺` in gold; positive cascades ▲ in
  green; negative cascades ▼ in red.
- Tab is lazy-loaded on click. Ctrl+Shift+FPL cycle now includes it in
  the tab rotation (Trajectory → Provenance → Wiki → …).

### `GET /api/forecasts/:id/cascade-provenance` endpoint

New backend handler in `src/handlers/forecast_benchmark.rs`. Read-only
projection over `fermi_forecast_updates` rows tagged
`revision_trigger IN ('cascade','cascade_undo')`. Response shape:

```json
{
  "forecast_id": "…",
  "question": "Will Spain win the 2026 FIFA World Cup?",
  "current_probability": 0.559,
  "baseline_probability": 0.119,
  "cumulative_cascade_pp": 44.0,
  "cascade_count": 42,
  "contributions": [
    { "ts": "…", "trigger_forecast_id": "…",
      "trigger_question": "Will Curaçao win…?",
      "prev_p": 0.500, "new_p": 0.520, "delta_pp": 2.0,
      "revision_trigger": "cascade", "is_undo": false,
      "reason": "cascade from … (resolved)" },
    …
  ]
}
```

Invariants enforced by the shape tests:

- `baseline + Σ delta_pp / 100 == current` — the waterfall's core
  arithmetic. If it drifts, either a cascade row was written wrong or
  a probability was updated outside `fermi_forecast_updates`.
- `cascade_count == contributions.len()`.
- `contributions` sorted by `|delta_pp|` descending.
- Per-row `(new_p - prev_p) * 100 == delta_pp`.
- `is_undo == (revision_trigger == "cascade_undo")`.

`trigger_forecast_id` is parsed out of the `reason` string, stable
across all four callers that write cascade rows (mutex,
`at_most_n`, `implies`, `apply_wc_cascades`). If we later promote it
to a real column, the endpoint's shape doesn't change — only the
extraction is deleted.

Auth follows the same ownership/team-membership gate as
`forecast_timeline_handler`; if you can read the forecast, you can
read its provenance.

### Wire-format tests

New `tests/forecast_cascade_provenance_shapes.rs` — 8 shape tests in
the style of `forecast_timeline_shapes.rs`. Assert the invariants
above hold on a hand-constructed canonical response, so a handler
change that breaks the contract breaks the test loudly.

## Migration

None. Read-only endpoint, no schema changes. New tab is purely
additive.

## Design context

This release is the first item in the "generalize cascades and
surface them in the UI" thread from the WC post-mortem
(`docs/fermi/reports/WORLD_CUP_SIMULATION_POST_MORTEM.md`). The
design proposal, rule-registry sketch, and the four other planned
surfaces (Group panel, Ledger, What-If preview, Invariant Health
widget) live in the same doc thread.

The immediate follow-up is the `CascadeRule` trait refactor —
repackages the three existing propagation kinds (`mutex`,
`at_most_n`, `implies`) behind a pluggable trait so `bracket`,
`k_of_n`, `budget`, `correlation`, `rollup`, and `bayesian` kinds can
slot in without touching a per-kind dispatcher.

## Known follow-ups

- **Trigger label heuristics.** The `shorten_question_for_provenance`
  helper handles the common `"Will X win Y?"` / `"Will X happen by
  Z?"` shapes. Domains that phrase questions differently will fall
  back to the full question. Not critical — the numbers carry the
  story.
- **Group-level view.** Provenance is per-forecast. The complementary
  view is per-group: "here's the mass flow through the WC 2026
  winner group over time." That's surface C (Cascade Ledger) in the
  roadmap doc; not in this release.
- **What-if preview.** The engine already supports `dry_run=true`;
  wiring a hover panel that shows "if you resolve Brazil NO now,
  here's how the top-10 shift" is a natural next patch on top of
  this surface.
- **First-class `trigger_forecast_id` column.** Currently parsed
  from `reason` at read time. If the parser ever misses a caller,
  the row still renders (falls back to short uuid), but a real
  column would make the join direct and the reason string
  optional.

## Also in this release (unrelated small fixes)

Two pre-existing fixes in `crates/fermi-console/src/main.rs` that were
sitting uncommitted in the working tree; folded in rather than left
dangling.

- **Dashboard marketplace card filter.** The Fresh tier of the
  Dashboard's marketplace card was empty because the filter matched
  `agent_type == "forecast_analyst"` — no curated agent uses that
  type. Switched to the `fermi-orchestra` tag filter that
  `render_agent_fleet_panel` already uses, so the Fresh tier now
  matches the fleet panel's population.
- **Fresh cockpit on PM market import.** Importing a second Polymarket
  market while a previous forecast was still open reused the same
  `CockpitState`, leaking its forecast_id, program, timeline,
  provenance, PM history, resolved metadata, session cost, agent_runs,
  and messages into the newly-imported question. Now unconditionally
  replaces the cockpit (same pattern as `on_new_forecast` /
  `on_reset_cockpit`) and clears `selected_forecast_id`. GC drops the
  previous state on Option overwrite.

## Files touched

- `src/handlers/forecast_benchmark.rs` — new
  `forecast_cascade_provenance_handler`.
- `src/api_server.rs` — route registration for
  `GET /api/forecasts/:id/cascade-provenance`.
- `tests/forecast_cascade_provenance_shapes.rs` — new wire-format
  shape tests (8 assertions).
- `crates/fermi-console/src/api/client.rs` — new
  `ApiClient::forecast_cascade_provenance` method.
- `crates/fermi-console/src/cockpit.rs` — `RightTab::Provenance`
  variant + state fields + `load_provenance` fetch +
  `render_provenance_tab` / `render_provenance_body` /
  `render_provenance_row` + `shorten_question_for_provenance` /
  `short_id` / `shorten_ts` helpers. Tab bar entry.
- `crates/fermi-console/src/main.rs` — Provenance in the
  Ctrl+Shift+FPL tab-cycle.
- `crates/fermi-console/Cargo.toml` — version bump.
