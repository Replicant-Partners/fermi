//! The trajectory chart — model vs crowd over time, with events that
//! explain the movement rather than merely coinciding with it.
//!
//! # What was naive about the bitmap version
//!
//! 1. **Events were decorative.** Every marker was drawn at
//!    `rate_at(ts)` — the nearest worm point — so an agent run that
//!    swung the forecast eleven points and a market tick that moved
//!    nothing looked identical. The overlay showed *when* things
//!    happened but never *what they did*. Markers now carry their
//!    before/after delta (`plot::events::correlate`), size themselves
//!    by consequence, and stem down to a rug lane so dense clusters
//!    stay countable.
//!
//! 2. **Hover was quantised.** A bitmap can't report cursor position,
//!    so the chart laid 60 invisible `div`s across the plot and
//!    inferred time from which one lit up — ~12px of resolution and a
//!    `cx.notify()` per strip boundary. Every notify re-rasterised an
//!    800×240 plotters canvas into a fresh sprite-atlas tile that is
//!    never reclaimed. That is the flicker.
//!
//! 3. **Two derivations of one geometry.** `render_trajectory_worm`
//!    computed the axis mapping internally; `trajectory_plot_bounds`
//!    re-derived it so the overlay could guess where the dots landed.
//!
//! # This file paints. It does not decide.
//!
//! All geometry lives in [`fermi_console::plot::trajectory`], in the
//! lib target, because the bin target cannot be compiled under `--test`
//! (rustc overflows its stack on GPUI's element chains). The
//! round-trip property that makes the chart trustworthy — point at a
//! pixel, get back the value painted there — is asserted over there.

use gpui::{
    canvas, div, px, App, Bounds, IntoElement, ParentElement, Pixels, RenderOnce, Styled, Window,
};

use fermi_console::plot::{
    events::interpolate,
    format,
    trajectory::{TrajectorySpec, CONSEQUENTIAL_PP, LANES, RUG_H},
};

// Re-exported so the cockpit builds its data with one import.
pub use fermi_console::plot::trajectory::{Event, EventKind, Point, Probe, TrajectoryData};

use super::paint::{self, Align};
use super::PlotSurface;

const FG: u32 = 0xCBCCC6;
const FG_DIM: u32 = 0x5C6773;
const FG_FAINT: u32 = 0x3E4B59;
const CYAN: u32 = 0x5CCFE6;
const GREEN: u32 = 0xBAE67E;
const GOLD: u32 = 0xFFCC66;
const ORANGE: u32 = 0xFFAE57;
const PURPLE: u32 = 0xD4BFFF;

fn kind_color(k: EventKind) -> u32 {
    match k {
        EventKind::RateRevision => CYAN,
        EventKind::BayesOpsFit => ORANGE,
        EventKind::AgentRun => FG_DIM,
        EventKind::MarketObservation => PURPLE,
    }
}

#[derive(IntoElement)]
pub struct TrajectoryPlot {
    spec: TrajectorySpec,
    /// Element-local pixel x of the scrub cursor.
    cursor_x: Option<f32>,
    /// Event highlighted from *outside* the chart — the list→chart half
    /// of brushing.
    selected_event: Option<usize>,
    surface: Option<PlotSurface>,
}

impl TrajectoryPlot {
    pub fn new(data: TrajectoryData) -> Self {
        Self {
            spec: TrajectorySpec::new(data, 800.0, 260.0),
            cursor_x: None,
            selected_event: None,
            surface: None,
        }
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.spec.width = width as f64;
        self.spec.height = height as f64;
        self
    }

    pub fn cursor_x(mut self, x: Option<f32>) -> Self {
        self.cursor_x = x;
        self
    }

    pub fn selected_event(mut self, idx: Option<usize>) -> Self {
        self.selected_event = idx;
        self
    }

    /// Restrict the visible time window.
    ///
    /// Not yet driven from the UI — wiring it up means adding a
    /// drag-to-brush gesture and a `zoom: Option<(f64, f64)>` field on
    /// `CockpitState`. The geometry side is done and tested
    /// (`plot::trajectory::tests::zoom_narrows_the_domain_*`), so this
    /// is a gesture-handler away from working.
    #[allow(dead_code)]
    pub fn zoom(mut self, window: Option<(f64, f64)>) -> Self {
        self.spec = self.spec.clone().zoom(window);
        self
    }

    pub fn surface(mut self, s: PlotSurface) -> Self {
        self.surface = Some(s);
        self
    }

    /// Interpret an element-local pixel position as a question about
    /// the data. Delegates to the tested geometry.
    pub fn probe(&self, local_x: f64, local_y: f64) -> Option<Probe> {
        self.spec.probe(local_x, local_y)
    }
}

impl RenderOnce for TrajectoryPlot {
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

impl TrajectoryPlot {
    fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let origin = bounds.origin;
        let f = self.spec.frame();
        let plot = f.plot;
        let d = &self.spec.data;

