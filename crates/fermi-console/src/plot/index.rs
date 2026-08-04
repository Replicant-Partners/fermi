//! Index chart geometry — model vs base vs crowd, version over version.
//!
//! # What this chart is for
//!
//! The trajectory chart answers "how did the number move through
//! *time*?". This one answers "how did it move through *my revisions*?"
//! — which is a different question, because versions are the moments
//! the operator actually committed to something. Ten market ticks
//! between two saves are noise on this axis; the saves are the signal.
//!
//! So the x-axis is the version ordinal, evenly spaced, not wall-clock.
//! Everything else — the visual language, the scrub, the divergence
//! band — deliberately matches the trajectory chart, because they're
//! read side by side and an operator shouldn't have to learn two idioms.
//!
//! # What was wrong with the old one
//!
//! It plotted three "series", but two of them were constants: the base
//! rate and the crowd price were copied unchanged into every version's
//! row. So a chart advertising three lines really showed one line and
//! two horizontals — and drew them as though they carried equal
//! information.
//!
//! Here the base rate is drawn as what it is (a reference level), and
//! the crowd becomes a *real* series when price history is available to
//! sample at each version's timestamp — "what was the market saying
//! when I saved v3?" is answerable, and it's the question that tells
//! you whether a revision was insight or drift.

use super::frame::{Frame, Margins};
use super::scale::extent;

/// One committed revision.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexVersion {
    pub label: String,
    pub model_pct: f64,
    /// Crowd price at the moment this version was saved, when price
    /// history covers it. `None` renders as a gap rather than as a
    /// fabricated value.
    pub crowd_pct: Option<f64>,
    pub note: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IndexData {
    pub versions: Vec<IndexVersion>,
    pub base_rate_pct: Option<f64>,
    /// Latest crowd price, drawn as a reference level when there's no
    /// per-version history to make a series from.
    pub crowd_now_pct: Option<f64>,
}

/// What the cursor is over.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexProbe {
    pub version: usize,
    pub model_pct: f64,
    pub crowd_pct: Option<f64>,
    pub base_pct: Option<f64>,
    /// model − crowd at this version.
    pub edge_pp: Option<f64>,
    /// model change from the previous version.
    pub delta_pp: Option<f64>,
}

/// A revision that moved the number by less than this is bookkeeping,
/// not a decision, and doesn't earn a delta badge.
pub const CONSEQUENTIAL_PP: f64 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub struct IndexSpec {
    pub data: IndexData,
    pub width: f64,
    pub height: f64,
}

impl IndexSpec {
    pub fn new(data: IndexData, width: f64, height: f64) -> Self {
        Self {
            data,
            width,
            height,
        }
    }

    pub fn len(&self) -> usize {
        self.data.versions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.versions.is_empty()
    }

    /// The one geometry, shared by painter and hit-tester.
    pub fn frame(&self) -> Frame {
        let n = self.len();
        // A single version has no span; give it one so the point lands
        // mid-chart instead of dividing by zero.
        let x_domain = if n <= 1 {
            (-0.5, 0.5)
        } else {
            (0.0, (n - 1) as f64)
        };

        let ys = self
            .data
            .versions
            .iter()
            .map(|v| v.model_pct)
            .chain(self.data.versions.iter().filter_map(|v| v.crowd_pct))
            .chain(self.data.base_rate_pct)
            .chain(self.data.crowd_now_pct);
        let y_domain = extent(ys).unwrap_or((0.0, 100.0));

        let m = Margins::new(10.0, 30.0, 16.0, 34.0);
        let mut f = Frame::new(self.width, self.height, m, x_domain, y_domain);
        f.y = f.y.padded(0.12, Some(0.0));
        f
    }

    /// Model series in `(ordinal, pct)` form.
    pub fn model_series(&self) -> Vec<(f64, f64)> {
        self.data
            .versions
            .iter()
            .enumerate()
            .map(|(i, v)| (i as f64, v.model_pct))
            .collect()
    }

