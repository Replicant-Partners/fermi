# Visualisation architecture

Status: **complete.** Every chart is now vector. `charts.rs` and the
`plotters` + `image` dependencies are gone.

This document answers three questions:

1. Why did the charts flicker, and what fixed it?
2. Does `longbridge/gpui-component` fit? (Short answer: not as a
   dependency. Yes as an architecture.)
3. What does "direct manipulation" mean concretely for a forecasting
   console, and what's built vs. what's next?

---

## 1. The flicker was a texture-atlas leak

Not a rendering glitch, not a GPU driver issue — a resource leak with a
visible symptom. The chain, all verifiable in `gpui-0.2.2`:

| Step | Location | Consequence |
|---|---|---|
| `RenderImage::new` mints an id from a global `AtomicUsize` | `assets.rs:61` | every rebuild is a *different image* as far as GPUI is concerned |
| `Window::paint_image` inserts into the sprite atlas keyed by that id | `window.rs:3143` | a new atlas tile per rebuild |
| `ImageSource::Render` is excluded from `remove_asset` | `elements/img.rs:561` | the tile is never reclaimed |

Every `charts.rs` function returned a fresh `Arc<RenderImage>`, and
every one was called from inside `render()`. So each frame allocated new
atlas tiles and freed none.

The trajectory view made this pathological. To find the mouse position
under a bitmap it laid **60 invisible hit-strips** across the plot, each
calling `cx.notify()` on hover. One mouse sweep = 60 re-renders × an
800×240 RGBA tile ≈ **45 MB of atlas churn**, forcing repeated atlas
growth and reallocation. That's the flicker. The driver list did the
same thing at smaller scale: one 120×24 sparkline bitmap *per driver
card*, nine tiles a frame on a typical panel.

Two quieter costs rode along:

- **Blurry on HiDPI.** Bitmaps were rasterised at logical pixel size and
  stretched by the scale factor.
- **CPU-bound hover.** 192,000 pixels rasterised by plotters per mouse
  move.

**The fix is not to cache the bitmap.** Caching would paper over the
leak while leaving the deeper problem: a rasteriser is a one-way
function. It maps data → pixels and offers no way back. Everything the
console wants next — scrubbing, brushing, dragging a threshold, zooming
a time window — is the *inverse* direction.

So charts now paint vectors directly into the GPUI scene via
`PathBuilder` / `window.paint_path`. Paths cost nothing to re-emit,
tessellate at device resolution, and sit next to a scale that inverts.

---

## 2. On `longbridge/gpui-component`

### Verdict: adopt the pattern, not the crate

**The blocker is dependency shape, and it's hard.** GPUI Component
requires:

```toml
gpui          = { git = "https://github.com/zed-industries/zed" }
gpui_platform = { git = "https://github.com/zed-industries/zed", features = ["font-kit", ...] }
```

The console pins `gpui = "0.2.2"` from crates.io. These are not the same
API: the git version has split platform initialisation into a separate
`gpui_platform` crate (`application()` moved out of `gpui`), moved to
edition 2024, and made `Pixels.0` public again — a difference this
migration already tripped over. GPUI Component is also
`publish = false`; there is no crates.io release to pin.

Adopting it would mean:

- moving a 24,000-line `cockpit.rs` and 20,000-line `main.rs` onto an
  unpinned git dependency that tracks Zed's `main`,
- wrapping the root view in their `Root` and reconciling their `Theme`
  system with the console's existing `theme` module,
- accepting their `init(cx)` lifecycle and asset/icon conventions.

That is a large, ongoing-maintenance bet. What we'd get for it is
`LineChart`, `AreaChart`, `BarChart`, `PieChart`, `RadarChart`,
`CandlestickChart`, `SankeyChart` — a well-made shadcn/Recharts-style
set.

**But those are presentation charts, and our problem isn't
presentation.** Look at what `LineChart` offers: `.x()`, `.y()`,
`.stroke()`, `.dot()`, `.grid()`, and a hover tooltip that snaps to the
nearest index. There is no API for annotating a series with events, no
brushing, no linked highlighting, no zoom, no draggable reference line,
no density/violin/ridgeline, and no tornado. Every single thing that
makes the Fermi trajectory chart *the trajectory chart* would have to be
built on top anyway — on a foundation we don't control and can't pin.

### What we took instead

