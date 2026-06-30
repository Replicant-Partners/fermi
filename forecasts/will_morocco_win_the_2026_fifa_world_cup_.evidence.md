# Will Morocco win the 2026 FIFA World Cup?

**Probability:** 1.6% · **Version:** v7 · **Updated:** 2026-06-30 10:38 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **2.4%** |
| Fermi estimate | **1.6%** |
| Divergence | +0.8pp below crowd (Consensus) |
| 24h volume | $2.3M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 1.6%**

Inside view: model evaluates to 1.6% (p5=1.0%, p95=2.3%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 0pp below (1.6% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 1.0% · median = 1.6% · p95 = 2.3% · σ = 0.004

```
▁▂▃▅▇██▇▆▄▃▃▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.7% | 21 | 0.2% |
| 0.8% | 172 | 1.7% |
| 1.0% | 473 | 4.7% |
| 1.1% | 928 | 9.3% |
| 1.3% | 1263 | 12.6% |
| 1.4% | 1478 | 14.8% |
| 1.6% | 1451 | 14.5% |
| 1.7% | 1259 | 12.6% |
| 1.9% | 1011 | 10.1% |
| 2.0% | 726 | 7.3% |
| 2.2% | 464 | 4.6% |
| 2.3% | 322 | 3.2% |
| 2.4% | 196 | 2.0% |
| 2.6% | 109 | 1.1% |
| 2.7% | 70 | 0.7% |
| 2.9% | 20 | 0.2% |
| 3.0% | 12 | 0.1% |
| 3.2% | 14 | 0.1% |
| 3.3% | 6 | 0.1% |
| 3.5% | 5 | 0.1% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | Initial: 1.6% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |
| v3 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |
| v4 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |
| v5 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |
| v6 | 2026-06-30 10:37 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |
| v7 | 2026-06-30 10:38 | 1.6% | 2.1% | 2.4% | -0.5pp | -0.8pp | 1.6% (→), 6 drivers |

**Model line:** ```▁▁▁▁▁▁▁``` (range 1.6% – 1.6%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Morocco (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Morocco_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Morocco: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Morocco (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Morocco |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco |
| fixture_context_agent | fixture_context | Upcoming fixtures for Morocco: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v7 · 2026-06-30 10:38 UTC_
