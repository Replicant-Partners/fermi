# Evidence System

**Version:** 0.5.0  
**Last Updated:** 2026-02-05  
**Status:** ✅ Production Ready

---

## Overview

The Evidence System allows you to **document and track the sources** supporting your forecast assumptions. Evidence blocks store research, data, and citations that inform your driver definitions, making forecasts **transparent, auditable, and collaborative**.

### Key Features

- **📄 Evidence Blocks** - Store sources, summaries, URLs, and key findings
- **🔗 Evidence References** - Link drivers to supporting evidence
- **📊 Rich Display** - Beautiful formatted output with citations
- **✅ Validation** - Semantic analyzer checks evidence references
- **⚠️ Warnings** - Suggests adding evidence to unsupported drivers

---

## Quick Start

### Basic Evidence Block

```fpl
evidence market_report {
    source: "Gartner Market Analysis 2026"
    summary: "Enterprise software market expected to grow 15-18% in 2026"
    url: "https://example.com/gartner-2026"
    relevance: 0.85
    date: "2026-01-15"
    key_findings: [
        "SaaS adoption accelerating in mid-market",
        "Average deal sizes up 22%"
    ]
}
```

### Linking Evidence to Drivers

```fpl
driver base_sales continuous {
    display_name: "Base Quarter Revenue"
    distribution: triangular(800000, 1200000, 1800000)
    unit: "USD"
    evidence_refs: ["market_report"]
}
```

---

## Evidence Block Fields

### Required Fields

#### `id` (identifier)
The unique identifier for this evidence block.

```fpl
evidence competitor_analysis {  // ← id
    source: "..."
}
```

#### `source` (string)
The name or title of the evidence source.

```fpl
evidence study {
    source: "MIT Technology Review Q1 2026"  // ← source
}
```

### Optional Fields

#### `summary` (string)
A brief summary of the evidence's key message.

```fpl
evidence research {
    source: "Industry Survey 2026"
    summary: "70% of enterprises plan to increase AI spending"
}
```

#### `url` (string)
A link to the full evidence source.

```fpl
evidence report {
    source: "McKinsey Report"
    url: "https://mckinsey.com/report-2026"
}
```

#### `relevance` (probability 0.0-1.0)
How relevant this evidence is to your forecast (0 = not relevant, 1 = highly relevant).

```fpl
evidence data {
    source: "Historical Sales Data"
    relevance: 0.95  // Highly relevant
}
```

**Display:**
- **0.80-1.00:** Green (high relevance)
- **0.50-0.79:** Yellow (medium relevance)
- **0.00-0.49:** Red (low relevance)

#### `date` (string)
When the evidence was published or collected.

```fpl
evidence analysis {
    source: "Q4 2025 Report"
    date: "2025-12-15"
}
```

#### `key_findings` (array of strings)
List of key findings or data points from the evidence.

```fpl
evidence survey {
    source: "Customer Survey 2026"
    key_findings: [
        "85% satisfaction rate",
        "Average NPS score: 72",
        "Churn rate decreased 15%"
    ]
}
```

---

## Linking Evidence to Drivers

Use the `evidence_refs` field in drivers to link to evidence blocks:

```fpl
// Define evidence
evidence market_data {
    source: "Industry Report 2026"
    summary: "Market growing at 20% CAGR"
}

// Reference it in a driver
driver market_size continuous {
    distribution: triangular(1000000, 1500000, 2000000)
    evidence_refs: ["market_data"]  // ← Link to evidence
}
```

### Multiple Evidence References

```fpl
driver revenue_forecast continuous {
    distribution: triangular(500000, 800000, 1200000)
    evidence_refs: [
        "historical_data",
        "market_trends",
        "competitor_analysis"
    ]
}
```

---

## CLI Output

When you run a forecast with evidence, Fermi displays a rich **Evidence Details** section:

```
Evidence Details:
  📄 market_report
     Source: Gartner Market Analysis 2026
     Summary: Enterprise software market expected to grow 15-18% in 2026
     URL: https://example.com/gartner-2026
     Relevance: 85%
     Date: 2026-01-15
     Key Findings:
       • SaaS adoption accelerating in mid-market
       • Average deal sizes up 22%
       • SMB segment showing 30% YoY growth
     Referenced by: new_customers, market_surge
```

### What's Displayed

