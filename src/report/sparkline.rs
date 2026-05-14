/// ASCII sparkline generation
///
/// Creates inline visualizations using Unicode block characters

/// Generate a sparkline from a series of values
pub fn generate(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    // Find min and max for normalization
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    // Avoid division by zero
    let range = if (max - min).abs() < 1e-10 {
        1.0
    } else {
        max - min
    };

    // Unicode block characters for sparklines
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    values
        .iter()
        .map(|&v| {
            let normalized = (v - min) / range;
            let index = (normalized * (chars.len() - 1) as f64).round() as usize;
            chars[index.min(chars.len() - 1)]
        })
        .collect()
}

/// Generate a sparkline from histogram bins
pub fn from_histogram(bins: &[(f64, usize)]) -> String {
    let counts: Vec<f64> = bins.iter().map(|(_, count)| *count as f64).collect();
    generate(&counts)
}

/// Generate a trend indicator
pub fn trend_indicator(values: &[f64]) -> String {
    if values.len() < 2 {
        return "─".to_string();
    }

    let first = values.first().unwrap();
    let last = values.last().unwrap();

    let change_percent = ((last - first) / first) * 100.0;

    if change_percent > 10.0 {
        "📈 ↗".to_string()
    } else if change_percent > 2.0 {
        "↗".to_string()
    } else if change_percent < -10.0 {
        "📉 ↘".to_string()
    } else if change_percent < -2.0 {
        "↘".to_string()
    } else {
        "→".to_string()
    }
}

/// Generate a percentile marker sparkline
pub fn percentile_marker(
    p5: f64,
    p25: f64,
    median: f64,
    p75: f64,
    p95: f64,
    min: f64,
    max: f64,
) -> String {
    // Create a visual representation of the distribution
    let range = max - min;

    if range < 1e-10 {
        return "│".to_string(); // Single point
    }

    // Calculate positions (0-20 scale for visual width)
    let width = 20;
    let pos = |val: f64| ((val - min) / range * width as f64).round() as usize;

    let mut line = vec![' '; width + 1];

    // Mark the percentiles
    line[pos(p5)] = '┤';
    line[pos(p25)] = '├';
    line[pos(median)] = '█';
    line[pos(p75)] = '┤';
    line[pos(p95)] = '├';

    // Fill the IQR range
    for i in pos(p25)..=pos(p75) {
        if line[i] == ' ' {
            line[i] = '─';
        }
    }

    line.iter().collect()
}

/// Generate a confidence indicator (bar with percentage)
pub fn confidence_bar(confidence: f64, width: usize) -> String {
    let filled = (confidence * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    format!(
        "[{}{}] {:.0}%",
        "█".repeat(filled),
        "░".repeat(empty),
        confidence * 100.0
    )
}

/// Generate a distribution shape indicator
pub fn distribution_shape(mean: f64, median: f64, std_dev: f64) -> String {
    let skew = (mean - median) / std_dev;

    if skew.abs() < 0.1 {
        "Symmetric ⬌".to_string()
    } else if skew > 0.5 {
        "Right-skewed ⮕".to_string()
    } else if skew > 0.1 {
        "Slightly right-skewed →".to_string()
    } else if skew < -0.5 {
        "Left-skewed ⬅".to_string()
    } else {
        "Slightly left-skewed ←".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparkline() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 4.0, 3.0, 2.0, 1.0];
        let spark = generate(&values);
        // Unicode block chars are multi-byte; use chars().count() not .len()
        assert_eq!(spark.chars().count(), 9);
        assert!(spark.contains('█')); // max value (5) maps to '█'
        assert!(spark.contains('▁')); // min value (1) maps to '▁'
    }

    #[test]
    fn test_confidence_bar() {
        let bar = confidence_bar(0.75, 10);
        assert!(bar.contains("75%"));
    }
}
