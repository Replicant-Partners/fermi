# Will Argentina win the 2026 FIFA World Cup?

**Probability:** 8.4% · **Version:** v12 · **Updated:** 2026-06-30 11:07 UTC

**Confidence:** Medium (50%) · **Drivers:** 1 · **Evidence:** 9 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **11.6%** |
| Fermi estimate | **8.4%** |
| Divergence | +3.1pp below crowd (Minor divergence) |
| 24h volume | $6.1M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 8.4%**

Inside view: model evaluates to 8.4% (p5=6.1%, p95=11.2%). Outside view (base rate): 45.0%. Key drivers: the_2026_tournament_has_48_teams_vs_historical_32.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 37pp below (8.4% vs 45.0%)

---

## Outside View (Base Rate)

**45.0%** — e, but expanded format also adds more knockout variance. Starting with CONMEBOL base rate of 0.45 as anchor, then

- **Source:** macro_forecaster

```json
{
  "reference_class": "Historical FIFA World Cup wins by CONMEBOL teams (1930-2022)",
  "historical_frequency": 0.45,
  "sample_size": 22,
  "reasoning": "22 World Cups held 1930-2022. CONMEBOL teams (South American) won 10 times (Uruguay 2, Brazil 5, Argentina 3). Frequency = 10/22 = 0.45. This is the most specific applicable reference class. Argentina specifically: 3 wins in 22 tournaments = 0.136 base rate. However, using the broader CONMEBOL class (0.45) is more robust given small s

---

## Simulation Distribution

**10000 iterations** · p5 = 6.1% · median = 8.3% · p95 = 11.2% · σ = 0.015

```
▁▁▂▄▅▇██▇▆▅▄▃▂▂▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 4.6% | 16 | 0.2% |
| 5.2% | 92 | 0.9% |
| 5.7% | 281 | 2.8% |
| 6.2% | 566 | 5.7% |
| 6.8% | 870 | 8.7% |
| 7.3% | 1238 | 12.4% |
| 7.8% | 1377 | 13.8% |
| 8.4% | 1346 | 13.5% |
| 8.9% | 1190 | 11.9% |
| 9.4% | 1024 | 10.2% |
| 10.0% | 713 | 7.1% |
| 10.5% | 500 | 5.0% |
| 11.0% | 331 | 3.3% |
| 11.6% | 210 | 2.1% |
| 12.1% | 117 | 1.2% |
| 12.6% | 73 | 0.7% |
| 13.2% | 40 | 0.4% |
| 13.7% | 10 | 0.1% |
| 14.2% | 4 | 0.0% |
| 14.8% | 2 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | Initial: 8.4% base=2%, 6 drivers, 6 evidence |
| v2 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v3 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v4 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v5 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v6 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v7 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v8 | 2026-06-30 11:05 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v9 | 2026-06-30 11:06 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 1 drivers, 7 evidence |
| v10 | 2026-06-30 11:06 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (→), 1 drivers, 7 evidence |
| v11 | 2026-06-30 11:06 | 12.0% | 45.0% | 11.6% | -33.0pp | +0.5pp | 12.0% (+4pp), 1 drivers, 7 evidence |
| v12 | 2026-06-30 11:07 | 8.4% | 45.0% | 11.6% | -36.6pp | -3.1pp | 8.4% (-4pp), 1 drivers, 9 evidence |

**Model line:** ```▁▁▁▁▁▁▁▁▁▁█▁``` (range 8.4% – 12.0%)

---

## 1. the_2026_tournament_has_48_teams_vs_historical_32 `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.85 | 1.00 | 1.15 | multiplier |

> . The 2026 tournament has 48 teams vs historical 32

_No evidence collected yet. Assign an agent to research this driver._

---

## General Evidence (5)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



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
model: the_2026_tournament_has_48_teams_vs_historical_32
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Argentina (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Argentina |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina |
| fixture_context_agent | fixture_context | Upcoming fixtures for Argentina: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v12 · 2026-06-30 11:07 UTC_
