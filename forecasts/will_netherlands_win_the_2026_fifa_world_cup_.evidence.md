# Will Netherlands win the 2026 FIFA World Cup?

**Probability:** 6.2% · **Version:** v2 · **Updated:** 2026-06-19 01:21 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **4.5%** |
| Fermi estimate | **6.2%** |
| Divergence | +1.7pp above crowd (Consensus) |
| 24h volume | $1.5M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 6.2%**

Inside view: model evaluates to 6.2% (p5=4.4%, p95=8.3%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 4pp above (6.2% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 4.4% · median = 6.1% · p95 = 8.3% · σ = 0.012

```
▁▁▂▄▆▇██▇▅▄▃▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 3.2% | 14 | 0.1% |
| 3.6% | 88 | 0.9% |
| 4.0% | 278 | 2.8% |
| 4.5% | 560 | 5.6% |
| 4.9% | 948 | 9.5% |
| 5.3% | 1298 | 13.0% |
| 5.7% | 1440 | 14.4% |
| 6.2% | 1454 | 14.5% |
| 6.6% | 1165 | 11.7% |
| 7.0% | 914 | 9.1% |
| 7.5% | 719 | 7.2% |
| 7.9% | 470 | 4.7% |
| 8.3% | 298 | 3.0% |
| 8.8% | 185 | 1.8% |
| 9.2% | 89 | 0.9% |
| 9.6% | 47 | 0.5% |
| 10.1% | 18 | 0.2% |
| 10.5% | 8 | 0.1% |
| 10.9% | 6 | 0.1% |
| 11.3% | 1 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-19 01:21 | 6.2% | 2.1% | 4.5% | +4.1pp | +1.7pp | Initial: 6.2% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-19 01:21 | 6.2% | 2.1% | 4.5% | +4.1pp | +1.7pp | 6.2% (→), 6 drivers |

**Model line:** ```▁▁``` (range 6.2% – 6.2%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.00 | 0.00 | 0.00 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Netherlands (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Netherlands_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Netherlands_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Netherlands_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Netherlands_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Netherlands: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Netherlands (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Netherlands |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Netherlands |
| fixture_context_agent | fixture_context | Upcoming fixtures for Netherlands: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v2 · 2026-06-19 01:21 UTC_
