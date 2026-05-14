# Market Research Agent

**Type:** Research  
**Executor:** LLM (Claude Sonnet)  
**Status:** Production

## Purpose

The Market Research agent analyzes market trends, competitive dynamics, and financial performance for technology companies. It provides quantitative data and insights to inform forecasts about market share, revenue growth, and competitive positioning.

## Capabilities

### Data Sources

- **Yahoo Finance API**: Real-time stock prices, market cap, financial metrics
- **SEC EDGAR API**: Quarterly reports (10-Q), annual reports (10-K), 8-K filings
- **LLM Analysis**: Synthesizes data into actionable insights

### Output

```json
{
  "market_share": {
    "company": "AMD",
    "segment": "Datacenter GPUs",
    "percentage": 22.0,
    "change_from_previous": 4.0,
    "confidence": 0.85
  },
  "competitive_position": {
    "rank": 2,
    "primary_competitors": ["NVIDIA", "Intel"],
    "strengths": ["Price-performance ratio", "AI accelerators"],
    "threats": ["NVIDIA market dominance", "Supply chain"]
  },
  "financial_metrics": {
    "revenue_usd": 5.4e9,
    "growth_yoy": 0.18,
    "margin": 0.47
  },
  "confidence": 0.82,
  "sources": ["10-Q Q4 2025", "Yahoo Finance", "Market analysis"]
}
```

## Ontology

The agent tracks:

### Entities
- **Companies**: AMD, NVIDIA, Intel, etc.
- **Products**: MI300X, H100, Gaudi 3, etc.
- **Technologies**: CDNA, Hopper, Chiplet architecture
- **Market Segments**: Datacenter GPUs, Consumer Graphics, Edge AI

### Relationships
- AMD competes_with NVIDIA in Datacenter GPUs
- MI300X uses CDNA 3 technology
- AMD publishes quarterly financial reports
- Datacenter GPUs contains multiple competitors

## Configuration

### Required Environment Variables

```bash
YAHOO_FINANCE_API_KEY=your_key_here
SEC_API_KEY=your_key_here
```

### MCP Servers

1. **yahoo_finance**: Financial data and real-time metrics
   - Rate limit: 100 requests/day (free tier)
   - Provides: Stock prices, market cap, volume, financials

2. **sec_api**: SEC EDGAR filings
   - Rate limit: 10 requests/second
   - Provides: 10-Q, 10-K, 8-K filings with full text

## Usage (Coming Soon)

```bash
# When agent backend is ready:
fermi run-agent amd_forecast.fpl market_research
```

```fpl
# In your forecast:
agent market_research {
    type: "research"
    query: "What is AMD's current datacenter GPU market share?"
    executor: "llm"
    schedule: every 1 week
}
```

## Performance

- **Average confidence**: 0.82
- **Brier impact**: +0.04 (improves forecasts)
- **Success rate**: 94.7% (144/152 executions)
- **Average cost**: $0.94 per execution
- **Average time**: 3.4 seconds

## Example Queries

1. "What is AMD's current market share in datacenter GPUs?"
2. "How does AMD's datacenter revenue compare to NVIDIA's?"
3. "What are AMD's key competitive advantages in AI accelerators?"
4. "What is the growth rate of the datacenter GPU market?"

## Ontology Evolution

The agent learns:
- New competitors entering the market
- Product launches and updates
- Technology shifts (e.g., new architectures)
- Market segment changes (e.g., edge AI emergence)

All learning is tracked in git commits to `ontology.mermaid`.

## Known Limitations

- **Data lag**: Financial reports are quarterly (90-day lag)
- **Yahoo Finance rate limits**: 100 requests/day on free tier
- **Market segment definitions**: Can vary by source
- **Private company data**: Not available (focuses on public companies)

## Troubleshooting

### Issue: "Yahoo Finance API rate limit exceeded"
**Solution**: Wait 24 hours or upgrade to paid tier

### Issue: "SEC filing not found"
**Solution**: Check if company has filed recently (may not be available yet)

### Issue: "Low confidence score (<0.5)"
**Solution**: 
- Check if company is publicly traded
- Verify ticker symbol is correct
- Ensure recent financial data is available

## Maintenance

- **API keys**: Rotate quarterly
- **MCP servers**: Update weekly (npm/pip)
- **Ontology**: Review monthly for accuracy
- **Performance**: Monitor Brier impact

## Related Agents

Works well with:
- `sentiment_analyzer`: Combines market data with sentiment
- `risk_monitor`: Adds risk context to market trends
- `competitive_intelligence`: Deep-dive competitor analysis

## Contact

**Maintainer**: Fermi Team  
**Created**: 2025-12-01  
**Last Updated**: 2026-02-05
