# ADR-007: Comprehensive Sparkline Types (Distribution + Historical + Confidence)

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** Module 2 Q2.3

## Context

Tufte-style sparklines provide inline data visualization without disrupting code flow. For a forecasting IDE, we want to show rich information about distributions, trends, and uncertainty directly in the editor.

**User's Requirement:** "all of the above" - implement all proposed sparkline types

**The Question:** What should sparklines show?
- A) Current distribution (shape of triangular(500, 1200, 2500))
- B) Historical trend (how estimate changed over time)
- C) Confidence interval (shaded p10-p90 band)
- D) All of the above (different sparklines for different contexts)

**Design Goals:**
1. **Glanceability:** Understand distribution shape in <1 second
2. **Information Density:** Pack maximum insight in minimal space
3. **Context-Appropriate:** Show relevant visualization based on code element
4. **Non-Intrusive:** Don't clutter code, easy to disable if distracting

## Decision

We will implement **all three sparkline types**, displayed contextually based on what the user is viewing:

### 1. Distribution Sparklines (for driver statements)
Show the shape of probability distributions inline with driver definitions.

**Visual Format:**
```fpl
driver revenue triangular(500, 1200, 2500)  ▁▃▅▇▅▃▁ [1200±800]
driver costs normal(150, 30)                ▂▅▇▇▇▅▂ [150±60]
driver units uniform(100, 200)              ▅▅▅▅▅▅▅ [150±29]
```

**Components:**
- Unicode bar chart (7 chars): Shows distribution shape
- Statistics badge: [mean±stdev] or [p50 p10-p90]

### 2. Historical Sparklines (for forecast titles)
Show how the forecast result has evolved over time as user iterates.

**Visual Format:**
```fpl
forecast "Q4 Revenue" {                     ▁▃▂▅▇▆▅ 1200 (+15% from v1)
    // ...
}
```

**Components:**
- Line chart (7 chars): Time series of p50 values
- Latest value: Current p50 result
- Change indicator: % change from first version

### 3. Confidence Band Sparklines (for estimate statements)
Show uncertainty range for the final forecast result.

**Visual Format:**
```fpl
estimate revenue - costs                    ████▓▓▓▒▒▒░░░ [800-1200-1800]
//                                          ↑   ↑   ↑   ↑
//                                          p10 p50 p90
```

**Components:**
- Shaded bars: Darker = more probable
- Percentile badge: [p10-p50-p90]

## Consequences

### Positive

1. **Rich Information:** Users see distribution shape, trends, and uncertainty without running forecasts
2. **Visual Debugging:** Spot unrealistic distributions at a glance (uniform when should be normal)
3. **Historical Context:** Understand how forecast evolved, catch regressions
4. **Confidence Awareness:** Constant reminder of uncertainty (key to good forecasting)
5. **Professional Tool Feel:** Sparklines are hallmark of sophisticated data tools (Tufte influence)

### Negative

1. **Visual Clutter:** Three types of sparklines might be overwhelming
2. **Implementation Complexity:** Each type requires different rendering logic
3. **Performance Cost:** Need to compute distributions/history for every visible line
4. **Screen Space:** Takes up horizontal space (but only 10-20 chars)
5. **Color Dependency:** Confidence bands might not work well in all themes

### Neutral

1. **Configurability:** Users can disable specific types or all sparklines
2. **Update Frequency:** Need to decide when to recompute (on keystroke, on save, on demand)

## Alternatives Considered

### A. Distribution Only
**Pros:** Simpler to implement, most immediately useful, clear visual semantics  
**Cons:** Miss historical context and uncertainty visualization  
**Rejected Because:** User explicitly wanted "all of the above" - we can start here and iterate

### B. Historical Only
**Pros:** Unique to forecasting IDEs, shows evolution over time  
**Cons:** Doesn't help understand current distribution shape  
**Rejected Because:** Need distribution sparklines for basic functionality

### C. Confidence Only
**Pros:** Emphasizes uncertainty (core to forecasting)  
**Cons:** Doesn't show shape or trends  
**Rejected Because:** Too narrow - users need distribution shapes too

## Implementation Notes

### Phase 1: Distribution Sparklines (Week 1-2)

