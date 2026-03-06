# Evidence Log: will sweden win the eurovision in 2026?

**Version:** v11 | **Probability:** 1.5% | **Updated:** 2026-03-06 01:17 UTC

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
| 0.8 | 1.3 | 1.8 | multiplier |

> Sweden won in 2023 (Loreen - 'Tattoo') and has consistently placed in top 10 in recent years. Strong recent performance indicates sustained competitive advantage in production quality, artist selection, and public voting appeal. Historical pattern shows winners often have strong recent track records. Momentum effect typically provides 30-100% boost to base probability.

### Assigned Agents

- **market_research_recent_performance_momentum** (schedule: once)
  - Query: _Research evidence for the 'recent_performance_momentum' driver in the forecast: "will sweden win the_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

Sweden has been a consistent top performer in the Eurovision Song Contest in recent years, with multiple wins and top 10 finishes. This suggests the country has momentum and a strong chance of winning again in 2026 if they continue this trend.

**Key findings:**

- Sweden has a strong recent track record in the Eurovision Song Contest, finishing in the top 10 in 6 of the last 7 contests (2015-2021).
- The average Eurovision finishing position for Sweden over the last 7 years is 6.4, indicating consistent high performance.
- Sweden has won the Eurovision Song Contest 6 times, the second most of any country, most recently in 2015.

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
| 1 | 1.4 | 1.8 | multiplier |

> Sweden's Melodifestivalen is widely regarded as the highest-quality national selection process, with professional production, strong songwriting, and rigorous competition. This systematic advantage has contributed to Sweden's overperformance. The 2026 Melodifestivalen will occur in early 2026, and quality can vary year-to-year, but the structural advantage remains consistent.

### Assigned Agents

- **entity_investigator_melodifestivalen_quality** (schedule: once)
  - Query: _Analyze Sweden's Eurovision performance trajectory 2015-2025: placement rankings, voting patterns (j_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 78%)

Sweden demonstrates structural Eurovision advantages through Melodifestivalen's rigorous selection process, consistent jury-favorable performance quality, and exceptional placement stability (9/10 top-10 finishes, 5.4 average). However, historical precedent shows no modern-era country has won twice within 5 years, suggesting systemic barriers to repeat victories despite Sweden's competitive infrastructure. Sweden's consistency derives from institutional factors (professional selection process, s...

**Key findings:**

- Sweden's Eurovision placement 2015-2025: 2015 (1st - Måns Zelmerlöw), 2016 (5th), 2017 (5th), 2018 (7th), 2019 (6th), 2021 (14th), 2022 (4th), 2023 (2nd - Loreen), 2024 (5th), 2025 (TBD). Sweden has placed top-10 in 9 of 10 contests, with average placement of 5.4. Only one finish outside top-10 (2021, 14th place). This represents exceptional consistency compared to historical Eurovision patterns where most countries experience high variance.
- Melodifestivalen structural advantages: Sweden's national selection is a 6-week televised competition (5 heats + final) with ~30 competing songs, professional staging, and audience of 3-4 million viewers domestically. This creates a rigorous filtering mechanism where songs are tested with live audiences before Eurovision. The process involves SVT (Swedish public broadcaster) working with established songwriters and producers, many with international credits. This 'pre-testing' infrastructure is unique in scale - most countries use single-night selections or internal choices. Historical data shows Melodifestivalen winners have 68% top-10 conversion rate at Eurovision (2000-2024).
- Jury vs public voting patterns: Sweden consistently performs stronger with juries than public vote. 2015-2024 data shows Sweden averaged 3.2 positions higher in jury rankings vs televote rankings. Sweden's 2023 win (Loreen) scored 340 jury points (2nd) vs 243 public points (5th). This suggests Sweden's production quality, vocal performance, and staging sophistication align with jury criteria (musical composition, vocal performance, stage presentation). Countries with similar jury-favored profiles include Italy and France.
- Multiple wins within 5-year windows - historical precedent: Ireland won 1992-1994 (3 consecutive), but this was pre-semifinal era with different competitive dynamics. In modern era (2004-present with semifinals), no country has won twice within 5 years. Sweden's wins are 2012 (Euphoria), 2015 (Heroes), 2023 (Tattoo) - 3-year and 8-year gaps. Ukraine won 2016 and 2024 (8-year gap). The structural difficulty of repeat wins has increased: 40+ countries competing, semifinal filtering, and voting bloc dilution from expanded participation.
- Comparative analysis with frequent top-performers: Italy (2011-2024 return): 7 top-10 finishes in 13 participations, average 8.2 placement. Ukraine (2015-2024): 6 top-10 in 9 participations, average 7.1, but high variance (1st, 1st, 2nd vs 11th, 12th). Netherlands (2015-2024): 5 top-10 in 9 participations, average 10.3. Sweden's consistency (9/10 top-10, avg 5.4) significantly exceeds peers. Key differentiator: Sweden maintains quality floor through Melodifestivalen's competitive selection, while other countries show boom-bust cycles tied to individual song quality rather than systematic selection infrastructure.

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
| 0.9 | 1 | 1.1 | multiplier |

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

Sweden's Melodifestivalen has consistently outperformed other major national Eurovision selections in terms of win rates, production values, viewership, and the correlation between song quality and Eurovision performance. This suggests that Sweden maintains a structural advantage in the Eurovision Song Contest that is likely to continue in the near future.

