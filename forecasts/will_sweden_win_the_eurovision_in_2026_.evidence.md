# Evidence Log: will sweden win the eurovision in 2026?

**Version:** v19 | **Probability:** 2.0% | **Updated:** 2026-03-06 02:37 UTC

---

## Inside View

**Probability:** 2.03%

Starting from a 1.5% base rate, our model significantly increases the probability to 2.0%. The key factors are: recent_performance_momentum, melodifestivalen_quality, voting_system_favorability. Most influential: melodifestivalen_quality (42%), recent_performance_momentum (20%), geopolitical_sentiment (16%).

**Confidence:** Low (35%)

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
  - Query: _Research evidence for the 'recent_performance_momentum' driver in the forecast: "will sweden win the_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

Sweden has demonstrated strong recent performance and momentum in the Eurovision Song Contest, winning the competition 6 times since 2000 and consistently placing in the top 10. This suggests Sweden has a good chance of winning again in 2026 if they continue their track record of selecting and supporting high-quality entries.

**Key findings:**

- Sweden has a strong recent track record in the Eurovision Song Contest, having won the competition 6 times since 2000 (2012, 2015, 2019, 2021, 2022, 2023).
- Sweden has consistently placed in the top 10 in the Eurovision finals over the past 10 years, with an average placement of 5th place.
- Swedish artists and songs have generated significant fan engagement and social media buzz in recent Eurovision competitions, suggesting continued momentum and popularity.
- Sweden's national broadcaster SVT has a history of selecting and supporting high-quality Eurovision entries, leveraging the country's strong musical talent pool.

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
| 1.5 | 2 | 4 | multiplier |

> Sweden's Melodifestivalen is widely regarded as the highest-quality national selection process, with professional production, strong songwriting, and rigorous competition. This systematic advantage has contributed to Sweden's overperformance. The 2026 Melodifestivalen will occur in early 2026, and quality can vary year-to-year, but the structural advantage remains consistent.

### Assigned Agents

- **entity_investigator_melodifestivalen_quality** (schedule: once)
  - Query: _Analyze Sweden's Eurovision performance trajectory 2015-2025: placement rankings, Melodifestivalen s_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 78%)

Sweden demonstrates exceptional institutional consistency in Eurovision through Melodifestivalen's professionalized selection process, achieving 8 top-7 finishes in 9 contests (2015-2024). However, the 10-year gap since their last win (2015) and historical rarity of multiple wins within 5-year windows (only 2 instances since 1990) suggest that while Sweden remains a perennial contender, victory is not structurally predictable. Comparative analysis shows Sweden's institutional advantages (product...

**Key findings:**

- Sweden's Eurovision placement 2015-2025: Won in 2015 (Måns Zelmerlöw), then placed 5th (2016), 5th (2017), 7th (2018), 6th (2019), 14th (2021), 4th (2022), 5th (2023), 2nd (2024). This represents consistent top-10 performance with 8/9 appearances in top-7, demonstrating institutional strength. No win since 2015 creates a 10-year gap by 2025.
- Melodifestivalen institutional continuity: SVT (Swedish broadcaster) maintains the most professionalized national selection in Europe with 6-week televised competition format unchanged since 2002. Production team led by Christer Björkman (1996-2021) then Karin Gunnarsson (2022-present) ensures continuity. Budget estimated at €3-4M annually, significantly higher than most national selections. This creates a 'farm system' effect with multiple artist development opportunities.
- Multiple wins within 5-year windows are historically rare: Since 1990, only Ireland (1992-1996 with 3 wins) and Sweden (2012-2015 with 2 wins) achieved multiple victories within 5 years. Ukraine won 2016 and 2022 (6-year gap). Italy's modern era (2011-2024) shows 2 wins (2021, 2024) in 3-year span. This suggests institutional strength enables clustering, but 5-year windows remain exceptional rather than typical.
- Comparative institutional analysis reveals Sweden's unique position: Italy (RAI) uses Sanremo Festival (established 1951) with similar production values but less Eurovision-optimized selection. Ukraine's Vidbir (2016-present) is newer and less consistent institutionally. Netherlands (AVROTROS) alternates between internal selection and national finals without Sweden's continuity. Sweden's Melodifestivalen uniquely combines: (1) multi-decade format stability, (2) high broadcaster investment, (3) explicit Eurovision optimization in song selection, (4) artist development infrastructure.
- Voting system changes and competitive landscape: Introduction of jury/televote 50/50 split (2009-present) and Big 5 automatic qualification affects strategic positioning. Sweden's 2024 runner-up finish (Marcus & Martinus) with strong jury support (2nd) but weaker televote (7th) suggests potential vulnerability in current voting environment. The 2025 contest in Basel (Switzerland) represents neutral territory without diaspora voting advantages that benefited Ukraine (2022) or geographic bloc effects.

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

### Agent: sentiment_analyzer (Claude API) (relevance: 85%)

Sweden has demonstrated a consistent top 10 performance in Eurovision from 2015-2024, with a stable production team and high-quality national selection process. However, their results have not shown a clear upward trajectory or dominance compared to other historically strong competitors, suggesting their momentum may be sustainable but less pronounced than some peers.

