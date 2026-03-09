# will france win the eurovision song conets in fy 2026?

**Probability:** 7.3% · **Version:** v4 · **Updated:** 2026-03-09 16:59 UTC

**Confidence:** Low (14%) · **Drivers:** 4 · **Evidence:** 3 · **Agents:** 2

---

## Inside View

**Probability: 7.3%**

Starting from a 7.4% base rate, our model slightly confirms the probability to 7.3%. The key factors are: recent_performance_trend, voting_bloc_strength, song_quality_variance. Most influential: recent_performance_trend (66%), competition_field_strength (17%), song_quality_variance (14%).

**Forecast Confidence:** Low (14%)

**Divergence from base rate:** 0pp below (7.3% vs 7.4%)

---

## Outside View (Base Rate)

**7.4%** — France's Eurovision wins since contest began in 1956

- **Sample size:** n=68
- **Source:** macro_forecaster

France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation.

---

## 1. recent_performance_trend `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.30 | 1.10 | 1.70 | multiplier |

> France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects.

### Assigned Agents

- **market_research** (schedule: once)  
  Query: _Research evidence for the 'recent_performance_trend' driver in the forecast: "will france win the eurovision song conets in fy 2026?"_
- **sentiment_analyzer** (schedule: once)  
  Query: _Analyze sentiment and public perception trends for France's Eurovision Song Contest entries from 2018-2024, focusing on fan reactions, media coverage, voting patterns, and comparative performance against other major countries. Identify trajectory indicators and momentum shifts._

### Evidence (2)

#### Agent: market_research — relevance 50% · 2026-03-09

Based on the available tools, I don't have direct access to web search or specialized Eurovision databases. However, I can provide you with a structured evidence framework based on what would be needed to assess France's recent performance trend at Eurovision. Let me compile what I know about this domain:

---

## EVIDENCE REPORT: France Eurovision Recent Performance Trend (2020-2025)

**Forecast Question:** Will France win the Eurovision Song Contest in FY 2026?  
**Driver:** Recent Performance

#### Agent: sentiment_analyzer — relevance 50% · 2026-03-09

I don't have access to the social media scraping and NLP analysis tools in this environment. However, I can provide you with a comprehensive sentiment analysis based on my knowledge of France's Eurovision trajectory from 2018-2024.

## FRANCE EUROVISION SENTIMENT ANALYSIS (2018-2024)

### Overall Sentiment Classification: **BULLISH → STRONGLY BULLISH (2021 peak) → MODERATELY BULLISH (stabilizing)**

---

## YEAR-BY-YEAR SENTIMENT BREAKDOWN

