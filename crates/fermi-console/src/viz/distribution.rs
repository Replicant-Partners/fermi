//! The distribution chart — vector, interactive, and honest about what
//! it knows.
//!
//! # What changed and why
//!
//! 1. **It flickered.** Every driver card rasterised its own 120×24
//!    sparkline into a fresh `RenderImage` on every frame, and the
//!    results panel did the same with a 400×80 histogram. Each one
//!    leaked a sprite-atlas tile (see `viz::paint` for the mechanism);
//!    a panel with eight drivers churned nine tiles per frame. Nothing
//!    here touches the atlas.
//!
//! 2. **The sparkline was a fiction.** The old renderer drew a triangle
//!    through `(p5, 0) → (p50, peak) → (p95, 0)`. That's not the
//!    driver's distribution, it's the *idea* of a distribution:
//!    always unimodal, and identically shaped for a triangular prior
//!    and a lognormal one. The curve now comes from
//!    [`plot::Density`], which records where its shape came from so
//!    the chart can caption itself instead of implying more precision
//!    than it has.
//!
//! 3. **It answered the wrong question.** Three fixed percentile ticks
//!    tell you where the mass is. A forecaster is asking "what's the
//!    probability we clear the bar?" — and the bar moves as the
//!    argument moves. So the chart carries a threshold and reports
//!    P(X ≥ t) live, computed from the source bins rather than by
//!    integrating a curve that was normalised for display.
//!
//! Geometry lives in `fermi_console::plot::distribution`, in the lib
//! target, because the bin target can't be compiled under `--test`.

use gpui::{
    canvas, div, px, App, Bounds, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window,
};

use fermi_console::plot::{density::Density, distribution::DistributionSpec, format};

pub use fermi_console::plot::distribution::{Chrome, Percentiles};

use super::paint::{self, Align};
use super::PlotSurface;

// Chart palette. Re-exported from `crate::theme` rather than redeclared,
// because these constants *were* redeclared — and then drifted: the axis
// labels here kept the pre-accessibility grey (2.7:1 against the panel
// background) long after the same token was fixed everywhere else.
//
// `AXIS_LABEL` is text and carries a contrast floor; `GRIDLINE` is a
// hairline and does not. Keeping them as separate names is what stops the
// next person from reaching for the quiet one to draw a label with.
use crate::theme;

const FG: u32 = theme::FG;
const AXIS_LABEL: u32 = theme::FG_DIM;
const GRIDLINE: u32 = theme::BORDER;
/// The hover crosshair. Brighter than [`GRIDLINE`] on purpose — it
/// tracks the cursor and has to be findable at a glance.
const CROSSHAIR: u32 = theme::FG_DIM;
const CAPTION: u32 = theme::FG_MUTED;
const CYAN: u32 = theme::CYAN;
const GREEN: u32 = theme::GREEN;
const GOLD: u32 = theme::GOLD;
const ORANGE: u32 = theme::ORANGE;

/// A probability-density chart.
#[derive(IntoElement)]
pub struct DistributionPlot {
    spec: DistributionSpec,
    background: Option<u32>,
    accent: u32,
    /// Data-space x under the cursor, supplied by the parent from a
    /// `PlotSurface` inversion.
    hover_x: Option<f64>,
    surface: Option<PlotSurface>,
    caption_source: bool,
}

// Builder surface. Several of these aren't wired into a call site yet
// — the threshold handle and the hover readout are the interactive
// half, and hooking them up means giving `CockpitState` somewhere to
// keep the dragged threshold per driver. The painting for them is
// implemented and exercised by `plot::distribution`'s tests, so the
// remaining work is state plumbing rather than rendering.
#[allow(dead_code)]
impl DistributionPlot {
    pub fn new(density: Density) -> Self {
        Self {
            spec: DistributionSpec::new(density, 400.0, 80.0),
            background: None,
            accent: CYAN,
            hover_x: None,
            surface: None,
            caption_source: true,
        }
    }

    /// Build from simulation histogram bins spanning `[lo, hi]`.
    pub fn from_bins(bins: &[u32], lo: f64, hi: f64) -> Self {
        Self::new(Density::from_bins(bins, lo, hi))
    }

    /// The inline driver sparkline: 120×24, no chrome, blends into the
    /// card behind it.
    ///
    /// Still a quantile-sourced curve, because a driver's prior *is*
    /// three numbers — but it now uses a skew-respecting two-sided
    /// Gaussian rather than a triangle, is tagged as inferred, and
    /// costs zero atlas tiles.
    pub fn sparkline(p5: f64, p50: f64, p95: f64) -> Self {
        let mut s = Self::new(Density::from_quantiles(p5, p50, p95, 96))
            .size(120.0, 24.0)
            .caption_source(false);
        s.spec.chrome = Chrome::Bare;
        s.spec.percentiles = Percentiles::new(p5, p50, p95);
        s
    }

