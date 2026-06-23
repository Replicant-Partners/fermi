# Will Jordan win the 2026 FIFA World Cup?

**Probability:** 1.0% · **Version:** v3 · **Updated:** 2026-06-23 14:09 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.0%** |
| Divergence | +0.9pp above crowd (Consensus) |
| 24h volume | $179K |
| Market confidence | High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.0%**

Inside view: model evaluates to 1.0% (p5=0.1%, p95=0.5%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 1pp below (1.0% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 0.1% · median = 0.3% · p95 = 0.5% · σ = 0.001

```
▂▄▇██▇▆▄▃▂▂▂▁▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.1% | 162 | 1.6% |
| 0.1% | 679 | 6.8% |
| 0.2% | 1280 | 12.8% |
| 0.2% | 1629 | 16.3% |
| 0.3% | 1567 | 15.7% |
| 0.3% | 1445 | 14.4% |
| 0.4% | 1062 | 10.6% |
| 0.4% | 757 | 7.6% |
| 0.5% | 561 | 5.6% |
| 0.5% | 344 | 3.4% |
| 0.6% | 247 | 2.5% |
| 0.6% | 129 | 1.3% |
| 0.6% | 63 | 0.6% |
| 0.7% | 33 | 0.3% |
| 0.7% | 15 | 0.1% |
| 0.8% | 13 | 0.1% |
| 0.8% | 6 | 0.1% |
| 0.9% | 6 | 0.1% |
| 0.9% | 0 | 0.0% |
| 1.0% | 2 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-23 14:09 | 2.0% | 2.1% | 0.1% | -0.1pp | +1.9pp | Initial: 2.0% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-23 14:09 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (-1pp), 6 drivers |
| v3 | 2026-06-23 14:09 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |

**Model line:** ```█▁▁``` (range 1.0% – 2.0%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Jordan (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Jordan_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Jordan_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Jordan_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Jordan_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Jordan: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Jordan (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Jordan |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Jordan |
| fixture_context_agent | fixture_context | Upcoming fixtures for Jordan: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v3 · 2026-06-23 14:09 UTC_