**Rendering Logic:**
```rust
fn render_distribution_sparkline(dist: &Distribution, width: usize) -> String {
    // Sample distribution to get histogram
    let samples = dist.sample(10_000);
    let histogram = create_histogram(&samples, width);
    
    // Convert to unicode bar chart
    let bars = histogram.iter()
        .map(|&count| unicode_bar(count, histogram.max()))
        .collect::<String>();
    
    // Add statistics badge
    let mean = samples.mean();
    let std = samples.std_dev();
    format!("{} [{}±{}]", bars, mean.round(), std.round())
}

fn unicode_bar(value: f64, max: f64) -> char {
    let ratio = value / max;
    match ratio {
        r if r > 0.875 => '▇',
        r if r > 0.750 => '▆',
        r if r > 0.625 => '▅',
        r if r > 0.500 => '▄',
        r if r > 0.375 => '▃',
        r if r > 0.250 => '▂',
        _ => '▁',
    }
}
```

**Zed Integration:**
```rust
// In fermi-lsp extension
impl InlayHintProvider for FermiLSP {
    fn inlay_hints(&self, document: &Document) -> Vec<InlayHint> {
        let mut hints = vec![];
        
        // Find all driver statements
        for driver in document.drivers() {
            let sparkline = render_distribution_sparkline(&driver.distribution, 7);
            hints.push(InlayHint {
                position: driver.end_position(),
                label: InlayHintLabel::String(sparkline),
                kind: InlayHintKind::Type,
                tooltip: Some(format!("Distribution: {}", driver.distribution)),
            });
        }
        
        hints
    }
}
```

### Phase 2: Historical Sparklines (Week 3-4)

**Version Tracking:**
```sql
-- Store forecast execution history
CREATE TABLE forecast_history (
    id UUID PRIMARY KEY,
    forecast_id UUID REFERENCES forecasts(id),
    version INT,
    p10 NUMERIC,
    p50 NUMERIC,
    p90 NUMERIC,
    executed_at TIMESTAMP
);

CREATE INDEX idx_forecast_history_lookup 
ON forecast_history(forecast_id, version DESC);
```

**Rendering Logic:**
```rust
fn render_historical_sparkline(history: &[HistoryPoint]) -> String {
    if history.len() < 2 {
        return "⊘ (no history)".to_string();
    }
    
    let values: Vec<f64> = history.iter().map(|h| h.p50).collect();
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    
    let sparkline = values.iter()
        .map(|&v| unicode_line_point(v, min, max))
        .collect::<String>();
    
    let latest = values.last().unwrap();
    let first = values.first().unwrap();
    let change_pct = ((latest - first) / first * 100.0).round();
    
    format!("{} {} ({:+}% from v1)", sparkline, latest.round(), change_pct)
}

fn unicode_line_point(value: f64, min: f64, max: f64) -> char {
    let ratio = (value - min) / (max - min);
    match ratio {
        r if r > 0.857 => '▇',
        r if r > 0.714 => '▆',
        r if r > 0.571 => '▅',
        r if r > 0.429 => '▄',
        r if r > 0.286 => '▃',
        r if r > 0.143 => '▂',
        _ => '▁',
    }
}
```

### Phase 3: Confidence Band Sparklines (Week 5-6)

**Rendering Logic:**
```rust
fn render_confidence_sparkline(result: &ForecastResult) -> String {
    // Get percentile distribution
    let percentiles = result.percentiles(20); // 20 buckets from p0 to p100
    
    // Create shaded bar chart
    let bars = percentiles.iter()
        .map(|&p| confidence_shade(p, result.p50))
        .collect::<String>();
    
    format!("{} [{}-{}-{}]", 
        bars, 
        result.p10.round(), 
        result.p50.round(), 
        result.p90.round()
    )
}

fn confidence_shade(percentile: f64, median: f64) -> char {
    // Distance from median (normalized)
    let distance = (percentile - median).abs() / median;
    
    match distance {
        d if d < 0.1 => '█', // Very close to median
        d if d < 0.2 => '▓',
        d if d < 0.3 => '▒',
        d if d < 0.5 => '░',
        _ => ' ',            // Far from median
    }
}
```

### Configuration

