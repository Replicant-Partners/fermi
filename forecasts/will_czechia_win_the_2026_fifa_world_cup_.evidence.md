# Will Czechia win the 2026 FIFA World Cup?

**Probability:** 1.9% · **Version:** v3 · **Updated:** 2026-06-25 17:57 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.1%** |
| Fermi estimate | **1.9%** |
| Divergence | +1.9pp above crowd (Consensus) |
| 24h volume | $358K |
| Market confidence | High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.9%**

Inside view: model evaluates to 1.9% (p5=1.2%, p95=2.8%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 0pp below (1.9% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 1.2% · median = 1.9% · p95 = 2.8% · σ = 0.005

```
▁▂▃▅▆██▇▆▅▃▂▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.8% | 4 | 0.0% |
| 1.0% | 113 | 1.1% |
| 1.1% | 389 | 3.9% |
| 1.3% | 828 | 8.3% |
| 1.5% | 1182 | 11.8% |
| 1.7% | 1550 | 15.5% |
| 1.8% | 1474 | 14.7% |
| 2.0% | 1369 | 13.7% |
| 2.2% | 1029 | 10.3% |
| 2.4% | 787 | 7.9% |
| 2.6% | 545 | 5.5% |
| 2.7% | 321 | 3.2% |
| 2.9% | 195 | 1.9% |
| 3.1% | 114 | 1.1% |
| 3.3% | 56 | 0.6% |
| 3.4% | 22 | 0.2% |
| 3.6% | 10 | 0.1% |
| 3.8% | 7 | 0.1% |
| 4.0% | 4 | 0.0% |
| 4.1% | 1 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-25 17:56 | 0.1% | 2.1% | 0.1% | -2.0pp | +0.1pp | Initial: 0.1% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-25 17:57 | 1.9% | 2.1% | 0.1% | -0.2pp | +1.9pp | 1.9% (+2pp), 6 drivers |
| v3 | 2026-06-25 17:57 | 1.9% | 2.1% | 0.1% | -0.2pp | +1.9pp | 1.9% (→), 6 drivers |

**Model line:** ```▁██``` (range 0.1% – 1.9%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Czechia (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Czechia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Czechia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Czechia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Czechia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Czechia: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Czechia (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Czechia |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Czechia |
| fixture_context_agent | fixture_context | Upcoming fixtures for Czechia: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v3 · 2026-06-25 17:57 UTC_
