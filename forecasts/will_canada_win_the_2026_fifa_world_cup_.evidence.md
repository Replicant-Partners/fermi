# Will Canada win the 2026 FIFA World Cup?

**Probability:** 3.1% · **Version:** v5 · **Updated:** 2026-06-30 10:37 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.2%** |
| Fermi estimate | **3.1%** |
| Divergence | +2.8pp above crowd (Minor divergence) |
| 24h volume | $1.9M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 3.1%**

Inside view: model evaluates to 2.2% (p5=1.4%, p95=3.1%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 1pp above (3.1% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 1.4% · median = 2.1% · p95 = 3.1% · σ = 0.005

```
▁▂▄▆██▇▆▅▄▃▂▁▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 1.0% | 29 | 0.3% |
| 1.2% | 213 | 2.1% |
| 1.4% | 619 | 6.2% |
| 1.6% | 1105 | 11.1% |
| 1.8% | 1518 | 15.2% |
| 2.0% | 1620 | 16.2% |
| 2.2% | 1495 | 14.9% |
| 2.5% | 1216 | 12.2% |
| 2.7% | 858 | 8.6% |
| 2.9% | 595 | 5.9% |
| 3.1% | 369 | 3.7% |
| 3.3% | 185 | 1.8% |
| 3.5% | 84 | 0.8% |
| 3.7% | 58 | 0.6% |
| 3.9% | 23 | 0.2% |
| 4.2% | 10 | 0.1% |
| 4.4% | 1 | 0.0% |
| 4.6% | 1 | 0.0% |
| 4.8% | 0 | 0.0% |
| 5.0% | 1 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 10:37 | 3.1% | 2.1% | 0.2% | +1.0pp | +2.8pp | Initial: 3.1% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-30 10:37 | 3.1% | 2.1% | 0.2% | +1.0pp | +2.8pp | 3.1% (→), 6 drivers |
| v3 | 2026-06-30 10:37 | 2.2% | 2.1% | 0.2% | +0.1pp | +1.9pp | 2.2% (-1pp), 6 drivers |
| v4 | 2026-06-30 10:37 | 2.2% | 2.1% | 0.2% | +0.1pp | +1.9pp | 2.2% (→), 6 drivers |
| v5 | 2026-06-30 10:37 | 3.1% | 2.1% | 0.2% | +1.0pp | +2.8pp | 3.1% (+1pp), 6 drivers |

**Model line:** ```██▁▁█``` (range 2.2% – 3.1%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Canada (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Canada_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Canada_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Canada_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Canada_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Canada: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Canada (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Canada |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Canada |
| fixture_context_agent | fixture_context | Upcoming fixtures for Canada: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v5 · 2026-06-30 10:37 UTC_
