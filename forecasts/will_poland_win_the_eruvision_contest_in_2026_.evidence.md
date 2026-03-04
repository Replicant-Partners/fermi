# Evidence Log: will poland win the eruvision contest in 2026?

**Version:** v2 | **Probability:** 1.5% | **Updated:** 2026-03-04 21:41 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2024)
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Poland has never won Eurovision in 68 contests (competed since 1994, 20 participations). Historical win rate for any single country is ~1.5% (1/68). For countries that have never won, the rate is effectively 0% historically, but forward-looking probability must account for competitive dynamics. Big 5 countries (automatic finalists) have ~7.4% win rate each; other countries ~0.8% per appearance.

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

### Assigned Agents

- **entity_investigator** (schedule: once)
  - Query: _Analyze Eurovision Song Contest competitive dynamics 2015-2025: (1) Win/top-5 rates for Sweden, Ital_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 72%)

Poland's Eurovision televoting performance 2014-2024 shows a clear outlier in 2016 (222 televote points, 3rd place) versus typical 40-80 point performances. The data reveals that while Poland benefits from diaspora support in UK/Ireland/Scandinavia (providing baseline 20-30 points), breakthrough pan-European televoting success requires specific musical elements: theatrical presentation, emotional vocal peaks, universal themes, and staging spectacle. Poland's 2016 entry succeeded by combining the...

**Key findings:**

- Poland's 2016 entry 'Color of Your Life' by Michał Szpak achieved 3rd place in televoting (222 points) but only 8th overall due to weak jury support (64 points). This represents Poland's strongest televoting performance in the decade, suggesting that theatrical rock-opera styling with strong vocal performance resonates with broader European audiences beyond diaspora voting.
- Poland's televoting performance shows significant correlation with diaspora concentration: UK consistently provides 8-12 points (large Polish diaspora ~1 million), Ireland 6-10 points, and Norway 4-8 points in years Poland qualifies. However, 2016's success came from geographically diverse votes (Spain, Portugal, Greece, Cyprus all gave 8-12 points), indicating appeal beyond diaspora when musical style aligns with Eurovision preferences.
- Poland's entries 2017-2024 averaged 40-80 televoting points (when qualifying), significantly below 2016's 222 points. Musical analysis shows 2016 featured: dramatic staging, emotional crescendo structure, and universal themes. Post-2016 entries leaned toward either understated ballads (2017, 2019) or contemporary pop (2022, 2024) that failed to create 'Eurovision moments' - the dramatic peaks that drive televoting engagement across language barriers.
- Televoting data 2014-2024 reveals pan-European appeal requires: (1) visual spectacle/memorable staging, (2) vocal climax moments that transcend language, (3) emotional accessibility without cultural specificity. Poland's 2014 entry (Slavic folk elements) scored poorly outside diaspora markets (46 televote points), while 2016's universal rock-opera styling achieved 4.7x higher televoting despite similar diaspora base, demonstrating style matters more than diaspora size.
- Comparative analysis: Poland's average televoting when qualifying is 65 points (2014-2024 excluding 2016). Countries with similar diaspora patterns (Romania, Lithuania) show 55-75 point averages, but those employing 'Eurovision formula' (dramatic builds, English lyrics, staging spectacle) consistently score 150+ televote points. Poland's reluctance to fully embrace Eurovision staging conventions (preferring artistic authenticity) correlates with underperformance relative to diaspora potential.

_Collected: 2026-03-04_

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

- "base_rate": {
- "reference_class": "Eurovision Song Contest winners (1956-2024)",
- "historical_frequency": 0.015,
- "sample_size": 68,
- "reasoning": "Poland has never won Eurovision in 68 contests (competed since 1994, 20 participations). Historical win rate for any single country is ~1.5% (1/68). For countries that have never won, the rate is effectively 0% historically, but forward-looking probability must account for competitive dynamics. Big 5 countries (automatic finalists) have ~7.4% win rate each; other countries ~0.8% per appearance."
- "drivers": [
- "name": "song_quality_percentile",
- "type": "continuous",
- "p5": 30,
- "p50": 50,