**User Settings:**
```json
{
  "fermi": {
    "sparklines": {
      "enabled": true,
      "types": {
        "distribution": true,
        "historical": true,
        "confidence": true
      },
      "width": 7,  // Number of characters
      "update_mode": "on_save",  // "on_keystroke" | "on_save" | "on_demand"
      "show_badges": true,  // Show [mean±std] text
      "theme": "auto"  // "light" | "dark" | "auto"
    }
  }
}
```

**Performance Optimization:**
```rust
// Cache sparklines to avoid recomputing on every render
struct SparklineCache {
    cache: HashMap<DriverId, (String, Instant)>,
    ttl: Duration,
}

impl SparklineCache {
    fn get_or_compute(&mut self, driver: &Driver) -> String {
        let now = Instant::now();
        
        if let Some((sparkline, computed_at)) = self.cache.get(&driver.id) {
            if now.duration_since(*computed_at) < self.ttl {
                return sparkline.clone();
            }
        }
        
        // Compute sparkline
        let sparkline = render_distribution_sparkline(&driver.distribution, 7);
        self.cache.insert(driver.id, (sparkline.clone(), now));
        sparkline
    }
}
```

## Visual Examples

### Complete Forecast with All Sparklines
```fpl
forecast "Q4 Revenue Forecast" {           ▁▃▅▇▅▃▁ 1200 (+8% from v1)
    // Historical sparkline shows evolution ↑
    
    driver revenue triangular(500, 1200, 2500)  ▁▃▅▇▅▃▁ [1200±800]
    //                Distribution sparkline ↑
    
    driver costs normal(150, 30)                ▂▅▇▇▇▅▂ [150±60]
    
    driver units uniform(100, 200)              ▅▅▅▅▅▅▅ [150±29]
    
    estimate revenue - costs                    ████▓▓▓▒▒▒░░░ [800-1200-1800]
    //                  Confidence sparkline ↑
}
```

### Dark Theme Variant
```fpl
// Unicode box-drawing for confidence bands in dark theme
estimate revenue - costs  ▓▓▓▓▒▒▒▒░░░░░░░ [800-1200-1800]
```

## References

- Edward Tufte's "The Visual Display of Quantitative Information"
- Module 2 Q2.3: Sparkline Content
- Zed inlay hints documentation: https://zed.dev/docs/configuring-languages#inlay-hints
- Unicode block elements: https://en.wikipedia.org/wiki/Block_Elements

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_distribution_sparkline() {
        let dist = Distribution::Triangular { min: 0, mode: 50, max: 100 };
        let sparkline = render_distribution_sparkline(&dist, 7);
        
        // Should show triangular shape (low-mid-high-mid-low)
        assert!(sparkline.contains('▇')); // Peak at mode
        assert!(sparkline.len() >= 7); // At least 7 chars
    }
    
    #[test]
    fn test_historical_sparkline() {
        let history = vec![
            HistoryPoint { p50: 100.0, version: 1 },
            HistoryPoint { p50: 120.0, version: 2 },
            HistoryPoint { p50: 150.0, version: 3 },
        ];
        
        let sparkline = render_historical_sparkline(&history);
        assert!(sparkline.contains("+50%")); // 50% increase
    }
    
    #[test]
    fn test_confidence_sparkline() {
        let result = ForecastResult {
            p10: 800.0,
            p50: 1200.0,
            p90: 1800.0,
        };
        
        let sparkline = render_confidence_sparkline(&result);
        assert!(sparkline.contains('█')); // Dense at center
        assert!(sparkline.contains("[800-1200-1800]"));
    }
}
```

## Success Metrics

- **Adoption Rate:** >80% of users keep sparklines enabled after 1 week
- **Performance:** <10ms to compute and render all sparklines for typical file
- **Visual Clarity:** User testing shows >90% can identify distribution type from sparkline
- **Cache Hit Rate:** >95% of sparkline requests served from cache
- **Error Rate:** <0.1% of sparklines fail to render (handle edge cases gracefully)

## Future Enhancements

1. **Interactive Sparklines:** Click sparkline to open detailed chart panel
2. **Animated Transitions:** When forecast changes, animate sparkline update
3. **Custom Color Mapping:** User-defined colors for confidence bands
4. **Multi-Line Sparklines:** For complex multimodal distributions
5. **Sparkline Diff:** Show before/after when editing driver parameters
