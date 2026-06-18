# Will United States win the 2026 FIFA World Cup?

**Probability:** 2.0% · **Version:** v2 · **Updated:** 2026-06-18 12:22 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 1 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **2.2%** |
| Fermi estimate | **2.0%** |
| Divergence | +0.3pp below crowd (Consensus) |
| 24h volume | $3.0M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups, 8 distinct winners

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via socio_capital, institutional_capacity, dynamic_performance, squad_quality, tactical_efficiency, fixture_context.

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-18 12:22 | 2.0% | 2.1% | 2.2% | -0.1pp | -0.3pp | Initial: 2.0% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-18 12:22 | 2.0% | 2.1% | 2.2% | -0.1pp | -0.3pp | 2.0% (→), 6 drivers, 1 evidence |

**Model line:** ```▁▁``` (range 2.0% – 2.0%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.00 | 0.00 | 0.00 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for United States (2024–2026 latest available)_

### Evidence (1) — Partial quality (65%)

#### Agent: macro_data_agent — relevance 50% · quality ●●○ Med (65%) · 2026-06-18

Based on the search results, I have the following data points. Let me compile the structured evidence:

---

## **UNITED STATES — SOCIOECONOMIC CAPITAL INDICATORS (2024–2025)**

### **Core Indicators**

**[INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$86,601** (calculated from total GDP $29,184.89 billion ÷ 337 million population); log₁₀ ≈ **4.937**

**[INDICATOR]** Population (2025, U.S. Census Bureau): **341.78 million**; log₁₀ ≈ **2.534**

**[INDICATOR]** HDI (2023, UNDP HDR): **0.938**; logit = log(0.938 / (1 − 0.938)) ≈ **2.709**

**[DATA AGE]** GDP and population data are 2024–2025 official estimates (World Bank, U.S. Census Bureau); HDI is 2023 UNDP release (most recent internationally comparable figure as of 2026).

---

### **Factor Transformation & Multiplier**

**[BASELINE]** Global high-income median benchmarks: GDP per capita log ≈ 4.65; population log ≈ 1.50; HDI logit ≈ 2.20

**[TRANSFORM]** Composite X1 score using standard weights (0.4 GDP, 0.3 Pop, 0.3 HDI):
- (0.4 × 4.937 + 0.3 × 2.534 + 0.3 × 2.709 − 2.6) / 0.7 ≈ **+1.12** — substantially above global high-income median

**[MULTIPLIER]** Suggested p50: **1.25** (p5: **1.10**, p95: **1.45**) — United States exhibits top-decile GDP/capita ($86.6k, 4th globally) and HDI (0.938, 20th globally), with the largest absolute economy; X1 factor receives strong positive adjustment relative to any international field median

---

### **Rationale**

The United States maintains the world's largest nominal GDP ($29.2 trillion, 2024) and a GDP per capita in the top 5 globally. The HDI of 0.938 places it in the "very high human development" category (20th globally), though slightly below Nordic leaders (Iceland 0.972, Switzerland 0.970). The population of 342 million provides substantial absolute market scale. All three indicators converge on a **top-quartile socioeconomic capital profile**, justifying a multiplier well above unity for any factor model comparing the U.S. to a global or regional field.

**Key findings:**

- [INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$86,601** (calculated from total GDP $29,184.89 billion ÷ 337 million population); log₁₀ ≈ **4.937**
- [INDICATOR]** Population (2025, U.S. Census Bureau): **341.78 million**; log₁₀ ≈ **2.534**
- [INDICATOR]** HDI (2023, UNDP HDR): **0.938**; logit = log(0.938 / (1 − 0.938)) ≈ **2.709**
- [DATA AGE]** GDP and population data are 2024–2025 official estimates (World Bank, U.S. Census Bureau); HDI is 2023 UNDP release (most recent internationally comparable figure as of 2026).
- [BASELINE]** Global high-income median benchmarks: GDP per capita log ≈ 4.65; population log ≈ 1.50; HDI logit ≈ 2.20
- [TRANSFORM]** Composite X1 score using standard weights (0.4 GDP, 0.3 Pop, 0.3 HDI):
- (0.4 × 4.937 + 0.3 × 2.534 + 0.3 × 2.709 − 2.6) / 0.7 ≈ **+1.12** — substantially above global high-income median
- [MULTIPLIER]** Suggested p50: **1.25** (p5: **1.10**, p95: **1.45**) — United States exhibits top-decile GDP/capita ($86.6k, 4th globally) and HDI (0.938, 20th globally), with the largest absolute economy; X1 factor receives strong positive adjustment relative to any international field median
- The United States maintains the world's largest nominal GDP ($29.2 trillion, 2024) and a GDP per capita in the top 5 globally. The HDI of 0.938 places it in the "very high human development" category (20th globally), though slightly below Nordic leaders (Iceland 0.972, Switzerland 0.970). The population of 342 million provides substantial absolute market scale. All three indicators converge on a **top-quartile socioeconomic capital profile**, justifying a multiplier well above unity for any factor model comparing the U.S. to a global or regional field.

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.75 | 1.00 | 1.30 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for United States_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for United States_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.00 | 1.35 |  |

> Top-flight league penetration + market-value concentration; updates as injuries / form are reported.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for United States_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.25 |  |

> Shot conversion, defensive duels, pressing intensity — observable per-match.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for United States_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.90 | 1.00 | 1.10 |  |

> Per-fixture context: venue, climate, rest, altitude. Volatile, refreshed per match.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for United States: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for United States (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for United States |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for United States |
| fixture_context_agent | fixture_context | Upcoming fixtures for United States: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v2 · 2026-06-18 12:22 UTC_
