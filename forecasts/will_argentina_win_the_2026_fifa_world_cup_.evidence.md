# Will Argentina win the 2026 FIFA World Cup?

**Probability:** 8.4% · **Version:** v13 · **Updated:** 2026-06-30 11:07 UTC

**Confidence:** Medium (50%) · **Drivers:** 3 · **Evidence:** 11 · **Agents:** 4

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

Inside view: model evaluates to 8.4% (p5=6.1%, p95=11.1%). Outside view (base rate): 16.7%. Key drivers: factor_1, factor_2, factor_3.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 8pp below (8.4% vs 16.7%)

---

## Outside View (Base Rate)

**16.7%** — a's specific strength and current form will be handled by drivers. Conservative base rate: 0.167 (Argentina-spe

- **Source:** macro_forecaster

```json
{
  "reference_class": "FIFA World Cup wins by CONMEBOL teams (1930-2022)",
  "historical_frequency": 0.458,
  "sample_size": 24,
  "reasoning": "22 World Cups held (excluding 1942/1946). CONMEBOL teams won 11 times (Uruguay 2, Brazil 5, Argentina 3). 11/24 = 0.458. Argentina-specific: 3 wins in 18 tournaments entered = 0.167. However, Argentina is currently ranked #1 FIFA, reigning champion, has Messi successor pipeline, and strong youth development. Using broader CONMEBOL rate (0.458) 

---

## Simulation Distribution

**10000 iterations** · p5 = 6.1% · median = 8.3% · p95 = 11.1% · σ = 0.015

```
▁▂▂▄▆▇██▇▆▅▃▃▂▂▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 4.7% | 22 | 0.2% |
| 5.2% | 101 | 1.0% |
| 5.7% | 281 | 2.8% |
| 6.3% | 563 | 5.6% |
| 6.8% | 936 | 9.4% |
| 7.3% | 1223 | 12.2% |
| 7.9% | 1368 | 13.7% |
| 8.4% | 1381 | 13.8% |
| 8.9% | 1237 | 12.4% |
| 9.5% | 1005 | 10.1% |
| 10.0% | 710 | 7.1% |
| 10.5% | 478 | 4.8% |
| 11.1% | 307 | 3.1% |
| 11.6% | 203 | 2.0% |
| 12.1% | 101 | 1.0% |
| 12.7% | 45 | 0.4% |
| 13.2% | 25 | 0.2% |
| 13.8% | 9 | 0.1% |
| 14.3% | 2 | 0.0% |
| 14.8% | 3 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | Initial: 8.4% base=2%, 6 drivers, 6 evidence |
| v2 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v3 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v4 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v5 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v6 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v7 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v8 | 2026-06-30 11:05 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 6 drivers, 6 evidence |
| v9 | 2026-06-30 11:06 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 1 drivers, 7 evidence |
| v10 | 2026-06-30 11:06 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 1 drivers, 7 evidence |
| v11 | 2026-06-30 11:06 | 12.0% | 16.7% | 11.6% | -4.7pp | +0.5pp | 12.0% (+4pp), 1 drivers, 7 evidence |
| v12 | 2026-06-30 11:07 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (-4pp), 1 drivers, 9 evidence |
| v13 | 2026-06-30 11:07 | 8.4% | 16.7% | 11.6% | -8.3pp | -3.1pp | 8.4% (→), 3 drivers, 11 evidence |

**Model line:** ```▁▁▁▁▁▁▁▁▁▁█▁▁``` (range 8.4% – 12.0%)

---

## 1. factor_1 `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 | multiplier |

> Main driver of the outcome

_No evidence collected yet. Assign an agent to research this driver._

---

## 2. factor_2 `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 | multiplier |

> Supporting factor

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. factor_3 `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 | multiplier |

> Key risk or uncertainty

_No evidence collected yet. Assign an agent to research this driver._

---

## General Evidence (7)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



### Agent: fermi (Claude API) — relevance 50% · quality ●●● High (70%)

```json
{
  "reference_class": "FIFA World Cup wins by CONMEBOL teams (1930-2022)",
  "historical_frequency": 0.458,
  "sample_size": 24,
  "reasoning": "22 World Cups held (excluding 1942/1946). CONMEBOL teams won 11 times (Uruguay 2, Brazil 5, Argentina 3). 11/24 = 0.458. Argentina-specific: 3 wins in 18 tournaments entered = 0.167. However, Argentina is currently ranked #1 FIFA, reigning champion, has Messi successor pipeline, and strong youth development. Using broader CONMEBOL rate (0.458) as base rate is appropriate because Argentina is consistently a top-2 CONMEBOL team and the question asks about a specific strong CONMEBOL nation, not a random one. Adjusting from 0.458 for Argentina's specific strength and current form will be handled by drivers. Conservative base rate: 0.167 (Argentina-specific historical rate) is more defensible than 0.458, but 0.458/5 CONMEBOL contenders ≈ 0.09 per strong team. Settling on Argentina's empirical rate: 0.167."
}
```

Wait, let me recalculate with proper reference class selection:

```json
{
  "reference_class": "Argentina's FIFA World Cup wins (1930-2022)",
  "historical_frequency": 0.167,
  "sample_size": 18,
  "reasoning": "Argentina has participated in 18 World Cups (all except 1938, 1950, 1954, 1970). Won 3 times (1978, 1986, 2022). 3/18 = 0.167. This is the most specific applicable reference class. Alternative consideration: reigning champions defending title have won 2/21 times (Italy 1938, Brazil 1962) = 0.095. But Argentina-specific historical rate (0.167) is more robust with larger sample size and accounts for Argentina's consistent elite status rather than the 'defending champion curse' which may be spurious."
}
```

**Key findings:**

- "reference_class": "FIFA World Cup wins by CONMEBOL teams (1930-2022)",
- "historical_frequency": 0.458,
- "sample_size": 24,
- "reasoning": "22 World Cups held (excluding 1942/1946). CONMEBOL teams won 11 times (Uruguay 2, Brazil 5, Argentina 3). 11/24 = 0.458. Argentina-specific: 3 wins in 18 tournaments entered = 0.167. However, Argentina is currently ranked #1 FIFA, reigning champion, has Messi successor pipeline, and strong youth development. Using broader CONMEBOL rate (0.458) as base rate is appropriate because Argentina is consistently a top-2 CONMEBOL team and the question asks about a specific strong CONMEBOL nation, not a random one. Adjusting from 0.458 for Argentina's specific strength and current form will be handled by drivers. Conservative base rate: 0.167 (Argentina-specific historical rate) is more defensible than 0.458, but 0.458/5 CONMEBOL contenders ≈ 0.09 per strong team. Settling on Argentina's empirical rate: 0.167."
- Wait, let me recalculate with proper reference class selection:
- "reference_class": "Argentina's FIFA World Cup wins (1930-2022)",
- "historical_frequency": 0.167,
- "sample_size": 18,
- "reasoning": "Argentina has participated in 18 World Cups (all except 1938, 1950, 1954, 1970). Won 3 times (1978, 1986, 2022). 3/18 = 0.167. This is the most specific applicable reference class. Alternative consideration: reigning champions defending title have won 2/21 times (Italy 1938, Brazil 1962) = 0.095. But Argentina-specific historical rate (0.167) is more robust with larger sample size and accounts for Argentina's consistent elite status rather than the 'defending champion curse' which may be spurious."

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: factor_1 * factor_2 * factor_3
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Argentina (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Argentina |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Argentina |
| fixture_context_agent | fixture_context | Upcoming fixtures for Argentina: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v13 · 2026-06-30 11:07 UTC_