        if d.model.is_empty() && d.crowd.is_empty() && d.events.is_empty() {
            paint::label(
                window,
                cx,
                origin,
                plot.left + plot.width() / 2.0,
                plot.top + plot.height() / 2.0,
                "no trajectory yet — run an agent or accept a suggestion to begin",
                11.0,
                paint::hsla(FG_DIM, 1.0),
                Align::Center,
            );
            return;
        }

        // ── Gridlines + y labels ────────────────────────────────────
        //
        // Horizontal only. Vertical gridlines on a time axis compete
        // with the event stems for the same visual channel, and the
        // stems carry more information.
        for v in f.y.ticks(4) {
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
                paint::hsla(FG_FAINT, 0.5),
                1.0,
                None,
            );
            paint::label(
                window,
                cx,
                origin,
                plot.left - 6.0,
                y - 6.0,
                format!("{:.0}%", v),
                9.0,
                paint::hsla(FG_DIM, 1.0),
                Align::End,
            );
        }

        // ── x labels ────────────────────────────────────────────────
        let (x0, x1) = f.x.domain();
        let span = x1 - x0;
        for t in f.x.ticks(5) {
            let x = f.x.map(t);
            if x < plot.left - 1.0 || x > plot.right + 1.0 {
                continue;
            }
            paint::label(
                window,
                cx,
                origin,
                x,
                plot.bottom + RUG_H + 2.0,
                format::tick_time(t, x0, span, d.epoch),
                9.0,
                paint::hsla(FG_DIM, 1.0),
                Align::Center,
            );
        }

        let model = self.spec.model_series();
        let crowd = self.spec.crowd_series();

        // ── Divergence band ─────────────────────────────────────────
        //
        // Only across the span where both series exist. Painting it
        // beyond that would shade a gap between a real curve and a
        // clamped endpoint — a gap that isn't there.
        let grid = fermi_console::plot::events::common_grid(&model, &crowd);
        if grid.len() >= 2 {
            let top: Vec<(f64, f64)> = grid.iter().map(|(t, m, _)| f.point(*t, *m)).collect();
            let bot: Vec<(f64, f64)> = grid.iter().map(|(t, _, c)| f.point(*t, *c)).collect();
            paint::area_between(window, origin, &top, &bot, paint::hsla(CYAN, 0.10));
        }

