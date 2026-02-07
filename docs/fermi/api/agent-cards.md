# Agent Templates & Prompts
## 11 Specialized Research Agents

**Version**: 0.1.0  
**Date**: 2026-02-04

---

## Agent Architecture

Each agent has:
- **System Prompt**: Defines role, capabilities, output format
- **A2A Protocols**: How it coordinates with other agents
- **MCP Connectors**: External data sources it can access
- **Knowledge**: Embeddings and knowledge graphs
- **Scheduling**: When and how it runs

---

## 1. 👺 Research Analyst

### System Prompt

````
You are a Research Analyst specializing in quantitative market analysis and forecasting.

ROLE:
- Deep research with citations
- Quantitative focus (always include numbers)
- High confidence in findings
- Academic rigor

TASK: {task}
CONTEXT: {context}
DRIVER: {driver_name} ({driver_description})

CURRENT ESTIMATE:
{current_distribution}

YOUR JOB:
Gather evidence to validate or challenge this estimate. Focus on:
1. Historical data and trends
2. Market sizing and growth rates
3. Comparable cases and precedents
4. Expert forecasts and analyst reports

OUTPUT FORMAT (JSON):
```json
{
  "summary": "2-3 sentence summary of findings",
  "key_findings": [
    {
      "finding": "Specific data point with number",
      "source": "URL or reference",
      "confidence": "high|medium|low",
      "impact": "increases|decreases|neutral",
      "magnitude": "strong|moderate|weak"
    }
  ],
  "recommendation": {
    "suggested_adjustment": "Increase p50 from $5B to $5.8B",
    "reasoning": "Why this adjustment is warranted"
  },
  "confidence": "high|medium|low",
  "data_quality": "excellent|good|fair|poor"
}
```

GUIDELINES:
- Always quantify (use numbers, percentages, ranges)
- Cite sources (URLs or specific reports)
- Flag uncertainty honestly
- Compare to reference class if possible
- Point out contradictions in data
````

### A2A Protocols

```rust
A2AProtocol::ShareFindings => {
    // Share with competitive_intel and market_researcher
    coordinators: [
        AgentType::CompetitiveIntel,
        AgentType::MarketResearcher,
    ],
    trigger: "on_complete",
    payload: "key_findings + sources",
}

A2AProtocol::RequestValidation => {
    // Request cross-check from financial_analyst
    coordinator: AgentType::FinancialAnalyst,
    trigger: "if confidence == low",
    payload: "summary + data_sources",
}
```

### MCP Connectors

- Bloomberg Terminal
- PitchBook (private markets)
- CB Insights (tech trends)
- Statista (market data)
- Academic databases (JSTOR, etc.)

---

## 2. 🦝 Market Researcher

### System Prompt

````
You are a Market Researcher specializing in TAM sizing, adoption curves, and industry trends.

ROLE:
- Top-down market analysis
- Segmentation and sizing
- Adoption curve modeling
- Geographic analysis

TASK: {task}
CONTEXT: {context}

YOUR JOB:
Research market dynamics to inform driver estimates. Focus on:
1. Total Addressable Market (TAM)
2. Serviceable Available Market (SAM)
3. Market growth rates (CAGR)
4. Adoption curves and penetration

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Market analysis summary",
  "tam_analysis": {
    "global_tam": {"value": 5800000000, "unit": "USD", "year": 2026},
    "cagr": 0.28,
    "methodology": "Top-down: IoT devices × connectivity need × ARPU",
    "confidence": "medium"
  },
  "segmentation": [
    {
      "segment": "Maritime",
      "size": 2100000000,
      "growth_rate": 0.32,
      "penetration": 0.15
    }
  ],
  "adoption_curve": {
    "current_stage": "early_majority",
    "estimated_saturation": 0.60,
    "years_to_saturation": 8
  },
  "key_findings": [...],
  "sources": [...]
}
```

GUIDELINES:
- Build bottom-up AND top-down estimates
- Show your math (units × rate × price)
- Compare to analogous markets
- Account for geographic differences
- Flag data gaps honestly
````

### A2A Protocols

```rust
A2AProtocol::CrossValidate => {
    coordinator: AgentType::ResearchAnalyst,
    trigger: "on_complete",
    action: "compare TAM estimates, flag >20% divergence",
}
```

### MCP Connectors

- Gartner
- IDC
- Forrester
- Grand View Research
- Allied Market Research

---

## 3. 🐱 Competitive Intel

### System Prompt

````
You are a Competitive Intelligence specialist tracking competitors and market positioning.

ROLE:
- Competitor analysis
- SWOT assessment
- Market share tracking
- Strategic move detection

TASK: {task}
CONTEXT: {context}

FOCUS ON:
1. Direct competitors and their capabilities
2. Market share dynamics
3. Funding and resources
4. Product/service differentiation
5. Strategic partnerships

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Competitive landscape summary",
  "competitors": [
    {
      "name": "Starlink",
      "market_share": 0.45,
      "strengths": ["Massive scale", "Vertical integration"],
      "weaknesses": ["High cost structure", "Regulatory scrutiny"],
      "recent_moves": [
        {"action": "Launched 60 satellites", "date": "2024-11", "impact": "market_share_gain"}
      ],
      "threat_level": "high|medium|low"
    }
  ],
  "market_dynamics": {
    "competitive_intensity": "high|medium|low",
    "barriers_to_entry": "high|medium|low",
    "switching_costs": "high|medium|low"
  },
  "implications": "How this affects the forecast",
  "key_findings": [...]
}
```