    /// A sparkline over a density that was built from real draws.
    ///
    /// [`Self::sparkline`] is hard-wired to `Density::from_quantiles`, which the
    /// density module labels a sketch — `shape_is_real()` is false for it,
    /// because a two-sided Gaussian through three percentiles cannot show skew,
    /// a bound, or a second mode. It was also the only sparkline constructor,
    /// so the driver card could draw nothing but a Triangular driver, and four
    /// of the five distribution types the engine samples rendered as blank.
    ///
    /// This takes a `Density` the caller has already built — in practice from
    /// `plot::curve::driver_curve`, which draws through the same per-family
    /// samplers the executor uses, so the picture is of what will actually be
    /// sampled.
    pub fn sparkline_from_density(density: Density, p5: f64, p50: f64, p95: f64) -> Self {
        let mut s = Self::new(density).size(120.0, 24.0).caption_source(false);
        s.spec.chrome = Chrome::Bare;
        s.spec.percentiles = Percentiles::new(p5, p50, p95);
        s
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.spec.width = width as f64;
        self.spec.height = height as f64;
        self
    }

    pub fn chrome(mut self, chrome: Chrome) -> Self {
        self.spec.chrome = chrome;
        self
    }

    pub fn percentiles(mut self, p: Percentiles) -> Self {
        self.spec.percentiles = p;
        self
    }

    /// Paint an opaque background behind the curve. Leave unset to let
    /// the parent's background show through — which is what an
    /// embedded sparkline wants, and what the bitmap version had to
    /// fake by being *told* the card's colour.
    pub fn background(mut self, rgb_hex: u32) -> Self {
        self.background = Some(rgb_hex);
        self
    }

    pub fn accent(mut self, rgb_hex: u32) -> Self {
        self.accent = rgb_hex;
        self
    }

    /// Set the decision threshold and derive P(X ≥ t) from the same
    /// bins the curve was built from.
    pub fn threshold_from_bins(mut self, t: f64, bins: &[u32], lo: f64, hi: f64) -> Self {
        self.spec = self.spec.with_threshold_from_bins(t, bins, lo, hi);
        self
    }

    pub fn hover_x(mut self, x: Option<f64>) -> Self {
        self.hover_x = x;
        self
    }

    /// Attach a surface so the parent can invert mouse positions.
    pub fn surface(mut self, s: PlotSurface) -> Self {
        self.surface = Some(s);
        self
    }

    pub fn caption_source(mut self, on: bool) -> Self {
        self.caption_source = on;
        self
    }

    /// Pixel → data-space x, through the same frame the painter uses.
    pub fn probe(&self, local_x: f64, local_y: f64) -> Option<f64> {
        self.spec.frame().hover_x(local_x, local_y)
    }
}

impl RenderOnce for DistributionPlot {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let (w, h) = (self.spec.width as f32, self.spec.height as f32);
        let surface = self.surface.clone();

        div().w(px(w)).h(px(h)).child(
            canvas(
                move |bounds: Bounds<Pixels>, _window, _cx| {
                    if let Some(s) = &surface {
                        s.record(bounds);
                    }
                    bounds
                },
                move |_bounds, painted: Bounds<Pixels>, window, cx| {
                    self.paint(painted, window, cx);
                },
            )
            .w(px(w))
            .h(px(h)),
        )
    }
}

impl DistributionPlot {
    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let origin = bounds.origin;
        let f = self.spec.frame();
        let plot = f.plot;
        let full = self.spec.chrome.is_full();
        let density = &self.spec.density;

        if let Some(bg) = self.background {
            paint::rect(
                window,
                origin,
                0.0,
                0.0,
                self.spec.width,
                self.spec.height,
                paint::hsla(bg, 1.0),
            );
        }

