//! Distribution chart geometry.
//!
//! Small, but split out for the same reason as `plot::trajectory`: the
//! bin target can't be tested, so anything worth asserting has to live
//! here. What's worth asserting is mostly that the chart doesn't lie —
//! that the probability it prints comes from the data and not from
//! integrating a curve that was normalised for display.

use super::density::{prob_at_least, prob_at_least_bins, Density};
use super::frame::{Frame, Margins};

/// How much chrome to draw. A sparkline in a dense driver list can't
/// afford axis labels; the results panel needs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chrome {
    /// Curve only — for inline sparklines.
    Bare,
    /// Curve, baseline, axis ends, percentile markers, provenance
    /// caption.
    Full,
}

impl Chrome {
    pub fn margins(self) -> Margins {
        match self {
            Chrome::Bare => Margins::BARE,
            Chrome::Full => Margins::new(8.0, 10.0, 16.0, 10.0),
        }
    }

    pub fn is_full(self) -> bool {
        matches!(self, Chrome::Full)
    }
}

/// Percentile markers to draw on the curve.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Percentiles {
    pub p5: Option<f64>,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
}

impl Percentiles {
    pub fn new(p5: f64, p50: f64, p95: f64) -> Self {
        Self {
            p5: Some(p5),
            p50: Some(p50),
            p95: Some(p95),
        }
    }
}

/// Curves are peak-normalised to 1.0; the headroom keeps the peak off
/// the top edge.
const Y_HEADROOM: f64 = 1.06;

/// Data plus size plus chrome: enough to place every mark.
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionSpec {
    pub density: Density,
    pub percentiles: Percentiles,
    /// Decision threshold in data space.
    pub threshold: Option<f64>,
    /// P(X ≥ threshold), computed from the *source* data.
    pub prob_above: Option<f64>,
    pub width: f64,
    pub height: f64,
    pub chrome: Chrome,
}

impl DistributionSpec {
    pub fn new(density: Density, width: f64, height: f64) -> Self {
        Self {
            density,
            percentiles: Percentiles::default(),
            threshold: None,
            prob_above: None,
            width,
            height,
            chrome: Chrome::Full,
        }
    }

    pub fn frame(&self) -> Frame {
        let (x0, x1) = self.density.x_extent().unwrap_or((0.0, 1.0));
        Frame::new(
            self.width,
            self.height,
            self.chrome.margins(),
            (x0, x1),
            (0.0, Y_HEADROOM),
        )
    }

    /// Whether a threshold is set *and* falls inside the visible range,
    /// so the painter and the readout agree about whether to draw it.
    pub fn visible_threshold(&self) -> Option<f64> {
        let t = self.threshold?;
        let (x0, x1) = self.density.x_extent()?;
        (t > x0 && t < x1).then_some(t)
    }

    /// Set a threshold and derive P(X ≥ t) from histogram bins.
    ///
    /// Deriving it from the *bins* rather than from `self.density`
    /// matters: the density is peak-normalised for display, so
    /// integrating it would produce a confident-looking wrong number.
    pub fn with_threshold_from_bins(mut self, t: f64, bins: &[u32], lo: f64, hi: f64) -> Self {
        self.threshold = Some(t);
        self.prob_above = prob_at_least_bins(bins, lo, hi, t);
        self
    }

    /// Same, from raw draws.
    pub fn with_threshold_from_samples(mut self, t: f64, samples: &[f64]) -> Self {
        self.threshold = Some(t);
        self.prob_above = prob_at_least(samples, t);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::density::DensitySource;

    fn bins() -> [u32; 4] {
        [1, 6, 6, 1]
    }

    fn spec() -> DistributionSpec {
        DistributionSpec::new(Density::from_bins(&bins(), 0.0, 4.0), 400.0, 80.0)
    }

    #[test]
    fn bare_chrome_reclaims_the_axis_gutter_for_ink() {
        let full = spec();
        let mut bare = spec();
        bare.chrome = Chrome::Bare;
        assert!(bare.frame().plot.height() > full.frame().plot.height());
        assert!(bare.frame().plot.width() > full.frame().plot.width());
    }

    #[test]
    fn the_x_scale_round_trips_in_both_chrome_modes() {
        for chrome in [Chrome::Full, Chrome::Bare] {
            let mut s = spec();
            s.chrome = chrome;
            let f = s.frame();
            for v in [0.5, 2.0, 3.5] {
                assert!((f.x.invert(f.x.map(v)) - v).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn threshold_probability_comes_from_the_bins_not_the_normalised_curve() {
        // Two equal bins over [0,2]: the true P(X ≥ 1) is exactly 0.5.
        let b = [10u32, 10];
        let s = DistributionSpec::new(Density::from_bins(&b, 0.0, 2.0), 400.0, 80.0)
            .with_threshold_from_bins(1.0, &b, 0.0, 2.0);
        assert!((s.prob_above.unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn threshold_probability_from_samples_matches_the_empirical_tail() {
        let samples: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let s = DistributionSpec::new(Density::from_samples(&samples, 64), 400.0, 80.0)
            .with_threshold_from_samples(75.0, &samples);
        assert!((s.prob_above.unwrap() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_threshold_outside_the_visible_range_is_not_drawn() {
        let mut s = spec();
        s.threshold = Some(999.0);
        assert!(s.visible_threshold().is_none());

        s.threshold = Some(-999.0);
        assert!(s.visible_threshold().is_none());

        s.threshold = Some(2.0);
        assert!(s.visible_threshold().is_some());

        s.threshold = None;
        assert!(s.visible_threshold().is_none());
    }

    #[test]
    fn an_empty_density_still_yields_a_usable_frame() {
        let s = DistributionSpec::new(Density::from_bins(&[], 0.0, 1.0), 400.0, 80.0);
        let f = s.frame();
        assert!(f.plot.width() > 0.0 && f.plot.height() > 0.0);
        assert!(s.visible_threshold().is_none());
    }

    #[test]
    fn a_quantile_sourced_curve_is_flagged_as_a_sketch() {
        let s = DistributionSpec::new(Density::from_quantiles(1.0, 2.0, 5.0, 96), 120.0, 24.0);
        assert_eq!(s.density.source, DensitySource::Quantiles);
        assert!(
            !s.density.source.shape_is_real(),
            "the UI must be able to caption this as inferred"
        );
    }

    #[test]
    fn the_y_domain_leaves_headroom_above_the_normalised_peak() {
        let f = spec().frame();
        assert!(f.y.domain().1 > 1.0);
        // The peak (density 1.0) must land below the top of the plot.
        assert!(f.y.map(1.0) > f.plot.top);
    }
}
