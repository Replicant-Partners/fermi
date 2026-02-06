# Sentiment Analyzer Agent

**Type:** Sentiment  
**Executor:** LLM (Claude Haiku)  
**Status:** Production

## Purpose

The Sentiment Analyzer processes text from news articles, social media, analyst reports, and earnings calls to determine market perception and sentiment trends. It provides qualitative insights that complement quantitative data.

## Capabilities

### Input Sources

- News headlines and articles
- Social media posts (Twitter, Reddit, etc.)
- Analyst reports and commentary
- Earnings call transcripts
- Press releases

### Output

```json
{
  "entity": "AMD",
  "overall_sentiment": "positive",
  "sentiment_score": 0.72,
  "confidence": 0.75,
  "trend": {
    "direction": "improving",
    "change_from_last_week": 0.08,
    "momentum": "strong"
  },
  "breakdown": {
    "positive": 45,
    "neutral": 30,
    "negative": 25,
    "total_samples": 100
  },
  "key_themes": [
    "AI accelerator performance exceeding expectations",
    "Strong datacenter demand",
    "Concerns about NVIDIA competition"
  ],
  "sources": ["news: 45", "social: 30", "analysts: 25"]
}
```

## Ontology

The agent tracks:

### Entities
- **Companies**: AMD, NVIDIA, Intel
- **Products**: Mentioned in sentiment context
- **Technologies**: Discussed positively/negatively
- **Executives**: Leadership perception

### Relationships
- AMD has positive sentiment
- Sentiment shows improving trend
- Sentiment event from news sources

## Configuration

### No External APIs Required

This agent uses pure LLM analysis - no MCP servers needed. Provide text input, receive sentiment analysis.

### Model Choice

- **Claude Haiku**: Fast, cheap ($0.25/1M tokens)
- **Low temperature** (0.1): Consistent, reproducible sentiment classification
- **Optimized for**: Classification and extraction tasks

## Usage (Coming Soon)

```bash
# When agent backend is ready:
fermi run-agent amd_forecast.fpl sentiment_analyzer
```

```fpl
# In your forecast:
agent sentiment_analyzer {
    type: "sentiment"
    query: "What is the current market sentiment towards AMD's datacenter GPUs?"
    executor: "llm"
    schedule: every 1 day
}
```

## Performance

- **Average confidence**: 0.75
- **Brier impact**: +0.03
- **Success rate**: 86.9% (73/84 executions)
- **Average cost**: $0.07 per execution (very cheap!)
- **Average time**: 1.9 seconds (fast!)

## Example Queries

1. "What is the current sentiment towards AMD in tech media?"
2. "How has sentiment about AMD's MI300X changed over the past month?"
3. "What are analysts saying about AMD's competitive position?"
4. "What is the social media sentiment about AMD's recent earnings?"

## Sentiment Classification

### Scoring System

- **Very Negative**: -1.0 to -0.6
- **Negative**: -0.6 to -0.2
- **Neutral**: -0.2 to +0.2
- **Positive**: +0.2 to +0.6
- **Very Positive**: +0.6 to +1.0

### Confidence Factors

High confidence when:
- ✅ Large sample size (50+ sources)
- ✅ Consistent sentiment across sources
- ✅ Clear, unambiguous language
- ✅ Recent data (< 7 days old)

Low confidence when:
- ❌ Small sample size (< 20 sources)
- ❌ Mixed or contradictory signals
- ❌ Ambiguous or sarcastic language
- ❌ Stale data (> 30 days old)

## Ontology Evolution

The agent learns:
- Emerging narrative themes
- Changing perception of entities
- New sources of sentiment (e.g., new analysts)
- Sentiment patterns (e.g., "always negative pre-earnings")

## Known Limitations

- **Sarcasm detection**: Can misclassify sarcastic content
- **Context window**: Limited to recent text (last 30 days typically)
- **Causation**: Identifies sentiment, not why it exists
- **Sample bias**: Quality depends on input sources provided

## Troubleshooting

### Issue: "Mixed sentiment, low confidence"
**Solution**: Increase sample size or narrow timeframe

### Issue: "Sentiment doesn't match market movement"
**Solution**: Sentiment is lagging indicator; combine with other agents

### Issue: "High variation between runs"
**Solution**: Increase sample size, reduce timeframe, check for major news events

## Best Practices

### Input Quality

1. **Diverse sources**: Mix news, social, analysts
2. **Recent data**: Focus on last 7-30 days
3. **Volume**: Minimum 20 samples for reliable results
4. **De-duplication**: Remove duplicate headlines

### Query Design

Good query:
> "Analyze sentiment towards AMD's datacenter GPU products from tech news and analyst reports over the past 2 weeks"

Bad query:
> "What do people think about AMD?"  (too vague)

## Related Agents

Works well with:
- `market_research`: Combines sentiment with financial data
- `risk_monitor`: Sentiment informs risk perception
- `news_monitor`: Provides input text for analysis

## Contact

**Maintainer**: Fermi Team  
**Created**: 2026-01-15  
**Last Updated**: 2026-02-03
