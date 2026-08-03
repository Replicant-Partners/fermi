//! Density estimation — turning simulation output into an honest curve.
//!
//! # Why this exists
//!
//! `charts::render_distribution_sparkline` drew a *triangle* through
//! `(p5, 0) → (p50, peak) → (p95, 0)` and called it a distribution.
//! For a bimodal posterior — precisely the case where the operator most
//! needs to see the shape — that triangle is not an approximation, it's
//! a fabrication. It shows one mode where there are two, and it implies
//! symmetric tails on a lognormal.
//!
//! This module builds densities from the data we actually have, and
//! records **which** data it used so the UI can say so. A chart that
//! quietly interpolates is worse than no chart; a chart that says
//! "shape inferred from 3 quantiles" is honest.
//!
//! GPUI-free, so it lives in the lib target and is testable.

/// Where a density's shape came from. Rendered as a provenance label so
/// the operator is never fooled by a smooth curve drawn over thin data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DensitySource {
    /// Kernel density estimate over raw Monte Carlo draws. Trustworthy.
    Samples,
    /// Reconstructed from simulation histogram bins. Trustworthy up to
    /// bin resolution.
    Histogram,
    /// Interpolated from summary quantiles only. A sketch, not a shape:
    /// it cannot show multimodality because the inputs don't contain it.
    Quantiles,
}

impl DensitySource {
    /// Short caption for the chart corner.
    pub fn caption(&self) -> &'static str {
        match self {
            DensitySource::Samples => "kde over draws",
            DensitySource::Histogram => "from sim histogram",
            DensitySource::Quantiles => "shape inferred from quantiles",
        }
    }

    /// Whether the curve is faithful enough to read shape (modes,
    /// skew) off, as opposed to only location and spread.
    pub fn shape_is_real(&self) -> bool {
        !matches!(self, DensitySource::Quantiles)
    }
}

/// A density curve on a regular grid, plus the summary statistics the
/// operator reads off it.
#[derive(Debug, Clone, PartialEq)]
pub struct Density {
    /// `(x, density)` pairs, ascending in x. Density is normalised so
    /// the maximum is 1.0 — charts care about shape, not absolute
    /// probability mass, and normalising keeps a 3-bin and a 300-bin
    /// curve visually comparable.
    pub points: Vec<(f64, f64)>,
    pub source: DensitySource,
}

impl Density {
    pub fn is_empty(&self) -> bool {
        self.points.len() < 2
    }

    pub fn x_extent(&self) -> Option<(f64, f64)> {
        Some((self.points.first()?.0, self.points.last()?.0))
    }

    /// Density at an arbitrary x, linearly interpolated. Used by the
    /// hover readout to place a dot on the curve under the cursor.
    pub fn at(&self, x: f64) -> Option<f64> {
        if self.points.len() < 2 {
            return None;
        }
        let first = self.points.first()?;
        let last = self.points.last()?;
        if x <= first.0 {
            return Some(first.1);
        }
        if x >= last.0 {
            return Some(last.1);
        }
        let i = self
            .points
            .partition_point(|(px, _)| *px <= x)
            .clamp(1, self.points.len() - 1);
        let (x0, y0) = self.points[i - 1];
        let (x1, y1) = self.points[i];
        let dx = x1 - x0;
        if dx.abs() < f64::EPSILON {
            return Some(y0);
        }
        Some(y0 + (x - x0) / dx * (y1 - y0))
    }

