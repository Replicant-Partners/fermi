# Evidence Log: will sweden win the eurovision in 2026?

**Version:** v31 | **Probability:** 1.8% | **Updated:** 2026-03-06 23:26 UTC

---

## Inside View

**Probability:** 1.80%

Starting from a 1.5% base rate, our model moderately increases the probability to 1.8%. The key factors are: recent_performance_momentum, melodifestivalen_quality, voting_system_favorability. Most influential: melodifestivalen_quality (32%), competition_strength (29%), recent_performance_momentum (22%).

**Confidence:** Low (28%)

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

### Assigned Agents

- **market_research_recent_performance_momentum** (schedule: once)
  - Query: _Investigate the technical credibility and progress of leading private fusion companies (Commonwealth_

### Evidence

#### Agent: market_research (Claude API) (relevance: 75%)

The leading private fusion companies have made progress on their respective technical approaches, but they have not yet achieved the key milestones necessary for commercial deployment by 2031. While the companies have attracted significant funding from sophisticated investors, their ambitious timelines appear to be driven more by marketing than by realistic assessments of the remaining technical hurdles.

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

## melodifestivalen_quality `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 1.7 | 2.7 | 4 | multiplier |

> Sweden's Melodifestivalen is widely regarded as the highest-quality national selection process, with professional production, strong songwriting, and rigorous competition. This systematic advantage has contributed to Sweden's overperformance. The 2026 Melodifestivalen will occur in early 2026, and quality can vary year-to-year, but the structural advantage remains consistent.

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
| 0.7 | 1 | 1.5 | multiplier |

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

### Agent: sentiment_analyzer (Claude API) (relevance: 85%)

Sweden has demonstrated a consistent top 10 performance in Eurovision from 2015-2024, with a stable production team and high-quality national selection process. However, their results have not shown a clear upward trajectory or dominance compared to other historically strong competitors, suggesting their momentum may be sustainable but less pronounced than some peers.

### Agent: entity_investigator (Claude API) (relevance: 78%)

Sweden demonstrates exceptional institutional consistency in Eurovision through Melodifestivalen's professionalized selection process, achieving 8 top-7 finishes in 9 contests (2015-2024). However, the 10-year gap since their last win (2015) and historical rarity of multiple wins within 5-year windows (only 2 instances since 1990) suggest that while Sweden remains a perennial contender, victory is not structurally predictable. Comparative analysis shows Sweden's institutional advantages (product...

