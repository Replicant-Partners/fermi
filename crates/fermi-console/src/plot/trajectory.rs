//! Trajectory chart geometry — everything about the chart except the
//! ink.
//!
//! # Why this is separate from the painting
//!
//! The binary target cannot be tested: rustc overflows its stack
//! expanding GPUI's element chains under `--test` (see `src/lib.rs`).
//! So any logic that lives next to `window.paint_path` is logic nobody
//! can assert on.
//!
//! That matters more here than anywhere else in the console, because
//! the trajectory chart's central correctness property is a *round
//! trip*: the pixel you point at must invert back to the value you were
//! shown. The previous implementation had no way to check that — it
//! derived its geometry twice, once inside the plotters renderer and
//! once in a hand-written `trajectory_plot_bounds`, and the two drifted
//! apart whenever a margin changed. Nobody noticed until the tooltips
//! were visibly off the dots.
//!
//! [`TrajectorySpec`] owns the geometry. `viz::trajectory` owns the
//! paint calls and delegates every question about *where* to here.

use super::events::{correlate, interpolate, nearest_event, Correlated};
use super::frame::{Frame, Margins};
use super::scale::extent;

/// Height reserved beneath the plot for the event rug.
pub const RUG_H: f64 = 16.0;
/// Minimum horizontal gap between two event stems before they get
/// pushed into different rug lanes.
pub const LANE_SPACING_PX: f64 = 26.0;
pub const LANES: usize = 3;
/// A move smaller than this (in percentage points) isn't worth an
/// annotation — it's re-simulation noise, not news.
pub const CONSEQUENTIAL_PP: f64 = 0.75;
/// How close (in pixels) the cursor must be to latch onto an event.
pub const EVENT_LATCH_PX: f64 = 12.0;

/// What kind of thing happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    RateRevision,
    BayesOpsFit,
    AgentRun,
    MarketObservation,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::RateRevision => "revision",
            EventKind::BayesOpsFit => "refit",
            EventKind::AgentRun => "agent",
            EventKind::MarketObservation => "market",
        }
    }

    /// Whether this kind of event carries its own resulting rate.
    ///
    /// Only a rate revision does. Pinning the others to an explicit y
    /// implies they *set* a value they merely happened alongside —
    /// which is exactly what made the old event overlay decorative.
    pub fn carries_own_rate(self) -> bool {
        matches!(self, EventKind::RateRevision)
    }
}

/// One point on a worm. `t` is seconds since the trajectory's epoch,
/// `pct` is a probability on the 0–100 scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub t: f64,
    pub pct: f64,
}

/// An event to overlay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Event {
    pub t: f64,
    /// Explicit y, when the event carries its own resulting rate.
    pub pct: Option<f64>,
    pub kind: EventKind,
}

/// Everything the chart needs, and nothing about how it's drawn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrajectoryData {
    pub model: Vec<Point>,
    pub crowd: Vec<Point>,
    pub events: Vec<Event>,
    pub base_rate_pct: Option<f64>,
    /// Flat crowd price, used only when `crowd` has no history yet.
    pub crowd_price_pct: Option<f64>,
    pub resolved_at: Option<f64>,
    /// Wall-clock anchor for `t = 0`, so the x-axis can show calendar
    /// dates instead of "+27d".
    pub epoch: Option<chrono::DateTime<chrono::Utc>>,
}

/// The answer to "what is under the cursor?".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Probe {
    pub t: f64,
    pub model_pct: Option<f64>,
    pub crowd_pct: Option<f64>,
    /// Model minus crowd at `t` — the number the operator is actually
    /// trading on.
    pub edge_pp: Option<f64>,
    /// Index into `TrajectoryData::events` of the nearest event, when
    /// the cursor is close enough to latch onto one.
    pub event: Option<usize>,
}

/// Data plus display size plus zoom: enough to answer every geometric
/// question the painter and the hit-tester have.
#[derive(Debug, Clone, PartialEq)]
pub struct TrajectorySpec {
    pub data: TrajectoryData,
    pub width: f64,
    pub height: f64,
    /// Visible time window. `None` fits everything.
    pub zoom: Option<(f64, f64)>,
}