1. **📄 Evidence ID** - Bold, white text
2. **Source** - The evidence source name
3. **Summary** - Brief description (if provided)
4. **URL** - Clickable link (blue, underlined)
5. **Relevance** - Color-coded percentage
6. **Date** - Publication or collection date
7. **Key Findings** - Bulleted list
8. **Referenced by** - List of drivers using this evidence

---

## Validation & Warnings

### Undefined Evidence Reference

If a driver references evidence that doesn't exist, you get an error:

```fpl
driver sales continuous {
    distribution: triangular(100, 200, 300)
    evidence_refs: ["nonexistent"]  // ❌ Error!
}
```

**Error:**
```
✗ Driver 'sales' references undefined evidence 'nonexistent'
```

### Missing Evidence Warning

If a driver has no `evidence_refs` and no `rationale`, you get a warning:

```fpl
driver revenue continuous {
    distribution: triangular(100, 200, 300)
    // ⚠️  No evidence_refs or rationale
}
```

**Warning:**
```
⚠ Driver 'revenue' has no evidence_refs or rationale. 
  Consider adding supporting evidence
```

---

## Best Practices

### 1. Document All Major Drivers

Every significant driver should have evidence or rationale:

✅ **Good:**
```fpl
driver customer_growth continuous {
    distribution: triangular(100, 150, 200)
    evidence_refs: ["market_research", "pipeline_data"]
}
```

❌ **Avoid:**
```fpl
driver customer_growth continuous {
    distribution: triangular(100, 150, 200)
    // No evidence or rationale
}
```

### 2. Use Rationale for Judgment Calls

When evidence is qualitative or based on expert judgment:

```fpl
driver adoption_rate binary {
    probability: 0.35
    rationale: "Based on 5 customer interviews and product team assessment"
    evidence_refs: ["interview_notes"]
}
```

### 3. Track Evidence Relevance

Rate evidence relevance to help others understand evidence quality:

```fpl
evidence tangential_study {
    source: "Related Industry Report"
    relevance: 0.45  // Somewhat relevant but not directly applicable
}

evidence core_data {
    source: "Our Internal Metrics"
    relevance: 0.98  // Highly relevant and directly applicable
}
```

### 4. Keep Summaries Concise

Summarize the **key takeaway**, not the full content:

✅ **Good:**
```fpl
evidence report {
    source: "Gartner 2026 Forecast"
    summary: "SaaS market expected to grow 18% in 2026"
}
```

❌ **Too Verbose:**
```fpl
evidence report {
    source: "Gartner 2026 Forecast"
    summary: "This report analyzes the software market across 50 countries and 200 companies, examining trends in cloud adoption, enterprise spending, and digital transformation initiatives. The report concludes that..."
}
```

### 5. Use Key Findings for Details

Save detailed points for `key_findings`:

```fpl
evidence analysis {
    source: "Competitive Analysis Q1 2026"
    summary: "Competitors showing strong growth across segments"
    key_findings: [
        "Competitor A: +22% revenue YoY",
        "Competitor B: +18% revenue YoY",
        "Average deal size up 15%",
        "SMB segment growth outpacing enterprise"
    ]
}
```

### 6. Link Multiple Sources

For complex drivers, cite all relevant sources:

```fpl
driver market_expansion continuous {
    distribution: triangular(500000, 1000000, 1800000)
    evidence_refs: [
        "market_sizing",      // External market data
        "competitor_intel",   // Competitive analysis
        "customer_pipeline",  // Internal pipeline
        "sales_team_input"    // Expert judgment
    ]
}
```

---

## Use Cases

### 1. Financial Forecasting

```fpl
evidence q4_results {
    source: "Q4 2025 Financial Results"
    summary: "Revenue $4.2M, up 15% YoY"
    relevance: 0.95
    date: "2026-01-15"
    key_findings: [
        "Enterprise segment: +22% growth",
        "SMB segment: +12% growth",
        "Renewal rate: 94%"
    ]
}

driver q1_revenue continuous {
    display_name: "Q1 2026 Revenue Forecast"
    distribution: triangular(4000000, 4500000, 5200000)
    evidence_refs: ["q4_results"]
}
```

### 2. Product Launch Forecasting

