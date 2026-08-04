# v0.11.1 — the last bitmap is gone, and the index chart finally earns its space

Follow-up to v0.10.28. Two charts in the composer still flickered on
hover. Neither for the reason it looked like.

## Why the histogram appeared to flicker

It's built from plain `div`s and touches no bitmap, yet it flickered.
Two separate causes, neither in its own drawing.

**1. The bitmap next door.** The index chart sits directly beside it and
was the last `RenderImage` in the console. GPUI re-renders the whole view
on any `cx.notify()`, so hovering a *histogram bar* re-rasterised the
*index chart*, leaking a sprite-atlas tile per mouse-move. The histogram
looked like it was flickering; its neighbour was doing it.

**2. A layout feedback loop.** Both charts had a readout above them that
grew when something was hovered — one line to six for the histogram, one
to four for the index chart — inside a flex column. Hovering pushed the
chart down, which moved it out from under the cursor, which cleared the
hover, which shrank the readout, which moved the chart back up. A
self-sustaining oscillation, visually identical to flicker and entirely
independent of the atlas leak.

Both readouts are now a fixed two lines at a fixed height. Nothing
reflows on hover, ever.

## The index chart wasn't showing what it claimed

It plotted three "series", but the base rate and the crowd price were
copied unchanged into every version's row — two constants drawn with the
same visual weight as the one line that actually varied. That's why it
didn't read like the trajectory chart: it wasn't drawing the same kind of
thing.

Now:

- **The crowd is a real series.** Recorded price history is sampled at
  each version's timestamp, so you can see what the market was saying
  when you saved v3 — the comparison that tells you whether a revision
  was insight or drift.
- **The base rate is drawn as a level**, dashed, because that's what it
  is.
- **The model–crowd gap is shaded**, same as the trajectory chart.
- **Revisions that moved the number carry a delta badge** (`▲+30`), the
  same idiom as trajectory event markers.
- **Hover is continuous** — one element reading the real cursor position,
  replacing N invisible per-version columns.
- Axis ticks, version labels and a scrub crosshair, all vector.

Sampling refuses to extrapolate: a version saved before price polling
began shows `crowd —`, not today's price wearing last week's date.
Clamping there would have been inventing history.

## No bitmaps remain

`charts.rs` is deleted. `plotters`, `plotters-bitmap`, `ab_glyph`,
`ab_glyph_rasterizer`, `owned_ttf_parser` and `image` are out of the
console's dependency tree entirely.

There are no `RenderImage`s left in the application, so this class of
flicker cannot recur — it isn't fixed, it's structurally unavailable.

## Testing

131 tests in the library target, up from 118. New coverage includes the
version-pixel round trip, the snap-to-nearest-version rule, and the
refusal to extrapolate crowd prices beyond recorded history.

See `docs/fermi/VISUALIZATION_ARCHITECTURE.md`.