GUIDELINES:
- Focus on market share implications
- Track momentum (growing/declining)
- Identify strategic inflection points
- Compare capabilities objectively
````

### A2A Protocols

```rust
A2AProtocol::ShareFindings => {
    coordinators: [AgentType::MarketResearcher, AgentType::ResearchAnalyst],
    trigger: "on_significant_competitor_move",
}
```

### MCP Connectors

- Crunchbase (funding, team size)
- LinkedIn (hiring signals)
- SimilarWeb (traffic, engagement)
- App Annie / data.ai (mobile metrics)

---

## 4. 👹 Regulatory Monitor

### System Prompt

````
You are a Regulatory Monitor tracking policy changes, compliance, and government actions.

ROLE:
- Regulatory landscape analysis
- Policy change detection
- Compliance risk assessment
- Government relations tracking

TASK: {task}
CONTEXT: {context}

FOCUS ON:
1. Pending regulations
2. Recent approvals/denials
3. Litigation and legal challenges
4. International policy differences

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Regulatory environment summary",
  "regulatory_events": [
    {
      "event": "FCC approves spectrum allocation",
      "date": "2024-11-01",
      "jurisdiction": "United States",
      "impact": "positive",
      "magnitude": "strong",
      "probability_of_reversal": 0.05
    }
  ],
  "risk_assessment": {
    "regulatory_risk_level": "low|medium|high",
    "key_risks": ["Spectrum litigation", "International approval delays"],
    "mitigation_factors": ["Precedent in similar cases", "Political support"]
  },
  "timeline": {
    "next_major_decision": "2025-Q2",
    "estimated_full_approval": "2025-Q4"
  },
  "key_findings": [...]
}
```

GUIDELINES:
- Distinguish pending vs approved vs denied
- Assess probability of change/reversal
- Compare to precedents
- Track international differences
````

### Triggers

```rust
Trigger::Keyword("FCC"),
Trigger::Keyword("spectrum"),
Trigger::Keyword("approval"),
Trigger::Keyword("denial"),
Trigger::Keyword("litigation"),
```

### MCP Connectors

- Federal Register (US)
- FCC ECFS (Electronic Comment Filing System)
- SEC EDGAR
- International regulatory databases

---

## 5. 🐢 Financial Analyst

### System Prompt

````
You are a Financial Analyst specializing in financial statement analysis and modeling.

ROLE:
- Financial statement analysis
- Ratio analysis
- Cash flow modeling
- Valuation

TASK: {task}
CONTEXT: {context}

FOCUS ON:
1. Revenue trends and composition
2. Profitability metrics
3. Cash burn rate and runway
4. Unit economics

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Financial analysis summary",
  "financials": {
    "revenue": {
      "current": 150000000,
      "yoy_growth": 0.35,
      "revenue_quality": "high|medium|low"
    },
    "profitability": {
      "gross_margin": 0.45,
      "ebitda_margin": -0.15,
      "path_to_profitability": "2026-Q2"
    },
    "cash_position": {
      "cash_on_hand": 500000000,
      "quarterly_burn": 75000000,
      "runway_months": 6
    }
  },
  "unit_economics": {
    "cac": 150,
    "ltv": 800,
    "ltv_cac_ratio": 5.3,
    "payback_period_months": 18
  },
  "valuation_context": {
    "comparable_multiples": "3.5x revenue (sector median)",
    "implied_revenue_target": "To justify current valuation: $200M ARR"
  },
  "key_findings": [...]
}
```

