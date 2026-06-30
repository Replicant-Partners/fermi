# Will Croatia win the 2026 FIFA World Cup?

**Probability:** 5.6% · **Version:** v5 · **Updated:** 2026-06-30 13:22 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 0 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **0.8%** |
| Fermi estimate | **5.6%** |
| Divergence | +4.8pp above crowd (Minor divergence) |
| 24h volume | $3.2M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 5.6%**

Inside view: model evaluates to 3.9% (p5=2.7%, p95=5.4%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 3pp above (5.6% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 2.7% · median = 3.8% · p95 = 5.4% · σ = 0.008

```
▁▁▂▃▅▇███▆▅▄▃▂▂▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 1.8% | 7 | 0.1% |
| 2.1% | 51 | 0.5% |
| 2.4% | 174 | 1.7% |
| 2.7% | 454 | 4.5% |
| 2.9% | 824 | 8.2% |
| 3.2% | 1197 | 12.0% |
| 3.5% | 1416 | 14.2% |
| 3.8% | 1372 | 13.7% |
| 4.1% | 1317 | 13.2% |
| 4.4% | 996 | 10.0% |
| 4.6% | 776 | 7.8% |
| 4.9% | 529 | 5.3% |
| 5.2% | 361 | 3.6% |
| 5.5% | 237 | 2.4% |
| 5.8% | 147 | 1.5% |
| 6.1% | 74 | 0.7% |
| 6.4% | 29 | 0.3% |
| 6.6% | 18 | 0.2% |
| 6.9% | 15 | 0.1% |
| 7.2% | 6 | 0.1% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 13:22 | 5.6% | 2.1% | 0.8% | +3.5pp | +4.9pp | Initial: 5.6% base=2%, 6 drivers, 0 evidence |
| v2 | 2026-06-30 13:22 | 5.6% | 2.1% | 0.8% | +3.5pp | +4.9pp | 5.6% (→), 6 drivers |
| v3 | 2026-06-30 13:22 | 5.6% | 2.1% | 0.8% | +3.5pp | +4.9pp | 5.6% (→), 6 drivers |
| v4 | 2026-06-30 13:22 | 5.6% | 2.1% | 0.8% | +3.5pp | +4.9pp | 5.6% (→), 6 drivers |
| v5 | 2026-06-30 13:22 | 5.6% | 2.1% | 0.8% | +3.5pp | +4.8pp | 5.6% (→), 6 drivers |

**Model line:** ```████▁``` (range 5.6% – 5.6%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Croatia (2024–2026 latest available)_

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Croatia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Croatia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Croatia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Croatia_

_No evidence collected yet. Assign an agent to research this driver._

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Croatia: venue, climate, rest days, altitude, opponent travel burden_

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Croatia (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Croatia |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Croatia |
| fixture_context_agent | fixture_context | Upcoming fixtures for Croatia: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v5 · 2026-06-30 13:22 UTC_
