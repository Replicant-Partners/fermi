//! Vector paint primitives — ink straight onto the GPU, no bitmaps.
//!
//! # Why this replaces the plotters pipeline
//!
//! Every chart in `charts.rs` rasterises with plotters into an RGB
//! `Vec<u8>`, converts it to RGBA, and wraps it in a fresh
//! `Arc<RenderImage>`. That last step is the flicker:
//!
//! * `RenderImage::new` mints a **new `ImageId`** from a global counter
//!   on every call (`gpui-0.2.2/src/assets.rs:61`).
//! * `Window::paint_image` inserts into the sprite atlas keyed by that
//!   id (`window.rs:3143`), so a new id means a brand-new atlas tile.
//! * `ImageSource::Render` is explicitly excluded from asset cleanup —
//!   `remove_asset` matches it to `{}` (`elements/img.rs:561`).
//!
//! So each render leaks an 800×240 tile (≈750 KB) into the atlas, which
//! never gets reclaimed. Since the trajectory view calls `cx.notify()`
//! on *every* hover-strip transition — 60 of them across the plot — a
//! single mouse sweep allocates ~45 MB of atlas and forces repeated
//! atlas growth/reallocation. The visible symptom is the flicker.
//!
//! Painting vectors sidesteps the atlas entirely: paths are tessellated
//! into the scene each frame and cost nothing to re-emit. It also fixes
//! two quieter problems — bitmaps were rasterised at logical pixel size
//! and stretched on HiDPI (blurry), and the CPU rasterised 192,000
//! pixels per mouse move (janky).
//!
//! # Coordinate convention
//!
//! Every function takes `origin` (the element's window-space bounds
//! origin) plus **plot-local** `f64` coordinates, matching what
//! `plot::Frame` produces. Callers never do window-space arithmetic,
//! which is what keeps painting and hit-testing in the same space.

use gpui::{
    px, App, Background, Bounds, Hsla, PathBuilder, Pixels, Point, SharedString, TextRun, Window,
};

/// Convert plot-local coordinates to a window-space point.
#[inline]
pub fn at(origin: Point<Pixels>, x: f64, y: f64) -> Point<Pixels> {
    Point {
        x: origin.x + px(x as f32),
        y: origin.y + px(y as f32),
    }
}

/// Guard against the degenerate paths lyon refuses to tessellate.
/// Returns the subset of points that are finite, since a single NaN
/// (an empty series, a divide-by-zero in a scale) would otherwise
/// silently drop the whole path.
fn finite(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    pts.iter()
        .copied()
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect()
}