    /// Kernel density estimate from raw draws.
    pub fn from_samples(samples: &[f64], grid_n: usize) -> Self {
        let mut xs: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
        if xs.len() < 2 || grid_n < 2 {
            return Self {
                points: Vec::new(),
                source: DensitySource::Samples,
            };
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let h = silverman_bandwidth(&xs);
        let lo = xs[0] - 3.0 * h;
        let hi = xs[xs.len() - 1] + 3.0 * h;
        let step = (hi - lo) / (grid_n - 1) as f64;

        let inv_h = 1.0 / h;
        let norm = 1.0 / (xs.len() as f64 * h * (2.0 * std::f64::consts::PI).sqrt());
        let points: Vec<(f64, f64)> = (0..grid_n)
            .map(|i| {
                let x = lo + step * i as f64;
                // Gaussian kernel. O(grid_n · n); with grid_n ≈ 160 and
                // n ≈ 10k that's 1.6M flops — under a millisecond, and
                // it only runs when the samples change, not per frame.
                let d: f64 = xs
                    .iter()
                    .map(|s| {
                        let u = (x - s) * inv_h;
                        (-0.5 * u * u).exp()
                    })
                    .sum();
                (x, d * norm)
            })
            .collect();

        Self {
            points: normalise(points),
            source: DensitySource::Samples,
        }
    }

    /// Reconstruct a curve from simulation histogram bin counts spread
    /// evenly across `[lo, hi]`. Bin centres become curve vertices.
    pub fn from_bins(bins: &[u32], lo: f64, hi: f64) -> Self {
        if bins.is_empty() || !(hi > lo) {
            return Self {
                points: Vec::new(),
                source: DensitySource::Histogram,
            };
        }
        let w = (hi - lo) / bins.len() as f64;
        let mut points: Vec<(f64, f64)> = Vec::with_capacity(bins.len() + 2);
        // Anchor at zero on both sides so the filled area closes cleanly
        // instead of dropping off a cliff at the first and last bin.
        points.push((lo, 0.0));
        for (i, &c) in bins.iter().enumerate() {
            points.push((lo + w * (i as f64 + 0.5), c as f64));
        }
        points.push((hi, 0.0));
        Self {
            points: normalise(points),
            source: DensitySource::Histogram,
        }
    }

    /// Last resort: a smooth unimodal curve consistent with three
    /// quantiles. Marked `Quantiles` so the UI can caption it as a
    /// sketch — it is incapable of showing a second mode.
    ///
    /// Uses a two-sided Gaussian (different σ each side of the median),
    /// which at least respects skew, unlike the triangle it replaces.
    pub fn from_quantiles(p5: f64, p50: f64, p95: f64, grid_n: usize) -> Self {
        if !(p95 > p5) || grid_n < 2 {
            return Self {
                points: Vec::new(),
                source: DensitySource::Quantiles,
            };
        }
        // 1.645 σ ≈ the 5th/95th percentile of a normal.
        const Z: f64 = 1.6448536269514722;
        let sigma_l = ((p50 - p5) / Z).max(1e-9);
        let sigma_r = ((p95 - p50) / Z).max(1e-9);
        let lo = p5 - sigma_l;
        let hi = p95 + sigma_r;
        let step = (hi - lo) / (grid_n - 1) as f64;
        let points: Vec<(f64, f64)> = (0..grid_n)
            .map(|i| {
                let x = lo + step * i as f64;
                let s = if x < p50 { sigma_l } else { sigma_r };
                let u = (x - p50) / s;
                (x, (-0.5 * u * u).exp())
            })
            .collect();
        Self {
            points: normalise(points),
            source: DensitySource::Quantiles,
        }
    }
}

/// Scale a curve so its peak is 1.0. Returns the input unchanged when
/// every value is zero (an all-empty histogram shouldn't become NaN).
fn normalise(mut points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let max = points.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max);
    if max > 0.0 {
        for (_, y) in points.iter_mut() {
            *y /= max;
        }
    }
    points
}

/// Silverman's rule-of-thumb bandwidth. `xs` must be sorted ascending
/// and contain at least two finite values.
pub fn silverman_bandwidth(xs: &[f64]) -> f64 {
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let sd = var.max(0.0).sqrt();
    let iqr = quantile_sorted(xs, 0.75) - quantile_sorted(xs, 0.25);
    // The IQR term makes the estimate robust to outliers; fall back to
    // sd alone when the middle 50% is degenerate (many tied values).
    let spread = if iqr > 0.0 {
        sd.min(iqr / 1.349).max(f64::MIN_POSITIVE)
    } else {
        sd
    };
    let h = 0.9 * spread * n.powf(-0.2);
    // A zero bandwidth would divide by zero; pick something proportional
    // to the data's own scale so constant series render as a tight spike.
    if h > 0.0 {
        h
    } else {
        (xs[xs.len() - 1] - xs[0]).abs().max(1.0) * 0.01
    }
}

/// Linear-interpolated quantile of an **already sorted** slice.
pub fn quantile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let q = q.clamp(0.0, 1.0);
    let pos = q * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let frac = pos - lo as f64;
    sorted[lo] + frac * (sorted[hi] - sorted[lo])
}

