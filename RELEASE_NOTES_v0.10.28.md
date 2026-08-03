# v0.10.28 — the charts stop flickering, and start answering back

The console's visualisations were rasterised bitmaps. This release
replaces them with vector rendering and makes them interactive. Three
things you'll notice immediately, one bug fix you'll see move.

## 1. The flicker is gone — it was a texture-atlas leak

Not a rendering glitch. Every chart rasterised itself into a fresh
`RenderImage` on every frame, and GPUI mints a new sprite-atlas tile per
image id — one that `ImageSource::Render` is explicitly excluded from
ever reclaiming:

| Step | Where | Consequence |
|---|---|---|
| `RenderImage::new` mints an id from a global counter | `assets.rs:61` | each rebuild is a *different image* to GPUI |
| `paint_image` inserts into the atlas keyed by that id | `window.rs:3143` | a new tile per rebuild |
| `ImageSource::Render` skipped by `remove_asset` | `img.rs:561` | the tile is never freed |

The trajectory view made it pathological. To find the mouse under a
bitmap it laid **60 invisible hit-strips** across the plot, each firing
a re-render on hover. One mouse sweep allocated roughly **45 MB** of
atlas and freed none, forcing repeated atlas growth. That was the
flicker. The driver list did the same at nine 120×24 tiles per frame.

Charts now paint vectors straight into the scene. Nothing touches the
atlas, everything renders at device resolution instead of being
stretched on HiDPI, and hovering no longer rasterises 192,000 pixels per
mouse move.

## 2. Trajectory events now say what they *did*

Every event marker used to sit at the nearest point on the worm. An
agent run that swung the forecast eleven points and a market tick that
moved nothing rendered **identically** — the overlay showed *when*
things happened and never *what they did*.

Each marker now carries the rate immediately before and after it, and
the delta between them:

- **colour** — event kind (revision / refit / agent / market)
- **size + badge** — consequence, e.g. `▲+11.2`
- **ring** — selection
- **stem** — down to a collision-packed rug lane, so dense clusters stay
  countable instead of smearing

When the rate series doesn't straddle an event, the marker reports *no
delta* rather than zero. Clamped interpolation would otherwise
manufacture "this event did nothing" out of "we have no data here".

Scrubbing is continuous now, with the readout placed on the curves
themselves rather than in a pill across the panel.

## 3. Drag a threshold on the outcome distribution

Click anywhere on the histogram to set a decision threshold; drag to
move it. **P(≥ t)** updates live as the headline number, and bars
recolour into cleared / straddling / short as the handle passes them.

Three fixed percentile ticks tell you where the mass is. The question
you're actually deciding — *"will this clear the bar?"* — needed the bar
to be a thing you can hold.

## 4. Driver sensitivity distinguishes a driver from a coupling

The bars drew total-order Sobol only, so a driver at `S₁ 0.05, Sₜ 0.40`
looked exactly like one at `S₁ 0.38, Sₜ 0.40`. Those call for opposite
actions: the first is a **coupling to go find**, the second is a **prior
to go tighten**.

Bars are now two segments — cyan for first-order, purple for the
interaction remainder — with a `⋈` marker on interaction-dominated
drivers. Also new:

- **Rank movement** since the last sim (`▲2`, `▼1`). "Which driver
  dominates" changing is the highest-signal event in a live forecast;
  every run used to destroy that signal by overwriting.
- **A verdict line** that says whether there's a driver worth tightening
  at all, or whether influence is spread and "reduce uncertainty on X"
  is unsound advice however tall X's bar is.
- Drivers added since the last sim get a count, not a bar — their spread
  is not a Sobol index and plotting both on one axis invites exactly the
  wrong reading.

## Bug fix: histogram anchor lines were in the wrong place

The model / base / crowd reference lines were positioned using a mapping
built from `[p5, p95]`, while the bars are laid out across the output's
full `[min, max]`. **Every anchor was mispositioned** — stretched toward
the edges — and any anchor outside the middle 90% vanished entirely
rather than sitting near the tail where it belonged.

Two derivations of one geometry, the same disease as the trajectory
chart's duplicated bounds. There is now one scale per chart, consulted
by the painter and the hit-tester alike.

**You will see the anchor lines move.** That is the fix landing.

## Under the hood

Geometry and statistics moved into a `plot` module in the library
target, where they are covered by **118 tests**. The binary target
cannot be compiled under `--test` — rustc overflows its stack expanding
GPUI's element chains — so anything living next to a paint call is
untestable by construction. Pinned invariants now include the
pixel→value round trip the old duplicated geometry could not guarantee,
and the agreement between bar colouring and the printed threshold
probability.

`charts.rs` shrank from 1234 lines to 128. Net **−1693 / +892**.

Also evaluated and declined: `longbridge/gpui-component`. It needs
`gpui` from Zed's git `main` against our pinned `0.2.2`, and its charts
are presentational — no event annotation, brushing, zoom or tornado — so
the substance would have been built on top regardless. Its plot-module
architecture was adopted instead, at no dependency cost.

See `docs/fermi/VISUALIZATION_ARCHITECTURE.md`.
