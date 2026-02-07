# Running Fermi Forecasts

## Quick Start

### Method 1: Simple Shell Script (Recommended)

Use the provided `run-forecast.sh` script:

```bash
./run-forecast.sh test_forecast.fpl
```

### Method 2: Direct Command

```bash
cargo run --release test_forecast.fpl

# Or if already built:
./target/release/fermi test_forecast.fpl
```

### Method 3: From Zed Terminal

1. Open terminal in Zed: `Cmd+J` (Mac) or `Ctrl+J` (Linux)
2. Run: `./run-forecast.sh your-file.fpl`

## Example Output

When you run a forecast, you'll see:

```
🔮 Running Fermi forecast: test_forecast.fpl
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

✓ Tokenization successful!
✓ Parsing successful!
✓ Semantic analysis passed!
✓ Simulation completed successfully!

Simulation Results:
  Iterations: 10000

  Statistics:
    Mean: 20931.93
    Std Dev: 5085.72

  Percentiles:
    5th: 13281.83
    50th (Median): 20519.10
    95th: 30198.04

[ASCII histogram showing distribution]

✓ Forecast Complete! Mean: 20931.93, Median: 20519.10
```

## Sample Forecast Files

### Basic Continuous Driver

```fpl
question "What will revenue be?"

driver base_revenue continuous {
    distribution: triangular(10000, 15000, 25000)
    unit: "USD"
    rationale: "Based on historical data"
}

model: base_revenue

simulate 10000 iterations
```

### With Binary Driver (Events)

```fpl
question "What will Q1 revenue be?"

driver base_sales continuous {
    distribution: triangular(10000, 15000, 25000)
    unit: "USD"
}

driver success_multiplier binary {
    probability: 0.65p
    impact_multiplier: 1.4
    rationale: "Major client renewal pending"
}

model: base_sales * (if success_multiplier then 1.4 else 1.0)

simulate 10000 iterations
```

### Multiple Drivers

```fpl
question "What will total revenue be?"

driver base_price continuous {
    distribution: triangular(10, 20, 30)
    unit: "USD"
}

driver volume continuous {
    distribution: normal(1000, 100)
    unit: "units"
}

driver growth_rate continuous {
    distribution: triangular(0.05, 0.15, 0.30)
    rationale: "Market growth estimates"
}

model: base_price * volume * (1 + growth_rate)

simulate 10000 iterations
```

## Understanding Results

### Statistics Explained

- **Mean**: Average outcome across all simulations
- **Median (50th percentile)**: Middle value - half of outcomes are above, half below
- **Std Dev**: Spread/uncertainty in the forecast
- **5th/95th percentile**: 90% confidence interval - outcomes typically fall in this range
- **25th/75th percentile**: Interquartile range (IQR) - where 50% of outcomes fall

### The Histogram

The ASCII histogram shows the distribution of outcomes:
- Peak shows most likely range
- Width shows uncertainty
- Skew shows asymmetric risks

Example interpretation:
```
     19993.7 -  21226.6 │ ██████████████████ 982  ← Most likely outcome range
```

## Tips

1. **Start simple**: Begin with one driver, then add complexity
2. **Higher iterations = more accuracy**: Use 10,000+ for final forecasts
3. **Check semantics**: The analyzer warns about missing evidence
4. **Compare runs**: Run multiple times to see stability
5. **Adjust distributions**: Use different distributions for different types of uncertainty

## Common Issues

### "No model found"
Add a `model:` line with your forecast expression.

### "Undefined variable"
Make sure all variables in your model are defined as drivers.

### "Parse error"
Check syntax - common issues:
- Missing braces `{}`
- Typos in distribution names
- Missing probability for binary drivers

## Distribution Guide

| Distribution | Use For | Example |
|--------------|---------|---------|
| `triangular(p5, p50, p95)` | Expert estimates | `triangular(100, 200, 500)` |
| `normal(mean, stddev)` | Natural variations | `normal(100, 15)` |
| `lognormal(median, sigma)` | Prices, incomes | `lognormal(50000, 0.5)` |
| `uniform(low, high)` | Complete uncertainty | `uniform(0, 100)` |
| `beta(alpha, beta)` | Probabilities [0-1] | `beta(2, 5)` |

## Binary Drivers

Binary drivers represent events that either happen or don't:

```fpl
driver event_happens binary {
    probability: 0.7p          # 70% chance it happens
    impact_multiplier: 1.5     # 50% increase if it happens
    rationale: "Historical frequency"
}

# Use in model with if-then-else:
model: base * (if event_happens then 1.5 else 1.0)
```

## Next Steps

- Read `AUTOCOMPLETE_FEATURES.md` for LSP features
- Check `docs/TODO.md` for planned features
- Try different distributions to model uncertainty
- Add evidence blocks to document your assumptions

---

**Happy Forecasting! 🎲**