/// P(X ≥ threshold) estimated from draws. This is the number a
/// forecaster is actually deciding on — "will revenue clear $200M?" —
/// and the reason the distribution chart needs a draggable threshold
/// rather than three fixed percentile ticks.
pub fn prob_at_least(samples: &[f64], threshold: f64) -> Option<f64> {
    let finite: Vec<f64> = samples.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    let hits = finite.iter().filter(|v| **v >= threshold).count();
    Some(hits as f64 / finite.len() as f64)
}

/// Same question, answered from histogram bins when raw draws aren't
/// retained. Partial bins are counted proportionally.
pub fn prob_at_least_bins(bins: &[u32], lo: f64, hi: f64, threshold: f64) -> Option<f64> {
    if bins.is_empty() || !(hi > lo) {
        return None;
    }
    let total: f64 = bins.iter().map(|c| *c as f64).sum();
    if total <= 0.0 {
        return None;
    }
    let w = (hi - lo) / bins.len() as f64;
    let mut acc = 0.0;
    for (i, &c) in bins.iter().enumerate() {
        let b0 = lo + w * i as f64;
        let b1 = b0 + w;
        if threshold <= b0 {
            acc += c as f64;
        } else if threshold < b1 {
            acc += c as f64 * (b1 - threshold) / w;
        }
    }
    Some((acc / total).clamp(0.0, 1.0))
}

/// Which side of a decision threshold a histogram bin falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinSide {
    /// Entirely below the threshold — contributes nothing to P(X ≥ t).
    Below,
    /// Contains the threshold. [`prob_at_least_bins`] counts this bin
    /// proportionally, so the chart must colour it distinctly rather
    /// than rounding it into one side.
    Straddles,
    /// Entirely at or above the threshold — counts in full.
    AtOrAbove,
}

/// Classify a bin against a threshold.
///
/// Exists so the *colour* of a bar and the *probability* printed above
/// the chart are derived from one rule. When those two disagree the
/// chart shows six green bars and claims 40%, and the operator has no
/// way to tell which half is lying.
pub fn bin_side(bin_lo: f64, bin_hi: f64, threshold: f64) -> BinSide {
    if bin_lo >= threshold {
        BinSide::AtOrAbove
    } else if bin_hi > threshold {
        BinSide::Straddles
    } else {
        BinSide::Below
    }
}

/// The outcome range a histogram spans, from its persisted bin
/// geometry.
///
/// Falls back to `fallback` when the geometry is missing — forecasts
/// saved before bin starts were persisted. The fallback is an
/// approximation, but it's used for *everything* that reads the
/// histogram, so the bars, the reference lines and the threshold stay
/// consistent with each other even when they're jointly imprecise.
pub fn bin_domain(bin_starts: &[f64], bin_width: f64, fallback: (f64, f64)) -> (f64, f64) {
    match (bin_starts.first(), bin_starts.last()) {
        (Some(first), Some(last)) if bin_width > 0.0 && last >= first => {
            (*first, *last + bin_width)
        }
        _ => fallback,
    }
}

