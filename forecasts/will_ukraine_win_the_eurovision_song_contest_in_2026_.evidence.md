# Evidence Log: will ukraine win the eurovision song contest in 2026?

**Version:** v11 | **Probability:** 4.0% | **Updated:** 2026-03-03 21:53 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision winners by country (2000-2024)
- **Historical frequency:** 4.0%
- **Sample size:** n=25
- **Source:** macro_forecaster

> Ukraine has won Eurovision twice in the modern era (2004, 2016) plus 2022, giving them 3 wins out of 25 contests = 12% historical win rate. However, the base rate for any single country winning in a given year with ~40 participants is approximately 2.5%. Ukraine's historical performance (12%) is significantly above base rate, suggesting structural advantages (strong musical tradition, diaspora voting, cultural appeal).

---

## geopolitical_sympathy_factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0 | 0.15 | 0.4 | probability_boost |

> Ukraine won 2022 with Kalush Orchestra amid full-scale invasion, receiving unprecedented sympathy votes (439 points from public, highest ever). By 2026, the war will be 4 years old. Historical precedent: sympathy effects decay significantly after 2-3 years (see Israel post-conflict, Balkan states). If war is ongoing but stalemated, sympathy will be substantially diminished. If war has ended, effect approaches zero. If Ukraine is losing territory or facing humanitarian crisis, could see modest boost.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twi

---

## song_quality_percentile `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 40 | 65 | 75 | percentile_rank |

> Ukraine's track record shows consistent quality: 2021 (5th place), 2023 (6th place with Tvorchi), 2024 (did not qualify from semi-final with alyona alyona & Jerry Heil, though this was controversial). Ukraine has strong musical infrastructure and takes Eurovision seriously. However, 2026 song is unknown. Median assumption: above-average entry (65th percentile) but not guaranteed top-tier.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twi

---

## voting_bloc_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 60 | 70 | 75 | expected_points |

> Ukraine benefits from Eastern European voting bloc and substantial diaspora across EU (Poland, Germany, Italy, Czech Republic). Historical data shows Ukraine typically receives 60-120 points from reliable partners regardless of song quality. This is structural and unlikely to change by 2026.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twi

---

## competition_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 2 | 5 | 8 | number_of_strong_competitors |

> Eurovision typically has 3-8 genuine contenders per year. Traditional powerhouses: Sweden, Italy, France, Netherlands, Switzerland, Australia, Norway. 2026 will likely see 4-6 strong entries. Ukraine must outperform all of them.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twi

---

## war_status_2026 `binary`

- **Probability:** 70%
- **Impact multiplier:** 1.3x

> Current trajectory suggests war likely ongoing in May 2026 (65% probability), though potentially frozen or low-intensity. If war has ended with Ukrainian victory/favorable terms, sympathy factor drops but national morale could produce exceptional entry. If war ended unfavorably, participation itself uncertain.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twi

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Eurovision winners by country (2000-2024)",
    "historical_frequency": 0.04,
    "sample_size": 25,
    "reasoning": "Ukraine has won Eurovision twice in the modern era (2004, 2016) plus 2022, giving them 3 wins out of 25 contests = 12% historical win rate. However, the base rate for any single country winning in a given year with ~40 participants is approximately 2.5%. Ukraine's historical performance (12%) is significantly above base rate, su...

- "base_rate": {
- "reference_class": "Eurovision winners by country (2000-2024)",
- "historical_frequency": 0.04,
- "sample_size": 25,
- "reasoning": "Ukraine has won Eurovision twice in the modern era (2004, 2016) plus 2022, giving them 3 wins out of 25 contests = 12% historical win rate. However, the base rate for any single country winning in a given year with ~40 participants is approximately 2.5%. Ukraine's historical performance (12%) is significantly above base rate, suggesting structural advantages (strong musical tradition, diaspora voting, cultural appeal)."
- "drivers": [
- "name": "geopolitical_sympathy_factor",
- "type": "continuous",
- "p5": 0.0,
- "p50": 0.15,