Their `crates/ui/src/plot` module is genuinely well-designed, and the
architecture transferred cleanly at zero dependency cost:

| GPUI Component | Fermi equivalent | Note |
|---|---|---|
| `plot::scale::{ScaleLinear, ScalePoint}` | `plot::scale::LinearScale` | ours is invertible in both directions and `nice()`-aware |
| `Plot::paint(bounds, window, cx)` | `viz::*` painters over `canvas()` | same idea, no derive macro needed |
| `Plot::tooltip_state(position, bounds)` | `PlotSurface` + `TrajectorySpec::probe` | ours records bounds at paint so hit-testing is *derived from* the render |
| `plot::shape::{Line, Area, Bar}` | `viz::paint::{polyline, area_between, dot, …}` | thin wrappers over `PathBuilder` |
| `plot::axis`, `plot::grid` | `plot::frame::Frame` + `LinearScale::ticks` | one object owns the whole geometry |

The one place we deliberately diverged: GPUI Component computes scales
inside `paint` and *recomputes them* in `tooltip_state`, with a comment
noting the two must be kept in sync. That's the same duplication bug the
old `charts::trajectory_plot_bounds` had. We hoist geometry into a
single `Frame` built once per render and consulted by both.

**Worth revisiting if:** the console ever moves to git-`gpui` for other
reasons (their `Table`, `Dock` layout, and code editor are strong), or
if GPUI Component publishes to crates.io with a pinnable version.

---

## 3. Direct manipulation, concretely

The Victor argument is that a visualisation which can only be *looked
at* has thrown away most of its value. The reader has questions —
"what if the base rate were higher?", "which of these events actually
moved the number?", "how much of this mass clears my threshold?" — and a
picture makes them go somewhere else to ask.

For a forecasting console this cashes out as four properties. Where each
one stands:

### 3.1 Invertibility — ✅ built

Every chart's geometry is a `plot::Frame` holding two `LinearScale`s
that map *both ways*. The painter calls `frame.point(t, pct)`; the mouse
handler calls `frame.hover_x(px, py)`. There is exactly one geometry, so
the value in the readout is the value under the cursor **by
construction** rather than by two pieces of arithmetic happening to
agree.

This is enforced by test, not by convention:

```rust
// plot::trajectory::tests
fn probing_a_pixel_returns_the_value_painted_at_it() {
    for t in [10.0, 50.0, 150.0, 199.0] {
        let (px, _) = f.point(t, 20.0);
        assert!((s.probe(px, ...).unwrap().t - t).abs() < 1e-6);
    }
}
```

### 3.2 Continuous, in-place readout — ✅ built

The 60 hit-strips are gone. `on_mouse_move` reads the real cursor
position, inverts it, and the scrub cursor paints dots *on the curves*
with the numbers beside them — where the eye already is, not in a legend
across the panel.

### 3.3 Events that explain rather than decorate — ✅ built

This was the substantive gap. Previously every event marker sat at
`rate_at(ts)`, the nearest worm point. An agent run that swung the
forecast eleven points and a market tick that moved nothing rendered
*identically*. The overlay showed **when** things happened and never
**what they did**.

`plot::events::correlate` now computes, per event, the interpolated rate
just before and just after, and the delta between them. The chart uses
three independent visual channels so none has to do two jobs:

- **colour** = event kind
- **size + delta badge** = consequence (`▲+11.2`)
- **ring** = selection

Plus a stem down to a collision-packed rug lane, so a dense cluster
stays countable instead of becoming a smear.

One subtlety that took a test to get right: when the rate series doesn't
straddle an event, `correlate` returns `delta: None` rather than `0.0`.
Clamped interpolation would otherwise manufacture "this event did
nothing" out of "we have no data here" — a confident lie, which is worse
than a gap.

```rust
fn events_outside_the_series_span_report_no_delta_rather_than_zero()
```

### 3.4 Sensitivity that distinguishes a driver from a coupling — ✅ built

The sensitivity bars drew **total-order only**. So a driver at
`S₁ = 0.05, Sₜ = 0.40` rendered identically to one at `S₁ = 0.38,
Sₜ = 0.40` — and those call for opposite actions. The first is a
coupling to go find; the second is a prior to go tighten. That's the
chart being actively misleading, not merely thin.