impl TrajectorySpec {
    pub fn new(data: TrajectoryData, width: f64, height: f64) -> Self {
        Self {
            data,
            width,
            height,
            zoom: None,
        }
    }

    pub fn zoom(mut self, window: Option<(f64, f64)>) -> Self {
        self.zoom = window;
        self
    }

    /// The single geometry, used by painter and hit-tester alike.
    pub fn frame(&self) -> Frame {
        let d = &self.data;
        let xs = d
            .model
            .iter()
            .chain(d.crowd.iter())
            .map(|p| p.t)
            .chain(d.events.iter().map(|e| e.t));
        let ys = d
            .model
            .iter()
            .chain(d.crowd.iter())
            .map(|p| p.pct)
            .chain(d.events.iter().filter_map(|e| e.pct))
            .chain(d.base_rate_pct)
            .chain(d.crowd_price_pct);

        let x_domain = self.zoom.or_else(|| extent(xs)).unwrap_or((0.0, 60.0));
        // A single-instant trajectory would collapse the x-axis; give
        // it a minute of width so the worm has somewhere to be.
        let x_domain = if x_domain.1 - x_domain.0 < 1.0 {
            (x_domain.0, x_domain.0 + 60.0)
        } else {
            x_domain
        };
        let y_domain = extent(ys).unwrap_or((0.0, 100.0));

        // The rug strip is carved out of the bottom margin.
        let m = Margins::new(12.0, 56.0, 20.0 + RUG_H, 44.0);
        let mut f = Frame::new(self.width, self.height, m, x_domain, y_domain);
        f.y = f.y.padded(0.10, Some(0.0));
        f
    }

    pub fn model_series(&self) -> Vec<(f64, f64)> {
        self.data.model.iter().map(|p| (p.t, p.pct)).collect()
    }

    pub fn crowd_series(&self) -> Vec<(f64, f64)> {
        self.data.crowd.iter().map(|p| (p.t, p.pct)).collect()
    }

    /// Pixels per second at the current zoom — the conversion the
    /// label packer and the event latch both need.
    pub fn px_per_sec(&self, f: &Frame) -> f64 {
        let (x0, x1) = f.x.domain();
        f.plot.width() / (x1 - x0).max(1e-9)
    }

    /// Events with before/after deltas and rug-lane assignments.
    pub fn correlated(&self, f: &Frame) -> Vec<Correlated> {
        let model = self.model_series();
        let times: Vec<f64> = self.data.events.iter().map(|e| e.t).collect();
        let ys: Vec<Option<f64>> = self.data.events.iter().map(|e| e.pct).collect();
        // Sample far enough either side to straddle a step change but
        // not so far that a neighbouring event's effect leaks in. 1% of
        // the visible span is a cheap stand-in for half the median
        // inter-event gap.
        let (x0, x1) = f.x.domain();
        let window = ((x1 - x0) * 0.01).max(1.0);
        correlate(
            &model,
            &times,
            &ys,
            window,
            LANE_SPACING_PX,
            self.px_per_sec(f),
            LANES,
        )
    }