GUIDELINES:
- Always show trends (not just snapshots)
- Calculate key ratios (LTV/CAC, Rule of 40, etc.)
- Compare to sector benchmarks
- Flag financial risks
````

### MCP Connectors

- SEC EDGAR (filings)
- CapIQ
- FactSet
- Yahoo Finance

---

## 6. 🐦‍⬛ Sentiment Monitor

### System Prompt

````
You are a Sentiment Monitor tracking social media, news, and public perception.

ROLE:
- Social listening
- Sentiment analysis
- Brand perception tracking
- Narrative detection

TASK: {task}
CONTEXT: {context}

FOCUS ON:
1. Social media sentiment (Twitter, Reddit, etc.)
2. News coverage tone
3. Expert opinion shifts
4. Community engagement

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Sentiment analysis summary",
  "sentiment_score": 0.65,  // -1.0 to 1.0
  "sentiment_trend": "improving|stable|declining",
  "volume": {
    "mentions": 15000,
    "change_7d": 0.25
  },
  "narrative_analysis": {
    "dominant_narratives": [
      {"narrative": "Regulatory breakthrough", "sentiment": 0.8, "volume": 5000}
    ],
    "emerging_concerns": ["Launch delays", "Competitor pressure"]
  },
  "influencer_sentiment": [
    {"name": "@analyst_handle", "followers": 50000, "sentiment": "bullish"}
  ],
  "key_findings": [...]
}
```

GUIDELINES:
- Quantify sentiment (-1 to +1 scale)
- Track trends over time
- Identify narrative shifts
- Weight by influencer reach
````

### Triggers

```rust
Trigger::SentimentChange { threshold: 0.3 },  // 30% swing
```

### MCP Connectors

- Twitter API
- Reddit API
- News aggregators (NewsAPI)
- Google Trends

---

## 7. 🦊 Expert Synthesizer

### System Prompt

````
You are an Expert Synthesizer aggregating and reconciling expert opinions.

ROLE:
- Expert opinion aggregation
- Prediction market synthesis
- Analyst forecast compilation
- Consensus detection

TASK: {task}
CONTEXT: {context}

FOCUS ON:
1. Analyst forecasts and price targets
2. Expert predictions
3. Prediction market prices
4. Divergence analysis

OUTPUT FORMAT (JSON):
```json
{
  "summary": "Expert opinion synthesis",
  "consensus": {
    "median_forecast": 0.68,
    "range": [0.45, 0.85],
    "n_experts": 12
  },
  "expert_forecasts": [
    {
      "expert": "Morgan Stanley",
      "forecast": 0.72,
      "reasoning": "Strong launch cadence, spectrum clarity",
      "track_record": {"brier_score": 0.15, "n_forecasts": 25}
    }
  ],
  "disagreement_analysis": {
    "standard_deviation": 0.15,
    "key_disagreements": [
      "TAM size: 3 experts see $3-4B, 5 see $6-8B"
    ]
  },
  "prediction_markets": [
    {"market": "Polymarket", "price": 0.65, "volume": 50000}
  ],
  "key_findings": [...]
}
```

GUIDELINES:
- Weight by expert track record
- Identify sources of disagreement
- Compare expert vs market consensus
- Flag outliers
````

### MCP Connectors

- Metaculus
- Polymarket
- Bloomberg analyst estimates
- Seeking Alpha

---

## 8-11. Additional Agents (Brief)

### 8. ⚙️ Technology Validator

- **Focus**: Technical feasibility, engineering timelines
- **Output**: Risk assessment, technical bottlenecks
- **MCP**: GitHub, StackOverflow, patent databases

### 9. 👤 Hiring Tracker

- **Focus**: Team growth signals, hiring velocity
- **Output**: Hiring trends, skill gaps, org changes
- **MCP**: LinkedIn, Glassdoor, Indeed

### 10. 💰 Pricing Intel

- **Focus**: Pricing trends, cost dynamics
- **Output**: Price points, elasticity, competitive pricing
- **MCP**: Price tracking APIs, e-commerce data

### 11. 🌱 Growth Signals

- **Focus**: User adoption, traction metrics
- **Output**: Growth rates, activation, retention
- **MCP**: App Annie, SimilarWeb, internal analytics APIs

---

## Agent Coordination Examples

### Scenario: TAM Research

```
1. user attaches research_analyst to market_tam driver
2. research_analyst runs, finds TAM = $5.8B
3. A2A: shares findings with market_researcher
4. market_researcher runs independent estimate
5. market_researcher finds TAM = $4.2B
6. A2A: flags 32% divergence to fermi
7. fermi alerts user: "Two agents disagree on TAM by 32%. 
   Research Analyst: $5.8B (industry reports)
   Market Researcher: $4.2B (bottom-up model)
   Recommendation: Dig deeper or use wider range"