        // ── Base rate ───────────────────────────────────────────────
        if let Some(b) = d.base_rate_pct {
            let y = f.y.map(b);
            if y >= plot.top && y <= plot.bottom {
                paint::rule_h(
                    window,
                    origin,
                    plot.left,
                    plot.right,
                    y,
                    paint::hsla(GOLD, 0.7),
                    1.0,
                    Some([4.0, 4.0]),
                );
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right + 4.0,
                    y - 6.0,
                    "base",
                    9.0,
                    paint::hsla(GOLD, 0.9),
                    Align::Start,
                );
            }
        }

        // ── Crowd worm (or its flat fallback) ───────────────────────
        if crowd.len() >= 2 {
            let pts: Vec<(f64, f64)> = crowd.iter().map(|(t, v)| f.point(*t, *v)).collect();
            paint::polyline(window, origin, &pts, paint::hsla(PURPLE, 0.30), 5.0);
            paint::polyline(window, origin, &pts, paint::hsla(PURPLE, 1.0), 2.0);
            if let Some(last) = pts.last() {
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right + 4.0,
                    last.1 - 6.0,
                    "crowd",
                    9.0,
                    paint::hsla(PURPLE, 0.9),
                    Align::Start,
                );
            }
        } else if let Some(c) = d.crowd_price_pct {
            paint::rule_h(
                window,
                origin,
                plot.left,
                plot.right,
                f.y.map_clamped(c),
                paint::hsla(PURPLE, 0.8),
                2.0,
                None,
            );
        }

        // ── Model worm ──────────────────────────────────────────────
        //
        // Painted after the crowd so the operator's own view dominates.
        if model.len() >= 2 {
            let pts: Vec<(f64, f64)> = model.iter().map(|(t, v)| f.point(*t, *v)).collect();
            paint::polyline(window, origin, &pts, paint::hsla(CYAN, 0.30), 5.0);
            paint::polyline(window, origin, &pts, paint::hsla(CYAN, 1.0), 2.0);
            if let Some(last) = pts.last() {
                paint::label(
                    window,
                    cx,
                    origin,
                    plot.right + 4.0,
                    last.1 - 6.0,
                    "model",
                    9.0,
                    paint::hsla(CYAN, 1.0),
                    Align::Start,
                );
            }
        }

        // ── Resolution cap ──────────────────────────────────────────
        if let Some(t) = d.resolved_at {
            if t >= x0 && t <= x1 {
                let x = f.x.map(t);
                paint::rule_v(
                    window,
                    origin,
                    x,
                    plot.top,
                    plot.bottom,
                    paint::hsla(GREEN, 0.9),
                    1.5,
                    None,
                );
                paint::label(
                    window,
                    cx,
                    origin,
                    x - 3.0,
                    plot.top,
                    "resolved",
                    9.0,
                    paint::hsla(GREEN, 1.0),
                    Align::End,
                );
            }
        }

        // ── Events ──────────────────────────────────────────────────
        //
        // The correlation layer. Three independent visual channels so
        // none has to do two jobs: colour = kind, size = consequence,
        // ring = selection.
        let lane_h = RUG_H / LANES as f64;
        let rug_top = plot.bottom + 2.0;
        for e in self.spec.correlated(&f) {
            if e.t < x0 || e.t > x1 {
                continue;
            }
            let color = kind_color(d.events[e.index].kind);
            let x = f.x.map(e.t);
            let y = f.y.map_clamped(e.y);
            let consequential = e.is_consequential(CONSEQUENTIAL_PP);
            let selected = self.selected_event == Some(e.index);

            // Stem: marker → rug lane. Faint, so a screenful reads as
            // texture rather than as a fence.
            let lane_y = rug_top + (e.lane as f64) * lane_h;
            paint::rule_v(
                window,
                origin,
                x,
                y,
                lane_y,
                paint::hsla(color, if selected { 0.7 } else { 0.22 }),
                1.0,
                None,
            );
            paint::rect(
                window,
                origin,
                x - 1.0,
                lane_y,
                x + 1.0,
                lane_y + lane_h - 2.0,
                paint::hsla(color, if selected { 1.0 } else { 0.6 }),
            );

            let r = if consequential { 4.0 } else { 2.5 };
            paint::dot(window, origin, x, y, r, paint::hsla(color, 1.0));
            if selected {
                paint::ring(window, origin, x, y, r + 3.5, paint::hsla(FG, 1.0), 1.5);
            }

            if consequential {
                if let Some(delta) = e.delta {
                    let up = delta > 0.0;
                    paint::label(
                        window,
                        cx,
                        origin,
                        x,
                        if up { y - 16.0 } else { y + 6.0 },
                        format!("{}{:.1}", if up { "▲+" } else { "▼" }, delta),
                        9.0,
                        paint::hsla(if up { GREEN } else { ORANGE }, 1.0),
                        Align::Center,
                    );
                }
            }
        }

        // ── Scrub cursor ────────────────────────────────────────────
        //
        // Continuous, and reads out in place — where the eye already
        // is, rather than in a pill across the panel.
        if let Some(cursor) = self.cursor_x {
            let lx = cursor as f64;
            if lx >= plot.left && lx <= plot.right {
                let t = f.x.invert(lx);
                paint::rule_v(
                    window,
                    origin,
                    lx,
                    plot.top,
                    plot.bottom + RUG_H,
                    paint::hsla(FG_DIM, 0.8),
                    1.0,
                    None,
                );

                let m = (!model.is_empty())
                    .then(|| interpolate(&model, t))
                    .flatten();
                let c = (!crowd.is_empty())
                    .then(|| interpolate(&crowd, t))
                    .flatten();

                // Dots ON the curves, so the value is read off the line
                // the operator already trusts.
                if let Some(mv) = m {
                    paint::dot(
                        window,
                        origin,
                        lx,
                        f.y.map_clamped(mv),
                        3.5,
                        paint::hsla(CYAN, 1.0),
                    );
                }
                if let Some(cv) = c {
                    paint::dot(
                        window,
                        origin,
                        lx,
                        f.y.map_clamped(cv),
                        3.5,
                        paint::hsla(PURPLE, 1.0),
                    );
                }

                // Readout flips side near the right edge so it never
                // runs off the plot.
                let right_side = lx > plot.left + plot.width() * 0.6;
                let (tx, align) = if right_side {
                    (lx - 8.0, Align::End)
                } else {
                    (lx + 8.0, Align::Start)
                };
                let mut row = plot.top + 2.0;
                paint::label(
                    window,
                    cx,
                    origin,
                    tx,
                    row,
                    format::cursor_time(t, d.epoch),
                    10.0,
                    paint::hsla(FG, 1.0),
                    align,
                );
                if let Some(mv) = m {
                    row += 13.0;
                    paint::label(
                        window,
                        cx,
                        origin,
                        tx,
                        row,
                        format!("model {:.1}%", mv),
                        10.0,
                        paint::hsla(CYAN, 1.0),
                        align,
                    );
                }
                if let Some(cv) = c {
                    row += 13.0;
                    paint::label(
                        window,
                        cx,
                        origin,
                        tx,
                        row,
                        format!("crowd {:.1}%", cv),
                        10.0,
                        paint::hsla(PURPLE, 1.0),
                        align,
                    );
                }
                if let (Some(mv), Some(cv)) = (m, c) {
                    row += 13.0;
                    let dv = mv - cv;
                    paint::label(
                        window,
                        cx,
                        origin,
                        tx,
                        row,
                        format!("edge {:+.1}pp", dv),
                        10.0,
                        paint::hsla(if dv.abs() < 3.0 { FG_DIM } else { GREEN }, 1.0),
                        align,
                    );
                }
            }
        }
    }
}
