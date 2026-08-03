//! `Frame` — the single source of truth for a plot's geometry.
//!
//! # Why this exists
//!
//! The trajectory chart used to derive its coordinate space in two
//! places: inside the bitmap renderer, and again inside
//! `trajectory_plot_bounds` so the hover overlay could guess where the
//! renderer had put things. Two derivations of one truth is a bug with
//! a delay fuse.
//!
//! A `Frame` is built **once** per render from the data and the
//! available size. The painter reads it to place ink; the hit-tester
//! reads it to interpret the mouse. They cannot disagree, because
//! there is only one of it.
//!
//! GPUI-free by construction (plain `f64`), so the geometry is testable
//! without a window.

use super::scale::{extent, LinearScale};

/// Space reserved outside the plot rect for axis labels and legends.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margins {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Margins {
    pub const fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Default for a chart with a y-axis in percent and a time x-axis.
    pub const AXES: Self = Self::new(10.0, 16.0, 22.0, 42.0);
    /// Default for a sparkline: no chrome at all.
    pub const BARE: Self = Self::new(1.0, 1.0, 1.0, 1.0);
}

/// An axis-aligned rectangle in local (element-relative) pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }

    /// Shrink from the bottom, returning `(remaining, carved)`. Used to
    /// reserve the event-rug strip beneath the trajectory plot.
    pub fn split_bottom(&self, h: f64) -> (Rect, Rect) {
        let h = h.min(self.height().max(0.0));
        (
            Rect {
                bottom: self.bottom - h,
                ..*self
            },
            Rect {
                top: self.bottom - h,
                ..*self
            },
        )
    }
}

/// A plot's complete geometry: where the ink goes, and how data maps
/// into it in both directions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub plot: Rect,
    pub x: LinearScale,
    pub y: LinearScale,
}

impl Frame {
    /// Build a frame from an element size, margins, and explicit
    /// domains.
    pub fn new(
        width: f64,
        height: f64,
        m: Margins,
        x_domain: (f64, f64),
        y_domain: (f64, f64),
    ) -> Self {
        // Never let margins invert the plot rect on a tiny element.
        let plot = Rect {
            left: m.left,
            top: m.top,
            right: (width - m.right).max(m.left + 1.0),
            bottom: (height - m.bottom).max(m.top + 1.0),
        };
        Self {
            x: LinearScale::new(x_domain, (plot.left, plot.right)),
            // Inverted: larger data values sit higher on screen.
            y: LinearScale::new(y_domain, (plot.bottom, plot.top)),
            plot,
        }
    }

    /// Build a frame whose domains are inferred from the data.
    ///
    /// `y_floor` clamps the padded lower bound — pass `Some(0.0)` for
    /// probabilities so the axis never implies a negative percentage.
    pub fn autoscale<X, Y>(
        width: f64,
        height: f64,
        m: Margins,
        xs: X,
        ys: Y,
        y_floor: Option<f64>,
    ) -> Self
    where
        X: IntoIterator<Item = f64>,
        Y: IntoIterator<Item = f64>,
    {
        let x_domain = extent(xs).unwrap_or((0.0, 1.0));
        let y_domain = extent(ys).unwrap_or((0.0, 1.0));
        let mut f = Self::new(width, height, m, x_domain, y_domain);
        f.y = f.y.padded(0.10, y_floor);
        f
    }

    /// Replace the x-domain (for zoom / brush) keeping everything else.
    pub fn with_x_domain(mut self, domain: (f64, f64)) -> Self {
        self.x = LinearScale::new(domain, (self.plot.left, self.plot.right));
        self
    }

    /// Replace the y-domain, keeping the inverted pixel orientation.
    pub fn with_y_domain(mut self, domain: (f64, f64)) -> Self {
        self.y = LinearScale::new(domain, (self.plot.bottom, self.plot.top));
        self
    }

    /// Data point → local pixel.
    pub fn point(&self, x: f64, y: f64) -> (f64, f64) {
        (self.x.map(x), self.y.map(y))
    }

    /// Local pixel → data point. The direct-manipulation direction.
    pub fn unpoint(&self, px: f64, py: f64) -> (f64, f64) {
        (self.x.invert(px), self.y.invert(py))
    }