```fpl
evidence beta_feedback {
    source: "Beta User Survey (n=150)"
    summary: "82% would recommend, avg rating 4.3/5"
    relevance: 0.88
    date: "2026-01-20"
    key_findings: [
        "82% recommend to colleagues",
        "Average rating: 4.3/5",
        "Top request: mobile app",
        "Willingness to pay: $49-79/mo"
    ]
}

driver adoption_rate binary {
    display_name: "Successful Product Launch"
    probability: 0.75
    impact_multiplier: 2.0
    evidence_refs: ["beta_feedback", "market_analysis"]
}
```

### 3. Market Sizing

```fpl
evidence tam_analysis {
    source: "TAM/SAM/SOM Analysis 2026"
    summary: "TAM $50B, SAM $5B, SOM $500M"
    url: "https://internal.example.com/market-sizing"
    relevance: 0.92
    date: "2026-01-10"
    key_findings: [
        "Total Addressable Market: $50B",
        "Serviceable Addressable Market: $5B",
        "Serviceable Obtainable Market: $500M",
        "5-year CAGR: 18%"
    ]
}

driver market_size continuous {
    display_name: "Addressable Market Size"
    distribution: triangular(400000000, 500000000, 650000000)
    unit: "USD"
    evidence_refs: ["tam_analysis"]
}
```

### 4. Risk Assessment

```fpl
evidence security_audit {
    source: "External Security Audit Q4 2025"
    summary: "No critical vulnerabilities, 3 medium-risk items"
    relevance: 0.90
    date: "2025-12-20"
    key_findings: [
        "Zero critical or high-risk findings",
        "3 medium-risk items (all remediated)",
        "SOC 2 Type II compliant",
        "Recommended for enterprise deployment"
    ]
}

driver security_incident binary {
    display_name: "Major Security Incident"
    probability: 0.05  // Low probability based on audit
    impact_multiplier: 0.3  // Significant negative impact
    evidence_refs: ["security_audit"]
}
```

---

## Integration with Base Rates

Evidence complements base rates from Tetlock's forecasting methodology:

```fpl
question "Will we close the enterprise deal?" {
    base_rate {
        reference_class: "Enterprise deals >$500K"
        historical_frequency: 0.35
        sample_size: 120
        source: "Internal CRM data 2023-2025"
        generated_by: human
    }
}

evidence deal_intel {
    source: "Account Executive Notes"
    summary: "Strong champion, budget approved, legal review complete"
    relevance: 0.85
    key_findings: [
        "Executive sponsor committed",
        "Budget already allocated",
        "Legal review 80% complete",
        "Competing with 1 vendor"
    ]
}

driver deal_success binary {
    probability: 0.55  // Adjusted from base rate 0.35 based on evidence
    rationale: "Base rate 35%, but strong inside view signals justify 55%"
    evidence_refs: ["deal_intel"]
}
```

---

## Tips & Tricks

### 1. Version Evidence with Dates

Track when evidence was collected:

```fpl
evidence pipeline_jan {
    source: "Sales Pipeline Snapshot"
    date: "2026-01-31"
    summary: "Pipeline $2.5M in Q1"
}

evidence pipeline_feb {
    source: "Sales Pipeline Snapshot"
    date: "2026-02-28"
    summary: "Pipeline $3.1M in Q1 (+24%)"
}
```

### 2. Use Evidence for Transparency

Share forecasts with stakeholders - evidence makes assumptions explicit:

```
"Why did you forecast $1.5M revenue?"

→ Check evidence_refs in drivers
→ Review key findings
→ See source dates and relevance
```

### 3. Track Agent-Generated Evidence

When agents fetch data, store it as evidence:

```fpl
evidence agent_market_scan {
    source: "Automated Market Scan (Agent: research_bot)"
    summary: "Competitor pricing analysis from 15 websites"
    date: "2026-02-05"
    relevance: 0.80
    key_findings: [
        "Average price: $79/month",
        "Range: $49-$149/month",
        "Most common tier: $99/month"
    ]
}
```

### 4. Evidence as Collaboration Tool

Evidence blocks serve as **shared context** for teams:

- **Sales:** Pipeline data, customer feedback
- **Product:** Beta results, usage metrics
- **Finance:** Historical performance, market data
- **Leadership:** Strategic insights, external research

---

## LSP Support

*(Coming Soon)*

The Fermi Language Server will provide:

- **Autocomplete:** Evidence field suggestions
- **Hover:** Evidence details on hover
- **Go-to-Definition:** Click evidence_ref → jump to evidence block
- **Find References:** Find all drivers using an evidence block

---

## Future Enhancements

### Planned Features