/// Narrowest interval containing `mass` of the sorted draws — the
/// highest-density interval. Unlike a central 90% interval it stays
/// meaningful on skewed and bounded distributions.
pub fn hdi_sorted(sorted: &[f64], mass: f64) -> Option<(f64, f64)> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    if n == 1 {
        return Some((sorted[0], sorted[0]));
    }
    let mass = mass.clamp(0.01, 0.999);
    let window = ((mass * n as f64).round() as usize).clamp(1, n - 1);
    let mut best = (sorted[0], sorted[window]);
    let mut best_w = best.1 - best.0;
    for i in 0..(n - window) {
        let w = sorted[i + window] - sorted[i];
        if w < best_w {
            best_w = w;
            best = (sorted[i], sorted[i + window]);
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    /// Deterministic pseudo-samples: a two-lump mixture. No RNG
    /// dependency, so the test is reproducible.
    fn bimodal() -> Vec<f64> {
        let mut v = Vec::new();
        for i in 0..500 {
            let t = i as f64 / 499.0;
            v.push(2.0 + t * 2.0); // lump near 2–4
            v.push(10.0 + t * 2.0); // lump near 10–12
        }
        v
    }

    #[test]
    fn kde_recovers_both_modes_where_the_triangle_could_not() {
        let d = Density::from_samples(&bimodal(), 200);
        assert_eq!(d.source, DensitySource::Samples);
        // Count interior local maxima above a noise floor.
        let peaks = d
            .points
            .windows(3)
            .filter(|w| w[1].1 > w[0].1 && w[1].1 > w[2].1 && w[1].1 > 0.25)
            .count();
        assert!(peaks >= 2, "expected 2 modes, found {peaks}");

        // The quantile sketch, by construction, cannot do this.
        let q = Density::from_quantiles(2.2, 6.0, 11.8, 200);
        let q_peaks = q
            .points
            .windows(3)
            .filter(|w| w[1].1 > w[0].1 && w[1].1 > w[2].1 && w[1].1 > 0.25)
            .count();
        assert_eq!(q_peaks, 1);
        assert!(!q.source.shape_is_real());
    }

    #[test]
    fn densities_are_peak_normalised() {
        for d in [
            Density::from_samples(&bimodal(), 64),
            Density::from_bins(&[1, 5, 9, 5, 1], 0.0, 5.0),
            Density::from_quantiles(1.0, 2.0, 3.0, 64),
        ] {
            let max = d.points.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max);
            assert!(close(max, 1.0, 1e-9), "peak was {max}");
            assert!(d.points.iter().all(|(_, y)| *y >= 0.0));
        }
    }

    #[test]
    fn degenerate_inputs_yield_empty_curves_not_panics() {
        assert!(Density::from_samples(&[], 100).is_empty());
        assert!(Density::from_samples(&[1.0], 100).is_empty());
        assert!(Density::from_bins(&[], 0.0, 1.0).is_empty());
        assert!(Density::from_bins(&[1, 2], 5.0, 5.0).is_empty());
        assert!(Density::from_quantiles(3.0, 2.0, 1.0, 64).is_empty());
        assert!(Density::from_samples(&[f64::NAN, f64::NAN], 64).is_empty());
    }

    #[test]
    fn constant_samples_do_not_divide_by_zero() {
        let d = Density::from_samples(&[7.0; 50], 32);
        assert!(d.points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn all_zero_histogram_stays_finite() {
        let d = Density::from_bins(&[0, 0, 0], 0.0, 3.0);
        assert!(d.points.iter().all(|(_, y)| *y == 0.0));
    }

    #[test]
    fn interpolation_at_x_matches_the_vertices() {
        let d = Density::from_bins(&[0, 10, 0], 0.0, 3.0);
        let (lo, hi) = d.x_extent().unwrap();
        assert!(close(d.at(lo).unwrap(), 0.0, 1e-9));
        assert!(close(d.at(1.5).unwrap(), 1.0, 1e-9));
        // Outside the support clamps to the endpoints rather than
        // extrapolating a negative density.
        assert!(close(d.at(lo - 100.0).unwrap(), 0.0, 1e-9));
        assert!(close(d.at(hi + 100.0).unwrap(), 0.0, 1e-9));
    }

    #[test]
    fn quantiles_interpolate_linearly() {
        let s = [0.0, 1.0, 2.0, 3.0, 4.0];
        assert!(close(quantile_sorted(&s, 0.0), 0.0, 1e-9));
        assert!(close(quantile_sorted(&s, 0.5), 2.0, 1e-9));
        assert!(close(quantile_sorted(&s, 1.0), 4.0, 1e-9));
        assert!(close(quantile_sorted(&s, 0.25), 1.0, 1e-9));
        assert!(quantile_sorted(&[], 0.5).is_nan());
        assert!(close(quantile_sorted(&[9.0], 0.3), 9.0, 1e-9));
    }

    #[test]
    fn prob_at_least_counts_the_right_tail() {
        let s: Vec<f64> = (0..100).map(|i| i as f64).collect();
        assert!(close(prob_at_least(&s, 0.0).unwrap(), 1.0, 1e-9));
        assert!(close(prob_at_least(&s, 50.0).unwrap(), 0.50, 1e-9));
        assert!(close(prob_at_least(&s, 1000.0).unwrap(), 0.0, 1e-9));
        assert!(prob_at_least(&[], 1.0).is_none());
    }

    #[test]
    fn prob_at_least_bins_splits_partial_bins() {
        // Two bins over [0,2]: 10 in [0,1), 10 in [1,2).
        let bins = [10u32, 10];
        assert!(close(
            prob_at_least_bins(&bins, 0.0, 2.0, 1.0).unwrap(),
            0.5,
            1e-9
        ));
        // Halfway into the second bin → half of it remains.
        assert!(close(
            prob_at_least_bins(&bins, 0.0, 2.0, 1.5).unwrap(),
            0.25,
            1e-9
        ));
        assert!(close(
            prob_at_least_bins(&bins, 0.0, 2.0, -5.0).unwrap(),
            1.0,
            1e-9
        ));
        assert!(close(
            prob_at_least_bins(&bins, 0.0, 2.0, 5.0).unwrap(),
            0.0,
            1e-9
        ));
        assert!(prob_at_least_bins(&[0, 0], 0.0, 2.0, 1.0).is_none());
    }

    #[test]
    fn hdi_is_narrower_than_the_central_interval_when_skewed() {
        // Right-skewed: dense near 0, long tail.
        let mut s: Vec<f64> = (0..900).map(|i| i as f64 / 900.0).collect();
        s.extend((0..100).map(|i| 1.0 + i as f64));
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let (h0, h1) = hdi_sorted(&s, 0.90).unwrap();
        let central = quantile_sorted(&s, 0.95) - quantile_sorted(&s, 0.05);
        assert!(
            h1 - h0 <= central + 1e-9,
            "hdi {} should not exceed central {}",
            h1 - h0,
            central
        );
        assert!(hdi_sorted(&[], 0.9).is_none());
        assert_eq!(hdi_sorted(&[4.0], 0.9), Some((4.0, 4.0)));
    }

    #[test]
    fn bin_side_classifies_the_three_cases() {
        assert_eq!(bin_side(2.0, 3.0, 1.0), BinSide::AtOrAbove);
        assert_eq!(bin_side(0.0, 1.0, 2.0), BinSide::Below);
        assert_eq!(bin_side(1.0, 2.0, 1.5), BinSide::Straddles);
        // A threshold exactly on a bin's lower edge puts the whole bin
        // above it — the bin's mass is all ≥ t.
        assert_eq!(bin_side(1.0, 2.0, 1.0), BinSide::AtOrAbove);
        // ...and exactly on the upper edge puts it entirely below.
        assert_eq!(bin_side(1.0, 2.0, 2.0), BinSide::Below);
    }

    /// The invariant that keeps the chart from contradicting itself:
    /// bars coloured "above" must be exactly the mass the printed
    /// probability is counting.
    #[test]
    fn bar_colouring_reproduces_the_printed_probability() {
        let bins = [3u32, 7, 11, 5, 1];
        let (lo, hi) = (0.0, 5.0);
        let w = (hi - lo) / bins.len() as f64;
        let total: f64 = bins.iter().map(|c| *c as f64).sum();

        for t in [0.0, 0.5, 1.0, 2.4, 3.3, 4.99, 5.0] {
            // Re-derive the probability the way the renderer colours.
            let mut mass = 0.0;
            for (i, &c) in bins.iter().enumerate() {
                let b0 = lo + w * i as f64;
                let b1 = b0 + w;
                match bin_side(b0, b1, t) {
                    BinSide::AtOrAbove => mass += c as f64,
                    BinSide::Straddles => mass += c as f64 * (b1 - t) / w,
                    BinSide::Below => {}
                }
            }
            let from_colour = mass / total;
            let printed = prob_at_least_bins(&bins, lo, hi, t).unwrap();
            assert!(
                (from_colour - printed).abs() < 1e-9,
                "threshold {t}: bars say {from_colour}, readout says {printed}"
            );
        }
    }

    #[test]
    fn bin_domain_prefers_persisted_geometry() {
        let starts = [0.0, 0.5, 1.0, 1.5];
        assert_eq!(bin_domain(&starts, 0.5, (9.0, 9.0)), (0.0, 2.0));
    }

    #[test]
    fn bin_domain_falls_back_when_geometry_is_missing() {
        // No starts recorded (an older saved forecast).
        assert_eq!(bin_domain(&[], 0.5, (0.1, 0.9)), (0.1, 0.9));
        // Starts present but no width.
        assert_eq!(bin_domain(&[0.0, 1.0], 0.0, (0.1, 0.9)), (0.1, 0.9));
        // Nonsense ordering.
        assert_eq!(bin_domain(&[5.0, 1.0], 0.5, (0.1, 0.9)), (0.1, 0.9));
    }

    #[test]
    fn source_captions_are_distinct() {
        let all = [
            DensitySource::Samples,
            DensitySource::Histogram,
            DensitySource::Quantiles,
        ];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.caption(), b.caption());
            }
        }
    }
}
