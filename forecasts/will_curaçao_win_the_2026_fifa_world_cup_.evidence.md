# Will Curaçao win the 2026 FIFA World Cup?

**Probability:** 1.0% · **Version:** v4 · **Updated:** 2026-06-25 07:23 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.0%** |
| Divergence | +0.9pp above crowd (Consensus) |
| 24h volume | $46K |
| Market confidence | Medium |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.0%**

Inside view: model evaluates to 1.0% (p5=0.0%, p95=0.2%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

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

**10000 iterations** · p5 = 0.0% · median = 0.1% · p95 = 0.2% · σ = 0.001

```
▁▄▇██▇▆▄▃▂▂▁▁▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.0% | 78 | 0.8% |
| 0.0% | 717 | 7.2% |
| 0.1% | 1404 | 14.0% |
| 0.1% | 1664 | 16.6% |
| 0.1% | 1606 | 16.1% |
| 0.1% | 1405 | 14.1% |
| 0.1% | 1073 | 10.7% |
| 0.2% | 753 | 7.5% |
| 0.2% | 514 | 5.1% |
| 0.2% | 319 | 3.2% |
| 0.2% | 180 | 1.8% |
| 0.3% | 115 | 1.1% |
| 0.3% | 68 | 0.7% |
| 0.3% | 49 | 0.5% |
| 0.3% | 36 | 0.4% |
| 0.3% | 5 | 0.1% |
| 0.4% | 5 | 0.1% |
| 0.4% | 5 | 0.1% |
| 0.4% | 3 | 0.0% |
| 0.4% | 1 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-25 07:22 | 0.1% | 2.1% | 0.1% | -2.0pp | +0.1pp | Initial: 0.1% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-25 07:23 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (+1pp), 6 drivers |
| v3 | 2026-06-25 07:23 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |
| v4 | 2026-06-25 07:23 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |

**Model line:** ```▁███``` (range 0.1% – 1.0%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Curaçao (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Curaçao_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Curaçao_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Curaçao_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Curaçao_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Curaçao: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Curaçao (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Curaçao |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Curaçao |
| fixture_context_agent | fixture_context | Upcoming fixtures for Curaçao: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v4 · 2026-06-25 07:23 UTC_
