//! `viz` — vector chart elements painted directly into the GPUI scene.
//!
//! # Relationship to `charts.rs`
//!
//! `charts.rs` is the legacy plotters→bitmap pipeline. This module is
//! its replacement, migrated chart by chart. The two coexist during the
//! transition; see `docs/fermi/VISUALIZATION_ARCHITECTURE.md` for the
//! running status and the rationale.
//!
//! # The two halves
//!
//! Geometry and statistics live in the **lib** target under
//! `fermi_console::plot` where they're testable. This module owns only
//! the `gpui` side: turning a `plot::Frame` into paths, and turning
//! mouse positions back into data.
//!
//! # Direct manipulation
//!
//! The interaction model these elements are built for is
//! Victor-flavoured: the chart is not a picture of the data, it is a
//! *handle on* the data.
//!
//! * Every element publishes its [`PlotSurface`], which records the
//!   element's window-space bounds during prepaint. Mouse handlers use
//!   it to invert screen coordinates back into data coordinates
//!   **through the same `Frame` the painter used** — so a value you
//!   point at is the value you were shown.
//! * Hover is continuous, not quantised. The trajectory chart's 60
//!   invisible hit-strips exist only because the bitmap couldn't
//!   report where the mouse was; a real element can.
//! * Readouts are in-place: the number appears where the eye already
//!   is, not in a legend across the panel.

pub mod distribution;
pub mod paint;
pub mod trajectory;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{canvas, Bounds, IntoElement, Pixels, Point, Styled};

/// Shared record of where an element actually landed on screen.
///
/// GPUI hands mouse events window-space coordinates, but a chart thinks
/// in element-local ones. Layout decides the offset between them, and
/// layout only happens during paint — so the paint pass writes the
/// bounds here and the event handlers read them back.
///
/// This is the mechanism that makes hit-testing *derived from* the
/// render rather than a parallel re-implementation of it. The old
/// `charts::trajectory_plot_bounds` was the parallel re-implementation,
/// and it drifted out of sync with the renderer every time a margin
/// changed.
#[derive(Clone, Default)]
pub struct PlotSurface {
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl PlotSurface {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called from the paint pass with the element's window-space bounds.
    pub fn record(&self, bounds: Bounds<Pixels>) {
        self.bounds.set(Some(bounds));
    }

    /// An invisible element that records this surface's bounds during
    /// paint.
    ///
    /// Drop it into any container whose coordinate space you need to
    /// invert mouse positions into — including containers built from
    /// ordinary `div`s rather than a chart canvas. It paints nothing;
    /// it exists purely so the geometry the layout engine chose is
    /// available to event handlers, which otherwise only ever see
    /// window-space coordinates.
    ///
    /// Position it absolutely and give it the parent's full size.
    pub fn tracker(&self) -> impl IntoElement {
        let s = self.clone();
        canvas(
            move |bounds: Bounds<Pixels>, _window, _cx| s.record(bounds),
            |_bounds, _: (), _window, _cx| {},
        )
        .absolute()
        .size_full()
    }

    /// The recorded bounds, for callers that need the raw rect.
    #[allow(dead_code)]
    pub fn bounds(&self) -> Option<Bounds<Pixels>> {
        self.bounds.get()
    }

    /// Window-space point → element-local `(x, y)` in `f64` pixels.
    ///
    /// Returns `None` before the first paint (no bounds recorded yet)
    /// or when the point is outside the element. Callers that want
    /// "clamp to edge" behaviour during a drag should use
    /// [`PlotSurface::local_unclamped`].
    pub fn local(&self, p: Point<Pixels>) -> Option<(f64, f64)> {
        let b = self.bounds.get()?;
        let x = (p.x - b.origin.x).to_f64();
        let y = (p.y - b.origin.y).to_f64();
        let (w, h) = (b.size.width.to_f64(), b.size.height.to_f64());
        (x >= 0.0 && x <= w && y >= 0.0 && y <= h).then_some((x, y))
    }

    /// Same, but without the containment test — during a drag the
    /// pointer routinely leaves the element and the gesture should keep
    /// tracking rather than snapping away.
    ///
    /// Unused until the first drag gesture lands (threshold handle,
    /// time brush); [`PlotSurface::local`] is what hover uses.
    #[allow(dead_code)]
    pub fn local_unclamped(&self, p: Point<Pixels>) -> Option<(f64, f64)> {
        let b = self.bounds.get()?;
        Some(((p.x - b.origin.x).to_f64(), (p.y - b.origin.y).to_f64()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{px, size, Bounds};

    fn surface() -> PlotSurface {
        let s = PlotSurface::new();
        s.record(Bounds {
            origin: Point {
                x: px(100.0),
                y: px(50.0),
            },
            size: size(px(800.0), px(240.0)),
        });
        s
    }

    #[test]
    fn reports_nothing_before_the_first_paint() {
        let s = PlotSurface::new();
        assert!(s.bounds().is_none());
        assert!(s
            .local(Point {
                x: px(0.0),
                y: px(0.0)
            })
            .is_none());
    }

    #[test]
    fn window_space_is_translated_to_element_local() {
        let s = surface();
        let got = s
            .local(Point {
                x: px(300.0),
                y: px(90.0),
            })
            .unwrap();
        assert!((got.0 - 200.0).abs() < 1e-6);
        assert!((got.1 - 40.0).abs() < 1e-6);
    }

    #[test]
    fn points_outside_the_element_are_rejected() {
        let s = surface();
        // Left of the element.
        assert!(s
            .local(Point {
                x: px(10.0),
                y: px(90.0)
            })
            .is_none());
        // Below it.
        assert!(s
            .local(Point {
                x: px(300.0),
                y: px(999.0)
            })
            .is_none());
    }

    #[test]
    fn drag_tracking_keeps_reporting_outside_the_element() {
        let s = surface();
        let got = s
            .local_unclamped(Point {
                x: px(10.0),
                y: px(90.0),
            })
            .unwrap();
        assert!((got.0 - -90.0).abs() < 1e-6, "got {}", got.0);
    }
}
