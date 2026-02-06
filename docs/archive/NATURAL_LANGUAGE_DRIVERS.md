# Natural Language Driver Names & Descriptions

## Overview

Drivers now support `display_name` and `description` fields to make simulation output more readable and understandable.

## New Fields

### `display_name`
Human-readable name that appears in simulation output.

**Example:**
```fpl
display_name: "Base Sales Revenue"
```

### `description`
Natural language explanation of what the driver represents.

**Example:**
```fpl
description: "The baseline quarterly sales figure before any adjustments or special events"
```

## Benefits

1. **Better Readability**: Output uses meaningful names instead of code variable names
2. **Stakeholder Communication**: Non-technical users can understand forecasts
3. **Self-Documenting**: Forecasts explain themselves
4. **Professional Output**: Results look polished and ready to share

## Complete Example

```fpl
question "What will Q1 revenue be?"

driver base_sales continuous {
    display_name: "Base Sales Revenue"
    description: "The baseline quarterly sales figure before any adjustments or special events"
    distribution: triangular(10000, 15000, 25000)
    unit: "USD"
    rationale: "Based on historical Q4 2025 data and seasonal patterns"
}

driver success_multiplier binary {
    display_name: "Major Client Renewal"
    description: "Whether the Fortune 500 client renews their annual contract"
    probability: 0.65p
    impact_multiplier: 1.4
    rationale: "Client has expressed strong interest, but budget approval is pending"
}

model: base_sales * (if success_multiplier then 1.4 else 1.0)

simulate 10000 iterations
```

## Output Comparison

### Without Display Names (Before)
```
2. Driver(base_sales)
   ├─ Type: Continuous
   ├─ Distribution: Triangular
   └─ Unit: "USD"
```

### With Display Names (After)
```
2. Driver(base_sales)
   ├─ Display Name: "Base Sales Revenue"
   ├─ Description: "The baseline quarterly sales figure before any adjustments or special events"
   ├─ Type: Continuous
   ├─ Distribution: Triangular
   └─ Unit: "USD"
```

## Best Practices

### Display Names
- Use Title Case
- Keep it short (2-5 words)
- Describe WHAT it is, not HOW it's calculated
- Good: "Base Sales Revenue", "Client Renewal Event"
- Avoid: "base_sales_var", "thing1"

### Descriptions
- Write complete sentences
- Explain the business meaning
- Include context that matters
- Good: "The baseline quarterly sales figure before any adjustments or special events"
- Avoid: "A number", "Sales"

## Usage in LSP

The Zed LSP extension now includes autocomplete for these fields:

1. Type `display_name:` inside a driver block
2. Get snippet: `display_name: "Human Readable Name"`
3. Type `description:` 
4. Get snippet: `description: "Natural language description"`

Hover over `display_name` or `description` keywords to see full documentation.

## Examples by Domain

### Finance
```fpl
driver base_revenue continuous {
    display_name: "Baseline Revenue"
    description: "Expected revenue from existing customers with no growth"
    distribution: triangular(100000, 150000, 200000)
    unit: "USD"
}
```

### Product Launch
```fpl
driver launch_success binary {
    display_name: "Successful Product Launch"
    description: "Whether the new product achieves its first-month adoption targets"
    probability: 0.7p
}
```

### Market Forecasting
```fpl
driver market_growth continuous {
    display_name: "Annual Market Growth Rate"
    description: "Year-over-year percentage growth in the total addressable market"
    distribution: triangular(0.05, 0.15, 0.30)
    unit: "percent"
}
```

### Operations
```fpl
driver delivery_time continuous {
    display_name: "Average Delivery Time"
    description: "Mean time from order placement to customer delivery"
    distribution: lognormal(5, 0.3)
    unit: "days"
}
```

## Migration Guide

### Updating Existing Forecasts

You don't need to add these fields immediately - they're optional. But for better readability:

**Before:**
```fpl
driver x continuous {
    distribution: normal(100, 10)
}
```

**After:**
```fpl
driver x continuous {
    display_name: "Customer Acquisition Count"
    description: "Number of new customers acquired each month"
    distribution: normal(100, 10)
    unit: "customers"
}
```

## Tips

1. **Start with display_name**: Even without description, this improves readability significantly
2. **Think about your audience**: Write descriptions for people who don't know the code
3. **Be specific**: "Q1 2026 Revenue" is better than "Revenue"
4. **Include units in description**: "Revenue in thousands of dollars" adds clarity
5. **Use consistent naming**: If one driver is "Base X", use "Base Y" for similar drivers

---

**Happy Forecasting! 🎯**
