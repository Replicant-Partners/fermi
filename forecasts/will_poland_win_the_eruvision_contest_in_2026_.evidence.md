# Evidence Log: will poland win the eruvision contest in 2026?

**Version:** v12 | **Probability:** 151081.5% | **Updated:** 2026-03-05 02:04 UTC

---

## song_quality_percentile `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 50 | 85 | percentile_rank |

> Poland's best Eurovision result was 2nd place (Michał Szpak, 2016). Historical performance: median finish ~15th place (out of ~26 finalists). Song quality is highly variable year-to-year. Poland has qualified for finals 11/20 times (55% qualification rate vs ~50% baseline for semi-finalists).

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## televoting_appeal_factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.6 | 1 | 1.8 | relative_to_average |

> Poland receives moderate diaspora voting support (UK, Ireland, Scandinavia have Polish communities). However, lacks strong regional voting bloc compared to Balkans, Nordics, or ex-Soviet states. 2016 runner-up finish showed Poland CAN achieve high televoting when song resonates broadly.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## jury_appeal_factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.5 | 0.9 | 1.4 | relative_to_average |

> Poland historically underperforms with juries vs televoting. Juries favor vocal technique, staging sophistication, and contemporary production. Poland's entries often more traditional/rock-oriented, less aligned with jury preferences (pop, ballads with strong vocals).

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze Eurovision jury voting patterns 2016-2025: What song characteristics (genre, vocal style, pr_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The analysis of Eurovision jury voting patterns from 2016-2025 indicates that contemporary pop/ballad styles, strong vocal performances, high production values, and emotive staging are key factors in achieving high jury scores. There is a noticeable gap between jury and public preferences, with Poland's entries tending to score better with the public than with juries, especially for more traditional or rock-oriented songs.

**Key findings:**

- Based on analysis of Eurovision jury voting from 2016-2025, songs with contemporary pop/ballad styles tend to score higher with juries than traditional or rock-oriented entries. Vocal performances, production quality, and staging that showcase the artist's technical ability and emotional delivery are key factors in jury evaluation.
- Poland's Eurovision entries have generally scored higher with the public than with juries over the past decade. Jury scores for Poland have tended to be lower for songs with more traditional or rock-influenced styles compared to contemporary pop ballads.
- There is a trend towards juries favoring songs with strong vocal performances, high production values, and emotive staging that showcase the artist's technical skill and interpretive abilities. Entries that fit this profile, regardless of genre, have tended to perform better with juries in recent years.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Eurovision Song Contest winners (1956-2024)",
    "historical_frequency": 0.015,
    "sample_size": 68,
    "reasoning": "Poland has never won Eurovi

---

## competitive_field_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 22 | 26 | 30 | number_of_strong_competitors |

> 2026 will have ~40 countries competing, ~26 finalists. Typically 5-8 countries enter as genuine contenders (based on national selection quality, artist profile, production budget). Poland competes against perennial strong performers: Sweden, Italy, Ukraine, Netherlands, Australia, France.

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
    "reasoning": "Poland has never won Eurovision in 68 contests (competed since 1994, 20 participations). Historical win rate for any single country is ~1.5% (1/68). For countries that have never won, the rate is effectively 0% historically, but forward-looking probability must account for competitive dynamics. Big 5 countries (automatic fina...

### Agent: entity_investigator (Claude API) (relevance: 72%)

Poland's Eurovision televoting performance 2014-2024 shows a clear outlier in 2016 (222 televote points, 3rd place) versus typical 40-80 point performances. The data reveals that while Poland benefits from diaspora support in UK/Ireland/Scandinavia (providing baseline 20-30 points), breakthrough pan-European televoting success requires specific musical elements: theatrical presentation, emotional vocal peaks, universal themes, and staging spectacle. Poland's 2016 entry succeeded by combining the...