1. **Evidence Search** - Filter/search evidence by date, relevance, source
2. **Evidence Export** - Export evidence to PDF/HTML for reporting
3. **Evidence Versioning** - Track evidence updates over time
4. **Agent Integration** - Agents automatically create evidence blocks
5. **Evidence Charts** - Visualize evidence relationships
6. **Evidence Templates** - Pre-defined evidence structures by type

---

## FAQ

### Q: Can evidence blocks be reused across forecasts?

**A:** Yes! Evidence blocks are part of the FPL source. You can copy evidence blocks between files or maintain a shared evidence library.

### Q: What's the difference between `rationale` and `evidence_refs`?

**A:** 
- **`rationale`**: Your reasoning or judgment (subjective)
- **`evidence_refs`**: Links to objective evidence blocks

Use both for maximum clarity:

```fpl
driver adoption continuous {
    distribution: triangular(100, 150, 200)
    rationale: "Conservative estimate given market uncertainty"
    evidence_refs: ["market_report", "beta_results"]
}
```

### Q: Do I need evidence for every driver?

**A:** Not required, but **strongly recommended** for:
- Major drivers with high impact
- Drivers with uncertain parameters
- Forecasts shared with stakeholders
- Forecasts that will be reviewed later

### Q: Can I link one evidence block to multiple drivers?

**A:** Yes! That's the power of evidence blocks:

```fpl
evidence market_growth {
    source: "Industry Report 2026"
    summary: "Market growing 20% annually"
}

driver sales continuous {
    evidence_refs: ["market_growth"]
}

driver customers continuous {
    evidence_refs: ["market_growth"]
}
```

### Q: How detailed should key_findings be?

**A:** Include 3-7 key points that:
- Are specific and quantitative
- Directly support your driver assumptions
- Can be verified by others

```fpl
key_findings: [
    "Revenue: $4.2M (+15% YoY)",     // Specific, quantitative
    "Customer count: 1,240 (+22%)",  // Verifiable
    "Churn rate: 6% (down from 8%)"  // Directly relevant
]
```

---

## Related Documentation

- [Natural Language Drivers](NATURAL_LANGUAGE_DRIVERS.md) - Display names and descriptions
- [Discrete Drivers](DISCRETE_DRIVERS.md) - Categorical distributions
- [Running Forecasts](RUNNING_FORECASTS.md) - Execution guide
- [Base Rates](BASE_RATES.md) - Tetlock methodology *(coming soon)*

---

## Complete Example

```fpl
question "What will be our Q2 2026 revenue?" {
    target_date: "2026-06-30"
    resolution_criteria: "Total revenue as reported in Q2 earnings"
}

// Evidence from external sources
evidence gartner_forecast {
    source: "Gartner Market Analysis 2026"
    summary: "Enterprise software market expected to grow 15-18% in 2026"
    url: "https://example.com/gartner-2026"
    relevance: 0.85
    date: "2026-01-15"
    key_findings: [
        "SaaS adoption accelerating in mid-market",
        "Average deal sizes up 22%",
        "SMB segment showing 30% YoY growth"
    ]
}

// Evidence from internal data
evidence internal_pipeline {
    source: "Internal Sales Pipeline Q1 2026"
    summary: "Strong pipeline with $2.5M in qualified leads"
    url: "https://internal.example.com/pipeline"
    relevance: 0.95
    date: "2026-02-01"
    key_findings: [
        "Pipeline up 35% vs last quarter",
        "Conversion rate stable at 25%",
        "Average deal size: $45K"
    ]
}

// Drivers with evidence
driver base_sales continuous {
    display_name: "Base Quarter Revenue"
    description: "Expected revenue from current customer base"
    distribution: triangular(800000, 1200000, 1800000)
    unit: "USD"
    rationale: "Based on historical renewal rates"
    evidence_refs: ["internal_pipeline"]
}

driver new_customers continuous {
    display_name: "New Customer Acquisitions"
    description: "Revenue from new customer deals"
    distribution: triangular(300000, 500000, 900000)
    unit: "USD"
    rationale: "Pipeline analysis plus market growth"
    evidence_refs: ["gartner_forecast", "internal_pipeline"]
}

model: base_sales + new_customers

simulate 10000
```

---

**Status:** ✅ Complete  
**Version:** 0.5.0  
**Last Updated:** 2026-02-05

*The Evidence System makes Fermi forecasts transparent, auditable, and collaborative. Happy forecasting! 🎯*
