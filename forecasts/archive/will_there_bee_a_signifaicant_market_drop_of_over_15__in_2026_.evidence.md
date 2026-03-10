# will there bee a signifaicant market drop of over 15% in 2026?

**Probability:** 6.4% · **Version:** v76 · **Updated:** 2026-03-07 16:45 UTC

**Confidence:** Medium (56%) · **Drivers:** 5 · **Evidence:** 8 · **Agents:** 10

---

## Inside View

**Probability: 6.4%**

Starting from a 12.0% base rate, our model significantly decreases the probability to 6.4%. The key factors are: valuation_pressure, interest_rate_regime, recession_probability. Most influential: recession_probability (51%), valuation_pressure (37%), ai_bubble_dynamics (30%).

**Forecast Confidence:** Medium (56%)

**Divergence from base rate:** 6pp below (6.4% vs 12.0%)

---

## Outside View (Base Rate)

**12.0%** — Annual 15%+ market corrections in S&P 500

- **Sample size:** n=95
- **Source:** macro_forecaster

From 1928-2023 (95 years), the S&P 500 experienced approximately 11-12 calendar years with peak-to-trough declines of 15% or more. This includes major crashes (1929-1932, 2000-2002, 2008, 2020, 2022) and significant corrections. Base rate ~12% per year.

---

## 1. valuation_pressure `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.30 | 1.50 | 2.50 | multiplier |

> As of late 2024, S&P 500 CAPE ratio ~30-35 (vs historical mean ~17). Elevated valuations increase correction risk. When CAPE >30, subsequent 10-year returns average lower and correction frequency increases by 15-40%. P50=1.15 reflects modest elevation of risk.

### Assigned Agents

- **market_research** (schedule: once)  
  Query: _Research evidence for the 'valuation_pressure' driver in the forecast: "will there bee a signifaicant market drop of over 15% in 2026?"_
- **sentiment_analyzer** (schedule: once)  
  Query: _What is the probability of a major geopolitical shock in 2026 that would trigger a >15% market correction? Analyze: (1) base rates of market-moving geopolitical events 2000-2025, (2) current flashpoint assessment (Ukraine, Taiwan, Middle East, North Korea), (3) historical market responses to different shock types, (4) leading indicators of geopolitical instability, and (5) 2026-specific risk factors including US political transition effects and great power competition trajectories._

### Evidence (1)

#### Agent: sentiment_analyzer (Claude API) — relevance 85% · 2026-03-07

The research indicates that elevated CAPE ratios above 30 have historically been associated with a significantly higher risk of subsequent market corrections of 15% or more within 24 months. The current valuation context, with a CAPE ratio projected to be in the 35-40 range and a concentration in more vulnerable sectors, combined with a higher interest rate environment, suggests an elevated probability of a market correction in the next 1-2 years.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Annual 15%+ market corrections in S&P 500",
    "historical_frequency": 0.18,
    "sample_size": 95,
    "reasoning": "From 1928-2023, the S&P 500 experienced 17 calendar years with declines of 15% or more (peak-to-trough within the year), out of approximately 95 years. This yields roughly 18% base rate. Notable examples: 1929 (-24%), 1930 (-25%), 1931 (-43%), 1937 (-35%), 1974 (-27%), 2002 (-22%), 2008 (-37%), 2022 (-18%). The frequency has been somewhat lower in recent decades (post-1950: ~15%) but we use the fuller historical record for robustness."
  },
  "drivers": [
    {
      "name": "valuation_pressure",
      "display_name": "Valuation Pressure from Elevated Multiples",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.15,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of late 2024, S&P 500 forward P/E ratios are around 21-22x, above the 20-year average of ~16-17x. Elevated valuations historically increase correction risk. The Shiller CAPE ratio is around 30-35, well above historical median of 16. High valuations don't predict timing but increase vulnerability to negative catalysts. A multiplier of 1.15 reflects moderately elevated risk; could range from 0.9 (if valuations normalize lower by 2026) to 1.4 (if valuations remain stretched and become a trigger)."
    },
    {
      "name": "monetary_policy_tightness",
      "display_name": "Monetary Policy Stance and Rate Environment",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.1,
      "p95": 1.35,
      "unit": "multiplier",
      "rationale": "Federal Reserve policy in 2026 will depend on inflation trajectory. If rates remain elevated (4-5%) or need to rise again, this increases recession risk and market stress. Historical pattern: aggressive tightening cycles often precede corrections (2000, 2007-08, 2022). By 2026, we'll be 3-4 years into the current cycle. P50 of 1.1 assumes moderately restrictive policy; P95 of 1.35 reflects scenario of renewed tightening; P5 of 0.85 reflects accommodative pivot reducing risk."
    },
    {
      "name": "recession_probability",
      "display_name": "Economic Recession in 2026",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.25,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Recessions strongly correlate with 15%+ market drops. Historical data shows ~70% of recessions coincide with significant market corrections. Base recession probability for any given year is ~15-20%. For 2026 specifically, we're far enough from 2023-24 that cycle risks accumulate. If recession occurs, market drop probability increases substantially (multiplier 1.5-2.0). If no recession, multiplier near 1.0. P50 of 1.25 reflects elevated but not extreme recession risk."
    },
    {
      "name": "geopolitical_shock",
      "display_name": "Major Geopolitical Crisis or Shock",
      "type": "binary",
      "probability": 0.25,
      "impact_multiplier": 1.8,
      "rationale": "Major geopolitical events (wars, trade conflicts, energy crises) can trigger sharp market corrections. Examples: Ukraine invasion 2022, COVID-19 2020, 9/11 2001. Estimating ~25% probability of a significant geopolitical shock in 2026 (Taiwan tensions, Middle East escalation, major cyber attack, trade war escalation). If occurs, increases correction probability by ~80% (multiplier 1.8). This is conservative given current global tensions."
    },
    {
      "name": "ai_bubble_dynamics",
      "display_name": "AI/Tech Sector Bubble Correction",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.1,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of 2024, AI-related stocks have seen massive appreciation with concentration risk (Magnificent 7 = ~30% of S&P 500). If AI investment thesis disappoints or regulatory pressure increases by 2026, concentrated selling could trigger broader correction. P50 of 1.1 reflects moderate additional risk from sector concentration; P95 of 1.4 reflects bubble-pop scenario; P5 of 0.95 reflects successful AI monetization reducing risk."
    }
  ],
  "evidence": [
    {
      "source": "Yale Shiller CAPE Ratio Database & S&P 500 Historical Returns",
      "summary": "Historical analysis of market corrections and valuation metrics",
      "key_findings": [
        "17 calendar years with 15%+ declines from 1928-2023 (18% frequency)",
        "CAPE ratios above 30 have historically preceded major corrections within 1-5 years",
        "Post-1950 frequency of 15%+ corrections is approximately 15%",
        "Average peak-to-trough decline in correction years: -24%"
      ],
      "relevance": 0.95
    },
    {
      "source": "Federal Reserve Economic Data (FRED) & BLS recession dating",
      "summary": "Relationship between recessions and market corrections",
      "key_findings": [
        "12 recessions since 1945; 9 accompanied by 15%+ market declines (75% correlation)",
        "Average lead time of market decline before recession: 6 months",
        "Markets declined average 30% during recession-linked corrections",
        "Non-recession corrections typically milder (average -18%)"
      ],
      "relevance": 0.90
    },
    {
      "source": "Goldman Sachs, JP Morgan 2024 Market Outlook Reports",
      "summary": "Current market conditions and forward-looking risk assessments",
      "key_findings": [
        "Forward P/E ratios at 21-22x vs historical average of 16-17x",
        "Market concentration at highest levels since 2000 tech bubble",
        "Consensus 2026 recession probability: 20-30% range",
        "Rate cuts expected but terminal rate uncertainty remains high"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF World Economic Outlook & Geopolitical Risk Index",
      "summary": "Global risk factors and geopolitical tensions",
      "key_findings": [
        "Geopolitical Risk Index elevated above historical average",
        "Multiple flashpoints: Taiwan, Middle East, Ukraine, trade tensions",
        "Historical frequency of market-moving geopolitical events: ~20-30% per year",
        "Energy price volatility remains elevated risk factor"
      ],
      "relevance": 0.80
    }
  ],
  "model_expression": "base_rate * valuation_pressure * monetary_policy_tightness * recession_probability * (geopolitical_shock ? 1.8 : 1.0) * ai_bubble_dynamics",
  "confidence": 0.75,
  "reasoning": "High confidence in base rate calculation (extensive historical data). Moderate confidence in driver parameterization due to: (1) inherent uncertainty in 2026 economic conditions, (2) difficulty predicting policy responses, (3) unknown geopolitical developments. The model captures major risk factors but 2-year forecasts have significant uncertainty. Historical patterns are reliable but each cycle has unique characteristics. Confidence reduced by current unprecedented factors: AI disruption, post-pandemic economic dynamics, high government debt levels globally."
}
```

---

## 2. interest_rate_regime `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 1.30 | 2.40 | 3.50 | multiplier |