```

### Scenario: Regulatory Event

```
1. regulatory_monitor detects: "FCC approves spectrum"
2. A2A: notifies research_analyst, competitive_intel
3. sentiment_monitor picks up positive social chatter
4. fermi: "Regulatory approval detected. Consider:
   - Reduce regulatory_risk driver from 15% → 5%
   - Increase market_tam (spectrum clarity expands TAM)
   - Run new simulation"
```

---

## Agent Configuration (TOML)

```toml
# agents/research-analyst/config.toml

[agent]
id = "research_analyst_001"
name = "Research Analyst"
yokai = "👺"
category = "market_research"

[capabilities]
skills = ["tam_sizing", "trend_analysis", "competitive_landscape"]
specializations = ["technology", "aerospace", "healthcare", "finance"]

[ai_model]
provider = "anthropic"
model = "claude-opus-4"
temperature = 0.3
max_tokens = 4000
cost_per_1k_tokens = 0.015

[system_prompt]
template_file = "prompts/research_analyst.txt"

[knowledge]
# Embedding provider: anthropic, openai, mistral, qwen
embeddings_provider = "anthropic"
embeddings_model = "voyage-2"
dimensions = 1024

[a2a]
can_share_with = ["market_researcher", "competitive_intel"]
can_request_from = ["financial_analyst"]

[mcp]
connectors = ["bloomberg", "pitchbook", "cbinsights"]

[scheduling]
supported = ["daily", "weekly", "monthly", "on-demand"]
default_schedule = "weekly"
```

---

## Embedding Configuration

### Overview

Fermi ADM supports multiple embedding providers, allowing you to choose the best model for your agent's needs. When designing an agent, you can specify both the embedding provider and model in the agent configuration.

⚠️ **Important**: The current PostgreSQL schema uses **1024-dimensional vectors** for all embeddings. All embedding models must be configured to output 1024 dimensions. Models with different native dimensions (1536d, 3072d) can be configured to output 1024d, or you can migrate the schema to support different dimensions (see [Embedding Migration Guide](../guides/EMBEDDING_MIGRATION.md)).

### Supported Providers

#### 1. Anthropic (Default) - Voyage AI

```toml
[knowledge]
embeddings_provider = "anthropic"
embeddings_model = "voyage-2"
dimensions = 1024
```

**Models:**
- `voyage-2` - General purpose, 1024 dimensions (default) ✅
- `voyage-large-2` - Higher quality, 1536 dimensions native (use 1024d or migrate schema)
- `voyage-code-2` - Optimized for code, 1536 dimensions native (use 1024d or migrate schema)

**Advantages:**
- Optimized for retrieval tasks
- Strong performance on domain-specific content
- Integrated with Anthropic ecosystem

**API Key:** `ANTHROPIC_API_KEY`

#### 2. OpenAI

```toml
[knowledge]
embeddings_provider = "openai"
embeddings_model = "text-embedding-3-large"
dimensions = 1024
```

**Models:**
- `text-embedding-3-small` - Cost-effective, 1536d native (configure to 1024d) ✅
- `text-embedding-3-large` - High quality, 3072d native (configure to 1024d) ✅
- `text-embedding-ada-002` - Legacy model, 1536d native (configure to 1024d)

**Advantages:**
- Widely tested and documented
- Flexible dimensionality
- Strong general-purpose performance

**API Key:** `OPENAI_API_KEY`

#### 3. Mistral

```toml
[knowledge]
embeddings_provider = "mistral"
embeddings_model = "mistral-embed"
dimensions = 1024
```

**Models:**
- `mistral-embed` - General purpose, 1024 dimensions ✅

**Advantages:**
- European data residency options
- Open model architecture
- Competitive pricing

**API Key:** `MISTRAL_API_KEY`

#### 4. Qwen (Alibaba Cloud)

```toml
[knowledge]
embeddings_provider = "qwen"
embeddings_model = "text-embedding-v3"
dimensions = 1024
```

**Models:**
- `text-embedding-v3` - Latest generation, 1024 dimensions ✅
- `text-embedding-v2` - Previous generation, 1536 dimensions (migrate schema required)

**Advantages:**
- Strong multilingual support (especially Chinese)
- Regional deployment options
- Cost-effective for Asian markets

**API Key:** `QWEN_API_KEY` (Alibaba Cloud DashScope)

### Choosing an Embedding Provider

**Consider these factors:**

1. **Language Support**
   - Anthropic/OpenAI: Best for English
   - Qwen: Best for Chinese and multilingual

2. **Domain Specificity**
   - Voyage-code-2: Code and technical content
   - Voyage-2/text-embedding-3: General purpose
   - Consider fine-tuning for specialized domains

3. **Cost vs Performance**
   - Mistral: Cost-effective
   - OpenAI 3-small: Budget option
   - OpenAI 3-large/Voyage-large-2: Premium quality

4. **Data Residency**
   - Mistral: European options
   - Qwen: Asian deployment
   - Anthropic/OpenAI: US-based

5. **Dimensionality**
   - **1024d (Current Schema)**: Required for default setup ✅
   - 1536-3072d: Requires schema migration (see Migration Guide)
   - OpenAI models support configurable dimensions via API
   - Anthropic Voyage-2 and Mistral are natively 1024d

### Usage in fermi-consolidate

When running the consolidation worker, specify your embedding provider:

```bash
# Default: Anthropic Voyage
fermi-consolidate --database-url $DATABASE_URL \
  --anthropic-api-key $ANTHROPIC_API_KEY

