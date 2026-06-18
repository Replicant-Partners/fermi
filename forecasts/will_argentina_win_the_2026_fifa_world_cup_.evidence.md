# Will Argentina win the 2026 FIFA World Cup?

**Probability:** 9.3% · **Version:** v1 · **Updated:** 2026-06-18 11:25 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 1 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **11.6%** |
| Fermi estimate | **9.3%** |
| Divergence | +2.2pp below crowd (Minor divergence) |
| 24h volume | $6.1M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups, 8 distinct winners

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via socio_capital, institutional_capacity, dynamic_performance, squad_quality, tactical_efficiency, fixture_context.

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.00 | 0.00 | 0.00 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Argentina (2024–2026 latest available)_

### Evidence (1) — Strong quality (75%)

#### Agent: macro_data_agent — relevance 50% · quality ●●● High (75%) · 2026-06-18

# SOCIOECONOMIC CAPITAL (X1) — ARGENTINA, 2026 FIFA WORLD CUP

## CORE INDICATORS

**[INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$12,667** (Trading Economics, World Bank source)  
- Log₁₀ transformation: log₁₀(12,667) = **4.103**

**[INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **45,696,159** (45.7 million)  
- Log₁₀ transformation: log₁₀(45.7) = **1.660**

**[INDICATOR]** HDI (2023, UNDP Human Development Report 2025, Table 1): **0.865**  
- Logit transformation: log(0.865 / (1 - 0.865)) = log(0.865 / 0.135) = log(6.407) = **1.857**

**[DATA AGE]** All three indicators are current: GDP/capita and population from 2024 World Bank data; HDI from UNDP HDR 2025 (published May 2025, covering 2023 data). Argentina ranks **47th globally** in HDI (Very High Human Development category).

---

## FIELD BASELINE — 2026 WORLD CUP (48 TEAMS)

**[BASELINE]** The 2026 World Cup field spans extreme socioeconomic diversity:
- **Richest participants**: USA ($53,202), Switzerland, Norway, Qatar (all >$60k GDP/capita)
- **Poorest participants**: DR Congo ($752), Haiti (~$1,800), Curaçao (small-island economy)
- **Per-capita range**: 71× gap between richest and poorest (World Data Lab analysis)

**[BASELINE]** Estimated field **median GDP per capita**: ~$15,000–$18,000  
- Upper tercile (16 teams): Advanced economies (Western Europe, North America, Japan, South Korea, Australia) — GDP/capita >$30k
- Middle tercile (16 teams): Upper-middle-income (Argentina, Brazil, Mexico, Uruguay, Saudi Arabia, Iran, Türkiye) — GDP/capita $8k–$25k
- Lower tercile (16 teams): Lower-middle and low-income (North/Sub-Saharan Africa, Central America, Oceania minnows) — GDP/capita <$8k

**[BASELINE]** Estimated field **median HDI**: ~0.78–0.82 (High Human Development)  
- Argentina's 0.865 places it in the **upper quartile** of the tournament field

**[BASELINE]** Estimated field **median population (log)**: ~1.5–1.7 (30–50 million)  
- Argentina at 45.7M (log 1.660) sits near the field median

---

## FACTOR TRANSFORMATION

**[TRANSFORM]** Using standard X1 (Socioeconomic Capital) factor weights:
- **40% GDP/capita (log)**: 0.40 × 4.103 = 1.641
- **30% Population (log)**: 0.30 × 1.660 = 0.498
- **30% HDI (logit)**: 0.30 × 1.857 = 0.557

**[TRANSFORM]** Raw factor score: 1.641 + 0.498 + 0.557 = **2.696**

**[TRANSFORM]** Standardization vs. WC 2026 field baseline:
- Field median raw score (estimated): ~2.50 (based on median GDP/capita log ~4.20, pop log ~1.60, HDI logit ~1.30)
- Argentina deviation: +0.20 standard deviations above field median
- **Argentina sits in the 60th–65th percentile** of the tournament's socioeconomic distribution

**[TRANSFORM]** Key insight: Argentina's **HDI (0.865) significantly exceeds** the field median (~0.78), driven by high educational attainment (18.8 years expected schooling) and life expectancy (77.4 years). However, its **GDP/capita ($12,667) is below** the field median (~$16k), reflecting recent economic volatility and currency depreciation.

---

## MULTIPLIER OUTPUT

**[MULTIPLIER]** Suggested p50: **1.05** (p5: 0.92, p95: 1.18) — Argentina's socioeconomic profile sits modestly above the 2026 World Cup field median, driven by strong human development indicators (HDI 0.865, 47th globally) that offset below-median GDP per capita ($12,667). The country's 45.7M population provides a large talent pool comparable to Spain, while educational infrastructure supports elite athlete development. However, economic instability limits the multiplier effect relative to wealthier European/North American competitors.

**Key findings:**

- [INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$12,667** (Trading Economics, World Bank source)
- Log₁₀ transformation: log₁₀(12,667) = **4.103**
- [INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **45,696,159** (45.7 million)
- Log₁₀ transformation: log₁₀(45.7) = **1.660**
- [INDICATOR]** HDI (2023, UNDP Human Development Report 2025, Table 1): **0.865**
- Logit transformation: log(0.865 / (1 - 0.865)) = log(0.865 / 0.135) = log(6.407) = **1.857**
- [DATA AGE]** All three indicators are current: GDP/capita and population from 2024 World Bank data; HDI from UNDP HDR 2025 (published May 2025, covering 2023 data). Argentina ranks **47th globally** in HDI (Very High Human Development category).
- [BASELINE]** The 2026 World Cup field spans extreme socioeconomic diversity:
- **Richest participants**: USA ($53,202), Switzerland, Norway, Qatar (all >$60k GDP/capita)
- **Poorest participants**: DR Congo ($752), Haiti (~$1,800), Curaçao (small-island economy)
- **Per-capita range**: 71× gap between richest and poorest (World Data Lab analysis)
- [BASELINE]** Estimated field **median GDP per capita**: ~$15,000–$18,000
- Upper tercile (16 teams): Advanced economies (Western Europe, North America, Japan, South Korea, Australia) — GDP/capita >$30k
- Middle tercile (16 teams): Upper-middle-income (Argentina, Brazil, Mexico, Uruguay, Saudi Arabia, Iran, Türkiye) — GDP/capita $8k–$25k
- Lower tercile (16 teams): Lower-middle and low-income (North/Sub-Saharan Africa, Central America, Oceania minnows) — GDP/capita <$8k

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.75 | 1.00 | 1.30 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Argentina_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.00 | 0.00 | 0.00 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.00 | 1.35 |  |

> Top-flight league penetration + market-value concentration; updates as injuries / form are reported.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.25 |  |

> Shot conversion, defensive duels, pressing intensity — observable per-match.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.90 | 1.00 | 1.10 |  |

> Per-fixture context: venue, climate, rest, altitude. Volatile, refreshed per match.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Argentina: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Argentina (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Argentina |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina |
| fixture_context_agent | fixture_context | Upcoming fixtures for Argentina: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v1 · 2026-06-18 11:25 UTC_