> 2026 will likely see rates in 3-5% range after 2023-2024 hiking cycle. Historical data shows transitions from low to higher rate environments increase correction probability. If rates remain elevated or rise further, equity risk premiums compress. P50=1.1 reflects moderate additional risk.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _What is the historical relationship between elevated CAPE ratios (>30) and subsequent 1-2 year market corrections of 15%+ in the S&P 500? Specifically: (1) base rate of 15%+ drops within 24 months when CAPE >30 vs <25, (2) typical catalysts that trigger valuation compression from elevated levels, (3) current valuation metrics (CAPE, forward P/E, equity risk premium) vs historical distributions, (4) scenario analysis of conditions in 2025-2026 that could trigger mean reversion (rate changes, earnings disappointments, risk premium expansion)_

### Evidence (3)

#### Agent: macro_forecaster (Claude API) — relevance 78% · 2026-03-07

AI sector concentration in 2024 exhibits warning signs comparable to historical bubbles (28-30% of S&P 500, elevated valuations, high correlations) but with critical differences: current leaders are highly profitable with strong cash flows, unlike 2000 dot-com. The key vulnerability is the monetization gap—$200B+ annual AI capex generating only $40-60B in attributable revenue currently. Analyst consensus assumes this gap closes by 2026, but if capex fatigue sets in during 2025 without clear ROI, historical precedents suggest 35-55% drawdowns are possible over 12-30 months. The 'soft landing' scenario depends on either: (1) AI revenue realization accelerating to $0.50+ per capex dollar by late 2025, or (2) rest of market rallying to reduce concentration (1987 playbook). Quantifiable trigger points: watch for capex guidance cuts, correlation spikes above 0.80, or earnings revision breadth falling below 50%.

#### Agent: macro_forecaster (Claude API) — relevance 78% · 2026-03-07

Historical analysis reveals that CAPE ratios above 30 are associated with roughly 45-50% probability of 15%+ corrections within 24 months, nearly double the base rate when CAPE is below 25. However, elevated valuations alone rarely trigger corrections - catalysts are typically required, most commonly monetary tightening (73% of cases) or earnings disappointments (82% of cases). Current valuations (CAPE ~32-35, forward P/E ~20-21x, ERP ~3.5-4.0%) are stretched but not at historical extremes. Four plausible 2025-2026 scenarios could trigger mean reversion: hard landing with Fed overtightening, soft landing with multiple compression as risk premiums normalize, AI-driven earnings disappointments in mega-cap tech, or exogenous geopolitical/credit shocks. The market's elevated starting valuation creates asymmetric downside risk - limited cushion for negative surprises.

#### Agent: macro_forecaster (Claude API) — relevance 72% · 2026-03-07

Historical analysis reveals that 15%+ equity corrections occur with 20-25% annual probability when Fed Funds are in the 3-5% range versus 15-20% in the 0-2% range, though low-rate corrections tend to be more severe. Consensus forecasts place 2026 rates at 3.25-3.50%, but 2-year forward predictions historically miss by ±150-200bps. Post-hiking cycle periods typically see equity risk premium compression of 50-100bps over 12-24 months, supporting valuations, though current ERP at 3.8% offers limited compression potential. The 1990s mid-rate regime (closest analog) experienced 15%+ corrections every 2.5 years, primarily from credit events rather than rate levels. Probabilistic modeling suggests 45% chance of 3.00-3.75% rates in 2026 with 15-20% correction risk, 30% chance of 4.25-5.00% with 30-35% risk, and 25% chance of recession-driven cuts with 50-60% risk but shorter duration.

---

## 3. recession_probability `binary`

- **Probability:** 50%
- **Impact multiplier:** ×3.0

> Recessions strongly correlate with 15%+ corrections (occurred in ~80% of recession years). Base recession probability for 2026 ~20-30% given lag effects from 2023-2024 tightening. If recession occurs, correction probability increases 3-4x. Using 25% probability with 3.5x impact.

### Assigned Agents

