//! Scales — the pure mapping between *data space* and *screen space*.
//!
//! # Why this exists
//!
//! The old bitmap charts computed their data→pixel mapping **twice**:
//! once inside `charts::render_trajectory_worm` (via plotters'
//! `build_cartesian_2d`) and again in `charts::trajectory_plot_bounds`,
//! a hand-written re-derivation whose only contract with the renderer
//! was a comment saying "same range-derivation logic". Every time the
//! renderer's margins changed, the hover overlay silently drifted off
//! the dots it was supposed to be sitting on.
//!
//! A scale is the fix: one object that owns the mapping, is handed to
//! both the painter and the hit-tester, and is **invertible**. Invertible
//! is the important word — direct manipulation requires going
//! *backwards* from a pixel the user touched to the datum it represents.
//! You cannot build a Bret-Victor-style instrument on a one-way
//! projection.
//!
//! Everything here is GPUI-free `f64` arithmetic so it lives in the lib
//! target and can actually be tested (see `src/lib.rs` for why the bin
//! target can't be).

/// A continuous linear mapping from a data `domain` to a pixel `range`.
///
/// The range is intentionally allowed to be inverted (`range.0 >
/// range.1`), which is the normal case for a y-axis: data increases
/// upward, pixels increase downward.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearScale {
    d0: f64,
    d1: f64,
    r0: f64,
    r1: f64,
}

impl LinearScale {
    /// Build a scale. A degenerate domain (`d0 == d1`) is widened by a
    /// symmetric epsilon rather than producing NaN — a single-sample
    /// distribution should render as a spike, not as a crash.
    pub fn new(domain: (f64, f64), range: (f64, f64)) -> Self {
        let (mut d0, mut d1) = domain;
        if !d0.is_finite() || !d1.is_finite() {
            d0 = 0.0;
            d1 = 1.0;
        }
        if (d1 - d0).abs() < f64::EPSILON {
            let pad = if d0.abs() > 1.0 { d0.abs() * 0.05 } else { 0.5 };
            d0 -= pad;
            d1 += pad;
        }
        Self {
            d0,
            d1,
            r0: range.0,
            r1: range.1,
        }
    }

    pub fn domain(&self) -> (f64, f64) {
        (self.d0, self.d1)
    }

    pub fn range(&self) -> (f64, f64) {
        (self.r0, self.r1)
    }

    /// Data → pixel.
    pub fn map(&self, v: f64) -> f64 {
        let t = (v - self.d0) / (self.d1 - self.d0);
        self.r0 + t * (self.r1 - self.r0)
    }

    /// Data → pixel, clamped to the range. Use when a stray outlier
    /// must not be painted outside the plot rect.
    pub fn map_clamped(&self, v: f64) -> f64 {
        let p = self.map(v);
        let (lo, hi) = if self.r0 <= self.r1 {
            (self.r0, self.r1)
        } else {
            (self.r1, self.r0)
        };
        p.clamp(lo, hi)
    }

    /// Pixel → data. This is the direct-manipulation direction: given a
    /// screen coordinate the operator touched, what value is that?
    pub fn invert(&self, p: f64) -> f64 {
        let t = (p - self.r0) / (self.r1 - self.r0);
        self.d0 + t * (self.d1 - self.d0)
    }

    /// Expand the domain outward to round numbers so axis labels read
    /// as `0 / 25 / 50 / 75 / 100` rather than `2.08 / 27.4 / …`.
    pub fn nice(self, target_ticks: usize) -> Self {
        let (lo, hi) = nice_bounds(self.d0, self.d1, target_ticks);
        Self {
            d0: lo,
            d1: hi,
            ..self
        }
    }

    /// Grow the domain by a fraction of its span on both ends, with an
    /// optional hard floor (used to keep probability axes from dipping
    /// below 0%).
    pub fn padded(self, frac: f64, floor: Option<f64>) -> Self {
        let span = self.d1 - self.d0;
        let pad = (span * frac).max(f64::EPSILON);
        let mut lo = self.d0 - pad;
        let hi = self.d1 + pad;
        if let Some(f) = floor {
            lo = lo.max(f);
        }
        Self {
            d0: lo,
            d1: hi,
            ..self
        }
    }

    /// Round tick values inside the domain, spaced on a 1/2/5×10ⁿ grid.
    pub fn ticks(&self, target: usize) -> Vec<f64> {
        if target == 0 {
            return Vec::new();
        }
        let step = nice_step((self.d1 - self.d0) / target as f64);
        if step <= 0.0 || !step.is_finite() {
            return vec![self.d0, self.d1];
        }
        let first = (self.d0 / step).ceil() * step;
        let mut out = Vec::new();
        let mut v = first;
        // Guard against pathological loops on absurd domains.
        let mut guard = 0;
        while v <= self.d1 + step * 1e-9 && guard < 1024 {
            // Snap away float dust so `0.30000000000000004` prints as `0.3`.
            out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
            v += step;
            guard += 1;
        }
        out
    }

    /// Fraction of the way across the range, 0..1, for a data value.
    /// Handy for gradient stops and alpha ramps.
    pub fn t(&self, v: f64) -> f64 {
        ((v - self.d0) / (self.d1 - self.d0)).clamp(0.0, 1.0)
    }
}

