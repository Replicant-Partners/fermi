# Discrete Drivers - Complete Guide

## Overview

Discrete drivers represent **categorical outcomes** with specific values and probabilities. Unlike continuous drivers that sample from a distribution, discrete drivers select from a defined set of possible values.

## When to Use Discrete Drivers

Use discrete drivers when you have:
- **Specific scenarios** (bear/stable/bull market)
- **Categorical outcomes** (low/medium/high success)
- **Fixed options** (product tiers, market states, outcome levels)
- **Known alternatives** with assigned probabilities

## Syntax

```fpl
driver name discrete {
    display_name: "Human Readable Name"
    description: "What this represents"
    values: [value1, value2, value3, ...]
    weights: [prob1, prob2, prob3, ...]
    unit: "what the values represent"
    rationale: "why these values and probabilities"
}
```

### Required Fields
- `values`: Array of numeric values (the possible outcomes)
- `weights`: Array of probabilities (must sum to 1.0)

### Optional Fields
- `display_name`: Human-readable name for output
- `description`: Natural language explanation
- `unit`: What the values represent
- `rationale`: Justification for your choices

## Complete Example

```fpl
question "What will total project cost be?"

driver base_cost continuous {
    display_name: "Base Project Cost"
    description: "The baseline cost estimate for the project"
    distribution: triangular(50000, 75000, 100000)
    unit: "USD"
    rationale: "Based on similar projects in the past"
}

driver market_scenario discrete {
    display_name: "Market Scenario"
    description: "Expected market conditions affecting costs"
    values: [0.8, 1.0, 1.3]
    weights: [0.2, 0.5, 0.3]
    unit: "multiplier"
    rationale: "Bear market (0.8x), stable (1.0x), bull market (1.3x) with historical frequencies"
}

model: base_cost * market_scenario

simulate 10000 iterations
```

### Output

```
Mean: 78814.88
Median: 76725.31
90% CI: 53564.13 to 110877.18
```

## How It Works

### Categorical Sampling

On each simulation iteration, the discrete driver:
1. Generates a random number between 0 and 1
2. Uses the cumulative probability to select a value
3. Returns that specific value

**Example:**
```fpl
values: [10, 20, 30]
weights: [0.5, 0.3, 0.2]
```

- 50% chance of value 10
- 30% chance of value 20  
- 20% chance of value 30

### Mathematical Foundation

This implements a **categorical distribution** (also called multinomial with n=1):
- P(X = value[i]) = weight[i]
- Uses inverse transform sampling with cumulative distribution function

## Common Use Cases

### 1. Market Scenarios

```fpl
driver market_state discrete {
    display_name: "Market State"
    values: [0.7, 1.0, 1.4]  # Bear, normal, bull multipliers
    weights: [0.25, 0.5, 0.25]
    rationale: "Historical market state frequencies over 20 years"
}
```

### 2. Product Success Levels

```fpl
driver product_success discrete {
    display_name: "Product Launch Success"
    values: [0.5, 1.0, 2.0, 5.0]  # Flop, modest, success, blockbuster
    weights: [0.1, 0.4, 0.4, 0.1]
    unit: "revenue multiplier"
    rationale: "Historical product launch outcomes"
}
```

### 3. Customer Segments

```fpl
driver customer_segment discrete {
    display_name: "Customer Segment Mix"
    values: [50, 100, 200]  # Small, medium, enterprise deal sizes
    weights: [0.6, 0.3, 0.1]
    unit: "USD thousands"
    rationale: "Current customer base distribution"
}
```

### 4. Regulatory Outcomes

```fpl
driver regulatory_outcome discrete {
    display_name: "Regulatory Decision"
    values: [0, 0.8, 1.0]  # Rejected, conditional, approved
    weights: [0.1, 0.3, 0.6]
    rationale: "Similar applications in past 5 years"
}
```

### 5. Quality Levels

```fpl
driver quality_multiplier discrete {
    display_name: "Production Quality"
    values: [0.6, 0.9, 1.0, 1.1]  # Poor, acceptable, good, excellent
    weights: [0.05, 0.2, 0.6, 0.15]
    rationale: "Manufacturing quality control statistics"
}
```

## Best Practices

### Choosing Values

1. **Be specific**: Use actual numeric values, not indices
   - ✅ Good: `values: [0.8, 1.0, 1.3]`
   - ❌ Bad: `values: [1, 2, 3]` (then multiply by constants)

2. **Make values meaningful**: Values should represent actual outcomes
   - ✅ Good: `values: [50000, 75000, 100000]` (actual costs)
   - ✅ Good: `values: [0.8, 1.0, 1.2]` (multipliers)

3. **Consider scale**: Values should be in units that make sense in your model
   - If multiplying, use multipliers (0.5x, 1.0x, 2.0x)
   - If adding, use absolute values (1000, 2000, 3000)

