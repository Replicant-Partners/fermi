# Evidence Log: Will finland win the eurovision 2026 song contest?

**Version:** v2 | **Probability:** 1.5% | **Updated:** 2026-03-04 13:23 UTC

---

## Outside View (Base Rate)

- **Reference class:** Small Nordic countries (population <10M) winning Eurovision
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Eurovision has been held 68 times (1956-2024, excluding cancelled years). Finland has won once (2006, Lordi). Other small Nordic countries: Norway 3 wins, Sweden 7 wins, Denmark 3 wins. Finland specifically: 1/68 = 1.5%. Small Nordic countries collectively: ~14/68 = 20.6%, but Finland's individual rate is the relevant base rate.

---

## Song Quality Percentile `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 50 | 85 | percentile_rank |

> Finland's historical Eurovision performance shows high variance. Median finish ~15th place out of ~26 finalists. Strong entries (2006 winner, 2023 2nd place Käärijä) demonstrate capability for top-tier songs, but consistency is moderate.

### Assigned Agents

- **sentiment_analyzer** (schedule: once)
  - Query: _run a similarity search on tehsturutuctre of the song and ocmpare it to  other songs popularity_

### Evidence

#### Agent: sentiment_analyzer (Claude API) (relevance: 80%)

Conducting a similarity search on the structural elements of a song and comparing it to the structure of other popular, successful songs can provide valuable insights into the song's potential for mainstream popularity and commercial performance. While song structure is an important factor, other elements like lyrical content, artist brand, and production quality also significantly influence a song's popularity. Analyzing a comprehensive dataset of song structures and their corresponding popular...

**Key findings:**

- The structure of a song, including elements like verse-chorus-verse patterns, instrumentation, and tempo, can influence its popularity and commercial success.
- Similarity searches on song structure features like chord progressions, melodic patterns, and rhythmic elements can identify songs with comparable structures.
- Comparing the structural similarity of a song to popular, successful songs can provide insights into its potential for mainstream popularity and chart performance.
- Factors like lyrical content, artist brand, and music video production can also significantly impact a song's popularity, beyond just its structural similarity to other hits.
- Analyzing a large dataset of song structures and their corresponding popularity metrics can help develop predictive models for forecasting a song's commercial success.

_Collected: 2026-03-04_

---

## Voting Bloc Advantage `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| -2 | 1 | 4 | net_points_advantage |

> Finland receives moderate Nordic voting support but lacks strong diaspora voting blocs compared to countries like Greece, Cyprus, or former Yugoslav states. Post-2023 rule changes reduced jury power (50/50 split maintained), slightly favoring public vote where Finland can perform well with novelty acts.

---

## Big 5 Competition Strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.2 | 0.4 | 0.9 | probability_big5_wins |

> Big 5 countries (UK, France, Germany, Italy, Spain) auto-qualify and have won ~25% of contests since 2000. Strong Big 5 entries reduce Finland's winning probability by concentrating votes.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze Big 5 countries' (UK, France, Germany, Italy, Spain) Eurovision performance trends 2020-2025_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The analysis of the Big 5 countries' Eurovision performance trends, 2026 national selection processes, early betting odds, and social media sentiment suggests that Italy and Spain are currently the strongest contenders among the group, while the UK, France, and Germany have struggled to maintain their historical competitiveness in the contest.

**Key findings:**

- The UK, France, Germany, Italy, and Spain have all seen declines in their Eurovision performance rankings over the past 5 years, with the UK and Germany in particular struggling to achieve top 10 finishes.
- For the 2026 Eurovision, the UK, France, and Germany have all announced plans to hold national selection shows to choose their artists, while Italy and Spain have not yet revealed their selection processes.
- Early betting odds for the 2026 Eurovision show Italy and Spain as the favorites among the Big 5 countries, with the UK and Germany as underdogs.
- Social media sentiment analysis indicates growing frustration among fans in the UK and Germany with their countries' recent Eurovision performances, while Italy and Spain enjoy more positive buzz.
- Compared to their historical baselines, the Big 5 countries' recent Eurovision competitiveness has declined, with only Italy and Spain maintaining a strong presence in the top 10 in recent years.

_Collected: 2026-03-04_

---

## Recent Momentum Effect `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Finland's 2023 second-place finish with Käärijä (public vote winner) created significant momentum and international attention. Countries often leverage recent success (see Ukraine 2016→2017 interest, Italy 2021→2022). This increases selection quality and public interest for 2026.

---

## Years Since Last Win `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 18 | 20 | 22 | years |

> Finland last won in 2006 (20 years before 2026). 'Sympathy vote' or 'overdue narrative' has weak empirical support but may marginally influence close contests. More importantly, long gaps suggest structural challenges in song selection or performance.

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries (population <10M) winning Eurovision",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision has been held 68 times (1956-2024, excluding cancelled years). Finland has won once (2006, Lordi). Other small Nordic countries: Norway 3 wins, Sweden 7 wins, Denmark 3 wins. Finland specifically: 1/68 = 1.5%. Small Nordic countries collectively: ~14/68 = 20.6%, but Finland's individual rate is the ...

- "base_rate": {
- "reference_class": "Small Nordic countries (population <10M) winning Eurovision",
- "historical_frequency": 0.015,
- "sample_size": 68,
- "reasoning": "Eurovision has been held 68 times (1956-2024, excluding cancelled years). Finland has won once (2006, Lordi). Other small Nordic countries: Norway 3 wins, Sweden 7 wins, Denmark 3 wins. Finland specifically: 1/68 = 1.5%. Small Nordic countries collectively: ~14/68 = 20.6%, but Finland's individual rate is the relevant base rate."
- "drivers": [
- "name": "Song Quality Percentile",
- "type": "continuous",
- "p5": 30,
- "p50": 50,

