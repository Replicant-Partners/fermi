# Evidence Log: will sweden win the eurovision in 2026?

**Version:** v2 | **Probability:** 1.5% | **Updated:** 2026-03-05 23:20 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2024)
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Sweden has won Eurovision 7 times out of 68 contests (1974, 1984, 1991, 1999, 2012, 2015, 2023), giving a base rate of 10.3%. However, for any single future year, we use 1/participating_countries. With ~37 typical participants, base rate ≈ 2.7%. Adjusting for Sweden's strong track record (7 wins, 2nd most after Ireland's 7), a reasonable base rate is 1.47% (approximately 1.5× the naive equal probability).

---

## recent_performance_momentum `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.9 | 1.3 | 2 | multiplier |

> Sweden won in 2023 (Loreen - 'Tattoo') and has strong recent form with 3 wins in the last 12 years (2012, 2015, 2023). Countries with recent wins often maintain competitive momentum through strong national selection processes (Melodifestivalen is highly professional). However, 2026 is 3 years after their last win, so momentum may moderate.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## melodifestivalen_quality `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 1.1 | 1.4 | 1.8 | multiplier |

> Sweden's Melodifestivalen is widely regarded as the most professional and competitive national selection, consistently producing high-quality entries. It attracts top songwriters and performers, has high production values, and serves as a proving ground. This structural advantage is persistent and significant.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## voting_bloc_dynamics `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.95 | 1.3 | 1.6 | multiplier |

> Sweden benefits from Nordic voting patterns (Norway, Denmark, Finland often exchange points) and general Western European support. However, voting has become more quality-focused with jury/televote split. The advantage exists but is moderate and depends on song quality.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze Sweden's Eurovision performance trajectory 2015-2025: placement rankings, Melodifestivalen s_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

Sweden has been one of the most consistently successful countries in the Eurovision Song Contest over the past decade, with a track record of top-10 placements, high qualification rates for the final, and historical precedent for back-to-back strong performances. This success can be attributed to the country's robust Melodifestivalen selection process, its strong songwriter networks, and its innovative staging and production values. Based on the analysis, Sweden's competitive advantage in Eurovi...

**Key findings:**

- Sweden has consistently placed in the top 10 of the Eurovision Song Contest over the past 5 years, with an average placement of 5.6 (2015-2019).
- Sweden has qualified for the Eurovision final in 9 out of the last 10 years, demonstrating a high frequency of top-10 placements.
- There is historical precedent for Sweden achieving back-to-back strong performances, having won the contest in 2012 and 2015.
- Sweden's competitive advantage in Eurovision can be attributed to its robust Melodifestivalen selection process, which allows the country to field high-quality entries, as well as its strong songwriter networks and innovative staging production values.
- The correlation between recent Eurovision success and subsequent performance is positive for Sweden, suggesting that its strong track record is likely to continue in the near future.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## host_nation_effect `binary`

- **Probability:** 15%
- **Impact multiplier:** 0.6x

> If Sweden hosts Eurovision 2026 (possible if they won a 2025 contest or another scenario), host nations historically have a disadvantage (no host has won since 2017, and it's rare). However, probability Sweden hosts in 2026 is low (~15%) since Switzerland hosted 2025 after Nemo's 2024 win. If they don't host, this driver = 1.0.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## competition_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.7 | 0.95 | 1.1 | multiplier |

> The competitive landscape in 2026 is uncertain. Strong competitors like Italy, Ukraine, France, and UK have been investing heavily. Australia and other countries are improving. A stronger field reduces Sweden's chances slightly, while a weaker field helps. Expected to be neutral to slightly negative.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 7 times out of 68 contests (1974, 1984, 1991, 1999, 2012, 2015, 2023), giving a base rate of 10.3%. However, for any single future year, we use 1/participating_countries. With ~37 typical participants, base rate ≈ 2.7%. Adjusting for Sweden's strong track record (7 wins, 2nd most after Ireland's 7...

- "base_rate": {
- "reference_class": "Eurovision Song Contest winners (1956-2024)",
- "historical_frequency": 0.0147,
- "sample_size": 68,
- "reasoning": "Sweden has won Eurovision 7 times out of 68 contests (1974, 1984, 1991, 1999, 2012, 2015, 2023), giving a base rate of 10.3%. However, for any single future year, we use 1/participating_countries. With ~37 typical participants, base rate ≈ 2.7%. Adjusting for Sweden's strong track record (7 wins, 2nd most after Ireland's 7), a reasonable base rate is 1.47% (approximately 1.5× the naive equal probability)."
- "drivers": [
- "name": "recent_performance_momentum",
- "display_name": "Recent Performance Momentum",
- "type": "continuous",
- "p5": 0.9,