### Choosing Weights

1. **Sum to 1.0**: Weights must be probabilities
   ```fpl
   weights: [0.2, 0.5, 0.3]  # ✅ Sums to 1.0
   weights: [20, 50, 30]     # ❌ Doesn't sum to 1.0
   ```

2. **Base on data**: Use historical frequencies when available
   - Historical market states
   - Past product outcomes
   - Previous project results

3. **Use expert judgment**: When no data exists
   - Ask domain experts
   - Use reference class forecasting
   - Document assumptions

4. **Avoid over-precision**: Don't use false precision
   - ✅ Good: `[0.2, 0.5, 0.3]`
   - ❌ Bad: `[0.237, 0.481, 0.282]` (false precision)

### Documentation

Always include:
```fpl
driver name discrete {
    display_name: "What This Is"
    description: "Detailed explanation of what each value means"
    values: [...]
    weights: [...]
    rationale: "Why these specific values and probabilities"
}
```

**Example of good documentation:**
```fpl
driver market_growth discrete {
    display_name: "Market Growth Scenario"
    description: "Three scenarios for market growth based on economic conditions"
    values: [0.02, 0.08, 0.15]
    weights: [0.25, 0.5, 0.25]
    unit: "annual growth rate"
    rationale: "Recession (2%), normal (8%), boom (15%) based on 30 years of market data"
}
```

## Validation

The semantic analyzer checks:

✅ **Required fields present**
- Must have both `values` and `weights`

✅ **Array lengths match**
```fpl
values: [10, 20, 30]
weights: [0.5, 0.3, 0.2]  # ✅ Same length (3)
```

✅ **Weights sum to 1.0** (within 0.001 tolerance)
```fpl
weights: [0.333, 0.333, 0.334]  # ✅ Sums to 1.0
weights: [0.3, 0.3, 0.3]        # ⚠️ Warning: sums to 0.9
```

✅ **All weights non-negative**
```fpl
weights: [0.5, 0.3, 0.2]   # ✅ All positive
weights: [0.6, 0.5, -0.1]  # ❌ Negative weight
```

## Combining with Other Drivers

### With Continuous Drivers

```fpl
driver base_value continuous {
    distribution: normal(100, 10)
}

driver scenario_multiplier discrete {
    values: [0.8, 1.0, 1.2]
    weights: [0.3, 0.4, 0.3]
}

model: base_value * scenario_multiplier
```

### With Binary Drivers

```fpl
driver base_revenue continuous {
    distribution: triangular(100000, 150000, 200000)
}

driver big_deal binary {
    probability: 0.3p
}

driver market_condition discrete {
    values: [0.9, 1.0, 1.1]
    weights: [0.2, 0.6, 0.2]
}

model: base_revenue * (if big_deal then 1.5 else 1.0) * market_condition
```

## Advanced Example

```fpl
question "What will Q1 revenue be with multiple scenarios?"

driver baseline_sales continuous {
    display_name: "Baseline Sales"
    distribution: triangular(80000, 100000, 130000)
    unit: "USD"
}

driver economic_scenario discrete {
    display_name: "Economic Scenario"
    description: "Macroeconomic conditions affecting consumer spending"
    values: [0.75, 0.9, 1.0, 1.15, 1.3]
    weights: [0.1, 0.2, 0.4, 0.2, 0.1]
    rationale: "Severe recession (0.75), mild recession (0.9), normal (1.0), growth (1.15), boom (1.3)"
}

driver competitive_position discrete {
    display_name: "Competitive Position"
    description: "Our market position relative to competitors"
    values: [0.8, 1.0, 1.2]
    weights: [0.25, 0.5, 0.25]
    rationale: "Lost share (0.8), maintained (1.0), gained share (1.2)"
}

driver new_product_success binary {
    display_name: "New Product Launch Success"
    probability: 0.65p
    impact_multiplier: 1.4
    rationale: "Product pre-orders suggest 65% chance of hitting targets"
}

model: baseline_sales * economic_scenario * competitive_position * 
       (if new_product_success then 1.4 else 1.0)

simulate 10000 iterations
```

## Tips

1. **Start simple**: Begin with 2-3 values, add more if needed
2. **Round probabilities**: Use nice numbers like 0.2, 0.3, 0.5
3. **Document scenarios**: Explain what each value represents
4. **Validate with SMEs**: Check weights with domain experts
5. **Test sensitivity**: Try different weights to see impact

## LSP Support

In Zed with the Fermi LSP:

- Type `discrete` after driver name → Get full snippet
- Type `values:` inside driver → Get array snippet
- Type `weights:` inside driver → Get array snippet
- Hover over `discrete`, `values`, or `weights` → See documentation

---

**Happy Forecasting with Discrete Drivers! 🎲**
