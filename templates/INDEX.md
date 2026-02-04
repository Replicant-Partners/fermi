# Quick Template Index

Fast reference guide for finding the right template.

## By Goal

### 📊 Revenue Planning
- **SaaS MRR**: `business-revenue.fpl` → "Q4 2024 SaaS Revenue"
- **E-commerce GMV**: `business-revenue.fpl` → "E-commerce Annual Revenue"
- **B2B Pipeline**: `business-revenue.fpl` → "B2B Sales Pipeline"
- **Product Revenue**: `product-launch.fpl` → "SaaS Product Launch - MRR Projection"

### 🚀 Product Launch
- **Mobile App**: `product-launch.fpl` → "Mobile App Launch - First Year Users"
- **SaaS Launch**: `product-launch.fpl` → "SaaS Product Launch - MRR Projection"
- **Hardware**: `product-launch.fpl` → "Hardware Product Launch - Units Sold"
- **API Platform**: `product-launch.fpl` → "API Platform Launch - Developer Adoption"

### 📈 Marketing ROI
- **Paid Ads**: `marketing-campaigns.fpl` → "Digital Marketing Campaign ROI"
- **Content**: `marketing-campaigns.fpl` → "Content Marketing Lead Generation"
- **Influencer**: `marketing-campaigns.fpl` → "Influencer Marketing Campaign"
- **Events**: `marketing-campaigns.fpl` → "Event Marketing - Conference ROI"

### 👥 Team Planning
- **Engineering**: `hiring-costs.fpl` → "Engineering Team Expansion - Annual Cost"
- **Sales Team**: `hiring-costs.fpl` → "Sales Team Scaling - Break-Even Timeline"
- **Customer Success**: `hiring-costs.fpl` → "Customer Success Team - ROI on Retention"
- **Marketing Team**: `hiring-costs.fpl` → "Marketing Team Build-Out - Quarterly Cost"

### ☁️ Infrastructure
- **AWS Costs**: `infrastructure-costs.fpl` → "AWS Cloud Infrastructure - Annual Cost"
- **SaaS Platform**: `infrastructure-costs.fpl` → "SaaS Platform Infrastructure - Monthly Cost"
- **Kubernetes**: `infrastructure-costs.fpl` → "Kubernetes Cluster - Monthly Operating Cost"
- **Disaster Recovery**: `infrastructure-costs.fpl` → "Multi-Region DR Infrastructure - Annual Cost"

### 🎯 Market Sizing
- **B2B SaaS TAM**: `market-sizing.fpl` → "B2B SaaS Market Size - TAM/SAM/SOM"
- **Consumer App**: `market-sizing.fpl` → "Consumer Mobile App - Market Opportunity"
- **E-commerce**: `market-sizing.fpl` → "E-commerce Vertical Market Size"
- **Enterprise**: `market-sizing.fpl` → "Enterprise Software - Industry TAM"
- **API/Developer**: `market-sizing.fpl` → "API Platform - Developer Market"
- **Marketplace**: `market-sizing.fpl` → "Marketplace Platform - Two-Sided Market"

### 💰 Fundraising
- **Seed Round**: `fundraising-scenarios.fpl` → "Seed Round - 18 Month Runway"
- **Series A**: `fundraising-scenarios.fpl` → "Series A - Growth Capital Requirements"
- **Bridge Round**: `fundraising-scenarios.fpl` → "Bridge Round - Runway Extension"
- **Venture Debt**: `fundraising-scenarios.fpl` → "Venture Debt - Complement to Equity Round"
- **RBF**: `fundraising-scenarios.fpl` → "Revenue-Based Financing - Non-Dilutive Capital"
- **Path to Profit**: `fundraising-scenarios.fpl` → "Profitability Path - Cash Flow Positive Timeline"

## By Industry

### SaaS
- Business revenue, product launch, market sizing, fundraising

### E-commerce
- Business revenue, market sizing, marketing campaigns

### Enterprise Software
- Market sizing, sales team planning, customer success

### Consumer Apps
- Product launch, market sizing, marketing campaigns

### Developer Tools/APIs
- Product launch, market sizing

### Marketplace/Platform
- Market sizing, product launch

## By Time Horizon

### Short-term (< 6 months)
- Marketing campaign ROI
- Team quarterly costs
- Bridge rounds

### Medium-term (6-18 months)
- Product launches
- Seed rounds
- Infrastructure scaling

### Long-term (18+ months)
- Market sizing
- Series A planning
- Path to profitability

## Template Stats

| Template | Forecasts | Drivers | Complexity |
|----------|-----------|---------|------------|
| business-revenue.fpl | 3 | 89 | Medium |
| product-launch.fpl | 4 | 112 | High |
| marketing-campaigns.fpl | 4 | 98 | High |
| hiring-costs.fpl | 4 | 87 | Medium |
| infrastructure-costs.fpl | 4 | 93 | Medium |
| market-sizing.fpl | 6 | 127 | High |
| fundraising-scenarios.fpl | 6 | 104 | High |

**Total: 31 forecasts, 710+ drivers**

## Usage Tips

### 1. Start Simple
Begin with a single forecast that matches your need. Don't try to use everything at once.

### 2. Customize Ranges
The default ranges are industry averages. Update them with your specific data.

### 3. Remove Unnecessary Drivers
If a driver doesn't apply to your situation, delete it and simplify the estimate.

### 4. Combine Templates
You can copy drivers from multiple templates into a single file for comprehensive models.

### 5. Version Control
Keep your customized forecasts in version control to track assumption changes over time.

## Quick Start

```bash
# Copy a template
cp templates/business-revenue.fpl my-q4-forecast.fpl

# Edit with your data
zed my-q4-forecast.fpl

# Run simulation
fermi run my-q4-forecast.fpl --simulations 10000

# View results
fermi report my-q4-forecast.fpl
```

## Need Help?

- **Syntax questions**: See main [README](../README.md)
- **Examples**: Check [examples/](../examples/)
- **Documentation**: Review [docs/](../docs/)
- **Templates overview**: Read [templates/README.md](./README.md)