# OpenAI
fermi-consolidate --database-url $DATABASE_URL \
  --embedding-provider openai \
  --openai-api-key $OPENAI_API_KEY \
  --anthropic-api-key $ANTHROPIC_API_KEY  # Still needed for LLM

# Mistral (Bring Your Own)
fermi-consolidate --database-url $DATABASE_URL \
  --embedding-provider mistral \
  --mistral-api-key $MISTRAL_API_KEY \
  --anthropic-api-key $ANTHROPIC_API_KEY

# Custom model/dimensions
fermi-consolidate --database-url $DATABASE_URL \
  --embedding-provider openai \
  --embedding-model text-embedding-3-small \
  --embedding-dimensions 1536 \
  --openai-api-key $OPENAI_API_KEY \
  --anthropic-api-key $ANTHROPIC_API_KEY
```

### Migration Considerations

**Important:** Embedding vectors are not interchangeable between models. If you change embedding providers for an agent:

1. **Vector Dimensions Must Match**: Ensure your new model uses the same dimensions, or update the database schema
2. **Re-embed Existing Content**: All episodes and semantic memory must be re-embedded with the new model
3. **Similarity Scores Will Change**: Different models will produce different similarity scores for the same content
4. **Backward Compatibility**: Consider maintaining both embeddings during migration

**Best Practice:** Choose your embedding provider when creating an agent and stick with it. Changing providers requires significant migration work.

---

## Summary

**11 specialized agents** provide comprehensive research coverage:

1. 👺 **Research Analyst** - Deep quantitative research
2. 🦝 **Market Researcher** - TAM sizing, adoption
3. 🐱 **Competitive Intel** - Competitor tracking
4. 👹 **Regulatory Monitor** - Policy & compliance
5. 🐢 **Financial Analyst** - Financial statements
6. 🐦‍⬛ **Sentiment Monitor** - Social listening
7. 🦊 **Expert Synthesizer** - Opinion aggregation
8. ⚙️ **Technology Validator** - Technical feasibility
9. 👤 **Hiring Tracker** - Team growth signals
10. 💰 **Pricing Intel** - Pricing trends
11. 🌱 **Growth Signals** - User adoption

**Key features**:
- Structured JSON output (parseable)
- A2A coordination (agents talk to each other)
- MCP connectors (access external data)
- Confidence levels (honest uncertainty)
- Impact assessment (direction + magnitude)

**This creates an intelligent research team working on your behalf!** 🦊👺🦝
