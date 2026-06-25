# Will Haiti win the 2026 FIFA World Cup?

**Probability:** 1.0% · **Version:** v3 · **Updated:** 2026-06-25 06:37 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.0%** |
| Divergence | +0.9pp above crowd (Consensus) |
| 24h volume | $42K |
| Market confidence | Medium |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.0%**

Inside view: model evaluates to 1.0% (p5=0.1%, p95=0.3%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

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

**10000 iterations** · p5 = 0.1% · median = 0.2% · p95 = 0.3% · σ = 0.001

```
▂▄▆▇█▇▆▄▃▃▂▂▁▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.0% | 154 | 1.5% |
| 0.1% | 652 | 6.5% |
| 0.1% | 1156 | 11.6% |
| 0.1% | 1500 | 15.0% |
| 0.2% | 1617 | 16.2% |
| 0.2% | 1431 | 14.3% |
| 0.2% | 1101 | 11.0% |
| 0.2% | 780 | 7.8% |
| 0.3% | 574 | 5.7% |
| 0.3% | 374 | 3.7% |
| 0.3% | 278 | 2.8% |
| 0.4% | 152 | 1.5% |
| 0.4% | 102 | 1.0% |
| 0.4% | 67 | 0.7% |
| 0.4% | 27 | 0.3% |
| 0.5% | 17 | 0.2% |
| 0.5% | 11 | 0.1% |
| 0.5% | 2 | 0.0% |
| 0.5% | 3 | 0.0% |
| 0.6% | 2 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-25 06:37 | 0.1% | 2.1% | 0.1% | -2.0pp | +0.1pp | Initial: 0.1% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-25 06:37 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (+1pp), 6 drivers |
| v3 | 2026-06-25 06:37 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |

**Model line:** ```▁██``` (range 0.1% – 1.0%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Haiti (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Haiti_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Haiti_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Haiti_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Haiti_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Haiti: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Haiti (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Haiti |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Haiti |
| fixture_context_agent | fixture_context | Upcoming fixtures for Haiti: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v3 · 2026-06-25 06:37 UTC_