Each bar is now two segments — cyan for first-order, purple for the
interaction remainder — summing to the total-order length, so the eye
reads influence as the whole bar and composition as the split.
Interaction-dominated drivers carry a `⋈` marker.

Three things the old chart threw away, now surfaced:

- **Movement.** `SobolLayout::diff_against` gives rank and magnitude
  deltas against the previous run (`▲2`, `▼1`). "Which driver
  dominates" *changing* is the highest-signal event in a live forecast:
  it means the model's structure moved, not just its numbers. Every sim
  used to destroy that signal by overwriting.
- **A verdict.** `SobolLayout::verdict()` returns whether there's a
  driver worth tightening, a leader that's really a coupling, or an
  evenly-spread profile where "reduce uncertainty on X" is unsound
  advice however tall X's bar is. This is a claim about the model that
  sends someone off to do work, so it lives in the lib target with a
  test per branch rather than in a `match` inside a render function.
- **Honest gaps.** Drivers added since the last sim get a count, not a
  bar. Their p95−p5 spread is not a Sobol index and plotting the two on
  one axis invites exactly the apples-to-oranges reading the chart is
  trying to prevent. (The pre-sim fallback still shows spread — but the
  header then says "Driver spread", not "Driver influence".)

The two render sites (forecast-index panel and wiki tab) were
near-identical 80-line copies. They're now one `render_sensitivity_bars`
with a size profile.

### 3.5 A threshold you can grab — ✅ built

Three fixed percentile ticks tell you where the mass is. A forecaster
is asking *"what's the probability we clear the bar?"* — and the bar
moves as the argument moves. That question had to be answered by
reading a CDF tooltip bin by bin.

The outcome histogram now takes a click anywhere to set a decision
threshold, and a drag to move it. `P(≥ t)` updates live as the
headline number above the chart, and the bars recolour into cleared
(green) / straddling (amber) / short (dimmed) as the handle passes
them. The parameter being reasoned about became a thing you can hold,
and the consequence updates under your finger.

Two details that needed care:

**The bars and the number must not contradict each other.** The
probability counts the straddling bin *proportionally*; if the colouring
rounded that bin into one side, the chart would show six green bars and
claim 40%. `plot::density::bin_side` is the single classifier both use,
and the invariant is pinned:

```rust
fn bar_colouring_reproduces_the_printed_probability()
```

**Drags leave the element.** GPUI only delivers `on_mouse_move` while
the pointer is over the element, so the handlers sit on a band taller
than the bars to absorb vertical drift, and a move with no button held
ends the gesture — otherwise a mouse-up delivered outside the window
would latch the drag forever.

#### Bug found on the way in

The anchor reference lines (model / base / crowd) were positioned with
a mapping built from `[p5, p95]`, while the bars were laid out across
the output's full `[min, max]`. **Every anchor line was in the wrong
place** — stretched toward the edges — and any anchor outside the middle
90% vanished instead of sitting near the tail where it belonged.

Same disease as `trajectory_plot_bounds`: two derivations of one
geometry. There is now one `LinearScale` over the real bin domain,
shared by the bars, the anchors, and the threshold.

### 3.6 The index chart, and why its neighbour appeared to flicker — ✅ built

The outcome histogram was reported as flickering on hover even though
it's built from plain `div`s and touches no bitmap. Two separate causes,
neither of them in the histogram's own drawing:

**The bitmap next door.** The index chart sits directly beside it and
was the last `RenderImage` in the console. GPUI re-renders the whole
view on any `cx.notify()`, so hovering a *histogram bar* re-rasterised
the *index chart*, leaking an atlas tile per mouse-move. The histogram
looked like it was flickering; its neighbour was doing it. This is what
"they interact" turned out to mean.

**A layout feedback loop.** Both charts had a readout above them that
grew when something was hovered — one line to six for the histogram, one
to four for the index chart — inside a flex column. Hovering pushed the
chart down, which moved it out from under the cursor, which cleared the
hover, which shrank the readout, which moved the chart back up.
Self-sustaining oscillation, visually identical to flicker, and entirely
independent of the atlas leak. Both readouts are now a fixed two lines
at a fixed height.

The index chart also wasn't showing what it claimed. It plotted three
"series", but the base rate and crowd price were copied unchanged into
every version's row — two constants drawn with the same visual weight as
the one line that varied. Now:

