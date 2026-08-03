# v0.10.22 — clamp the whole interval, not just the point estimate

## Why

v0.10.21 fixed simulations being discarded on save by clamping
`predicted_probability` into `[0,1]`. It fixed one of three columns.
The failure simply moved one column to the right:

```
[08:13:53] ✗ save — Saved locally, but backend save failed:
Server error: error returned from database: new row for relation
"fermi_forecasts" violates check constraint
"fermi_forecasts_confidence_interval_high_check"
Occurred 2x (last at 08:14:10)
```

`confidence_interval_low` and `confidence_interval_high` are filled
straight from the simulation's `p5` and `p95` — exactly as unbounded as
the mean. A model whose mean is `1.068` has a p95 of `1.655`.

Both columns carry `CHECK (col >= 0 AND col <= 1)` (mig-048, mig-094).
Unlike `predicted_probability`, the handler does **not** range-check
them in Rust, so the value reaches Postgres and returns as a constraint
violation wrapped in a 500 rather than a clean 400.

That diagnostic is quoted verbatim above from the Activity panel, which
is what made this a five-minute diagnosis instead of an afternoon.

## Changes

### 1. `clamp_wire_interval_bound`

`crates/fermi-console/src/wire.rs`

Applied to both bounds at both persistence sites (`persist_backend_save`
and `publish_forecast`). Clamps into `[0,1]`; returns `None` for
non-finite input so the field is omitted rather than invented — both
columns are nullable, and unlike a point estimate an interval bound has
no defensible stand-in.

Six new tests, including the observed `1.655` and a property check that
clamping can collapse an interval to a point but can never invert it.

### 2. Permanent failures stop being reported as retryable

The event above said `transient: yes`. It was not transient — a CHECK
violation fails identically forever, and autosave was re-issuing it
every cycle (hence `Occurred 2x` and climbing).

The cause: `ApiError::is_transient()` treats every 5xx as retryable,
which is reasonable for transport errors but wrong for Postgres, which
returns 500 for permanently-doomed writes.

`SaveErrorDiagnosis` gains `recognised` and `retryable`. When a branch
positively identifies the failure, its verdict now outranks the
transport-level heuristic — both for the remedy button and for the
`retryable` context line, which now reads
`no — permanent, retrying cannot help`.

### 3. CHECK violations are a recognised failure class

That event also said *"This failure didn't match any known pattern"* —
it fell through to the catch-all.

`classify_backend_save_error` gains a branch that extracts the
constraint name (`extract_quoted_constraint`) and puts it in the
summary, so the row is self-identifying and distinct violations don't
coalesce into each other. When the constraint is a
`confidence_interval` one, the detail explains the p5/p95 provenance
and points at the base-rate fix.

The catch-all is now correctly marked `recognised: false`, so callers
fall back to the transport heuristic only when we genuinely don't know.

## Validation

- `cargo test -p fermi-console --lib` — 24/24
- `cargo test -p fermi-activity` — 18/18
- `cargo check --workspace` — 0 errors

## Related

- v0.10.17 — the Activity panel that produced the diagnostic above
- v0.10.21 — clamped `predicted_probability`; this finishes the family