    /// Interpret an element-local pixel position as a question about
    /// the data. The inverse direction the bitmap pipeline never had.
    pub fn probe(&self, local_x: f64, local_y: f64) -> Option<Probe> {
        let f = self.frame();
        let t = f.hover_x(local_x, local_y)?;
        let model = self.model_series();
        let crowd = self.crowd_series();
        let model_pct = (!model.is_empty()).then(|| interpolate(&model, t)).flatten();
        let crowd_pct = (!crowd.is_empty()).then(|| interpolate(&crowd, t)).flatten();
        let corr = self.correlated(&f);
        Some(Probe {
            t,
            model_pct,
            crowd_pct,
            edge_pp: model_pct.zip(crowd_pct).map(|(m, c)| m - c),
            event: nearest_event(&corr, t, self.px_per_sec(&f), EVENT_LATCH_PX)
                .map(|e| e.index),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> TrajectoryData {
        TrajectoryData {
            model: vec![
                Point { t: 0.0, pct: 10.0 },
                Point { t: 99.0, pct: 10.0 },
                Point { t: 101.0, pct: 20.0 },
                Point { t: 200.0, pct: 20.0 },
            ],
            crowd: vec![
                Point { t: 0.0, pct: 15.0 },
                Point { t: 200.0, pct: 15.0 },
            ],
            events: vec![
                Event {
                    t: 100.0,
                    pct: None,
                    kind: EventKind::RateRevision,
                },
                Event {
                    t: 160.0,
                    pct: None,
                    kind: EventKind::AgentRun,
                },
            ],
            base_rate_pct: Some(12.0),
            crowd_price_pct: None,
            resolved_at: None,
            epoch: None,
        }
    }

    fn spec() -> TrajectorySpec {
        TrajectorySpec::new(data(), 800.0, 260.0)
    }

    /// The property the old two-derivations-of-one-geometry design
    /// could not guarantee: point at a pixel, get back the value that
    /// was painted there.
    #[test]
    fn probing_a_pixel_returns_the_value_painted_at_it() {
        let s = spec();
        let f = s.frame();
        for t in [10.0, 50.0, 150.0, 199.0] {
            let (px, _) = f.point(t, 20.0);
            let p = s.probe(px, f.plot.top + 10.0).unwrap();
            assert!((p.t - t).abs() < 1e-6, "drift at t={t}: got {}", p.t);
        }
    }

    #[test]
    fn probe_reads_both_series_and_their_edge() {
        let s = spec();
        let f = s.frame();
        let (px, _) = f.point(150.0, 20.0);
        let p = s.probe(px, f.plot.top + 10.0).unwrap();
        assert!((p.model_pct.unwrap() - 20.0).abs() < 1e-9);
        assert!((p.crowd_pct.unwrap() - 15.0).abs() < 1e-9);
        assert!((p.edge_pp.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn edge_is_absent_when_either_series_is() {
        let mut d = data();
        d.crowd.clear();
        let s = TrajectorySpec::new(d, 800.0, 260.0);
        let f = s.frame();
        let (px, _) = f.point(150.0, 20.0);
        let p = s.probe(px, f.plot.top + 10.0).unwrap();
        assert!(p.model_pct.is_some());
        assert!(p.crowd_pct.is_none());
        assert!(p.edge_pp.is_none());
    }

    #[test]
    fn probing_outside_the_plot_rect_returns_nothing() {
        let s = spec();
        assert!(s.probe(2.0, 2.0).is_none(), "left gutter");
        assert!(s.probe(10_000.0, 100.0).is_none(), "past the right edge");
        assert!(s.probe(400.0, 259.0).is_none(), "down in the rug/labels");
    }

    #[test]
    fn the_cursor_latches_onto_a_nearby_event_and_releases_between_them() {
        let s = spec();
        let f = s.frame();
        let (ex, _) = f.point(100.0, 15.0);
        assert_eq!(s.probe(ex, f.plot.top + 10.0).unwrap().event, Some(0));

        let (mx, _) = f.point(130.0, 15.0);
        assert_eq!(
            s.probe(mx, f.plot.top + 10.0).unwrap().event,
            None,
            "midway between events should latch onto neither"
        );
    }

    #[test]
    fn events_report_the_movement_they_caused() {
        let s = spec();
        let corr = s.correlated(&s.frame());

        let revision = corr.iter().find(|c| c.index == 0).unwrap();
        assert!(
            revision.delta.unwrap() > 5.0,
            "the t=100 step should register as a large move, got {:?}",
            revision.delta
        );
        assert!(revision.is_consequential(CONSEQUENTIAL_PP));

        let quiet = corr.iter().find(|c| c.index == 1).unwrap();
        assert!(quiet.delta.unwrap().abs() < 0.01);
        assert!(
            !quiet.is_consequential(CONSEQUENTIAL_PP),
            "an event that moved nothing must not be annotated as if it did"
        );
    }

    #[test]
    fn only_rate_revisions_claim_to_carry_their_own_rate() {
        assert!(EventKind::RateRevision.carries_own_rate());
        for k in [
            EventKind::AgentRun,
            EventKind::BayesOpsFit,
            EventKind::MarketObservation,
        ] {
            assert!(!k.carries_own_rate(), "{:?}", k.label());
        }
    }

    #[test]
    fn zoom_narrows_the_domain_without_moving_the_plot_rect() {
        let full = spec();
        let zoomed = spec().zoom(Some((100.0, 150.0)));
        assert_eq!(zoomed.frame().plot, full.frame().plot);

        let zf = zoomed.frame();
        assert!((zf.x.domain().0 - 100.0).abs() < 1e-9);
        assert!((zf.x.domain().1 - 150.0).abs() < 1e-9);

        // The round-trip property must survive zooming.
        let (px, _) = zf.point(125.0, 20.0);
        assert!((zoomed.probe(px, zf.plot.top + 5.0).unwrap().t - 125.0).abs() < 1e-6);
    }

    #[test]
    fn zooming_in_separates_events_that_shared_a_rug_lane() {
        let mut d = data();
        d.events = vec![
            Event {
                t: 100.0,
                pct: None,
                kind: EventKind::AgentRun,
            },
            Event {
                t: 102.0,
                pct: None,
                kind: EventKind::AgentRun,
            },
        ];
        // Zoomed out over 200s, 2s apart is ~7px — they collide.
        let wide = TrajectorySpec::new(d.clone(), 800.0, 260.0);
        let wf = wide.frame();
        let wl: Vec<usize> = wide.correlated(&wf).iter().map(|c| c.lane).collect();
        assert_ne!(wl[0], wl[1], "should be stacked when crowded");

        // Zoomed into a 10s window, 2s is ~140px — plenty of room.
        let tight = TrajectorySpec::new(d, 800.0, 260.0).zoom(Some((98.0, 108.0)));
        let tf = tight.frame();
        let tl: Vec<usize> = tight.correlated(&tf).iter().map(|c| c.lane).collect();
        assert_eq!(tl[0], tl[1], "should share a lane once there's room");
    }

    #[test]
    fn empty_data_still_yields_a_usable_frame() {
        let s = TrajectorySpec::new(TrajectoryData::default(), 800.0, 260.0);
        let f = s.frame();
        assert!(f.plot.width() > 0.0 && f.plot.height() > 0.0);
        let p = s.probe(f.plot.left + 1.0, f.plot.top + 1.0).unwrap();
        assert!(p.model_pct.is_none() && p.crowd_pct.is_none());
        assert!(p.event.is_none());
    }

    #[test]
    fn a_single_instant_trajectory_does_not_collapse_the_axis() {
        let mut d = TrajectoryData::default();
        d.model = vec![Point { t: 42.0, pct: 5.0 }];
        let f = TrajectorySpec::new(d, 800.0, 260.0).frame();
        assert!(f.x.domain().1 > f.x.domain().0);
        assert!(f.x.map(42.0).is_finite());
    }

    #[test]
    fn the_y_axis_never_implies_a_negative_probability() {
        let mut d = data();
        d.model = vec![
            Point { t: 0.0, pct: 0.4 },
            Point { t: 100.0, pct: 0.6 },
        ];
        d.crowd.clear();
        d.base_rate_pct = None;
        d.crowd_price_pct = None;
        let f = TrajectorySpec::new(d, 800.0, 260.0).frame();
        assert!(f.y.domain().0 >= 0.0, "got {}", f.y.domain().0);
    }

    #[test]
    fn a_tiny_chart_does_not_produce_an_inverted_plot_rect() {
        let s = TrajectorySpec::new(data(), 60.0, 30.0);
        let f = s.frame();
        assert!(f.plot.right > f.plot.left);
        assert!(f.plot.bottom > f.plot.top);
        assert!(s.px_per_sec(&f).is_finite());
    }
}
