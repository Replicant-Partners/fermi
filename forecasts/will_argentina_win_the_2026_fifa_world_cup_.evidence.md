# Will Argentina win the 2026 FIFA World Cup?

**Probability:** 8.4% · **Version:** v7 · **Updated:** 2026-06-25 02:46 UTC

**Confidence:** Medium (50%) · **Drivers:** 3 · **Evidence:** 5 · **Agents:** 4

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

Inside view: model evaluates to 8.4% (p5=6.1%, p95=11.2%). Outside view (base rate): 13.6%. Key drivers: is_more_appropriate_for_initial_anchoring_because, argentina_is_consistently_among_top_conmebol_teams, conmebol_structural_advantages.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 5pp below (8.4% vs 13.6%)

---

## Outside View (Base Rate)

**13.6%** — 22 tournaments = 13.6% base rate. however, using

- **Source:** macro_forecaster

```json
{
  "reference_class": "Historical FIFA World Cup wins by CONMEBOL nations (1930-2022)",
  "historical_frequency": 0.45,
  "sample_size": 22,
  "reasoning": "22 World Cups held 1930-2022. CONMEBOL nations (South American) won 10 times (Uruguay 2, Brazil 5, Argentina 3). Argentina specifically: 3 wins in 22 tournaments = 13.6% base rate. However, using the broader CONMEBOL reference class (45%) is more appropriate for initial anchoring because: (1) Argentina is consistently among top CONM

---

## Simulation Distribution

**10000 iterations** · p5 = 6.1% · median = 8.3% · p95 = 11.2% · σ = 0.015

```
▁▁▂▄▅▇██▇▆▄▄▃▂▂▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 4.6% | 19 | 0.2% |
| 5.1% | 84 | 0.8% |
| 5.7% | 302 | 3.0% |
| 6.2% | 575 | 5.8% |
| 6.8% | 890 | 8.9% |
| 7.3% | 1231 | 12.3% |
| 7.9% | 1423 | 14.2% |
| 8.4% | 1379 | 13.8% |
| 9.0% | 1266 | 12.7% |
| 9.5% | 950 | 9.5% |
| 10.1% | 658 | 6.6% |
| 10.6% | 521 | 5.2% |
| 11.1% | 319 | 3.2% |
| 11.7% | 182 | 1.8% |
| 12.2% | 116 | 1.2% |
| 12.8% | 49 | 0.5% |
| 13.3% | 21 | 0.2% |
| 13.9% | 9 | 0.1% |
| 14.4% | 5 | 0.1% |
| 15.0% | 1 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | Initial: 8.5% base=2%, 6 drivers, 4 evidence |
| v2 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | 8.5% (→), 6 drivers, 4 evidence |
| v3 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | 8.5% (→), 6 drivers, 4 evidence |
| v4 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | 8.5% (→), 6 drivers, 4 evidence |
| v5 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | 8.5% (→), 6 drivers, 4 evidence |
| v6 | 2026-06-25 02:45 | 8.5% | 13.6% | 11.6% | -5.1pp | -3.1pp | 8.5% (→), 6 drivers, 4 evidence |
| v7 | 2026-06-25 02:46 | 8.4% | 13.6% | 11.6% | -5.2pp | -3.1pp | 8.4% (→), 3 drivers, 5 evidence |

**Model line:** ```██████▁``` (range 8.4% – 8.5%)

---

## 1. is_more_appropriate_for_initial_anchoring_because `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 1.70 | 2.00 | 2.30 | multiplier |

> is more appropriate for initial anchoring because:

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. argentina_is_consistently_among_top_conmebol_teams `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 1.70 | 2.00 | 2.30 | multiplier |

> Argentina is consistently among top CONMEBOL teams

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. conmebol_structural_advantages `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.85 | 1.00 | 1.15 | multiplier |

> CONMEBOL structural advantages

_No evidence collected yet. Assign an agent to research this driver._

---

## General Evidence (1)

_Evidence not linked to a specific driver._

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
model: is_more_appropriate_for_initial_anchoring_because * argentina_is_consistently_among_top_conmebol_teams * conmebol_structural_advantages
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Argentina (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Argentina |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina |
| fixture_context_agent | fixture_context | Upcoming fixtures for Argentina: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v7 · 2026-06-25 02:46 UTC_
