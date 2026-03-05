# Evidence Log: will poland win the eruvision contest in 2026?

**Version:** v15 | **Probability:** 50.0% | **Updated:** 2026-03-05 21:42 UTC

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

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze historical televoting patterns for Poland in Eurovision (2014-2024), identifying characteris_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The research indicates that Poland has the potential to achieve strong results in Eurovision, particularly when leveraging the support of its diaspora communities in key voting markets and presenting acts that resonate with the European public. While past performance has been inconsistent, the overall trend suggests increasing public engagement and a positive sentiment toward Polish culture and music.

**Key findings:**

- Poland's televoting performance in Eurovision has been inconsistent, with some high-placing entries (e.g. Donatan & Cleo in 2014, Michał Szpak in 2016) but also several low-scoring results. However, there appears to be a trend of increasing public support in recent years.
- Polish diaspora communities in the UK, Ireland, and Scandinavia have demonstrated strong engagement and voting for Polish Eurovision entries, contributing significantly to their overall results.
- Social media sentiment analysis indicates a generally positive reception for recent Polish Eurovision candidates, with fans appreciating the unique cultural elements and musical styles represented.
- Compared to other mid-tier Eurovision countries, Poland has shown the potential for breakthrough results when the right act and song resonates with the European public. This is exemplified by Michał Szpak's 8th place finish in 2016, which was Poland's best result in the 2010s.
- Current European public sentiment toward Polish culture and music appears to be favorable, with growing interest and appreciation for the country's diverse musical offerings, particularly in the context of Eurovision.

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

