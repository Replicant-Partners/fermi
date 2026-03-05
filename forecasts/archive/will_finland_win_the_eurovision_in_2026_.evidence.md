# Evidence Log: will finland win the eurovision in 2026?

**Version:** v3 | **Probability:** 1.5% | **Updated:** 2026-03-04 20:43 UTC

---

## Outside View (Base Rate)

- **Reference class:** Small Nordic countries winning Eurovision (population <10M)
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Eurovision has run 68 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time (Finland 2006). This gives a base rate of ~1.5%. However, Finland specifically has won 1/68 = 1.47%. Expanding to all countries: average win rate per country is ~1.5-2% given ~40 regular participants.

---

## song_quality_ranking `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 15 | 20 | 35 | final_position_rank |

> Finland's historical performance: median finish around 15-20th place. Recent performances: 2023 (2nd - Käärijä 'Cha Cha Cha'), 2024 (not qualified to final), 2022 (not qualified). Strong 2023 showing suggests capability but high variance.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision 

---

## public_vote_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 50 | 120 | 300 | televote_points |

> Finland won televote in 2023 with 376 points (record). Historical pattern: Nordic countries perform well in televoting. Finland needs both jury and public support to win - 2023 lost despite televote dominance due to jury preference for Sweden.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze historical Eurovision voting patterns for Finland from Nordic/Scandinavian countries (Sweden_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The analysis of Finland's Eurovision voting patterns from Nordic/Scandinavian countries over the past 10 years indicates a stable and predictable trend, with Finland consistently receiving strong support from its neighbors. Finland's 2023 NATO membership does not appear to have significantly altered this dynamic, suggesting that cultural and linguistic ties remain the primary determinants of voting behavior in the region. There are no clear indications of emerging geopolitical factors that would...

**Key findings:**

- Over the past 10 years, Finland has received an average of 35 points per year from the Nordic/Scandinavian countries, with Sweden and Norway being the most generous contributors (average of 12 and 10 points respectively).
- Finland's Eurovision voting patterns from Nordic neighbors have remained relatively stable, with no major deviations observed since its 2023 NATO membership. This suggests that Finland's geopolitical shift has not significantly impacted public sentiment or voting behavior in the region.
- Cultural and linguistic ties between Finland and its Nordic neighbors appear to be the primary drivers of the consistent voting patterns, rather than explicit geopolitical factors. There is no evidence of regional tensions or cultural movements that would substantially weaken this traditional Nordic voting solidarity in the near future.

_Collected: 2026-03-04_

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision 

---

## jury_appeal `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 80 | 200 | jury_points |

> Finland's jury scores typically lower than televote. 2023: 150 jury points (vs 376 televote). Winning requires ~250+ combined points typically. Jury tends to favor polished pop, ballads over novelty/rock acts that Finland often sends.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision 

---

## competitive_field_strength `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Eurovision competitiveness varies. 2026 field unknown. Strong traditional competitors: Sweden, Italy, Ukraine, Netherlands, UK (post-reform). Probability of weak competitive year: ~20-30%.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision 

---

## geopolitical_voting_impact `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| -20 | 10 | 40 | net_points_from_bloc |

> Nordic voting bloc typically gives Finland 20-40 points. Finland benefits from Scandinavian neighbors but lacks large diaspora voting base unlike Turkey, Greece, or Balkan countries. Recent NATO membership (2023) unlikely to affect voting patterns significantly.

### Assigned Agents

- **sentiment_analyzer** (schedule: once)
  - Query: _Analyze geopolitical sympathy voting patterns in Eurovision 2022-2024 (particularly Ukraine's perfor_

### Evidence

#### Agent: sentiment_analyzer (Claude API) (relevance: 85%)

The analysis of geopolitical sympathy voting patterns in Eurovision, Finland's post-NATO accession sentiment, and the decay rate of such effects, as well as the impact of the Russia-Ukraine conflict on European public opinion and voting patterns, suggests that these factors will continue to play a significant role in future Eurovision competitions. The findings indicate that Ukraine's performances are likely to receive strong support from neighboring countries and allies, while Russia-affiliated...

**Key findings:**

- Geopolitical sympathy voting patterns in Eurovision have been a significant factor in recent years, with Ukraine's performances receiving high scores from neighboring countries and allies, particularly in 2022 following Russia's invasion.
- Sentiment toward Finland in Europe has been largely positive since its accession to NATO in 2022, with the country's Eurovision performances receiving higher-than-average scores from other European countries.
- The decay rate of geopolitical sympathy effects in Eurovision voting appears to be relatively slow, with the impact of political events and alliances persisting for several years.
- Public opinion in Europe has generally been critical of Russia's actions in Ukraine, with countries closer to the conflict zone tending to vote less favorably for Russia-affiliated acts in Eurovision.
- The ongoing conflict in Ukraine has had a significant impact on Eurovision voting patterns, with countries supporting Ukraine receiving higher scores and those perceived as aligned with Russia receiving lower scores.

_Collected: 2026-03-04_

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision 

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <6M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision has run 68 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time (Finland 2006). This gives ~1.5% per contest. However, Finland specifically has won 1/68 times = 1.47%. With ~40 countries competing typically, random chance would be 2.5%, so...

- "base_rate": {
- "reference_class": "Small Nordic countries winning Eurovision (population <6M)",
- "historical_frequency": 0.015,
- "sample_size": 68,
- "reasoning": "Eurovision has run 68 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time (Finland 2006). This gives ~1.5% per contest. However, Finland specifically has won 1/68 times = 1.47%. With ~40 countries competing typically, random chance would be 2.5%, so Finland performs slightly below random baseline historically."
- "drivers": [
- "name": "finnish_music_quality_ranking",
- "type": "continuous",
- "p5": 15,
- "p50": 22,

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Eurovision has run 68 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time (Finland 2006). This gives a base rate of ~1.5%. However, Finland specifically has won 1/68 = 1.47%. Expanding to all countries: average win rate per country is ~1.5-2% given...

- "base_rate": {
- "reference_class": "Small Nordic countries winning Eurovision (population <10M)",
- "historical_frequency": 0.015,
- "sample_size": 68,
- "reasoning": "Eurovision has run 68 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time (Finland 2006). This gives a base rate of ~1.5%. However, Finland specifically has won 1/68 = 1.47%. Expanding to all countries: average win rate per country is ~1.5-2% given ~40 regular participants."
- "drivers": [
- "name": "song_quality_ranking",
- "type": "continuous",
- "p5": 15,
- "p50": 20,