- the base rate is drawn as the reference level it is (dashed),
- the crowd becomes a **real series** by sampling recorded price history
  at each version's timestamp — "what was the market saying when I saved
  v3?", which is what tells you whether a revision was insight or drift,
- the model–crowd gap is shaded, as on the trajectory chart,
- revisions that moved the number carry a delta badge, the same idiom as
  trajectory event markers,
- hover is continuous rather than N invisible per-version columns.

Sampling uses `events::sample_within`, which returns `None` outside the
recorded span instead of clamping. Clamping would have reported today's
price as the price at a version saved before polling began — inventing
history rather than admitting a gap.

### 3.7 Honesty about provenance — ✅ built

The old driver sparkline drew a triangle through `(p5, 0) → (p50, peak)
→ (p95, 0)`. For a bimodal posterior — exactly when shape matters most —
that isn't an approximation, it's a fabrication: one mode where there
are two, symmetric tails on a lognormal.

`plot::Density` now tags every curve with where its shape came from
(`Samples` / `Histogram` / `Quantiles`) and the chart captions itself.
Quantile-derived curves get a warning colour so nobody reads shape off
them. The test that pins this:

```rust
fn kde_recovers_both_modes_where_the_triangle_could_not()
```

Related: `P(X ≥ threshold)` is computed from the source bins, never by
integrating the display curve — which is peak-normalised and would give
a plausible-looking wrong answer.

---

## 4. What's next

Ordered by value per unit of risk.

### 4.1 Brush-to-zoom on the trajectory — geometry done, needs a gesture

`TrajectorySpec::zoom` works and is tested, including the property that
zooming in separates event labels that shared a rug lane. It needs a
drag gesture and a `zoom` field on `CockpitState`.

### 4.2 Bidirectional brushing — half done

Chart → list works: hovering the trajectory latches the nearest event
and highlights its row. List → chart is one line away (the event row's
`on_hover` already sets `hovered_trajectory_event`, which the chart
reads as `selected_event`) — worth verifying end to end.

### 4.3 Threshold on the driver sparklines

`DistributionPlot` paints a threshold handle and split shading already;
only the outcome histogram is wired. Per-driver thresholds would need a
`HashMap<String, f64>` on `CockpitState` and the same drag band. Lower
value than the outcome threshold — a driver's prior isn't usually what
you're deciding against — so it's parked.

### 4.4 ~~Migrate the index chart~~ — done

`charts.rs` is deleted. `plotters`, `plotters-bitmap`, `ab_glyph`,
`ab_glyph_rasterizer`, `owned_ttf_parser` and `image` are out of the
console's dependency tree. There are no `RenderImage`s left in the
application, and therefore no way for this class of flicker to recur.

---

## 5. Module map

131 tests, all in the lib target.

```
crates/fermi-console/src/
├── plot/            # lib target — GPUI-free, tested (110 tests)
│   ├── scale.rs         invertible data↔pixel mapping, nice ticks
│   ├── frame.rs         the one geometry object, shared painter/hit-tester
│   ├── density.rs       KDE, histogram & quantile curves, provenance tags,
│   │                     threshold probability + bin classification
│   ├── events.rs        event↔trajectory correlation, rug-lane packing
│   ├── sobol.rs         variance decomposition, run-over-run diff, verdict
│   ├── format.rs        scale-adaptive axis and readout formatting
│   ├── distribution.rs  distribution chart geometry
│   ├── index.rs         version-index geometry + probe()
│   └── trajectory.rs    trajectory chart geometry + probe()
├── viz/             # bin target — paints, decides nothing
│   │                 (sensitivity bars stay in cockpit.rs: plain divs,
│   │                  no canvas, with all judgement in plot::sobol)
│   ├── paint.rs         PathBuilder primitives
│   ├── mod.rs           PlotSurface (window↔element coordinate bridge)
│   ├── distribution.rs
│   ├── index.rs
│   └── trajectory.rs
└── (charts.rs deleted — no bitmaps remain)
```

**Why the split:** the bin target *cannot be tested*. `rustc` overflows
its stack expanding GPUI's element chains under `--test` — a documented,
still-reproducible condition (see `src/lib.rs`). Any logic that lives
next to `window.paint_path` is logic nobody can assert on. So all
geometry and statistics live in the lib target, and `viz` is reduced to
paint calls that delegate every question about *where* and *how much*.

Run them with:

```bash
cargo test -p fermi-console --lib
```