/// Round `raw` **up** to the nearest 1, 2, 2.5, or 5 times a power of
/// ten.
///
/// Rounding up (rather than to nearest) guarantees the tick count never
/// exceeds the caller's target by more than the two endpoints, which is
/// what keeps axis labels from colliding on a narrow chart.
///
/// The `2.5` rung matters more than it looks: without it, a 0–100%
/// probability axis asking for 4 ticks snaps to a step of 50 and
/// renders `0 / 50 / 100`, throwing away the quartile gridlines that
/// make a forecast chart readable. Matplotlib's `MaxNLocator` carries
/// the same `[1, 2, 2.5, 5, 10]` ladder for the same reason.
pub fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 || !raw.is_finite() {
        return 0.0;
    }
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    // Tolerance absorbs float dust so `norm == 2.4999999` still snaps to
    // the 2.5 rung instead of jumping to 5.
    const EPS: f64 = 1e-9;
    let snapped = if norm <= 1.0 + EPS {
        1.0
    } else if norm <= 2.0 + EPS {
        2.0
    } else if norm <= 2.5 + EPS {
        2.5
    } else if norm <= 5.0 + EPS {
        5.0
    } else {
        10.0
    };
    snapped * mag
}

/// Expand `[min, max]` outward to the nearest nice step boundaries.
pub fn nice_bounds(min: f64, max: f64, target_ticks: usize) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() || target_ticks == 0 {
        return (min, max);
    }
    if (max - min).abs() < f64::EPSILON {
        return (min - 0.5, max + 0.5);
    }
    let step = nice_step((max - min) / target_ticks as f64);
    if step <= 0.0 {
        return (min, max);
    }
    ((min / step).floor() * step, (max / step).ceil() * step)
}

/// Compute a shared domain that covers every finite value in `values`,
/// returning `None` when there's nothing to show. Saves every caller
/// from re-writing the same `fold(INFINITY, f64::min)` incantation —
/// which is exactly the duplication that let the old chart and its
/// hover overlay disagree about the axis.
pub fn extent<I: IntoIterator<Item = f64>>(values: I) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for v in values {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if lo.is_finite() && hi.is_finite() {
        Some((lo, hi))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn maps_and_inverts_are_exact_inverses() {
        let s = LinearScale::new((0.0, 100.0), (40.0, 760.0));
        for v in [0.0, 12.5, 33.3, 99.9, 100.0] {
            assert!(close(s.invert(s.map(v)), v), "round-trip failed for {v}");
        }
    }

    #[test]
    fn inverted_range_handles_y_axis_orientation() {
        // y: data 0 at bottom (pixel 200), data 100 at top (pixel 10).
        let s = LinearScale::new((0.0, 100.0), (200.0, 10.0));
        assert!(close(s.map(0.0), 200.0));
        assert!(close(s.map(100.0), 10.0));
        assert!(close(s.invert(105.0), 50.0));
    }

    #[test]
    fn degenerate_domain_does_not_produce_nan() {
        let s = LinearScale::new((7.0, 7.0), (0.0, 100.0));
        assert!(s.map(7.0).is_finite());
        assert!(close(s.map(7.0), 50.0));
    }

    #[test]
    fn non_finite_domain_falls_back_to_unit() {
        let s = LinearScale::new((f64::NAN, 1.0), (0.0, 10.0));
        assert!(s.map(0.5).is_finite());
    }

    #[test]
    fn clamping_keeps_outliers_inside_the_plot_rect() {
        let s = LinearScale::new((0.0, 10.0), (0.0, 100.0));
        assert!(close(s.map_clamped(999.0), 100.0));
        assert!(close(s.map_clamped(-999.0), 0.0));
        // Inverted range clamps to the same physical interval.
        let inv = LinearScale::new((0.0, 10.0), (100.0, 0.0));
        assert!(close(inv.map_clamped(999.0), 0.0));
        assert!(close(inv.map_clamped(-999.0), 100.0));
    }

    #[test]
    fn nice_step_snaps_to_1_2_5_decades() {
        assert!(close(nice_step(0.03), 0.05));
        assert!(close(nice_step(1.0), 1.0));
        assert!(close(nice_step(1.1), 2.0));
        assert!(close(nice_step(3.0), 5.0));
        assert!(close(nice_step(7.0), 10.0));
        assert!(close(nice_step(2.4), 2.5));
        assert!(close(nice_step(23.0), 25.0));
        assert_eq!(nice_step(0.0), 0.0);
        assert_eq!(nice_step(-1.0), 0.0);
    }

    #[test]
    fn ticks_land_on_round_numbers_inside_the_domain() {
        let s = LinearScale::new((0.0, 100.0), (0.0, 1.0));
        let t = s.ticks(4);
        assert_eq!(t, vec![0.0, 25.0, 50.0, 75.0, 100.0]);
        for v in &t {
            assert!(*v >= 0.0 && *v <= 100.0);
        }
    }

    #[test]
    fn ticks_terminate_on_pathological_domains() {
        let s = LinearScale::new((0.0, 1e18), (0.0, 1.0));
        assert!(s.ticks(3).len() < 1024);
        assert!(LinearScale::new((0.0, 1.0), (0.0, 1.0)).ticks(0).is_empty());
    }

    #[test]
    fn nice_widens_to_round_bounds() {
        let s = LinearScale::new((2.08, 27.4), (0.0, 1.0)).nice(4);
        let (lo, hi) = s.domain();
        assert!(lo <= 2.08 && hi >= 27.4);
        assert!(close(lo, 0.0), "lo was {lo}");
        assert!(close(hi, 30.0), "hi was {hi}");
    }

    #[test]
    fn padded_respects_the_floor() {
        let s = LinearScale::new((1.0, 5.0), (0.0, 1.0)).padded(0.5, Some(0.0));
        assert!(close(s.domain().0, 0.0));
        assert!(close(s.domain().1, 7.0));
    }

    #[test]
    fn extent_ignores_non_finite_values() {
        let e = extent([1.0, f64::NAN, 5.0, f64::INFINITY, -2.0]).unwrap();
        assert!(close(e.0, -2.0) && close(e.1, 5.0));
        assert!(extent([f64::NAN, f64::INFINITY]).is_none());
        assert!(extent(std::iter::empty()).is_none());
    }
}