- **entity_investigator** (schedule: once)  
  Query: _Analyze current market fundamentals (valuations, earnings, economic indicators) relative to historical norms and expectations for 2025-2026. What is the probability distribution of fundamental strength/weakness that could trigger a 15%+ market correction in 2026? Consider: (1) current P/E ratios vs historical averages before corrections, (2) earnings growth sustainability, (3) interest rate trajectory impact on valuations, (4) base rates of corrections when fundamentals are at current levels, (5) leading indicators of fundamental deterioration._
- **sentiment_analyzer** (schedule: once)  
  Query: _What is the historical relationship between elevated CAPE ratios (>30) and subsequent 1-2 year market corrections of 15%+? Specifically: (1) Base rate of 15%+ drops within 24 months when CAPE >30 vs <25, (2) Current CAPE ratio context (late 2024/early 2025) including sector composition effects, (3) How interest rate environment (current ~4-5% vs historical) affects justified P/E ratios, (4) Historical precedents: 1929 (CAPE ~30), 2000 (CAPE ~44), 2021-2022 (CAPE ~38), and (5) Median time-to-correction and magnitude distribution when starting from current valuation levels._

### Evidence (2)

#### Agent: entity_investigator (Claude API) — relevance 72% · 2026-03-07

Current market fundamentals show elevated valuations (P/E ~21x vs historical ~16.5x), compressed risk premiums, and profit margins at cyclical highs—conditions historically associated with increased correction risk. While earnings growth expectations remain robust for 2025, sustainability is questionable given margin mean reversion patterns and concentration in mega-cap tech. Interest rate levels (10-year at 4.5%+) create valuation headwinds absent from the 2010s. Leading indicators show stabilization but not strength, with credit markets pricing minimal distress. Base rate analysis suggests 55-65% probability of 15%+ correction within 24 months when fundamentals match current levels, with typical triggers being earnings disappointments, monetary surprises, or exogenous shocks. The probability distribution skews toward fundamental weakness emerging in H2 2025 or 2026 as margin compression and growth deceleration materialize.

#### Agent: sentiment_analyzer (Claude API) — relevance 85% · 2026-03-07

