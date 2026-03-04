# Evidence Log: will poland win the eruvision contest in 2026?

**Version:** v1 | **Probability:** 1.5% | **Updated:** 2026-03-03 23:56 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2024)
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Poland has never won Eurovision in 68 contests (competed 1994-2024 with gaps). Historical win rate for Poland specifically is 0/~25 = 0%. For any single country in modern era (post-1998, ~40 countries competing), base rate is approximately 1/40 = 2.5%. Poland's track record suggests below-average performance: best result was 2nd place (1994, Edyta Górniak), with most entries finishing mid-to-lower table.

---

## song_quality_ranking `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 15 | 25 | 38 | final_position_out_of_40 |

> Poland's historical performance: median finish ~20th-25th place. Recent entries (2019-2024) have not qualified for finals or finished lower half. Quality depends on national selection process and artist appeal. No structural advantage in production quality or artist development compared to consistent winners (Sweden, Italy, Ukraine).

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## jury_televote_appeal `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 95 | 180 | combined_points |

> Winners typically score 400-600 points in modern voting. Poland's best (1994) scored 166 points under old system. Recent qualifiers score 50-150 points. Poland lacks diaspora voting bloc advantage (unlike Russia, Ukraine, Greece) and cultural proximity clusters are weak. Jury appeal requires exceptional song craft.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## geopolitical_sentiment `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Eurovision voting has geopolitical dimensions. Poland's position: EU member, NATO ally, Ukraine war supporter. Could benefit from solidarity voting if Ukraine-related narrative resonates (2026 = 4 years post-invasion). However, Poland lacks the direct sympathy narrative Ukraine had in 2022. Neutral-to-slightly-positive context.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## national_selection_investment `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Poland's broadcaster TVP has not demonstrated consistent strategic investment in Eurovision success (unlike SVT Sweden, RAI Italy). Recent political changes in Poland (2023 government transition) may affect cultural policy. No evidence of systematic artist development program or hiring international songwriters/producers like winning countries do.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovision in 68 contests (competed 1994-2024 with gaps). Historical win rate for Poland specifically is 0/~25 = 0%. For any single country in modern era (post-1998, ~40 countries competing), base rate is approximately 1/40 = 2.5%. Poland's track record suggests below-average performance: best result was ...

- "base_rate": {
- "reference_class": "Eurovision Song Contest winners (1956-2024)",
- "historical_frequency": 0.015,
- "sample_size": 68,
- "reasoning": "Poland has never won Eurovision in 68 contests (competed 1994-2024 with gaps). Historical win rate for Poland specifically is 0/~25 = 0%. For any single country in modern era (post-1998, ~40 countries competing), base rate is approximately 1/40 = 2.5%. Poland's track record suggests below-average performance: best result was 2nd place (1994, Edyta Górniak), with most entries finishing mid-to-lower table."
- "drivers": [
- "name": "song_quality_ranking",
- "type": "continuous",
- "p5": 15,
- "p50": 25,

