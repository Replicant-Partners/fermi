# v0.10.21 — simulations actually persist; the Activity panel actually fills

Two bugs, one of them silent data loss.

## 1. Every simulation on a forecast without a base rate was discarded on save

**Symptom.** Run a simulation, watch the probability move, press
Ctrl+S, see "Saved just now". Reopen the forecast from the Portfolio —
the pre-simulation value is back. The simulation is gone.

**Cause.** `run_simulation` decides whether a forecast is
probabilistic by asking whether the question carries a `base_rate`:

```rust
let is_probability_forecast = question.base_rate.is_some();

if is_probability_forecast {
    self.predicted_probability = results.mean.clamp(0.01, 0.99);
} else {
    self.predicted_probability = results.mean;   // unclamped
}
```

The `else` branch exists for genuinely non-probabilistic forecasts —
counts, magnitudes, durations. But the default Fermi decomposition
emits a **multiplier chain**: `strength_factor * conditions *
disruption`, every driver centred on 1.0. Their product sits around
1.0 and routinely exceeds it. An observed run produced `1.068`.

Server-side, both `create_forecast_handler` and
`update_forecast_handler` reject anything outside `[0,1]`:

```
HTTP 400: predicted_probability must be between 0 and 1
```

So the backend write failed on **every** save. The local snapshot still
succeeded, the save chip still said "Saved just now", and the only
signal was a warning in a three-line banner that truncated it at
`Work will not survive c`.

**Fix.** `clamp_wire_probability` at the persistence boundary — both
`persist_backend_save` and `publish_forecast`, so no call path can
reintroduce it. The true mean is preserved verbatim in
`simulation_results`, which has no range constraint, and stays visible
on the Trajectory tab.

Non-finite input maps to `0.5` rather than propagating: `NaN` and
infinity serialise to JSON `null`, which fails deserialisation
server-side with a far less legible error.

When the clamp changes the value, the Activity panel gets a warning
explaining that a multiplier chain isn't a probability, and pointing at
the fix — anchor the question with a base rate so the model reads
`base_rate * driver * driver`.

The helper lives in the console's `lib` target with **5 tests**,
including the exact production value.

## 2. Six of seven cockpits were never observed

**Symptom.** The Activity panel introduced in v0.10.17 stayed
permanently empty — `All 0 / Problems 0` — after opening a forecast
from the Portfolio, even while the banner showed messages. The
`Activity ↗` pill in the banner did nothing when clicked.

**Cause.** `CockpitState` can't reach `FermiConsole` directly, so it
queues cross-surface intent in `pending_*` fields that only a
`cx.observe` handler drains. That observer was registered inline in
`navigate()` **and nowhere else** — while **eight** call sites
construct a cockpit:

`navigate` · `open_forecast` · `open_workspace_forecast` ·
`on_new_forecast` · `on_reset_cockpit` · `on_import_forecast` ·
`import_polymarket_forecast` · the dashboard hero

Seven of the eight produced an unobserved cockpit. Every signal queued
from those sessions was dropped: toasts, the invite-share modal,
forecast-list refreshes, and — since v0.10.17 — the Activity log mirror
and the panel-open request.

This is **pre-existing**, not a v0.10.17 regression. It was invisible
because a toast that never fires reads as "no news." An empty log and a
dead button don't.

The pill is the clearest illustration:

```rust
this.pending_open_activity = true;   // cockpit can't reach the parent
cx.notify();                         // observer is meant to drain this
```

No observer → flag set, never read, nothing happens, no error.

**Fix.** `install_cockpit()` — one helper that owns adoption *and*
observation, so they can't be separated again. All eight sites route
through it; the only direct assignment to `self.cockpit` is inside it.

## Validation

- `cargo test -p fermi-console --lib` — 18/18
- `cargo test -p fermi-activity` — 18/18
- `cargo check --workspace` — 0 errors

## Related

- v0.10.17 — the Activity panel these fixes make usable
- v0.10.18 — the `lib` target that makes `clamp_wire_probability`
  testable at all
