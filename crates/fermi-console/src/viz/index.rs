//! The index chart — model vs base vs crowd, version over version.
//!
//! The last bitmap in the console, and the last hit-strip workaround.
//!
//! # Why it flickered, and why its *neighbour* flickered too
//!
//! This chart rasterised itself into a fresh `Arc<RenderImage>` on
//! every render, and GPUI never reclaims those atlas tiles (see
//! `viz::paint`). That alone made it flicker under its own hover.
//!
//! But it also sits directly beside the outcome histogram, and GPUI
//! re-renders the whole view on any `cx.notify()`. So hovering a
//! *histogram bar* re-rasterised *this* chart, leaking a tile per
//! mouse-move. The histogram appeared to flicker; the bitmap next to it
//! was doing the flickering. That's why the two felt coupled.
//!
//! # Why it didn't look like the trajectory chart
//!
//! Because it wasn't drawing the same thing. It plotted three "series",
//! but the base rate and the crowd price were copied unchanged into
//! every version — two constants dressed as data, given the same visual
//! weight as the one line that actually varied.
//!
//! Now: the base rate is drawn as the reference level it is, the crowd
//! becomes a real series wherever price history covers the versions,
//! the gap between model and crowd is shaded, and revisions that moved
//! the number carry a delta badge — the same idiom as the trajectory
//! chart's event markers, because they're read side by side.

use gpui::{
    canvas, div, px, App, Bounds, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window,
};

use fermi_console::plot::index::{IndexSpec, CONSEQUENTIAL_PP};

pub use fermi_console::plot::index::{IndexData, IndexProbe};

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
const CYAN: u32 = theme::CYAN;
const GREEN: u32 = theme::GREEN;
const GOLD: u32 = theme::GOLD;
const ORANGE: u32 = theme::ORANGE;
const PURPLE: u32 = theme::PURPLE;

#[derive(IntoElement)]
pub struct IndexPlot {
    spec: IndexSpec,
    /// Version highlighted from outside the chart, or by the parent's
    /// hover handler.
    selected: Option<usize>,
    surface: Option<PlotSurface>,
}

impl IndexPlot {
    pub fn new(data: IndexData) -> Self {
        Self {
            spec: IndexSpec::new(data, 240.0, 70.0),
            selected: None,
            surface: None,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.spec.width = width as f64;
        self.spec.height = height as f64;
        self
    }

    pub fn selected(mut self, idx: Option<usize>) -> Self {
        self.selected = idx;
        self
    }

    pub fn surface(mut self, s: PlotSurface) -> Self {
        self.surface = Some(s);
        self
    }

    /// Pixel → version. Delegates to the tested geometry.
    pub fn probe(&self, local_x: f64, local_y: f64) -> Option<IndexProbe> {
        self.spec.probe(local_x, local_y)
    }
}

impl RenderOnce for IndexPlot {
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
                move |_b, painted: Bounds<Pixels>, window, cx| self.paint(painted, window, cx),
            )
            .w(px(w))
            .h(px(h)),
        )
    }
}

impl IndexPlot {
    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let origin = bounds.origin;
        let f = self.spec.frame();
        let plot = f.plot;
        let d = &self.spec.data;
        let n = self.spec.len();

        if n == 0 {
            paint::label(
                window,
                cx,
                origin,
                plot.left + plot.width() / 2.0,
                plot.top + plot.height() / 2.0 - 4.0,
                "no versions yet — save to start the index",
                9.0,
                paint::hsla(AXIS_LABEL, 1.0),
                Align::Center,
            );
            return;
        }

        // ── Gridlines + y labels ────────────────────────────────────
        for v in f.y.ticks(3) {
            let y = f.y.map(v);
            if y < plot.top - 1.0 || y > plot.bottom + 1.0 {
                continue;
            }
            paint::rule_h(
                window,
                origin,
                plot.left,
                plot.right,
                y,
                paint::hsla(GRIDLINE, 0.45),
                1.0,
                None,
            );
            paint::label(
                window,
                cx,
                origin,
                plot.left - 4.0,
                y - 5.0,
                format!("{:.0}%", v),
                8.0,
                paint::hsla(AXIS_LABEL, 1.0),
                Align::End,
            );
        }

        let model = self.spec.model_series();
        let crowd = self.spec.crowd_series();
        let crowd_is_series = self.spec.crowd_is_series();

        // ── Divergence band ─────────────────────────────────────────
        //
        // Only where the crowd is a real series *and* covers the same
        // versions. Shading against a constant would draw a gap that's
        // an artefact of the reference line, not of the data.
        if crowd_is_series && crowd.len() == model.len() {
            let top: Vec<(f64, f64)> = model.iter().map(|(x, y)| f.point(*x, *y)).collect();
            let bot: Vec<(f64, f64)> = crowd.iter().map(|(x, y)| f.point(*x, *y)).collect();
            paint::area_between(window, origin, &top, &bot, paint::hsla(CYAN, 0.10));
        }

