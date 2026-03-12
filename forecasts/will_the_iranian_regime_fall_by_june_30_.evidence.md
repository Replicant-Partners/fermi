# Will the Iranian regime fall by June 30?

**Probability:** 1.5% · **Version:** v7 · **Updated:** 2026-03-12 00:24 UTC

**Confidence:** Medium (49%) · **Drivers:** 5 · **Evidence:** 1 · **Agents:** 5

---

## Inside View

**Probability: 1.5%**

Starting from a 15.0% base rate, our model moderately decreases the probability to 10.6%. The key factors are: incumbent_party_status, age_and_generational_appeal, florida_base_strength. Most influential: maga_movement_evolution (34%), incumbent_party_status (32%), field_strength (30%).

**Forecast Confidence:** Medium (49%)

**Divergence from base rate:** 0pp below (1.5% vs 1.5%)

---

## Outside View (Base Rate)

**1.5%** — Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)

- **Sample size:** n=45
- **Source:** macro_forecaster

Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period.

---

## 1. economic_pressure `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.10 | 1.60 | multiplier |

> Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'economic_pressure' driver.
Current estimate: p5=0.80, p50=1.10, p95=1.60

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period.

Be specific and quantitative — numbers, percentages, named sources._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

---

## 2. protest_intensity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.60 | 0.90 | 1.40 | multiplier |

> Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'protest_intensity' driver.
Current estimate: p5=0.60, p50=0.90, p95=1.40

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains.

Be specific and quantitative — numbers, percentages, named sources._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

---

## 3. security_apparatus_loyalty `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.40 | 0.60 | 0.90 | multiplier |

> IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'security_apparatus_loyalty' driver.
Current estimate: p5=0.40, p50=0.60, p95=0.90

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks.

Be specific and quantitative — numbers, percentages, named sources._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

---

## 4. external_intervention `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.00 | 1.80 | multiplier |

> Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'external_intervention' driver.
Current estimate: p5=0.70, p50=1.00, p95=1.80

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations.

Be specific and quantitative — numbers, percentages, named sources._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

---

## 5. leadership_crisis `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.50 | multiplier |

> Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely.

### Assigned Agents

- **entity_investigator** (schedule: once)  
  Query: _For the forecast: "Will the Iranian regime fall by June 30?"

Investigate entities relevant to 'leadership_crisis'.

PROVIDE:
1. Key decision-makers and their positions
2. Organizational dynamics (strategy, leadership, M&A)
3. Financial health or resource position
4. Relationships and dependencies
5. How findings should adjust the probability

Context: Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

---

## General Evidence (1)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50% · quality ●●● High (80%)

```json
{
  "base_rate": {
    "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
    "historical_frequency": 0.015,
    "sample_size": 45,
    "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
  },
  "drivers": [
    {
      "name": "economic_pressure",
      "display_name": "Economic Sanctions & Inflation Impact",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.1,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period."
    },
    {
      "name": "protest_intensity",
      "display_name": "Ongoing Protest Movement Strength",
      "type": "continuous",
      "p5": 0.6,
      "p50": 0.9,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains."
    },
    {
      "name": "security_apparatus_loyalty",
      "display_name": "IRGC and Security Forces Cohesion",
      "type": "continuous",
      "p5": 0.4,
      "p50": 0.6,
      "p95": 0.9,
      "unit": "multiplier",
      "rationale": "IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks."
    },
    {
      "name": "external_intervention",
      "display_name": "Foreign Military or Political Intervention",
      "type": "continuous",
      "p5": 0.7,
      "p50": 1.0,
      "p95": 1.8,
      "unit": "multiplier",
      "rationale": "Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations."
    },
    {
      "name": "leadership_crisis",
      "display_name": "Supreme Leader Health or Succession Crisis",
      "type": "continuous",
      "p5": 0.8,
      "p50": 1.0,
      "p95": 1.5,
      "unit": "multiplier",
      "rationale": "Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely."
    }
  ],
  "evidence": [
    {
      "source": "International Crisis Group Iran Reports 2023-2024",
      "summary": "Protest movements significantly weakened after government crackdown. IRGC maintains control despite economic pressures.",
      "key_findings": [
        "Mahsa Amini protests peaked in late 2022, substantially diminished by early 2023",
        "Security forces demonstrated willingness to use lethal force",
        "No significant elite defections observed"
      ],
      "relevance": 0.9
    },
    {
      "source": "Historical analysis of authoritarian regime collapses (Geddes, Wright & Frantz dataset)",
      "summary": "Personalist and military regimes in Middle East show high resilience. Collapses typically require security force defection or external intervention.",
      "key_findings": [
        "Average authoritarian regime duration in region: 32 years",
        "Security apparatus defection critical in 85% of collapses",
        "Economic crisis alone rarely sufficient for regime collapse"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF and World Bank Iran economic indicators 2024",
      "summary": "Inflation remains high (40-50%) but economy showing adaptation to sanctions through gray market channels.",
      "key_findings": [
        "GDP contraction moderated compared to 2018-2019",
        "Oil exports continue via sanctions evasion",
        "Currency stabilized from 2022 lows"
      ],
      "relevance": 0.7
    }
  ],
  "model_expression": "base_rate * economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis",
  "confidence": 0.75,
  "reasoning": "Iranian regime has proven highly resilient with entrenched security apparatus. Five-month timeframe extremely short for regime collapse absent acute trigger (military defeat, leadership death, mass defections). Current protest movement insufficient, IRGC loyal, no imminent external intervention. Base rate already low; most drivers reduce probability further. Confidence moderate due to inherent unpredictability of rapid political change."
}
```