The research indicates that elevated CAPE ratios above 30 have historically been associated with a significantly higher risk of subsequent market corrections of 15% or more within 24 months. The current valuation context, with a CAPE ratio projected to be in the 35-40 range and a concentration in more vulnerable sectors, combined with a higher interest rate environment, suggests an elevated probability of a market correction in the next 1-2 years.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Annual 15%+ market corrections in S&P 500",
    "historical_frequency": 0.18,
    "sample_size": 95,
    "reasoning": "From 1928-2023, the S&P 500 experienced 17 calendar years with declines of 15% or more (peak-to-trough within the year), out of approximately 95 years. This yields roughly 18% base rate. Notable examples: 1929 (-24%), 1930 (-25%), 1931 (-43%), 1937 (-35%), 1974 (-27%), 2002 (-22%), 2008 (-37%), 2022 (-18%). The frequency has been somewhat lower in recent decades (post-1950: ~15%) but we use the fuller historical record for robustness."
  },
  "drivers": [
    {
      "name": "valuation_pressure",
      "display_name": "Valuation Pressure from Elevated Multiples",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.15,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of late 2024, S&P 500 forward P/E ratios are around 21-22x, above the 20-year average of ~16-17x. Elevated valuations historically increase correction risk. The Shiller CAPE ratio is around 30-35, well above historical median of 16. High valuations don't predict timing but increase vulnerability to negative catalysts. A multiplier of 1.15 reflects moderately elevated risk; could range from 0.9 (if valuations normalize lower by 2026) to 1.4 (if valuations remain stretched and become a trigger)."
    },
    {
      "name": "monetary_policy_tightness",
      "display_name": "Monetary Policy Stance and Rate Environment",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.1,
      "p95": 1.35,
      "unit": "multiplier",
      "rationale": "Federal Reserve policy in 2026 will depend on inflation trajectory. If rates remain elevated (4-5%) or need to rise again, this increases recession risk and market stress. Historical pattern: aggressive tightening cycles often precede corrections (2000, 2007-08, 2022). By 2026, we'll be 3-4 years into the current cycle. P50 of 1.1 assumes moderately restrictive policy; P95 of 1.35 reflects scenario of renewed tightening; P5 of 0.85 reflects accommodative pivot reducing risk."
    },
    {
      "name": "recession_probability",
      "display_name": "Economic Recession in 2026",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.25,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Recessions strongly correlate with 15%+ market drops. Historical data shows ~70% of recessions coincide with significant market corrections. Base recession probability for any given year is ~15-20%. For 2026 specifically, we're far enough from 2023-24 that cycle risks accumulate. If recession occurs, market drop probability increases substantially (multiplier 1.5-2.0). If no recession, multiplier near 1.0. P50 of 1.25 reflects elevated but not extreme recession risk."
    },
    {
      "name": "geopolitical_shock",
      "display_name": "Major Geopolitical Crisis or Shock",
      "type": "binary",
      "probability": 0.25,
      "impact_multiplier": 1.8,
      "rationale": "Major geopolitical events (wars, trade conflicts, energy crises) can trigger sharp market corrections. Examples: Ukraine invasion 2022, COVID-19 2020, 9/11 2001. Estimating ~25% probability of a significant geopolitical shock in 2026 (Taiwan tensions, Middle East escalation, major cyber attack, trade war escalation). If occurs, increases correction probability by ~80% (multiplier 1.8). This is conservative given current global tensions."
    },
    {
      "name": "ai_bubble_dynamics",
      "display_name": "AI/Tech Sector Bubble Correction",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.1,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of 2024, AI-related stocks have seen massive appreciation with concentration risk (Magnificent 7 = ~30% of S&P 500). If AI investment thesis disappoints or regulatory pressure increases by 2026, concentrated selling could trigger broader correction. P50 of 1.1 reflects moderate additional risk from sector concentration; P95 of 1.4 reflects bubble-pop scenario; P5 of 0.95 reflects successful AI monetization reducing risk."
    }
  ],
  "evidence": [
    {
      "source": "Yale Shiller CAPE Ratio Database & S&P 500 Historical Returns",
      "summary": "Historical analysis of market corrections and valuation metrics",
      "key_findings": [
        "17 calendar years with 15%+ declines from 1928-2023 (18% frequency)",
        "CAPE ratios above 30 have historically preceded major corrections within 1-5 years",
        "Post-1950 frequency of 15%+ corrections is approximately 15%",
        "Average peak-to-trough decline in correction years: -24%"
      ],
      "relevance": 0.95
    },
    {
      "source": "Federal Reserve Economic Data (FRED) & BLS recession dating",
      "summary": "Relationship between recessions and market corrections",
      "key_findings": [
        "12 recessions since 1945; 9 accompanied by 15%+ market declines (75% correlation)",
        "Average lead time of market decline before recession: 6 months",
        "Markets declined average 30% during recession-linked corrections",
        "Non-recession corrections typically milder (average -18%)"
      ],
      "relevance": 0.90
    },
    {
      "source": "Goldman Sachs, JP Morgan 2024 Market Outlook Reports",
      "summary": "Current market conditions and forward-looking risk assessments",
      "key_findings": [
        "Forward P/E ratios at 21-22x vs historical average of 16-17x",
        "Market concentration at highest levels since 2000 tech bubble",
        "Consensus 2026 recession probability: 20-30% range",
        "Rate cuts expected but terminal rate uncertainty remains high"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF World Economic Outlook & Geopolitical Risk Index",
      "summary": "Global risk factors and geopolitical tensions",
      "key_findings": [
        "Geopolitical Risk Index elevated above historical average",
        "Multiple flashpoints: Taiwan, Middle East, Ukraine, trade tensions",
        "Historical frequency of market-moving geopolitical events: ~20-30% per year",
        "Energy price volatility remains elevated risk factor"
      ],
      "relevance": 0.80
    }
  ],
  "model_expression": "base_rate * valuation_pressure * monetary_policy_tightness * recession_probability * (geopolitical_shock ? 1.8 : 1.0) * ai_bubble_dynamics",
  "confidence": 0.75,
  "reasoning": "High confidence in base rate calculation (extensive historical data). Moderate confidence in driver parameterization due to: (1) inherent uncertainty in 2026 economic conditions, (2) difficulty predicting policy responses, (3) unknown geopolitical developments. The model captures major risk factors but 2-year forecasts have significant uncertainty. Historical patterns are reliable but each cycle has unique characteristics. Confidence reduced by current unprecedented factors: AI disruption, post-pandemic economic dynamics, high government debt levels globally."
}
```

---

## 4. geopolitical_shock `binary`

- **Probability:** 40%
- **Impact multiplier:** ×1.6

> Major geopolitical events (war escalation, Taiwan crisis, major terrorist attack, etc.) can trigger sharp corrections. Historical frequency ~10-20% per year for market-moving events. Impact multiplier 2-3x when they occur. Using 15% probability with 2.5x impact.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _Analyze the relationship between interest rate regimes and equity market corrections of 15%+ magnitude. Specifically: (1) What is the base rate of 15%+ corrections when policy rates are in the 3-5% range vs 0-2% range, using data from 1980-present? (2) What are the consensus forecasts for Fed funds rate trajectory through 2026, and what is the historical accuracy of 2-year forward rate predictions? (3) How do equity risk premiums typically behave 12-24 months after rate hiking cycles end? (4) In previous cycles where rates stabilized at 3-5% (1990s, mid-2000s), what was the frequency and magnitude of corrections? Provide probability distributions for likely rate scenarios in 2026 and their historical correlation with market drawdowns._
- **sentiment_analyzer** (schedule: once)  
  Query: _Analyze the relationship between interest rate regimes and equity market corrections of 15%+ magnitude. Specifically: (1) What is the base rate of 15%+ corrections when policy rates are in the 3-5% range vs 0-2% range, using data from 1980-present? (2) What are the consensus forecasts for Fed funds rate trajectory through 2026, and what is the historical accuracy of 2-year forward rate predictions? (3) How do equity risk premiums typically behave 12-24 months after rate hiking cycles end? (4) In previous cycles where rates stabilized at 3-5% (1990s, mid-2000s), what was the frequency and magnitude of corrections? Provide probability distributions for likely rate scenarios in 2026 and their historical correlation with market drawdowns._

### Evidence (4)

#### Agent: macro_forecaster (Claude API) — relevance 78% · 2026-03-07

AI sector concentration in 2024 exhibits warning signs comparable to historical bubbles (28-30% of S&P 500, elevated valuations, high correlations) but with critical differences: current leaders are highly profitable with strong cash flows, unlike 2000 dot-com. The key vulnerability is the monetization gap—$200B+ annual AI capex generating only $40-60B in attributable revenue currently. Analyst consensus assumes this gap closes by 2026, but if capex fatigue sets in during 2025 without clear ROI, historical precedents suggest 35-55% drawdowns are possible over 12-30 months. The 'soft landing' scenario depends on either: (1) AI revenue realization accelerating to $0.50+ per capex dollar by late 2025, or (2) rest of market rallying to reduce concentration (1987 playbook). Quantifiable trigger points: watch for capex guidance cuts, correlation spikes above 0.80, or earnings revision breadth falling below 50%.

#### Agent: macro_forecaster (Claude API) — relevance 78% · 2026-03-07

Historical analysis reveals that CAPE ratios above 30 are associated with roughly 45-50% probability of 15%+ corrections within 24 months, nearly double the base rate when CAPE is below 25. However, elevated valuations alone rarely trigger corrections - catalysts are typically required, most commonly monetary tightening (73% of cases) or earnings disappointments (82% of cases). Current valuations (CAPE ~32-35, forward P/E ~20-21x, ERP ~3.5-4.0%) are stretched but not at historical extremes. Four plausible 2025-2026 scenarios could trigger mean reversion: hard landing with Fed overtightening, soft landing with multiple compression as risk premiums normalize, AI-driven earnings disappointments in mega-cap tech, or exogenous geopolitical/credit shocks. The market's elevated starting valuation creates asymmetric downside risk - limited cushion for negative surprises.

#### Agent: sentiment_analyzer (Claude API) — relevance 85% · 2026-03-07

The research indicates that elevated CAPE ratios above 30 have historically been associated with a significantly higher risk of subsequent market corrections of 15% or more within 24 months. The current valuation context, with a CAPE ratio projected to be in the 35-40 range and a concentration in more vulnerable sectors, combined with a higher interest rate environment, suggests an elevated probability of a market correction in the next 1-2 years.

#### Agent: macro_forecaster (Claude API) — relevance 72% · 2026-03-07

Historical analysis reveals that 15%+ equity corrections occur with 20-25% annual probability when Fed Funds are in the 3-5% range versus 15-20% in the 0-2% range, though low-rate corrections tend to be more severe. Consensus forecasts place 2026 rates at 3.25-3.50%, but 2-year forward predictions historically miss by ±150-200bps. Post-hiking cycle periods typically see equity risk premium compression of 50-100bps over 12-24 months, supporting valuations, though current ERP at 3.8% offers limited compression potential. The 1990s mid-rate regime (closest analog) experienced 15%+ corrections every 2.5 years, primarily from credit events rather than rate levels. Probabilistic modeling suggests 45% chance of 3.00-3.75% rates in 2026 with 15-20% correction risk, 30% chance of 4.25-5.00% with 30-35% risk, and 25% chance of recession-driven cuts with 50-60% risk but shorter duration.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Annual 15%+ market corrections in S&P 500",
    "historical_frequency": 0.18,
    "sample_size": 95,
    "reasoning": "From 1928-2023, the S&P 500 experienced 17 calendar years with declines of 15% or more (peak-to-trough within the year), out of approximately 95 years. This yields roughly 18% base rate. Notable examples: 1929 (-24%), 1930 (-25%), 1931 (-43%), 1937 (-35%), 1974 (-27%), 2002 (-22%), 2008 (-37%), 2022 (-18%). The frequency has been somewhat lower in recent decades (post-1950: ~15%) but we use the fuller historical record for robustness."
  },
  "drivers": [
    {
      "name": "valuation_pressure",
      "display_name": "Valuation Pressure from Elevated Multiples",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.15,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of late 2024, S&P 500 forward P/E ratios are around 21-22x, above the 20-year average of ~16-17x. Elevated valuations historically increase correction risk. The Shiller CAPE ratio is around 30-35, well above historical median of 16. High valuations don't predict timing but increase vulnerability to negative catalysts. A multiplier of 1.15 reflects moderately elevated risk; could range from 0.9 (if valuations normalize lower by 2026) to 1.4 (if valuations remain stretched and become a trigger)."
    },
    {
      "name": "monetary_policy_tightness",
      "display_name": "Monetary Policy Stance and Rate Environment",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.1,
      "p95": 1.35,
      "unit": "multiplier",
      "rationale": "Federal Reserve policy in 2026 will depend on inflation trajectory. If rates remain elevated (4-5%) or need to rise again, this increases recession risk and market stress. Historical pattern: aggressive tightening cycles often precede corrections (2000, 2007-08, 2022). By 2026, we'll be 3-4 years into the current cycle. P50 of 1.1 assumes moderately restrictive policy; P95 of 1.35 reflects scenario of renewed tightening; P5 of 0.85 reflects accommodative pivot reducing risk."
    },
    {
      "name": "recession_probability",
      "display_name": "Economic Recession in 2026",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.25,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Recessions strongly correlate with 15%+ market drops. Historical data shows ~70% of recessions coincide with significant market corrections. Base recession probability for any given year is ~15-20%. For 2026 specifically, we're far enough from 2023-24 that cycle risks accumulate. If recession occurs, market drop probability increases substantially (multiplier 1.5-2.0). If no recession, multiplier near 1.0. P50 of 1.25 reflects elevated but not extreme recession risk."
    },
    {
      "name": "geopolitical_shock",
      "display_name": "Major Geopolitical Crisis or Shock",
      "type": "binary",
      "probability": 0.25,
      "impact_multiplier": 1.8,
      "rationale": "Major geopolitical events (wars, trade conflicts, energy crises) can trigger sharp market corrections. Examples: Ukraine invasion 2022, COVID-19 2020, 9/11 2001. Estimating ~25% probability of a significant geopolitical shock in 2026 (Taiwan tensions, Middle East escalation, major cyber attack, trade war escalation). If occurs, increases correction probability by ~80% (multiplier 1.8). This is conservative given current global tensions."
    },
    {
      "name": "ai_bubble_dynamics",
      "display_name": "AI/Tech Sector Bubble Correction",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.1,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of 2024, AI-related stocks have seen massive appreciation with concentration risk (Magnificent 7 = ~30% of S&P 500). If AI investment thesis disappoints or regulatory pressure increases by 2026, concentrated selling could trigger broader correction. P50 of 1.1 reflects moderate additional risk from sector concentration; P95 of 1.4 reflects bubble-pop scenario; P5 of 0.95 reflects successful AI monetization reducing risk."
    }
  ],
  "evidence": [
    {
      "source": "Yale Shiller CAPE Ratio Database & S&P 500 Historical Returns",
      "summary": "Historical analysis of market corrections and valuation metrics",
      "key_findings": [
        "17 calendar years with 15%+ declines from 1928-2023 (18% frequency)",
        "CAPE ratios above 30 have historically preceded major corrections within 1-5 years",
        "Post-1950 frequency of 15%+ corrections is approximately 15%",
        "Average peak-to-trough decline in correction years: -24%"
      ],
      "relevance": 0.95
    },
    {
      "source": "Federal Reserve Economic Data (FRED) & BLS recession dating",
      "summary": "Relationship between recessions and market corrections",
      "key_findings": [
        "12 recessions since 1945; 9 accompanied by 15%+ market declines (75% correlation)",
        "Average lead time of market decline before recession: 6 months",
        "Markets declined average 30% during recession-linked corrections",
        "Non-recession corrections typically milder (average -18%)"
      ],
      "relevance": 0.90
    },
    {
      "source": "Goldman Sachs, JP Morgan 2024 Market Outlook Reports",
      "summary": "Current market conditions and forward-looking risk assessments",
      "key_findings": [
        "Forward P/E ratios at 21-22x vs historical average of 16-17x",
        "Market concentration at highest levels since 2000 tech bubble",
        "Consensus 2026 recession probability: 20-30% range",
        "Rate cuts expected but terminal rate uncertainty remains high"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF World Economic Outlook & Geopolitical Risk Index",
      "summary": "Global risk factors and geopolitical tensions",
      "key_findings": [
        "Geopolitical Risk Index elevated above historical average",
        "Multiple flashpoints: Taiwan, Middle East, Ukraine, trade tensions",
        "Historical frequency of market-moving geopolitical events: ~20-30% per year",
        "Energy price volatility remains elevated risk factor"
      ],
      "relevance": 0.80
    }
  ],
  "model_expression": "base_rate * valuation_pressure * monetary_policy_tightness * recession_probability * (geopolitical_shock ? 1.8 : 1.0) * ai_bubble_dynamics",
  "confidence": 0.75,
  "reasoning": "High confidence in base rate calculation (extensive historical data). Moderate confidence in driver parameterization due to: (1) inherent uncertainty in 2026 economic conditions, (2) difficulty predicting policy responses, (3) unknown geopolitical developments. The model captures major risk factors but 2-year forecasts have significant uncertainty. Historical patterns are reliable but each cycle has unique characteristics. Confidence reduced by current unprecedented factors: AI disruption, post-pandemic economic dynamics, high government debt levels globally."
}
```