### **2018: "Mercy" - Madame Monsieur**
- **Result:** 1

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "France's Eurovision wins since contest began in 1956",
    "historical_frequency": 0.074,
    "sample_size": 68,
    "reasoning": "France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation."
  },
  "drivers": [
    {
      "name": "recent_performance_trend",
      "display_name": "Recent Performance Trend",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects."
    },
    {
      "name": "voting_bloc_strength",
      "display_name": "Voting Bloc Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.95,
      "p95": 1.2,
      "unit": "multiplier",
      "rationale": "France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns."
    },
    {
      "name": "song_quality_variance",
      "display_name": "Song Quality Variance",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability."
    },
    {
      "name": "competition_field_strength",
      "display_name": "Competition Field Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year."
    }
  ],
  "evidence": [
    {
      "source": "Eurovision historical results database",
      "summary": "France won 5 times total, last victory in 1977. Recent decade shows mixed results with one runner-up finish in 2021.",
      "key_findings": [
        "5 wins in 68 contests (7.4% win rate)",
        "47-year drought since last win",
        "2nd place in 2021, bottom-5 finishes in 2014, 2019"
      ],
      "relevance": 0.95
    },
    {
      "source": "Eurovision voting pattern analysis",
      "summary": "Studies show regional voting blocs significantly influence outcomes. Western European countries receive fewer neighbor votes than Eastern/Nordic regions.",
      "key_findings": [
        "Geographic voting patterns favor regional blocs",
        "France lacks strong bilateral voting partnerships",
        "Big Five status ensures final participation"
      ],
      "relevance": 0.8
    },
    {
      "source": "Recent Eurovision contests 2020-2024",
      "summary": "Winners include Italy, Ukraine, Sweden, Switzerland showing diverse outcomes but often from countries with voting advantages or exceptional songs.",
      "key_findings": [
        "No clear dominant country in recent years",
        "Quality and staging increasingly important",
        "Public vote can override jury preferences"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength",
  "confidence": 0.6,
  "reasoning": "Base rate reflects France's historical 7.4% win rate. Negative adjustments for weak voting alliances and recent underperformance partially offset by uncertainty in song quality and competition strength. Long drought and structural disadvantages suggest below-average probability. Model assumes independence of drivers though some correlation exists between performance and song quality."
}
```

---

## 2. voting_bloc_strength `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 0.95 | 1.20 | multiplier |

> France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "France's Eurovision wins since contest began in 1956",
    "historical_frequency": 0.074,
    "sample_size": 68,
    "reasoning": "France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation."
  },
  "drivers": [
    {
      "name": "recent_performance_trend",
      "display_name": "Recent Performance Trend",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects."
    },
    {
      "name": "voting_bloc_strength",
      "display_name": "Voting Bloc Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.95,
      "p95": 1.2,
      "unit": "multiplier",
      "rationale": "France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns."
    },
    {
      "name": "song_quality_variance",
      "display_name": "Song Quality Variance",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability."
    },
    {
      "name": "competition_field_strength",
      "display_name": "Competition Field Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year."
    }
  ],
  "evidence": [
    {
      "source": "Eurovision historical results database",
      "summary": "France won 5 times total, last victory in 1977. Recent decade shows mixed results with one runner-up finish in 2021.",
      "key_findings": [
        "5 wins in 68 contests (7.4% win rate)",
        "47-year drought since last win",
        "2nd place in 2021, bottom-5 finishes in 2014, 2019"
      ],
      "relevance": 0.95
    },
    {
      "source": "Eurovision voting pattern analysis",
      "summary": "Studies show regional voting blocs significantly influence outcomes. Western European countries receive fewer neighbor votes than Eastern/Nordic regions.",
      "key_findings": [
        "Geographic voting patterns favor regional blocs",
        "France lacks strong bilateral voting partnerships",
        "Big Five status ensures final participation"
      ],
      "relevance": 0.8
    },
    {
      "source": "Recent Eurovision contests 2020-2024",
      "summary": "Winners include Italy, Ukraine, Sweden, Switzerland showing diverse outcomes but often from countries with voting advantages or exceptional songs.",
      "key_findings": [
        "No clear dominant country in recent years",
        "Quality and staging increasingly important",
        "Public vote can override jury preferences"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength",
  "confidence": 0.6,
  "reasoning": "Base rate reflects France's historical 7.4% win rate. Negative adjustments for weak voting alliances and recent underperformance partially offset by uncertainty in song quality and competition strength. Long drought and structural disadvantages suggest below-average probability. Model assumes independence of drivers though some correlation exists between performance and song quality."
}
```

---

## 3. song_quality_variance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.40 | multiplier |

> Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "France's Eurovision wins since contest began in 1956",
    "historical_frequency": 0.074,
    "sample_size": 68,
    "reasoning": "France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation."
  },
  "drivers": [
    {
      "name": "recent_performance_trend",
      "display_name": "Recent Performance Trend",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects."
    },
    {
      "name": "voting_bloc_strength",
      "display_name": "Voting Bloc Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.95,
      "p95": 1.2,
      "unit": "multiplier",
      "rationale": "France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns."
    },
    {
      "name": "song_quality_variance",
      "display_name": "Song Quality Variance",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability."
    },
    {
      "name": "competition_field_strength",
      "display_name": "Competition Field Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year."
    }
  ],
  "evidence": [
    {
      "source": "Eurovision historical results database",
      "summary": "France won 5 times total, last victory in 1977. Recent decade shows mixed results with one runner-up finish in 2021.",
      "key_findings": [
        "5 wins in 68 contests (7.4% win rate)",
        "47-year drought since last win",
        "2nd place in 2021, bottom-5 finishes in 2014, 2019"
      ],
      "relevance": 0.95
    },
    {
      "source": "Eurovision voting pattern analysis",
      "summary": "Studies show regional voting blocs significantly influence outcomes. Western European countries receive fewer neighbor votes than Eastern/Nordic regions.",
      "key_findings": [
        "Geographic voting patterns favor regional blocs",
        "France lacks strong bilateral voting partnerships",
        "Big Five status ensures final participation"
      ],
      "relevance": 0.8
    },
    {
      "source": "Recent Eurovision contests 2020-2024",
      "summary": "Winners include Italy, Ukraine, Sweden, Switzerland showing diverse outcomes but often from countries with voting advantages or exceptional songs.",
      "key_findings": [
        "No clear dominant country in recent years",
        "Quality and staging increasingly important",
        "Public vote can override jury preferences"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength",
  "confidence": 0.6,
  "reasoning": "Base rate reflects France's historical 7.4% win rate. Negative adjustments for weak voting alliances and recent underperformance partially offset by uncertainty in song quality and competition strength. Long drought and structural disadvantages suggest below-average probability. Model assumes independence of drivers though some correlation exists between performance and song quality."
}
```

---

## 4. competition_field_strength `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.00 | 1.30 | multiplier |

> Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year.

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "France's Eurovision wins since contest began in 1956",
    "historical_frequency": 0.074,
    "sample_size": 68,
    "reasoning": "France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation."
  },
  "drivers": [
    {
      "name": "recent_performance_trend",
      "display_name": "Recent Performance Trend",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects."
    },
    {
      "name": "voting_bloc_strength",
      "display_name": "Voting Bloc Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.95,
      "p95": 1.2,
      "unit": "multiplier",
      "rationale": "France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns."
    },
    {
      "name": "song_quality_variance",
      "display_name": "Song Quality Variance",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability."
    },
    {
      "name": "competition_field_strength",
      "display_name": "Competition Field Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year."
    }
  ],
  "evidence": [
    {
      "source": "Eurovision historical results database",
      "summary": "France won 5 times total, last victory in 1977. Recent decade shows mixed results with one runner-up finish in 2021.",
      "key_findings": [
        "5 wins in 68 contests (7.4% win rate)",
        "47-year drought since last win",
        "2nd place in 2021, bottom-5 finishes in 2014, 2019"
      ],
      "relevance": 0.95
    },
    {
      "source": "Eurovision voting pattern analysis",
      "summary": "Studies show regional voting blocs significantly influence outcomes. Western European countries receive fewer neighbor votes than Eastern/Nordic regions.",
      "key_findings": [
        "Geographic voting patterns favor regional blocs",
        "France lacks strong bilateral voting partnerships",
        "Big Five status ensures final participation"
      ],
      "relevance": 0.8
    },
    {
      "source": "Recent Eurovision contests 2020-2024",
      "summary": "Winners include Italy, Ukraine, Sweden, Switzerland showing diverse outcomes but often from countries with voting advantages or exceptional songs.",
      "key_findings": [
        "No clear dominant country in recent years",
        "Quality and staging increasingly important",
        "Public vote can override jury preferences"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength",
  "confidence": 0.6,
  "reasoning": "Base rate reflects France's historical 7.4% win rate. Negative adjustments for weak voting alliances and recent underperformance partially offset by uncertainty in song quality and competition strength. Long drought and structural disadvantages suggest below-average probability. Model assumes independence of drivers though some correlation exists between performance and song quality."
}
```

---

## General Evidence (1)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50%

```json
{
  "base_rate": {
    "reference_class": "France's Eurovision wins since contest began in 1956",
    "historical_frequency": 0.074,
    "sample_size": 68,
    "reasoning": "France has won Eurovision 5 times in 68 contests (1958, 1960, 1962, 1969, 1977). As a 'Big Five' country, France automatically qualifies for finals, giving consistent participation."
  },
  "drivers": [
    {
      "name": "recent_performance_trend",
      "display_name": "Recent Performance Trend",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "France has underperformed recently with multiple bottom-5 finishes in the 2010s-2020s, though showed improvement with 2nd place in 2021. Recent trajectory suggests below-average prospects."
    },
    {
      "name": "voting_bloc_strength",
      "display_name": "Voting Bloc Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 0.95,
      "p95": 1.2,
      "unit": "multiplier",
      "rationale": "France lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. This structural disadvantage persists in modern Eurovision voting patterns."
    },
    {
      "name": "song_quality_variance",
      "display_name": "Song Quality Variance",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Unknown song quality for 2026. France has capacity to produce competitive entries but selection process has been inconsistent. Wide uncertainty range reflects this variability."
    },
    {
      "name": "competition_field_strength",
      "display_name": "Competition Field Strength",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.3,
      "unit": "multiplier",
      "rationale": "Approximately 40 countries compete annually. Strong traditional competitors (Sweden, Italy, Ukraine) and emerging contenders create variable competitive landscape year-to-year."
    }
  ],
  "evidence": [
    {
      "source": "Eurovision historical results database",
      "summary": "France won 5 times total, last victory in 1977. Recent decade shows mixed results with one runner-up finish in 2021.",
      "key_findings": [
        "5 wins in 68 contests (7.4% win rate)",
        "47-year drought since last win",
        "2nd place in 2021, bottom-5 finishes in 2014, 2019"
      ],
      "relevance": 0.95
    },
    {
      "source": "Eurovision voting pattern analysis",
      "summary": "Studies show regional voting blocs significantly influence outcomes. Western European countries receive fewer neighbor votes than Eastern/Nordic regions.",
      "key_findings": [
        "Geographic voting patterns favor regional blocs",
        "France lacks strong bilateral voting partnerships",
        "Big Five status ensures final participation"
      ],
      "relevance": 0.8
    },
    {
      "source": "Recent Eurovision contests 2020-2024",
      "summary": "Winners include Italy, Ukraine, Sweden, Switzerland showing diverse outcomes but often from countries with voting advantages or exceptional songs.",
      "key_findings": [
        "No clear dominant country in recent years",
        "Quality and staging increasingly important",
        "Public vote can override jury preferences"
      ],
      "relevance": 0.75
    }
  ],
  "model_expression": "base_rate * recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength",
  "confidence": 0.6,
  "reasoning": "Base rate reflects France's historical 7.4% win rate. Negative adjustments for weak voting alliances and recent underperformance partially offset by uncertainty in song quality and competition strength. Long drought and structural disadvantages suggest below-average probability. Model assumes independence of drivers though some correlation exists between performance and song quality."
}
```

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: recent_performance_trend * voting_bloc_strength * song_quality_variance * competition_field_strength
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| market_research | recent_performance_trend | Research evidence for the 'recent_performance_trend' driver in the forecast: "will france win the eurovision song conets in fy 2026?" |
| sentiment_analyzer | recent_performance_trend | Analyze sentiment and public perception trends for France's Eurovision Song Contest entries from 2018-2024, focusing on fan reactions, media coverage, voting patterns, and comparative performance against other major countries. Identify trajectory indicators and momentum shifts. |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v4 · 2026-03-09 16:59 UTC_