    /// Local pixel x → data x, but only when the cursor is actually
    /// inside the plot rect. Returning `Option` here is what stops the
    /// hover readout from reporting phantom values when the mouse is
    /// out over the axis gutter.
    pub fn hover_x(&self, px: f64, py: f64) -> Option<f64> {
        self.plot.contains(px, py).then(|| self.x.invert(px))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn plot_rect_accounts_for_margins() {
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 10.0), (0.0, 100.0));
        assert!(close(f.plot.left, 42.0));
        assert!(close(f.plot.top, 10.0));
        assert!(close(f.plot.right, 784.0));
        assert!(close(f.plot.bottom, 218.0));
    }

    #[test]
    fn y_axis_is_inverted_so_bigger_is_higher() {
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 10.0), (0.0, 100.0));
        let (_, y_lo) = f.point(0.0, 0.0);
        let (_, y_hi) = f.point(0.0, 100.0);
        assert!(y_hi < y_lo, "100% should paint above 0%");
    }

    #[test]
    fn paint_and_hit_test_agree_exactly() {
        // This is the regression the old duplicated `trajectory_plot_bounds`
        // could not guarantee: a dot painted at t is hit-tested at t.
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 86400.0), (0.0, 100.0));
        for t in [0.0, 1234.0, 43200.0, 86400.0] {
            let (px, _) = f.point(t, 50.0);
            assert!(close(f.x.invert(px), t), "drift at t={t}");
        }
    }

    #[test]
    fn hover_outside_the_plot_rect_reports_nothing() {
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 10.0), (0.0, 100.0));
        assert!(f.hover_x(400.0, 100.0).is_some());
        // In the left axis gutter.
        assert!(f.hover_x(10.0, 100.0).is_none());
        // Down in the x-label gutter.
        assert!(f.hover_x(400.0, 235.0).is_none());
    }

    #[test]
    fn tiny_elements_do_not_produce_inverted_rects() {
        let f = Frame::new(20.0, 8.0, Margins::AXES, (0.0, 1.0), (0.0, 1.0));
        assert!(f.plot.right > f.plot.left);
        assert!(f.plot.bottom > f.plot.top);
    }

    #[test]
    fn autoscale_pads_and_respects_the_floor() {
        let f = Frame::autoscale(
            800.0,
            240.0,
            Margins::AXES,
            [0.0, 100.0],
            [2.0, 12.0],
            Some(0.0),
        );
        let (lo, hi) = f.y.domain();
        assert!(close(lo, 1.0), "lo was {lo}");
        assert!(close(hi, 13.0), "hi was {hi}");

        // With a tight-to-zero series the floor kicks in.
        let g = Frame::autoscale(
            800.0,
            240.0,
            Margins::AXES,
            [0.0, 1.0],
            [0.2, 1.0],
            Some(0.0),
        );
        assert!(g.y.domain().0 >= 0.0);
    }

    #[test]
    fn autoscale_survives_empty_data() {
        let f = Frame::autoscale(
            800.0,
            240.0,
            Margins::AXES,
            Vec::<f64>::new(),
            Vec::<f64>::new(),
            None,
        );
        assert!(f.point(0.5, 0.5).0.is_finite());
    }

    #[test]
    fn split_bottom_carves_the_rug_strip() {
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 1.0), (0.0, 1.0));
        let (upper, rug) = f.plot.split_bottom(14.0);
        assert!(close(rug.height(), 14.0));
        assert!(close(upper.bottom, rug.top));
        assert!(close(upper.height() + rug.height(), f.plot.height()));
    }

    #[test]
    fn split_bottom_cannot_overrun_the_rect() {
        let f = Frame::new(800.0, 30.0, Margins::BARE, (0.0, 1.0), (0.0, 1.0));
        let (upper, rug) = f.plot.split_bottom(9999.0);
        assert!(upper.height() >= 0.0);
        assert!(rug.height() <= f.plot.height() + 1e-9);
    }

    #[test]
    fn zooming_the_x_domain_keeps_the_pixel_range() {
        let f = Frame::new(800.0, 240.0, Margins::AXES, (0.0, 100.0), (0.0, 1.0));
        let z = f.with_x_domain((25.0, 75.0));
        assert_eq!(z.x.range(), f.x.range());
        assert!(close(z.x.map(25.0), f.plot.left));
        assert!(close(z.x.map(75.0), f.plot.right));
    }
}
