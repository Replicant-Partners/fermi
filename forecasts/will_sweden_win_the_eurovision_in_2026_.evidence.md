# Evidence Log: will sweden win the eurovision in 2026?

**Version:** v15 | **Probability:** 1.7% | **Updated:** 2026-03-06 02:11 UTC

---

## Inside View

**Probability:** 1.67%

Starting from a 1.5% base rate, our model moderately increases the probability to 1.7%. The key factors are: recent_performance_momentum, melodifestivalen_quality, voting_system_favorability. Most influential: recent_performance_momentum (40%), geopolitical_sentiment (24%), melodifestivalen_quality (20%).

**Confidence:** Medium (42%)

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2024)
- **Historical frequency:** 1.47%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Sweden has won Eurovision 7 times out of 68 contests (1974, 1984, 1991, 1999, 2012, 2015, 2023), giving a base rate of 10.3%. However, for any single future year, we should consider the broader reference class of 'any country winning in a given year' which is 1/~40 participating countries = 2.5%. Sweden's historical success rate of 10.3% represents a 4.1x multiplier over random chance. Using a conservative base rate of 1.5% accounts for Sweden's strong track record while avoiding overconfidence.

---

## recent_performance_momentum `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.8 | 1.3 | 1.7 | multiplier |

> Sweden won in 2023 (Loreen - 'Tattoo') and has consistently placed in top 10 in recent years. Strong recent performance indicates sustained competitive advantage in production quality, artist selection, and public voting appeal. Historical pattern shows winners often have strong recent track records. Momentum effect typically provides 30-100% boost to base probability.

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
| 1 | 1.7 | 2.5 | multiplier |

> Sweden's Melodifestivalen is widely regarded as the highest-quality national selection process, with professional production, strong songwriting, and rigorous competition. This systematic advantage has contributed to Sweden's overperformance. The 2026 Melodifestivalen will occur in early 2026, and quality can vary year-to-year, but the structural advantage remains consistent.

### Assigned Agents

- **entity_investigator_melodifestivalen_quality** (schedule: once)
  - Query: _look up the curretn elo rating for Barca and compare it to other competitors -account for elo trend_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 72%)

Sweden's Eurovision trajectory 2015-2024 reveals a nation with strong institutional advantages (Melodifestivalen selection process, production expertise, historical success) experiencing recent volatility. The 2023 victory followed by 2024 non-qualification exemplifies the 'winner's curse' phenomenon observed across Eurovision history. While Sweden maintains structural advantages over most competitors, their momentum sustainability appears challenged by: (1) post-win selection pressure, (2) evol...

_Collected: 2026-03-06_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## voting_system_favorability `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.85 | 1 | 1.15 | multiplier |

> Eurovision voting combines jury votes (50%) and public televoting (50%). Sweden historically performs well with both juries (professional appeal) and public (broad accessibility). Any changes to voting rules by 2026 could affect this, but current system is neutral to slightly favorable for Sweden's style. No major changes announced yet.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.0147,
    "sample_size": 68,
    "reasoning": "Sweden has won Eurovision 

---

## geopolitical_sentiment `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.9 | 1 | 1.5 | multiplier |

> Eurovision voting can be influenced by geopolitical factors, regional alliances, and current events. Sweden generally maintains neutral to positive international perception. By 2026, unforeseen events could shift sentiment, but Sweden's stable position suggests minimal impact. Small uncertainty range reflects potential for minor shifts.

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
| 0.7 | 0.95 | 1.2 | multiplier |

> The strength of competing entries varies significantly year-to-year. Strong years (multiple standout songs) reduce any single country's chances. 2026 competition quality is unknown, but historically, high-competition years reduce Sweden's probability by 5-30%, while weak years could increase it by up to 20%. Median slightly below 1.0 reflects that competition is typically strong.

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
    "reasoning": "Sweden has won Eurovision 7 times out of 68 contests (1974, 1984, 1991, 1999, 2012, 2015, 2023), giving a base rate of 10.3%. However, for any single future year, we should consider the broader reference class of 'any country winning in a given year' which is 1/~40 participating countries = 2.5%. Sweden's historical success ...

### Agent: market_research (Claude API) (relevance: 85%)

Sweden has been one of the most successful and consistent performers in the Eurovision Song Contest over the past decade, winning the competition 6 times since 2000 and frequently placing in the top 5. This recent performance momentum suggests Sweden would be a strong contender to win the Eurovision in 2026 as well.

### Agent: sentiment_analyzer (Claude API) (relevance: 85%)

Sweden has demonstrated a consistent top 10 performance in Eurovision from 2015-2024, with a stable production team and high-quality national selection process. However, their results have not shown a clear upward trajectory or dominance compared to other historically strong competitors, suggesting their momentum may be sustainable but less pronounced than some peers.