        // ── Base rate: a level, drawn as a level ────────────────────
        if let Some(b) = d.base_rate_pct {
            let y = f.y.map(b);
            if y >= plot.top && y <= plot.bottom {
                paint::rule_h(
                    window,
                    origin,
                    plot.left,
                    plot.right,
                    y,
                    paint::hsla(GOLD, 0.75),
                    1.0,
                    Some([4.0, 3.0]),
                );
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right + 3.0,
                    y - 5.0,
                    "base",
                    8.0,
                    paint::hsla(GOLD, 0.9),
                    Align::Start,
                );
            }
        }

        // ── Crowd: a series when we have one, else a level ──────────
        if crowd_is_series {
            let pts: Vec<(f64, f64)> = crowd.iter().map(|(x, y)| f.point(*x, *y)).collect();
            paint::polyline(window, origin, &pts, paint::hsla(PURPLE, 0.30), 4.0);
            paint::polyline(window, origin, &pts, paint::hsla(PURPLE, 1.0), 1.5);
            for (px_, py_) in &pts {
                paint::dot(window, origin, *px_, *py_, 2.0, paint::hsla(PURPLE, 1.0));
            }
            if let Some(last) = pts.last() {
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right + 3.0,
                    last.1 - 5.0,
                    "crowd",
                    8.0,
                    paint::hsla(PURPLE, 0.9),
                    Align::Start,
                );
            }
        } else if let Some(c) = d.crowd_now_pct {
            let y = f.y.map_clamped(c);
            paint::rule_h(
                window,
                origin,
                plot.left,
                plot.right,
                y,
                paint::hsla(PURPLE, 0.75),
                1.0,
                Some([4.0, 3.0]),
            );
            paint::label(
                window,
                cx,
                origin,
                plot.right + 3.0,
                y - 5.0,
                "crowd",
                8.0,
                paint::hsla(PURPLE, 0.9),
                Align::Start,
            );
        }

        // ── Model worm ──────────────────────────────────────────────
        //
        // Painted last so the operator's own view dominates, with the
        // same underlay-plus-core weighting as the trajectory chart.
        let model_px: Vec<(f64, f64)> = model.iter().map(|(x, y)| f.point(*x, *y)).collect();
        if model_px.len() >= 2 {
            paint::polyline(window, origin, &model_px, paint::hsla(CYAN, 0.30), 4.0);
            paint::polyline(window, origin, &model_px, paint::hsla(CYAN, 1.0), 2.0);
        }

        // ── Version markers + delta badges ──────────────────────────
        let deltas = self.spec.deltas();
        for (i, (px_, py_)) in model_px.iter().enumerate() {
            let selected = self.selected == Some(i);
            let consequential = deltas[i]
                .map(|dv| dv.abs() >= CONSEQUENTIAL_PP)
                .unwrap_or(false);
            let r = if consequential { 3.5 } else { 2.5 };
            paint::dot(window, origin, *px_, *py_, r, paint::hsla(CYAN, 1.0));
            if selected {
                paint::ring(
                    window,
                    origin,
                    *px_,
                    *py_,
                    r + 3.0,
                    paint::hsla(FG, 1.0),
                    1.5,
                );
            }

            // Badge only the revisions that moved something, and only
            // when there's room — on a 240px chart with six versions,
            // labelling every point is noise.
            if consequential && n <= 6 {
                if let Some(dv) = deltas[i] {
                    let up = dv > 0.0;
                    paint::label(
                        window,
                        cx,
                        origin,
                        *px_,
                        if up { py_ - 13.0 } else { py_ + 5.0 },
                        format!("{}{:.0}", if up { "▲+" } else { "▼" }, dv),
                        8.0,
                        paint::hsla(if up { GREEN } else { ORANGE }, 1.0),
                        Align::Center,
                    );
                }
            }
        }

        // ── x labels ────────────────────────────────────────────────
        //
        // Thin them out rather than overlapping: on a narrow chart only
        // the ends are legible, and the ends are what frame the story.
        let step = ((n as f64 / 5.0).ceil() as usize).max(1);
        for (i, v) in d.versions.iter().enumerate() {
            if i % step != 0 && i != n - 1 {
                continue;
            }
            let x = f.x.map(i as f64);
            paint::label(
                window,
                cx,
                origin,
                x,
                plot.bottom + 2.0,
                v.label.clone(),
                8.0,
                paint::hsla(AXIS_LABEL, 1.0),
                Align::Center,
            );
        }

        // ── Selection crosshair ─────────────────────────────────────
        if let Some(i) = self.selected.filter(|i| *i < n) {
            let x = f.x.map(i as f64);
            paint::rule_v(
                window,
                origin,
                x,
                plot.top,
                plot.bottom,
                paint::hsla(CROSSHAIR, 0.8),
                1.0,
                None,
            );
        }
    }
}