        if density.is_empty() {
            if full {
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.left + plot.width() / 2.0,
                    plot.top + plot.height() / 2.0 - 5.0,
                    "no distribution yet — run a simulation",
                    10.0,
                    paint::hsla(AXIS_LABEL, 1.0),
                    Align::Center,
                );
            }
            return;
        }

        // Curve in pixel space, computed once so the fill, the stroke
        // and the threshold split cannot disagree about where it is.
        let curve: Vec<(f64, f64)> = density
            .points
            .iter()
            .map(|(x, y)| f.point(*x, *y))
            .collect();
        let baseline = f.y.map(0.0);
        let accent = paint::hsla(self.accent, 1.0);

        // ── Threshold split ─────────────────────────────────────────
        //
        // Shade the two sides of the decision boundary differently.
        // The point isn't "here is a bump", it's "this much of the bump
        // clears the bar".
        match self.spec.visible_threshold() {
            Some(t) => {
                let tx = f.x.map(t);
                let mut left: Vec<(f64, f64)> =
                    curve.iter().copied().filter(|(x, _)| *x <= tx).collect();
                let mut right: Vec<(f64, f64)> =
                    curve.iter().copied().filter(|(x, _)| *x >= tx).collect();
                // Stitch the exact crossing point into both halves so
                // the two fills meet the rule with no sliver gap.
                if let Some(y_at_t) = density.at(t) {
                    let cross = (tx, f.y.map(y_at_t));
                    left.push(cross);
                    right.insert(0, cross);
                }
                paint::area_to_baseline(
                    window,
                    origin,
                    &left,
                    baseline,
                    paint::hsla(self.accent, 0.10),
                );
                paint::area_to_baseline(window, origin, &right, baseline, paint::hsla(GREEN, 0.28));
            }
            None => {
                paint::area_to_baseline(
                    window,
                    origin,
                    &curve,
                    baseline,
                    paint::hsla(self.accent, 0.16),
                );
            }
        }

        paint::polyline(window, origin, &curve, accent, if full { 1.5 } else { 1.0 });

        if full {
            paint::rule_h(
                window,
                origin,
                plot.left,
                plot.right,
                baseline,
                paint::hsla(GRIDLINE, 1.0),
                1.0,
                None,
            );
        }

        // ── Percentile markers ──────────────────────────────────────
        //
        // Ticks that rise from the baseline *to the curve* rather than
        // full-height rules: a marker that stops at the density tells
        // you how much mass is actually there.
        let (dx0, dx1) = f.x.domain();
        for (val, color, weight) in [
            (self.spec.percentiles.p5, GOLD, 1.0),
            (self.spec.percentiles.p95, GOLD, 1.0),
            (self.spec.percentiles.p50, GREEN, 1.5),
        ] {
            let Some(v) = val else { continue };
            if v <= dx0 || v >= dx1 {
                continue;
            }
            let top = density.at(v).map(|d| f.y.map(d)).unwrap_or(f.y.map(0.35));
            paint::rule_v(
                window,
                origin,
                f.x.map(v),
                baseline,
                top,
                paint::hsla(color, 0.8),
                weight,
                None,
            );
        }

        // ── Threshold handle ────────────────────────────────────────
        if let Some(t) = self.spec.visible_threshold() {
            let tx = f.x.map(t);
            paint::rule_v(
                window,
                origin,
                tx,
                plot.top,
                baseline,
                paint::hsla(GREEN, 1.0),
                1.5,
                Some([3.0, 3.0]),
            );
            // A visible grab affordance. An invisible drag target isn't
            // direct manipulation, it's a guessing game.
            paint::dot(
                window,
                origin,
                tx,
                plot.top + 3.0,
                3.5,
                paint::hsla(GREEN, 1.0),
            );

            if full {
                if let Some(p) = self.spec.prob_above {
                    let near_edge = tx > plot.right - 60.0;
                    paint::label(
                        window,
                        cx,
                        origin,
                        if near_edge { tx - 6.0 } else { tx + 6.0 },
                        plot.top + 1.0,
                        format!("P(≥) {:.0}%", p * 100.0),
                        10.0,
                        paint::hsla(GREEN, 1.0),
                        if near_edge { Align::End } else { Align::Start },
                    );
                }
            }
        }

        // ── Hover readout ───────────────────────────────────────────
        if let Some(hx) = self.hover_x {
            if hx > dx0 && hx < dx1 {
                if let Some(d) = density.at(hx) {
                    let (cx_px, cy_px) = f.point(hx, d);
                    paint::rule_v(
                        window,
                        origin,
                        cx_px,
                        plot.top,
                        baseline,
                        paint::hsla(CROSSHAIR, 0.7),
                        1.0,
                        None,
                    );
                    paint::dot(window, origin, cx_px, cy_px, 3.0, paint::hsla(FG, 1.0));
                    if full {
                        paint::label(
                            window,
                            cx,
                            origin,
                            cx_px,
                            (cy_px - 14.0).max(plot.top),
                            format::value(hx),
                            10.0,
                            paint::hsla(FG, 1.0),
                            Align::Center,
                        );
                    }
                }
            }
        }

        // ── Axis ends + provenance caption ──────────────────────────
        if full {
            for (v, align) in [(dx0, Align::Start), (dx1, Align::End)] {
                paint::label(
                    window,
                    cx,
                    origin,
                    f.x.map(v),
                    plot.bottom + 2.0,
                    format::value(v),
                    9.0,
                    paint::hsla(AXIS_LABEL, 1.0),
                    align,
                );
            }

            if self.caption_source {
                // A curve inferred from three quantiles gets a warning
                // colour: the operator must not read shape off it.
                let (color, alpha) = if density.source.shape_is_real() {
                    (CAPTION, 1.0)
                } else {
                    (ORANGE, 0.85)
                };
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right,
                    plot.top - 1.0,
                    density.source.caption(),
                    8.0,
                    paint::hsla(color, alpha),
                    Align::End,
                );
            }
        }
    }
}