---

## 5. ai_bubble_dynamics `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.40 | 1.70 | 5.00 | multiplier |

> Significant AI investment boom 2023-2025 with concentration in mega-cap tech. Magnificent 7 stocks represent ~30% of S&P 500. Bubble dynamics and concentration risk increase correction probability. If AI monetization disappoints or regulatory pressure increases, correction risk elevated 20-60%.

### Assigned Agents

- **market_research** (schedule: once)  
  Query: _What is the probability of a US recession occurring in 2026? Analyze: (1) Current yield curve signals and historical lead times, (2) Base rates of recessions (frequency per decade since 1950), (3) Current position in business cycle and typical expansion durations, (4) Leading economic indicators (Conference Board LEI, unemployment trends, manufacturing PMI), (5) Fed policy trajectory and historical tightening cycle outcomes. Provide probability distribution for 2026 recession risk with P10, P50, P90 estimates._

_No evidence collected yet. Assign an agent to research this driver._

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Annual 15%+ market corrections in S&P 500",
    "historical_frequency": 0.18,
    "sample_size": 95,
    "reasoning": "From 1928-2023, the S&P 500 experienced 17 calendar years with declines of 15% or more (peak-to-trough within the year), out of approximately 95 years. This yields roughly 18% base rate. Notable examples: 1929 (-24%), 1930 (-25%), 1931 (-43%), 1937 (-35%), 1974 (-27%), 2002 (-22%), 2008 (-37%), 2022 (-18%). The frequency has been somewhat lower in recent decades (post-1950: ~15%) but we use the fuller historical record for robustness."
  },
  "drivers": [
    {
      "name": "valuation_pressure",
      "display_name": "Valuation Pressure from Elevated Multiples",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.15,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of late 2024, S&P 500 forward P/E ratios are around 21-22x, above the 20-year average of ~16-17x. Elevated valuations historically increase correction risk. The Shiller CAPE ratio is around 30-35, well above historical median of 16. High valuations don't predict timing but increase vulnerability to negative catalysts. A multiplier of 1.15 reflects moderately elevated risk; could range from 0.9 (if valuations normalize lower by 2026) to 1.4 (if valuations remain stretched and become a trigger)."
    },
    {
      "name": "monetary_policy_tightness",
      "display_name": "Monetary Policy Stance and Rate Environment",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.1,
      "p95": 1.35,
      "unit": "multiplier",
      "rationale": "Federal Reserve policy in 2026 will depend on inflation trajectory. If rates remain elevated (4-5%) or need to rise again, this increases recession risk and market stress. Historical pattern: aggressive tightening cycles often precede corrections (2000, 2007-08, 2022). By 2026, we'll be 3-4 years into the current cycle. P50 of 1.1 assumes moderately restrictive policy; P95 of 1.35 reflects scenario of renewed tightening; P5 of 0.85 reflects accommodative pivot reducing risk."
    },
    {
      "name": "recession_probability",
      "display_name": "Economic Recession in 2026",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.25,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Recessions strongly correlate with 15%+ market drops. Historical data shows ~70% of recessions coincide with significant market corrections. Base recession probability for any given year is ~15-20%. For 2026 specifically, we're far enough from 2023-24 that cycle risks accumulate. If recession occurs, market drop probability increases substantially (multiplier 1.5-2.0). If no recession, multiplier near 1.0. P50 of 1.25 reflects elevated but not extreme recession risk."
    },
    {
      "name": "geopolitical_shock",
      "display_name": "Major Geopolitical Crisis or Shock",
      "type": "binary",
      "probability": 0.25,
      "impact_multiplier": 1.8,
      "rationale": "Major geopolitical events (wars, trade conflicts, energy crises) can trigger sharp market corrections. Examples: Ukraine invasion 2022, COVID-19 2020, 9/11 2001. Estimating ~25% probability of a significant geopolitical shock in 2026 (Taiwan tensions, Middle East escalation, major cyber attack, trade war escalation). If occurs, increases correction probability by ~80% (multiplier 1.8). This is conservative given current global tensions."
    },
    {
      "name": "ai_bubble_dynamics",
      "display_name": "AI/Tech Sector Bubble Correction",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.1,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of 2024, AI-related stocks have seen massive appreciation with concentration risk (Magnificent 7 = ~30% of S&P 500). If AI investment thesis disappoints or regulatory pressure increases by 2026, concentrated selling could trigger broader correction. P50 of 1.1 reflects moderate additional risk from sector concentration; P95 of 1.4 reflects bubble-pop scenario; P5 of 0.95 reflects successful AI monetization reducing risk."
    }
  ],
  "evidence": [
    {
      "source": "Yale Shiller CAPE Ratio Database & S&P 500 Historical Returns",
      "summary": "Historical analysis of market corrections and valuation metrics",
      "key_findings": [
        "17 calendar years with 15%+ declines from 1928-2023 (18% frequency)",
        "CAPE ratios above 30 have historically preceded major corrections within 1-5 years",
        "Post-1950 frequency of 15%+ corrections is approximately 15%",
        "Average peak-to-trough decline in correction years: -24%"
      ],
      "relevance": 0.95
    },
    {
      "source": "Federal Reserve Economic Data (FRED) & BLS recession dating",
      "summary": "Relationship between recessions and market corrections",
      "key_findings": [
        "12 recessions since 1945; 9 accompanied by 15%+ market declines (75% correlation)",
        "Average lead time of market decline before recession: 6 months",
        "Markets declined average 30% during recession-linked corrections",
        "Non-recession corrections typically milder (average -18%)"
      ],
      "relevance": 0.90
    },
    {
      "source": "Goldman Sachs, JP Morgan 2024 Market Outlook Reports",
      "summary": "Current market conditions and forward-looking risk assessments",
      "key_findings": [
        "Forward P/E ratios at 21-22x vs historical average of 16-17x",
        "Market concentration at highest levels since 2000 tech bubble",
        "Consensus 2026 recession probability: 20-30% range",
        "Rate cuts expected but terminal rate uncertainty remains high"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF World Economic Outlook & Geopolitical Risk Index",
      "summary": "Global risk factors and geopolitical tensions",
      "key_findings": [
        "Geopolitical Risk Index elevated above historical average",
        "Multiple flashpoints: Taiwan, Middle East, Ukraine, trade tensions",
        "Historical frequency of market-moving geopolitical events: ~20-30% per year",
        "Energy price volatility remains elevated risk factor"
      ],
      "relevance": 0.80
    }
  ],
  "model_expression": "base_rate * valuation_pressure * monetary_policy_tightness * recession_probability * (geopolitical_shock ? 1.8 : 1.0) * ai_bubble_dynamics",
  "confidence": 0.75,
  "reasoning": "High confidence in base rate calculation (extensive historical data). Moderate confidence in driver parameterization due to: (1) inherent uncertainty in 2026 economic conditions, (2) difficulty predicting policy responses, (3) unknown geopolitical developments. The model captures major risk factors but 2-year forecasts have significant uncertainty. Historical patterns are reliable but each cycle has unique characteristics. Confidence reduced by current unprecedented factors: AI disruption, post-pandemic economic dynamics, high government debt levels globally."
}
```

---

## General Evidence (3)

_Evidence not linked to a specific driver._

### Mock Executor — relevance 75%

Generated by mock executor for testing purposes

### Agent: fermi (Claude API) — relevance 50%

```json
{
  "base_rate": {
    "reference_class": "Annual 15%+ market corrections in S&P 500",
    "historical_frequency": 0.18,
    "sample_size": 95,
    "reasoning": "From 1928-2023, the S&P 500 experienced 17 calendar years with declines of 15% or more (peak-to-trough within the year), out of approximately 95 years. This yields roughly 18% base rate. Notable examples: 1929 (-24%), 1930 (-25%), 1931 (-43%), 1937 (-35%), 1974 (-27%), 2002 (-22%), 2008 (-37%), 2022 (-18%). The frequency has been somewhat lower in recent decades (post-1950: ~15%) but we use the fuller historical record for robustness."
  },
  "drivers": [
    {
      "name": "valuation_pressure",
      "display_name": "Valuation Pressure from Elevated Multiples",
      "type": "continuous",
      "p5": 0.9,
      "p50": 1.15,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of late 2024, S&P 500 forward P/E ratios are around 21-22x, above the 20-year average of ~16-17x. Elevated valuations historically increase correction risk. The Shiller CAPE ratio is around 30-35, well above historical median of 16. High valuations don't predict timing but increase vulnerability to negative catalysts. A multiplier of 1.15 reflects moderately elevated risk; could range from 0.9 (if valuations normalize lower by 2026) to 1.4 (if valuations remain stretched and become a trigger)."
    },
    {
      "name": "monetary_policy_tightness",
      "display_name": "Monetary Policy Stance and Rate Environment",
      "type": "continuous",
      "p5": 0.85,
      "p50": 1.1,
      "p95": 1.35,
      "unit": "multiplier",
      "rationale": "Federal Reserve policy in 2026 will depend on inflation trajectory. If rates remain elevated (4-5%) or need to rise again, this increases recession risk and market stress. Historical pattern: aggressive tightening cycles often precede corrections (2000, 2007-08, 2022). By 2026, we'll be 3-4 years into the current cycle. P50 of 1.1 assumes moderately restrictive policy; P95 of 1.35 reflects scenario of renewed tightening; P5 of 0.85 reflects accommodative pivot reducing risk."
    },
    {
      "name": "recession_probability",
      "display_name": "Economic Recession in 2026",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.25,
      "p95": 1.6,
      "unit": "multiplier",
      "rationale": "Recessions strongly correlate with 15%+ market drops. Historical data shows ~70% of recessions coincide with significant market corrections. Base recession probability for any given year is ~15-20%. For 2026 specifically, we're far enough from 2023-24 that cycle risks accumulate. If recession occurs, market drop probability increases substantially (multiplier 1.5-2.0). If no recession, multiplier near 1.0. P50 of 1.25 reflects elevated but not extreme recession risk."
    },
    {
      "name": "geopolitical_shock",
      "display_name": "Major Geopolitical Crisis or Shock",
      "type": "binary",
      "probability": 0.25,
      "impact_multiplier": 1.8,
      "rationale": "Major geopolitical events (wars, trade conflicts, energy crises) can trigger sharp market corrections. Examples: Ukraine invasion 2022, COVID-19 2020, 9/11 2001. Estimating ~25% probability of a significant geopolitical shock in 2026 (Taiwan tensions, Middle East escalation, major cyber attack, trade war escalation). If occurs, increases correction probability by ~80% (multiplier 1.8). This is conservative given current global tensions."
    },
    {
      "name": "ai_bubble_dynamics",
      "display_name": "AI/Tech Sector Bubble Correction",
      "type": "continuous",
      "p5": 0.95,
      "p50": 1.1,
      "p95": 1.4,
      "unit": "multiplier",
      "rationale": "As of 2024, AI-related stocks have seen massive appreciation with concentration risk (Magnificent 7 = ~30% of S&P 500). If AI investment thesis disappoints or regulatory pressure increases by 2026, concentrated selling could trigger broader correction. P50 of 1.1 reflects moderate additional risk from sector concentration; P95 of 1.4 reflects bubble-pop scenario; P5 of 0.95 reflects successful AI monetization reducing risk."
    }
  ],
  "evidence": [
    {
      "source": "Yale Shiller CAPE Ratio Database & S&P 500 Historical Returns",
      "summary": "Historical analysis of market corrections and valuation metrics",
      "key_findings": [
        "17 calendar years with 15%+ declines from 1928-2023 (18% frequency)",
        "CAPE ratios above 30 have historically preceded major corrections within 1-5 years",
        "Post-1950 frequency of 15%+ corrections is approximately 15%",
        "Average peak-to-trough decline in correction years: -24%"
      ],
      "relevance": 0.95
    },
    {
      "source": "Federal Reserve Economic Data (FRED) & BLS recession dating",
      "summary": "Relationship between recessions and market corrections",
      "key_findings": [
        "12 recessions since 1945; 9 accompanied by 15%+ market declines (75% correlation)",
        "Average lead time of market decline before recession: 6 months",
        "Markets declined average 30% during recession-linked corrections",
        "Non-recession corrections typically milder (average -18%)"
      ],
      "relevance": 0.90
    },
    {
      "source": "Goldman Sachs, JP Morgan 2024 Market Outlook Reports",
      "summary": "Current market conditions and forward-looking risk assessments",
      "key_findings": [
        "Forward P/E ratios at 21-22x vs historical average of 16-17x",
        "Market concentration at highest levels since 2000 tech bubble",
        "Consensus 2026 recession probability: 20-30% range",
        "Rate cuts expected but terminal rate uncertainty remains high"
      ],
      "relevance": 0.85
    },
    {
      "source": "IMF World Economic Outlook & Geopolitical Risk Index",
      "summary": "Global risk factors and geopolitical tensions",
      "key_findings": [
        "Geopolitical Risk Index elevated above historical average",
        "Multiple flashpoints: Taiwan, Middle East, Ukraine, trade tensions",
        "Historical frequency of market-moving geopolitical events: ~20-30% per year",
        "Energy price volatility remains elevated risk factor"
      ],
      "relevance": 0.80
    }
  ],
  "model_expression": "base_rate * valuation_pressure * monetary_policy_tightness * recession_probability * (geopolitical_shock ? 1.8 : 1.0) * ai_bubble_dynamics",
  "confidence": 0.75,
  "reasoning": "High confidence in base rate calculation (extensive historical data). Moderate confidence in driver parameterization due to: (1) inherent uncertainty in 2026 economic conditions, (2) difficulty predicting policy responses, (3) unknown geopolitical developments. The model captures major risk factors but 2-year forecasts have significant uncertainty. Historical patterns are reliable but each cycle has unique characteristics. Confidence reduced by current unprecedented factors: AI disruption, post-pandemic economic dynamics, high government debt levels globally."
}
```

### Agent: fermi (Claude API) — relevance 75%

Historical analysis shows 15%+ market corrections occur in roughly 12% of calendar years. For 2026 specifically, several risk factors elevate this probability: elevated valuations, transition to higher interest rate regime, moderate recession risk, geopolitical uncertainties, and concentration/bubble dynamics in AI/tech sectors. The probabilistic model combining these factors suggests a 20-30% probability range, meaningfully above the base rate but not indicating a correction is more likely than not.

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: valuation_pressure * interest_rate_regime * (if recession_probability then 3.0 else 1.0) * (if geopolitical_shock then 1.6 else 1.0) * ai_bubble_dynamics
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| market_research | fundamentals | Research evidence for the 'fundamentals' driver in the forecast: "will there bee a signifaicant market drop of over 15% in 2026?" |
| entity_investigator | market_conditions | Research evidence for the 'market_conditions' driver in the forecast: "will there bee a signifaicant market drop of over 15% in 2026?" |
| market_research | valuation_pressure | Research evidence for the 'valuation_pressure' driver in the forecast: "will there bee a signifaicant market drop of over 15% in 2026?" |
| entity_investigator | recession_probability | Analyze current market fundamentals (valuations, earnings, economic indicators) relative to historical norms and expectations for 2025-2026. What is the probability distribution of fundamental strength/weakness that could trigger a 15%+ market correction in 2026? Consider: (1) current P/E ratios vs historical averages before corrections, (2) earnings growth sustainability, (3) interest rate trajectory impact on valuations, (4) base rates of corrections when fundamentals are at current levels, (5) leading indicators of fundamental deterioration. |
| market_research | ai_bubble_dynamics | What is the probability of a US recession occurring in 2026? Analyze: (1) Current yield curve signals and historical lead times, (2) Base rates of recessions (frequency per decade since 1950), (3) Current position in business cycle and typical expansion durations, (4) Leading economic indicators (Conference Board LEI, unemployment trends, manufacturing PMI), (5) Fed policy trajectory and historical tightening cycle outcomes. Provide probability distribution for 2026 recession risk with P10, P50, P90 estimates. |
| macro_forecaster | geopolitical_shock | Analyze the relationship between interest rate regimes and equity market corrections of 15%+ magnitude. Specifically: (1) What is the base rate of 15%+ corrections when policy rates are in the 3-5% range vs 0-2% range, using data from 1980-present? (2) What are the consensus forecasts for Fed funds rate trajectory through 2026, and what is the historical accuracy of 2-year forward rate predictions? (3) How do equity risk premiums typically behave 12-24 months after rate hiking cycles end? (4) In previous cycles where rates stabilized at 3-5% (1990s, mid-2000s), what was the frequency and magnitude of corrections? Provide probability distributions for likely rate scenarios in 2026 and their historical correlation with market drawdowns. |
| macro_forecaster | interest_rate_regime | What is the historical relationship between elevated CAPE ratios (>30) and subsequent 1-2 year market corrections of 15%+ in the S&P 500? Specifically: (1) base rate of 15%+ drops within 24 months when CAPE >30 vs <25, (2) typical catalysts that trigger valuation compression from elevated levels, (3) current valuation metrics (CAPE, forward P/E, equity risk premium) vs historical distributions, (4) scenario analysis of conditions in 2025-2026 that could trigger mean reversion (rate changes, earnings disappointments, risk premium expansion) |
| sentiment_analyzer | geopolitical_shock | Analyze the relationship between interest rate regimes and equity market corrections of 15%+ magnitude. Specifically: (1) What is the base rate of 15%+ corrections when policy rates are in the 3-5% range vs 0-2% range, using data from 1980-present? (2) What are the consensus forecasts for Fed funds rate trajectory through 2026, and what is the historical accuracy of 2-year forward rate predictions? (3) How do equity risk premiums typically behave 12-24 months after rate hiking cycles end? (4) In previous cycles where rates stabilized at 3-5% (1990s, mid-2000s), what was the frequency and magnitude of corrections? Provide probability distributions for likely rate scenarios in 2026 and their historical correlation with market drawdowns. |
| sentiment_analyzer | valuation_pressure | What is the probability of a major geopolitical shock in 2026 that would trigger a >15% market correction? Analyze: (1) base rates of market-moving geopolitical events 2000-2025, (2) current flashpoint assessment (Ukraine, Taiwan, Middle East, North Korea), (3) historical market responses to different shock types, (4) leading indicators of geopolitical instability, and (5) 2026-specific risk factors including US political transition effects and great power competition trajectories. |
| sentiment_analyzer | recession_probability | What is the historical relationship between elevated CAPE ratios (>30) and subsequent 1-2 year market corrections of 15%+? Specifically: (1) Base rate of 15%+ drops within 24 months when CAPE >30 vs <25, (2) Current CAPE ratio context (late 2024/early 2025) including sector composition effects, (3) How interest rate environment (current ~4-5% vs historical) affects justified P/E ratios, (4) Historical precedents: 1929 (CAPE ~30), 2000 (CAPE ~44), 2021-2022 (CAPE ~38), and (5) Median time-to-correction and magnitude distribution when starting from current valuation levels. |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v76 · 2026-03-07 16:45 UTC_