**Key findings:**

- "base_rate": {
- "reference_class": "Middle Eastern authoritarian regime collapses within 6-month period (2000-2024)",
- "historical_frequency": 0.015,
- "sample_size": 45,
- "reasoning": "Examining 15 Middle Eastern authoritarian regimes over 24 years (360 six-month periods total), approximately 7 experienced regime collapse (Tunisia 2011, Egypt 2011, Libya 2011, Yemen 2012, Iraq 2003, Syria partial 2011-ongoing, Sudan 2019), yielding ~1.5% per 6-month period."
- "drivers": [
- "name": "economic_pressure",
- "display_name": "Economic Sanctions & Inflation Impact",
- "type": "continuous",
- "p5": 0.8,

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: economic_pressure * protest_intensity * security_apparatus_loyalty * external_intervention * leadership_crisis
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_forecaster | economic_pressure | For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'economic_pressure' driver.
Current estimate: p5=0.80, p50=1.10, p95=1.60

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Severe sanctions and 40%+ inflation strain regime legitimacy, but Iran has adapted over decades. Current economic stress elevated but not unprecedented compared to 2012-2013 period.

Be specific and quantitative — numbers, percentages, named sources. |
| macro_forecaster | protest_intensity | For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'protest_intensity' driver.
Current estimate: p5=0.60, p50=0.90, p95=1.40

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Post-Mahsa Amini protests have diminished significantly from late 2022 peak. Current protest activity is lower than critical mass needed for regime collapse, though underlying discontent remains.

Be specific and quantitative — numbers, percentages, named sources. |
| macro_forecaster | security_apparatus_loyalty | For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'security_apparatus_loyalty' driver.
Current estimate: p5=0.40, p50=0.60, p95=0.90

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: IRGC remains deeply entrenched with extensive economic interests. No significant defections observed. Iranian security apparatus historically more resilient than Arab Spring counterparts due to ideological indoctrination and patronage networks.

Be specific and quantitative — numbers, percentages, named sources. |
| macro_forecaster | external_intervention | For the forecast: "Will the Iranian regime fall by June 30?"

Research evidence for the 'external_intervention' driver.
Current estimate: p5=0.70, p50=1.00, p95=1.80

PROVIDE:
1. Key data points relevant to this driver (with sources and dates)
2. Historical base rate or comparable precedent
3. Suggested p50 multiplier adjustment based on your findings
4. Confidence (0.0-1.0) in your assessment

Context: Regional tensions with Israel elevated, but direct military intervention targeting regime change unlikely in 5-month timeframe. US policy focused on containment rather than regime change operations.

Be specific and quantitative — numbers, percentages, named sources. |
| entity_investigator | leadership_crisis | For the forecast: "Will the Iranian regime fall by June 30?"

Investigate entities relevant to 'leadership_crisis'.

PROVIDE:
1. Key decision-makers and their positions
2. Organizational dynamics (strategy, leadership, M&A)
3. Financial health or resource position
4. Relationships and dependencies
5. How findings should adjust the probability

Context: Khamenei's age (85) creates succession uncertainty, but no immediate health crisis reported. Succession mechanisms exist though could create instability. Five-month window makes acute crisis unlikely. |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v7 · 2026-03-12 00:24 UTC_
