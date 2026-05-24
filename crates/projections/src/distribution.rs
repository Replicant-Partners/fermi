use serde::{Deserialize, Serialize};

/// Distribution summary for a single output dimension across N runs.
/// Shape-stable regardless of N.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionSummary {
    pub dimension: String,
    pub n_runs: usize,
    pub n_failed: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub p5: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p95: f64,
    pub max: f64,
    /// Histogram bins: (bin_low, count). Auto-sized (Freedman-Diaconis rule,
    /// capped at 50 bins).
    pub histogram: Vec<(f64, usize)>,
}

impl DistributionSummary {
    /// Build a summary from a sorted vector of sample values.
    pub fn from_samples(dimension: String, mut samples: Vec<f64>, n_failed: usize) -> Self {
        let n = samples.len();
        if n == 0 {
            return Self::empty(dimension, n_failed);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mean = samples.iter().sum::<f64>() / n as f64;
        let variance = samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        let std_dev = variance.sqrt();

        let pct = |p: f64| -> f64 {
            let idx = ((n as f64 - 1.0) * p).floor() as usize;
            samples[idx.min(n - 1)]
        };

        let histogram = build_histogram(&samples);

        Self {
            dimension,
            n_runs: n,
            n_failed,
            mean,
            std_dev,
            min: samples[0],
            p5: pct(0.05),
            p25: pct(0.25),
            p50: pct(0.50),
            p75: pct(0.75),
            p95: pct(0.95),
            max: samples[n - 1],
            histogram,
        }
    }

    fn empty(dimension: String, n_failed: usize) -> Self {
        Self {
            dimension,
            n_runs: 0,
            n_failed,
            mean: 0.0,
            std_dev: 0.0,
            min: 0.0,
            p5: 0.0,
            p25: 0.0,
            p50: 0.0,
            p75: 0.0,
            p95: 0.0,
            max: 0.0,
            histogram: vec![],
        }
    }
}

/// Freedman-Diaconis bin width: h = 2 * IQR * n^(-1/3), capped at 50 bins.
fn build_histogram(sorted: &[f64]) -> Vec<(f64, usize)> {
    let n = sorted.len();
    if n < 2 {
        return vec![];
    }

    let min = sorted[0];
    let max = sorted[n - 1];
    if (max - min).abs() < f64::EPSILON {
        return vec![(min, n)];
    }

    let q1 = sorted[(n as f64 * 0.25) as usize];
    let q3 = sorted[(n as f64 * 0.75) as usize];
    let iqr = q3 - q1;

    let bin_width = if iqr > 0.0 {
        (2.0 * iqr * (n as f64).powf(-1.0 / 3.0)).max((max - min) / 50.0)
    } else {
        (max - min) / 20.0
    };

    let n_bins = (((max - min) / bin_width).ceil() as usize).clamp(2, 50);
    let actual_width = (max - min) / n_bins as f64;

    let mut counts = vec![0usize; n_bins];
    for &v in sorted {
        let idx = ((v - min) / actual_width).floor() as usize;
        counts[idx.min(n_bins - 1)] += 1;
    }

    counts
        .into_iter()
        .enumerate()
        .map(|(i, c)| (min + i as f64 * actual_width, c))
        .collect()
}

/// Full output of a projection run: one DistributionSummary per output dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionOutput {
    pub dimensions: Vec<DistributionSummary>,
    pub n_requested: usize,
    pub n_completed: usize,
    pub seed: Option<u64>,
    pub executor_kind: String,
    pub sweep_kind: String,
}

impl ProjectionOutput {
    /// Retrieve a summary for a specific dimension by name.
    pub fn dimension(&self, name: &str) -> Option<&DistributionSummary> {
        self.dimensions.iter().find(|d| d.dimension == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_from_known_distribution() {
        let samples: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let summary = DistributionSummary::from_samples("yield".into(), samples, 0);
        assert!((summary.mean - 50.5).abs() < 0.1);
        assert!((summary.p50 - 50.0).abs() < 1.0);
        assert_eq!(summary.n_runs, 100);
        assert!(!summary.histogram.is_empty());
    }

    #[test]
    fn histogram_counts_sum_to_n() {
        let samples: Vec<f64> = (0..200).map(|i| i as f64 * 0.5).collect();
        let summary = DistributionSummary::from_samples("x".into(), samples, 0);
        let total: usize = summary.histogram.iter().map(|(_, c)| c).sum();
        assert_eq!(total, 200);
    }
}
