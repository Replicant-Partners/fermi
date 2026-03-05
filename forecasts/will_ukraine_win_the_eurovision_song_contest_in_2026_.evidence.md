# Evidence Log: will ukraine win the eurovision song contest in 2026?

**Version:** v10 | **Probability:** 2.8% | **Updated:** 2026-03-05 02:03 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2025)
- **Historical frequency:** 1.5%
- **Sample size:** n=67
- **Source:** macro_forecaster

> Ukraine has won Eurovision 3 times (2004, 2016, 2022) out of ~67 contests. However, the relevant reference class is 'any single country winning in a given year' which is approximately 1/40 = 0.025 given ~40 participating countries in recent years. Using Ukraine's historical win rate of 3/67 ≈ 0.045 as a country-specific base rate, adjusted downward to 0.015 to account for the fact that 2026 is further from recent geopolitical sympathy factors.

---

## song_quality_performance `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.6 | 1 | 2 | multiplier |

> Ukraine has strong musical talent and Eurovision track record. Quality of song, staging, and performance can significantly boost or reduce chances. p50=1.0 assumes average quality; p95=2.5 represents exceptional entry like Kalush Orchestra (2022); p5=0.6 represents below-average entry.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Research evidence for the 'song_quality_performance' driver in the forecast: "will ukraine win the e_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

Based on Ukraine's strong track record of success in the Eurovision Song Contest, the quality of their recent entries, and the continued strength of their music industry, there is a good chance that Ukraine could win the competition again in 2026. However, the unpredictable nature of the contest and the potential for other countries to produce high-quality entries means that the outcome is still uncertain.

**Key findings:**

- Ukraine has a strong history of success in the Eurovision Song Contest, winning the competition 3 times (2004, 2016, 2022).
- The quality of Ukraine's Eurovision entries has been consistently high, with their winning songs in 2004, 2016, and 2022 all receiving critical acclaim and strong public support.
- Ukraine's music industry has continued to produce talented artists and songwriters who are capable of creating high-quality Eurovision entries, as evidenced by their recent victories.
- The 2022 Ukrainian Eurovision winner, Kalush Orchestra, demonstrated the country's ability to generate songs that resonate with both European audiences and the global public.
- Political and public support for Ukraine's Eurovision participation is likely to remain strong in the coming years, which could boost the country's chances of winning again in 2026.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 

---

## geopolitical_sympathy_factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.7 | 1.1 | 1.5 | multiplier |

> By 2026, the Russia-Ukraine war will be 4 years old. Sympathy voting helped Ukraine win in 2022, but this effect diminishes over time. p50=1.1 assumes moderate residual sympathy; p95=1.8 if conflict still active and intense; p5=0.7 if war has ended or sympathy fatigue has set in.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 

---

## voting_bloc_support `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.8 | 1.15 | 1.5 | multiplier |

> Ukraine historically receives strong support from neighboring countries and diaspora communities. This is relatively stable but can vary based on regional politics and song appeal. p50=1.15 represents typical bloc advantage.

### Assigned Agents

- **sentiment_analyzer** (schedule: once)
  - Query: _Research evidence for the 'voting_bloc_support' driver in the forecast: "will ukraine win the eurovi_

### Evidence

#### Agent: sentiment_analyzer (Claude API) (relevance: 75%)

Based on Ukraine's strong track record in the Eurovision Song Contest, the influence of political factors and voting blocs, and the potential boost from being the reigning champion, there is a reasonable probability that Ukraine could win the 2026 Eurovision Song Contest. However, the uncertain political and economic situation in the country introduces some uncertainty into this forecast.

**Key findings:**

- Ukraine has a strong history of success in the Eurovision Song Contest, winning the competition a total of 3 times (2004, 2016, 2022).
- Ukraine's participation in Eurovision is heavily influenced by political factors, with the country often using the contest as a platform to showcase its national identity and solidarity.
- Voting blocs in Eurovision, particularly those formed by former Soviet states, have historically played a significant role in determining the outcome of the competition. Ukraine has often benefited from the support of these voting blocs.
- The 2026 Eurovision Song Contest will be the first time Ukraine has the opportunity to defend its title as the reigning champion, which could generate additional support and enthusiasm from the public and voting juries.
- However, the political and economic situation in Ukraine in 2026 is difficult to predict, and this could impact the country's ability to participate or the public's perception of its entry.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 

---

## competition_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.5 | 0.95 | 1.2 | multiplier |

> The quality of competing entries significantly affects any country's chances. p50=0.95 assumes slightly stronger than average competition (reducing Ukraine's chances); p5=0.5 represents exceptionally strong competition from multiple favorites; p95=1.2 represents weak competition field.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 

---

## ukraine_participates `binary`

- **Probability:** 98%
- **Impact multiplier:** 0.0x

> Ukraine must participate to win. High probability (0.92) they will participate barring extreme circumstances (complete infrastructure collapse, EBU suspension). If they don't participate, win probability = 0.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2025)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Ukraine has won Eurovision 3 times (2004, 2016, 2022) out of ~67 contests. However, the relevant reference class is 'any single country winning in a given year' which is approximately 1/40 = 0.025 given ~40 participating countries in recent years. Using Ukraine's historical win rate of 3/67 ≈ 0.045 as a country-specific bas...

- "base_rate": {
- "reference_class": "Eurovision Song Contest winners (1956-2025)",
- "historical_frequency": 0.015,
- "sample_size": 67,
- "reasoning": "Ukraine has won Eurovision 3 times (2004, 2016, 2022) out of ~67 contests. However, the relevant reference class is 'any single country winning in a given year' which is approximately 1/40 = 0.025 given ~40 participating countries in recent years. Using Ukraine's historical win rate of 3/67 ≈ 0.045 as a country-specific base rate, adjusted downward to 0.015 to account for the fact that 2026 is further from recent geopolitical sympathy factors."
- "drivers": [
- "name": "song_quality_performance",
- "display_name": "Song Quality & Performance Excellence",
- "type": "continuous",
- "p5": 0.6,