    /// Crowd series, restricted to versions where the price is known.
    ///
    /// Gaps are dropped rather than interpolated: a missing crowd price
    /// means the market wasn't being polled then, and drawing a line
    /// across the hole would invent a reading that never existed.
    pub fn crowd_series(&self) -> Vec<(f64, f64)> {
        self.data
            .versions
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.crowd_pct.map(|c| (i as f64, c)))
            .collect()
    }

    /// True when the crowd has enough real per-version readings to draw
    /// as a series rather than as a single reference level.
    pub fn crowd_is_series(&self) -> bool {
        self.crowd_series().len() >= 2
    }

    /// Model change from the previous version, per version. The first
    /// entry is always `None` — there's nothing to compare against.
    pub fn deltas(&self) -> Vec<Option<f64>> {
        let mut out = Vec::with_capacity(self.len());
        let mut prev: Option<f64> = None;
        for v in &self.data.versions {
            out.push(prev.map(|p| v.model_pct - p));
            prev = Some(v.model_pct);
        }
        out
    }

    /// Interpret an element-local pixel position. Snaps to the nearest
    /// version, which is the only thing there is to point at on an
    /// ordinal axis.
    pub fn probe(&self, local_x: f64, local_y: f64) -> Option<IndexProbe> {
        if self.is_empty() {
            return None;
        }
        let f = self.frame();
        let raw = f.hover_x(local_x, local_y)?;
        let idx = (raw.round().max(0.0) as usize).min(self.len() - 1);
        let v = &self.data.versions[idx];
        let deltas = self.deltas();
        Some(IndexProbe {
            version: idx,
            model_pct: v.model_pct,
            crowd_pct: v.crowd_pct,
            base_pct: self.data.base_rate_pct,
            edge_pp: v.crowd_pct.map(|c| v.model_pct - c),
            delta_pp: deltas[idx],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(label: &str, model: f64, crowd: Option<f64>) -> IndexVersion {
        IndexVersion {
            label: label.to_string(),
            model_pct: model,
            crowd_pct: crowd,
            note: String::new(),
        }
    }

    fn spec() -> IndexSpec {
        IndexSpec::new(
            IndexData {
                versions: vec![
                    v("v1", 10.0, Some(12.0)),
                    v("v2", 40.0, Some(15.0)),
                    v("v3", 42.0, Some(18.0)),
                ],
                base_rate_pct: Some(20.0),
                crowd_now_pct: Some(18.0),
            },
            240.0,
            70.0,
        )
    }

    #[test]
    fn probing_a_version_pixel_returns_that_version() {
        let s = spec();
        let f = s.frame();
        for i in 0..s.len() {
            let (px, _) = f.point(i as f64, 20.0);
            let p = s.probe(px, f.plot.top + 5.0).unwrap();
            assert_eq!(p.version, i, "pixel for v{} probed as v{}", i, p.version);
        }
    }

    #[test]
    fn probing_between_versions_snaps_to_the_nearer_one() {
        let s = spec();
        let f = s.frame();
        let (x0, _) = f.point(0.0, 20.0);
        let (x1, _) = f.point(1.0, 20.0);
        // 40% of the way from v1 to v2 rounds back to v1.
        let near_first = x0 + (x1 - x0) * 0.4;
        assert_eq!(s.probe(near_first, f.plot.top + 5.0).unwrap().version, 0);
        // 60% rounds forward to v2.
        let near_second = x0 + (x1 - x0) * 0.6;
        assert_eq!(s.probe(near_second, f.plot.top + 5.0).unwrap().version, 1);
    }

    #[test]
    fn probe_carries_the_comparison_numbers() {
        let s = spec();
        let f = s.frame();
        let (px, _) = f.point(1.0, 20.0);
        let p = s.probe(px, f.plot.top + 5.0).unwrap();
        assert!((p.model_pct - 40.0).abs() < 1e-9);
        assert!((p.crowd_pct.unwrap() - 15.0).abs() < 1e-9);
        assert!((p.base_pct.unwrap() - 20.0).abs() < 1e-9);
        assert!((p.edge_pp.unwrap() - 25.0).abs() < 1e-9);
        assert!((p.delta_pp.unwrap() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn probing_outside_the_plot_rect_returns_nothing() {
        let s = spec();
        assert!(s.probe(1.0, 1.0).is_none());
        assert!(s.probe(10_000.0, 30.0).is_none());
    }

    #[test]
    fn the_first_version_has_no_delta() {
        let d = spec().deltas();
        assert_eq!(d[0], None, "nothing precedes v1 to compare against");
        assert!((d[1].unwrap() - 30.0).abs() < 1e-9);
        assert!((d[2].unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn missing_crowd_prices_are_dropped_not_interpolated() {
        let s = IndexSpec::new(
            IndexData {
                versions: vec![
                    v("v1", 10.0, Some(12.0)),
                    v("v2", 40.0, None),
                    v("v3", 42.0, Some(18.0)),
                ],
                base_rate_pct: None,
                crowd_now_pct: None,
            },
            240.0,
            70.0,
        );
        let series = s.crowd_series();
        assert_eq!(series.len(), 2, "the unknown reading must not be invented");
        assert!((series[0].0 - 0.0).abs() < 1e-9);
        assert!((series[1].0 - 2.0).abs() < 1e-9);

        // And the probe reports the gap honestly.
        let f = s.frame();
        let (px, _) = f.point(1.0, 20.0);
        let p = s.probe(px, f.plot.top + 5.0).unwrap();
        assert!(p.crowd_pct.is_none());
        assert!(p.edge_pp.is_none());
    }

    #[test]
    fn crowd_needs_two_readings_to_be_a_series() {
        let mut d = spec().data;
        assert!(IndexSpec::new(d.clone(), 240.0, 70.0).crowd_is_series());

        // Only one known reading — a level, not a series.
        d.versions[1].crowd_pct = None;
        d.versions[2].crowd_pct = None;
        assert!(!IndexSpec::new(d, 240.0, 70.0).crowd_is_series());
    }

    #[test]
    fn a_single_version_does_not_collapse_the_axis() {
        let s = IndexSpec::new(
            IndexData {
                versions: vec![v("v1", 33.0, None)],
                base_rate_pct: None,
                crowd_now_pct: None,
            },
            240.0,
            70.0,
        );
        let f = s.frame();
        assert!(f.x.domain().1 > f.x.domain().0);
        let (px, _) = f.point(0.0, 33.0);
        assert!(px.is_finite());
        assert_eq!(s.probe(px, f.plot.top + 5.0).unwrap().version, 0);
    }

    #[test]
    fn empty_data_is_harmless() {
        let s = IndexSpec::new(IndexData::default(), 240.0, 70.0);
        assert!(s.is_empty());
        let f = s.frame();
        assert!(f.plot.width() > 0.0 && f.plot.height() > 0.0);
        assert!(s.probe(f.plot.left + 1.0, f.plot.top + 1.0).is_none());
    }

    #[test]
    fn the_y_axis_covers_every_series_and_reference() {
        let f = spec().frame();
        let (lo, hi) = f.y.domain();
        // Model spans 10–42, crowd 12–18, base 20. All must fit.
        assert!(lo <= 10.0, "lo {lo} clips the lowest model point");
        assert!(hi >= 42.0, "hi {hi} clips the highest model point");
        assert!(lo >= 0.0, "probability axis must not go negative");
    }

    #[test]
    fn a_tiny_chart_still_produces_a_valid_rect() {
        let s = IndexSpec::new(spec().data, 60.0, 24.0);
        let f = s.frame();
        assert!(f.plot.right > f.plot.left);
        assert!(f.plot.bottom > f.plot.top);
    }
}
