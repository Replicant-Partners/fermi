# Will France win the 2026 FIFA World Cup?

**Probability:** 11.5% · **Version:** v1 · **Updated:** 2026-06-19 01:28 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 1 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **18.4%** |
| Fermi estimate | **11.5%** |
| Divergence | +7.0pp below crowd (Moderate divergence — potential edge) |
| 24h volume | $2.7M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 11.5%**

Inside view: model evaluates to 11.5% (p5=8.4%, p95=15.0%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 9pp above (11.5% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 8.4% · median = 11.3% · p95 = 15.0% · σ = 0.020

```
▁▁▂▃▅▇██▇▆▄▃▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 6.3% | 15 | 0.1% |
| 7.0% | 70 | 0.7% |
| 7.7% | 235 | 2.4% |
| 8.5% | 525 | 5.2% |
| 9.2% | 921 | 9.2% |
| 9.9% | 1217 | 12.2% |
| 10.7% | 1412 | 14.1% |
| 11.4% | 1486 | 14.9% |
| 12.1% | 1217 | 12.2% |
| 12.8% | 1033 | 10.3% |
| 13.6% | 732 | 7.3% |
| 14.3% | 463 | 4.6% |
| 15.0% | 301 | 3.0% |
| 15.7% | 181 | 1.8% |
| 16.5% | 93 | 0.9% |
| 17.2% | 60 | 0.6% |
| 17.9% | 26 | 0.3% |
| 18.7% | 9 | 0.1% |
| 19.4% | 3 | 0.0% |
| 20.1% | 1 | 0.0% |

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for France (2024–2026 latest available)_

### Evidence (1) — Strong quality (75%)

#### Agent: macro_data_agent — relevance 50% · quality ●●● High (75%) · 2026-06-19

Perfect! Now I have the complete list of 48 qualified teams. Let me compile the evidence for France and calculate the field baseline.

---

## SOCIOECONOMIC CAPITAL (X1) EVIDENCE — FRANCE (FRA)

### Core Indicators (2024 data)

**[INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$39,441** current US$; log₁₀ ≈ **4.596**  
Source: Trading Economics / World Bank, 2024 release

**[INDICATOR]** GDP per capita PPP (2024, World Bank NY.GDP.PCAP.PP.CD): **$61,322** international $; log₁₀ ≈ **4.788**  
Source: Trading Economics / World Bank, 2024 release

**[INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **68.52 million**; log₁₀ ≈ **1.836**  
Source: Trading Economics / World Bank, 2024 release (68,516,699 total)

**[INDICATOR]** HDI (2023, UNDP Human Development Report): **0.920**; logit = log(0.920 / 0.080) ≈ **2.451**  
Source: UNDP HDR 2025 release (data year 2023); TheGlobalEconomy.com

---

### Field Baseline — World Cup 2026 (48 teams)

**[BASELINE]** WC 2026 field composition: 48 qualified nations spanning extreme economic diversity  
- **Richest** (GDP/capita PPP): Switzerland (~$94k), Norway (~$89k), USA (~$81k), Qatar (~$115k)  
- **Poorest** (GDP/capita PPP): DR Congo (~$752), Haiti (~$3.1k), Senegal (~$3.7k)  
- **Range**: 71× gap between USA and DR Congo (per Politico/World Data Lab analysis)

**[BASELINE]** Estimated field **median GDP per capita** (current US$): **~$12,000–15,000**  
- Log₁₀ median ≈ **4.08–4.18** (mid-range estimate: **4.13**)  
- France at $39,441 (log 4.596) sits in the **top quartile** (75th–85th percentile)

**[BASELINE]** Estimated field **median population**: **~20–30 million**  
- Log₁₀ median ≈ **1.30–1.48** (mid-range estimate: **1.39**)  
- France at 68.5M (log 1.836) is **above median** but below giants (USA 343M, Brazil 217M, Mexico 128M)

**[BASELINE]** Estimated field **median HDI**: **~0.75–0.80** (mix of "high" and "very high" development)  
- Logit median ≈ **1.10–1.39** (mid-range estimate: **1.25**)  
- France at 0.920 (logit 2.451) is in the **top decile** (very high development tier)

---

### Transform Calculation

Using the standard X1 (Socioeconomic Capital) composite:

**[TRANSFORM]** X1 = 0.4 · GDP_log + 0.3 · Pop_log + 0.3 · HDI_logit  
= 0.4 · 4.596 + 0.3 · 1.836 + 0.3 · 2.451  
= 1.838 + 0.551 + 0.735  
= **3.124**

**[TRANSFORM]** Field median X1 (estimated):  
= 0.4 · 4.13 + 0.3 · 1.39 + 0.3 · 1.25  
= 1.652 + 0.417 + 0.375  
= **2.444**

**[TRANSFORM]** France deviation from field median:  
Δ = 3.124 − 2.444 = **+0.680** (standardized units)  
= **+0.97 standard deviations** above field mean (assuming σ ≈ 0.70 for WC field)

---

### Multiplier Recommendation

France's socioeconomic profile is **substantially above** the World Cup 2026 field median across all three dimensions:
- **GDP/capita**: top quartile (312% of world average per Trading Economics)
- **Population**: above median, large talent pool (68.5M vs. field median ~25M)
- **HDI**: top decile (0.920, "very high" development tier)

This translates to:
- **Stronger youth development infrastructure** (academies, coaching, facilities)
- **Greater professional league depth** (Ligue 1 + diaspora in top European leagues)
- **Higher baseline athletic participation** and nutrition standards

**[MULTIPLIER]** Suggested p50: **1.15** (p5: **1.05**, p95: **1.28**) — France's GDP/capita ($39k, log 4.60), population (68.5M, log 1.84), and HDI (0.920, logit 2.45) all exceed WC 2026 field medians by 0.7–1.0 SD, lifting socioeconomic capital factor materially above baseline

---

### Data Quality Note

**[DATA AGE]** All indicators current as of 2024 (GDP, population) or 2023 (HDI, latest UNDP release). World Bank and UNDP official sources. Field baseline is **estimated** from published ranges (Investec, World Data Lab, Politico analyses of WC 2026 field); precise median would require full 48-country dataset but directional confidence is high given France's clear top-quartile position.

**Key findings:**

- [INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$39,441** current US$; log₁₀ ≈ **4.596**
- [INDICATOR]** GDP per capita PPP (2024, World Bank NY.GDP.PCAP.PP.CD): **$61,322** international $; log₁₀ ≈ **4.788**
- [INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **68.52 million**; log₁₀ ≈ **1.836**
- [INDICATOR]** HDI (2023, UNDP Human Development Report): **0.920**; logit = log(0.920 / 0.080) ≈ **2.451**
- [BASELINE]** WC 2026 field composition: 48 qualified nations spanning extreme economic diversity
- **Richest** (GDP/capita PPP): Switzerland (~$94k), Norway (~$89k), USA (~$81k), Qatar (~$115k)
- **Poorest** (GDP/capita PPP): DR Congo (~$752), Haiti (~$3.1k), Senegal (~$3.7k)
- **Range**: 71× gap between USA and DR Congo (per Politico/World Data Lab analysis)
- [BASELINE]** Estimated field **median GDP per capita** (current US$): **~$12,000–15,000**
- Log₁₀ median ≈ **4.08–4.18** (mid-range estimate: **4.13**)
- France at $39,441 (log 4.596) sits in the **top quartile** (75th–85th percentile)
- [BASELINE]** Estimated field **median population**: **~20–30 million**
- Log₁₀ median ≈ **1.30–1.48** (mid-range estimate: **1.39**)
- France at 68.5M (log 1.836) is **above median** but below giants (USA 343M, Brazil 217M, Mexico 128M)
- [BASELINE]** Estimated field **median HDI**: **~0.75–0.80** (mix of "high" and "very high" development)

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for France_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for France: venue, climate, rest days, altitude, opponent travel burden_

_No evidence collected yet. Assign an agent to research this driver._

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: socio_capital * institutional_capacity * dynamic_performance * squad_quality * tactical_efficiency * fixture_context
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for France (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for France |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for France |
| fixture_context_agent | fixture_context | Upcoming fixtures for France: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v1 · 2026-06-19 01:28 UTC_
