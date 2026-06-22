# Will Tunisia win the 2026 FIFA World Cup?

**Probability:** 1.0% · **Version:** v3 · **Updated:** 2026-06-22 12:38 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.0%** |
| Divergence | +0.9pp above crowd (Consensus) |
| 24h volume | $26K |
| Market confidence | Medium |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.0%**

Inside view: model evaluates to 1.0% (p5=0.3%, p95=0.9%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

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

**10000 iterations** · p5 = 0.3% · median = 0.5% · p95 = 0.9% · σ = 0.002

```
▁▂▄▆██▇▆▅▄▃▂▂▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.2% | 13 | 0.1% |
| 0.2% | 173 | 1.7% |
| 0.3% | 613 | 6.1% |
| 0.4% | 1115 | 11.2% |
| 0.4% | 1501 | 15.0% |
| 0.5% | 1606 | 16.1% |
| 0.6% | 1483 | 14.8% |
| 0.6% | 1175 | 11.8% |
| 0.7% | 828 | 8.3% |
| 0.8% | 609 | 6.1% |
| 0.9% | 377 | 3.8% |
| 0.9% | 223 | 2.2% |
| 1.0% | 130 | 1.3% |
| 1.1% | 79 | 0.8% |
| 1.1% | 41 | 0.4% |
| 1.2% | 17 | 0.2% |
| 1.3% | 9 | 0.1% |
| 1.3% | 4 | 0.0% |
| 1.4% | 2 | 0.0% |
| 1.5% | 2 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-22 12:38 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | Initial: 1.0% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-22 12:38 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |
| v3 | 2026-06-22 12:38 | 1.0% | 2.1% | 0.1% | -1.1pp | +0.9pp | 1.0% (→), 6 drivers |

**Model line:** ```▁▁▁``` (range 1.0% – 1.0%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Tunisia (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Tunisia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Tunisia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Tunisia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Tunisia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Tunisia: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Tunisia (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Tunisia |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Tunisia |
| fixture_context_agent | fixture_context | Upcoming fixtures for Tunisia: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v3 · 2026-06-22 12:38 UTC_
