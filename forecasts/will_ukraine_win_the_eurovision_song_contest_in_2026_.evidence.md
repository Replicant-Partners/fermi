# Evidence Log: will ukraine win the eurovision song contest in 2026?

**Version:** v4 | **Probability:** 1.5% | **Updated:** 2026-03-04 20:52 UTC

---

## war_status_2026 `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> If war continues into 2026, sympathy vote remains strong but may fatigue. If war ends with Ukrainian victory/favorable peace, celebration narrative could boost chances. If Ukraine loses territory or war drags on, sympathy may decline.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    

---

## song_quality_percentile `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 55 | 85 | percentile_rank |

> Ukraine has strong Eurovision history: 3 wins (2004, 2016, 2022), 2 second places. Average finish when participating is top 10. Song quality matters significantly - even with sympathy, poor songs don't win (Ukraine placed 12th in 2023 with less compelling entry despite ongoing war).

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    

---

## sympathy_vote_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 20 | 45 | 85 | percentage_boost |

> 2022 saw unprecedented sympathy voting (+30-40% boost estimated). By 2026, this will be 4 years into conflict. Historical precedent: sympathy votes decay over time (Israel's wins became less frequent, former Yugoslav states' post-war boost lasted ~5-10 years). Voter fatigue is real.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    

---

## participation_probability `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Ukraine participated in 2023 (Liverpool hosted as 2022 winner) and 2024 (Basel). High likelihood of continued participation unless catastrophic scenario. EBU has been supportive. Financial constraints possible but diaspora/international support likely covers costs.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    

---

## voting_bloc_support `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 40 | 65 | 85 | percentage_bloc_support |

> Ukraine traditionally receives strong votes from Poland, Lithuania, Baltic states, Moldova. Post-2022, Western European support increased dramatically. Assuming continued EU/NATO solidarity, bloc voting remains favorable but may normalize from 2022 peak.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Eurovision winners from countries experiencing active military conflict or recent war",
    "historical_frequency": 0.15,
    "sample_size": 67,
    "reasoning": "Since Eurovision began in 1956 (67 contests), countries in active conflict rarely win. Ukraine won in 2016 (post-Crimea annexation) and 2022 (during full invasion, with massive sympathy vote). Israel won in 1978-1979 (conflict periods) and 2018. Serbia won in 2007 (post-conflict). Appr...

### Agent: sentiment_analyzer (Claude API) (relevance: 85%)

Based on my research, there appears to be substantial and broad-based support for Ukraine among the voting public and political leadership in the EU. Public opinion polls, parliamentary votes, and statements from key European leaders all indicate a high degree of solidarity with Ukraine in its conflict with Russia.

