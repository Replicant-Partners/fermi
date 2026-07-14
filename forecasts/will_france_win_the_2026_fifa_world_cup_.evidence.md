# Will France win the 2026 FIFA World Cup?

**Probability:** 12.0% · **Version:** v9 · **Updated:** 2026-07-14 09:15 UTC

**Confidence:** Medium (50%) · **Drivers:** 3 · **Evidence:** 6 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **39.4%** |
| Fermi estimate | **12.0%** |
| Divergence | +27.4pp below crowd (Significant disagreement — verify assumptions) |
| 24h volume | $1.7M |
| Market confidence | Very High |
| 1-week trend | ↑ +6.3pp |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 12.0%**

Inside view: model evaluates to 11.5% (p5=8.4%, p95=15.0%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 0pp below (12.0% vs 12.0%)

---

## Outside View (Base Rate)

**12.0%** — s) vs broader European rate (52%) and France-specific rate (9.5%), a calibrated base rate of 12-15% is appropriat

- **Source:** macro_forecaster

```json
{
  "reference_class": "European teams winning FIFA World Cup (1930-2022)",
  "historical_frequency": 0.52,
  "sample_size": 21,
  "reasoning": "Of 21 completed World Cups (excluding 1942/1946), European teams won 11 times (Germany 4, Italy 4, Spain 1, England 1, France 1). France specifically has won 2 of 21 tournaments (1998, 2018) = 9.5% base rate. However, the more relevant reference class is 'strong European teams' (those reaching semifinals in last 3 cycles), which win ~25% of tour

---

## Simulation Distribution

**10000 iterations** · p5 = 8.4% · median = 11.3% · p95 = 15.0% · σ = 0.020

```
▁▁▂▄▆▇██▇▆▅▃▃▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 6.2% | 15 | 0.1% |
| 7.0% | 69 | 0.7% |
| 7.7% | 209 | 2.1% |
| 8.4% | 539 | 5.4% |
| 9.2% | 924 | 9.2% |
| 9.9% | 1227 | 12.3% |
| 10.6% | 1424 | 14.2% |
| 11.4% | 1404 | 14.0% |
| 12.1% | 1320 | 13.2% |
| 12.9% | 988 | 9.9% |
| 13.6% | 733 | 7.3% |
| 14.3% | 505 | 5.1% |
| 15.1% | 316 | 3.2% |
| 15.8% | 160 | 1.6% |
| 16.6% | 83 | 0.8% |
| 17.3% | 47 | 0.5% |
| 18.0% | 21 | 0.2% |
| 18.8% | 8 | 0.1% |
| 19.5% | 4 | 0.0% |
| 20.3% | 4 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | Initial: 25.9% base=2%, 6 drivers, 4 evidence |
| v2 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v3 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v4 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v5 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v6 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v7 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v8 | 2026-07-14 09:14 | 25.9% | 12.0% | 39.4% | +13.9pp | -13.5pp | 25.9% (→), 6 drivers, 4 evidence |
| v9 | 2026-07-14 09:15 | 12.0% | 12.0% | 39.4% | +0.0pp | -27.4pp | 12.0% (-14pp), 3 drivers, 6 evidence |

**Model line:** ```████████▁``` (range 12.0% – 25.9%)

---

## 1. 9_5_base_rate_however `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 1.70 | 2.00 | 2.30 | multiplier |

> = 9.5% base rate. However

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. vs_broader_european_rate `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 1.70 | 2.00 | 2.30 | multiplier |

> vs broader European rate

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. and_france_specific_rate `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.85 | 1.00 | 1.15 | multiplier |

> and France-specific rate

_No evidence collected yet. Assign an agent to research this driver._

---

## General Evidence (2)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: d_9_5_base_rate_however * vs_broader_european_rate * and_france_specific_rate
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for France (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for France |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for France |
| fixture_context_agent | fixture_context | Upcoming fixtures for France: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v9 · 2026-07-14 09:15 UTC_
