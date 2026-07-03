# Will Cape Verde win the 2026 FIFA World Cup?

**Probability:** 1.7% · **Version:** v1 · **Updated:** 2026-07-03 12:29 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.7%** |
| Divergence | +1.7pp above crowd (Consensus) |
| 24h volume | $2.7M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.7%**

Inside view: model evaluates to 1.0% (p5=0.1%, p95=0.4%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 0pp below (1.7% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 0.1% · median = 0.2% · p95 = 0.4% · σ = 0.001

```
▁▄▆██▇▅▄▃▂▂▂▁▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.1% | 93 | 0.9% |
| 0.1% | 637 | 6.4% |
| 0.1% | 1322 | 13.2% |
| 0.2% | 1601 | 16.0% |
| 0.2% | 1691 | 16.9% |
| 0.2% | 1451 | 14.5% |
| 0.3% | 1049 | 10.5% |
| 0.3% | 780 | 7.8% |
| 0.3% | 524 | 5.2% |
| 0.4% | 348 | 3.5% |
| 0.4% | 217 | 2.2% |
| 0.4% | 133 | 1.3% |
| 0.5% | 75 | 0.8% |
| 0.5% | 38 | 0.4% |
| 0.5% | 21 | 0.2% |
| 0.6% | 8 | 0.1% |
| 0.6% | 7 | 0.1% |
| 0.6% | 3 | 0.0% |
| 0.7% | 1 | 0.0% |
| 0.7% | 1 | 0.0% |

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Cape Verde (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Cape Verde_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Cape Verde_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Cape Verde_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Cape Verde_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Cape Verde: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Cape Verde (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Cape Verde |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Cape Verde |
| fixture_context_agent | fixture_context | Upcoming fixtures for Cape Verde: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v1 · 2026-07-03 12:29 UTC_