/// A connected line through `pts`.
pub fn polyline(
    window: &mut Window,
    origin: Point<Pixels>,
    pts: &[(f64, f64)],
    color: Hsla,
    width: f32,
) {
    let pts = finite(pts);
    if pts.len() < 2 {
        return;
    }
    let mut b = PathBuilder::stroke(px(width));
    b.move_to(at(origin, pts[0].0, pts[0].1));
    for (x, y) in &pts[1..] {
        b.line_to(at(origin, *x, *y));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// A dashed line through `pts`. `dash` is `[on, off]` in pixels.
pub fn polyline_dashed(
    window: &mut Window,
    origin: Point<Pixels>,
    pts: &[(f64, f64)],
    color: Hsla,
    width: f32,
    dash: [f32; 2],
) {
    let pts = finite(pts);
    if pts.len() < 2 {
        return;
    }
    let mut b = PathBuilder::stroke(px(width)).dash_array(&[px(dash[0]), px(dash[1])]);
    b.move_to(at(origin, pts[0].0, pts[0].1));
    for (x, y) in &pts[1..] {
        b.line_to(at(origin, *x, *y));
    }
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Fill the region between two curves. `top` and `bottom` must be in
/// the same x-order; `bottom` is walked in reverse to close the ring.
///
/// This is what makes model-vs-crowd *divergence* a shape rather than a
/// gap the eye has to measure between two hairlines.
pub fn area_between(
    window: &mut Window,
    origin: Point<Pixels>,
    top: &[(f64, f64)],
    bottom: &[(f64, f64)],
    color: impl Into<Background>,
) {
    let top = finite(top);
    let bottom = finite(bottom);
    if top.len() < 2 || bottom.len() < 2 {
        return;
    }
    let mut b = PathBuilder::fill();
    b.move_to(at(origin, top[0].0, top[0].1));
    for (x, y) in &top[1..] {
        b.line_to(at(origin, *x, *y));
    }
    for (x, y) in bottom.iter().rev() {
        b.line_to(at(origin, *x, *y));
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Fill between a curve and a horizontal baseline — the density-curve
/// fill, and the histogram silhouette.
pub fn area_to_baseline(
    window: &mut Window,
    origin: Point<Pixels>,
    curve: &[(f64, f64)],
    baseline_y: f64,
    color: impl Into<Background>,
) {
    let curve = finite(curve);
    if curve.len() < 2 {
        return;
    }
    let mut b = PathBuilder::fill();
    let first = curve[0];
    let last = curve[curve.len() - 1];
    b.move_to(at(origin, first.0, baseline_y));
    for (x, y) in &curve {
        b.line_to(at(origin, *x, *y));
    }
    b.line_to(at(origin, last.0, baseline_y));
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Horizontal reference rule.
pub fn rule_h(
    window: &mut Window,
    origin: Point<Pixels>,
    x0: f64,
    x1: f64,
    y: f64,
    color: Hsla,
    width: f32,
    dash: Option<[f32; 2]>,
) {
    let pts = [(x0, y), (x1, y)];
    match dash {
        Some(d) => polyline_dashed(window, origin, &pts, color, width, d),
        None => polyline(window, origin, &pts, color, width),
    }
}

/// Vertical reference rule — the time cursor, the threshold handle, the
/// resolution marker.
pub fn rule_v(
    window: &mut Window,
    origin: Point<Pixels>,
    x: f64,
    y0: f64,
    y1: f64,
    color: Hsla,
    width: f32,
    dash: Option<[f32; 2]>,
) {
    let pts = [(x, y0), (x, y1)];
    match dash {
        Some(d) => polyline_dashed(window, origin, &pts, color, width, d),
        None => polyline(window, origin, &pts, color, width),
    }
}

/// Approximate a circle with a polygon. At the radii charts use (2–6px)
/// 20 segments is indistinguishable from a true arc and avoids the
/// cubic-bezier bookkeeping.
fn circle_pts(cx: f64, cy: f64, r: f64, segments: usize) -> Vec<(f64, f64)> {
    (0..segments)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / segments as f64;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Filled dot — an event marker, a data point on a worm.
pub fn dot(window: &mut Window, origin: Point<Pixels>, cx: f64, cy: f64, r: f64, color: Hsla) {
    if !cx.is_finite() || !cy.is_finite() || r <= 0.0 {
        return;
    }
    let pts = circle_pts(cx, cy, r, 20);
    let mut b = PathBuilder::fill();
    b.move_to(at(origin, pts[0].0, pts[0].1));
    for (x, y) in &pts[1..] {
        b.line_to(at(origin, *x, *y));
    }
    b.close();
    if let Ok(path) = b.build() {
        window.paint_path(path, color);
    }
}

/// Unfilled ring — used to halo a dot against a busy background, and to
/// mark the *selected* event without changing its colour (colour already
/// encodes event kind, so selection has to use a different channel).
pub fn ring(
    window: &mut Window,
    origin: Point<Pixels>,
    cx: f64,
    cy: f64,
    r: f64,
    color: Hsla,
    width: f32,
) {
    if !cx.is_finite() || !cy.is_finite() || r <= 0.0 {
        return;
    }
    let mut pts = circle_pts(cx, cy, r, 24);
    pts.push(pts[0]);
    polyline(window, origin, &pts, color, width);
}

/// Where a label sits relative to its anchor point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Paint a single line of text at a plot-local anchor.
///
/// Returns the shaped width so callers can lay out chips and avoid
/// collisions — the event-lane packer needs this.
pub fn label(
    window: &mut Window,
    cx_app: &mut App,
    origin: Point<Pixels>,
    x: f64,
    y: f64,
    text: impl Into<SharedString>,
    size: f32,
    color: Hsla,
    align: Align,
) -> f32 {
    // Returns the shaped width so callers can pack labels without
    // guessing at glyph metrics.
    let text: SharedString = text.into();
    if text.is_empty() {
        return 0.0;
    }
    let font = window.text_style().font();
    let run = TextRun {
        len: text.len(),
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(text, px(size), &[run], None);
    let w = shaped.width.to_f64();
    let dx = match align {
        Align::Start => 0.0,
        Align::Center => -w / 2.0,
        Align::End => -w,
    };
    // `y` is the text baseline-ish top; shift up by half the size so
    // callers can think in terms of "centre the label on this row".
    let _ = shaped.paint(at(origin, x + dx, y), px(size * 1.4), window, cx_app);
    w as f32
}

/// Solid rectangle, in plot-local coordinates.
pub fn rect(
    window: &mut Window,
    origin: Point<Pixels>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    color: impl Into<Background>,
) {
    if !(x1 > x0) || !(y1 > y0) {
        return;
    }
    let bounds = Bounds {
        origin: at(origin, x0, y0),
        size: gpui::size(px((x1 - x0) as f32), px((y1 - y0) as f32)),
    };
    window.paint_quad(gpui::fill(bounds, color));
}

/// Convert a `0xRRGGBB` theme constant into an `Hsla` with an alpha.
///
/// The theme module stores colours as `u32` for use with `gpui::rgb()`;
/// paint code needs `Hsla` so it can vary opacity per layer (a
/// divergence fill and its boundary line are the same hue at different
/// alphas, which is what makes them read as one object).
pub fn hsla(rgb_hex: u32, alpha: f32) -> Hsla {
    let c: Hsla = gpui::rgb(rgb_hex).into();
    c.alpha(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_filter_drops_nan_without_dropping_the_path() {
        let pts = [(0.0, 0.0), (f64::NAN, 1.0), (2.0, 2.0)];
        let f = finite(&pts);
        assert_eq!(f.len(), 2);
        assert_eq!(f[1], (2.0, 2.0));
    }

    #[test]
    fn circle_points_are_on_the_circle() {
        let pts = circle_pts(10.0, 20.0, 5.0, 20);
        assert_eq!(pts.len(), 20);
        for (x, y) in pts {
            let r = ((x - 10.0).powi(2) + (y - 20.0).powi(2)).sqrt();
            assert!((r - 5.0).abs() < 1e-9);
        }
    }
}
