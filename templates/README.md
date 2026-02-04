# FPL Forecast Templates

Comprehensive collection of real-world forecast templates for the Fermi Forecasting Language.

## Template Categories

### 1. Business Revenue (`business-revenue.fpl`)
Revenue projection templates for various business models:
- **Q4 2024 SaaS Revenue** - MRR, churn, upsells, enterprise deals
- **E-commerce Annual Revenue** - Traffic, conversion, order value with seasonality
- **B2B Sales Pipeline** - Lead generation through close, multi-stage funnel

**Use cases:** Quarterly business reviews, board reporting, sales planning

### 2. Product Launch (`product-launch.fpl`)
New product adoption and revenue forecasts:
- **Mobile App Launch** - First year user acquisition and growth
- **SaaS Product Launch** - Beta conversion, pricing tiers, MRR projection
- **Hardware Product Launch** - Pre-orders, retail, online sales
- **API Platform Launch** - Developer adoption, usage tiers

**Use cases:** Go-to-market planning, launch budgeting, milestone setting

### 3. Marketing Campaigns (`marketing-campaigns.fpl`)
Campaign ROI and performance forecasts:
- **Digital Marketing Campaign ROI** - Multi-channel (Google, Facebook, LinkedIn)
- **Content Marketing Lead Generation** - Blog posts, viral content, email nurture
- **Influencer Marketing Campaign** - Micro/macro influencers, engagement rates
- **Event Marketing** - Conference ROI, booth traffic, pipeline generation

**Use cases:** Marketing budget allocation, campaign planning, ROI analysis

### 4. Hiring Costs (`hiring-costs.fpl`)
Team scaling and compensation forecasts:
- **Engineering Team Expansion** - Recruitment, salaries, benefits, equipment
- **Sales Team Scaling** - AE/SDR quotas, commissions, break-even timeline
- **Customer Success Team** - ROI on retention, churn reduction, expansion
- **Marketing Team Build-Out** - Full marketing org costs

**Use cases:** Hiring plans, budget requests, team ROI analysis

### 5. Infrastructure Costs (`infrastructure-costs.fpl`)
Cloud and technical infrastructure projections:
- **AWS Cloud Infrastructure** - EC2, RDS, S3, data transfer, annual scaling
- **SaaS Platform Infrastructure** - Application servers, databases, CDN, monitoring
- **Kubernetes Cluster** - Worker nodes, storage, load balancing, networking
- **Multi-Region DR Infrastructure** - Disaster recovery, replication, failover

**Use cases:** Infrastructure budgeting, scaling planning, DR cost justification

### 6. Market Sizing (`market-sizing.fpl`)
TAM/SAM/SOM calculations and market opportunity:
- **B2B SaaS Market Size** - TAM/SAM/SOM methodology
- **Consumer Mobile App** - Global user base, conversion funnels
- **E-commerce Vertical** - Category GMV, competitive landscape
- **Enterprise Software** - Fortune 5000 to SMB penetration
- **API Platform** - Developer market, tier distribution
- **Marketplace Platform** - Two-sided market dynamics

**Use cases:** Pitch decks, investor presentations, strategic planning

### 7. Fundraising Scenarios (`fundraising-scenarios.fpl`)
Capital requirements and financing options:
- **Seed Round** - 18-month runway calculation
- **Series A** - Growth capital for scaling
- **Bridge Round** - Runway extension to next milestone
- **Venture Debt** - Complement to equity rounds
- **Revenue-Based Financing** - Non-dilutive capital
- **Profitability Path** - Cash flow positive timeline

**Use cases:** Fundraising prep, board discussions, financial planning

## How to Use These Templates

### 1. Copy and Customize
```bash
cp templates/business-revenue.fpl my-forecast.fpl
```
Edit the drivers to match your specific situation and assumptions.

### 2. Adjust Distributions
Each driver uses appropriate probability distributions:
- `triangular(min, mode, max)` - When you have best-case, likely, worst-case
- `normal(mean, stddev)` - For normally distributed metrics
- `lognormal(mean, stddev)` - For skewed positive values (prices, deals)
- `uniform(min, max)` - For truly unknown ranges
- `beta(alpha, beta)` - For rates and percentages with known shape

### 3. Chain Estimates
Build complex models by using earlier drivers in later calculations:
```fpl
driver base_revenue estimate units * price
driver adjusted_revenue estimate base_revenue * discount_factor
estimate adjusted_revenue * market_share
```

### 4. Run Simulations
Use the Fermi execution engine to run Monte Carlo simulations:
```bash
fermi run my-forecast.fpl --simulations 10000
```

### 5. Analyze Results
Review percentile outputs (P10, P50, P90) to understand the range of possible outcomes.

## Template Conventions

### Naming
- **Drivers** use `snake_case` and descriptive names
- **Estimates** are explicitly marked with `estimate` keyword
- **Units** are implied by names (e.g., `_monthly`, `_annual`, `_pct`)

### Distribution Selection Guide
- **triangular()** - Default choice when you can estimate min/mode/max
- **normal()** - Use for stable, symmetric distributions
- **lognormal()** - Use for prices, salaries, deal sizes (right-skewed)
- **uniform()** - Use when truly uncertain (conversion rates without data)
- **beta()** - Use for known probability distributions

### Time Periods
- Monthly metrics: `_monthly` or `_per_month`
- Annual metrics: `_annual` or `_yearly`
- Cumulative: `_total` or `_cumulative`
- Growth rates: `_rate` or `_growth_rate`

### Percentages
- Use decimals, not percentages: `0.15` not `15`
- Name with `_pct` or `_rate` suffix
- Example: `churn_rate uniform(0.02, 0.05)` means 2-5%

## Best Practices

### 1. Start Conservative
Use conservative ranges for your drivers. It's better to be pleasantly surprised than miss projections.

### 2. Validate Assumptions
Reference real data when possible:
- Industry benchmarks
- Historical performance
- Competitor data
- Customer surveys

### 3. Document Sources
Add comments explaining where numbers come from:
```fpl
// Industry average churn: 5-8% annually (ChurnZero 2024 Report)
driver annual_churn_rate uniform(0.05, 0.08)
```

### 4. Test Sensitivity
Run simulations and see which drivers have the biggest impact on outcomes.

### 5. Update Regularly
As actual data comes in, update your distributions to be more accurate.

## Contributing Templates

Have a useful forecast template? Submit a PR with:
1. The `.fpl` file in the `templates/` directory
2. Clear comments explaining the use case
3. Reasonable default ranges based on research
4. Update to this README with description

## Questions?

- Check the main [README](../README.md) for FPL syntax
- Review [examples/](../examples/) for simpler forecasts
- See [docs/](../docs/) for detailed language documentation

## License

These templates are provided as examples and starting points. Adjust them for your specific situation - they are not financial advice!
