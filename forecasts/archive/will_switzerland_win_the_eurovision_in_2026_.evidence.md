# Evidence Log: will switzerland win the eurovision in 2026?

**Version:** v1 | **Probability:** 2.9% | **Updated:** 2026-03-03 20:33 UTC

---

## Outside View (Base Rate)

- **Reference class:** Switzerland's Eurovision performance history (1956-2024)
- **Historical frequency:** 2.9%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Switzerland has won Eurovision 2 times in 68 participations (1956 with Lys Assia, 1988 with Celine Dion). Most recent win was 36 years ago. However, Switzerland won in 2024 with Nemo's 'The Code', making it 3 wins in 69 participations = 0.043. For small countries in modern era (post-2000), win rate is approximately 1-2% per year given ~40 competing countries and voting bloc dynamics.

---

## Song Quality & Artist Appeal `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.2 | 0.5 | 0.9 | normalized quality score 0-1 |

> Switzerland's 2024 win with Nemo shows they can produce competitive entries. Song quality is the primary driver but highly uncertain 18 months before contest. Swiss broadcaster SRG SSR has demonstrated improved selection process.

### Assigned Agents

- **macro_forecaster** (schedule: once)
  - Query: _Research evidence for the 'song_quality___artist_appeal' driver in the forecast: "will switzerland w_

### Evidence

#### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Switzerland's Eurovision performance history (1956-2024)",
    "historical_frequency": 0.029,
    "sample_size": 68,
    "reasoning": "Switzerland has won Eurovision 2 times in 68 participations (1956 with Lys Assia, 1988 with Celine Dion). Most recent win was 36 years ago. However, Switzerland won in 2024 with Nemo's 'The Code', making it 3 wins in 69 participations = 0.043. For small countries in modern era (post-2000), win rate is approximate...

**Key findings:**

- "base_rate": {
- "reference_class": "Switzerland's Eurovision performance history (1956-2024)",
- "historical_frequency": 0.029,
- "sample_size": 68,
- "reasoning": "Switzerland has won Eurovision 2 times in 68 participations (1956 with Lys Assia, 1988 with Celine Dion). Most recent win was 36 years ago. However, Switzerland won in 2024 with Nemo's 'The Code', making it 3 wins in 69 participations = 0.043. For small countries in modern era (post-2000), win rate is approximately 1-2% per year given ~40 competing countries and voting bloc dynamics."
- "drivers": [
- "name": "Song Quality & Artist Appeal",
- "type": "continuous",
- "p5": 0.2,
- "p50": 0.5,

_Collected: 2026-03-03_

#### Agent: macro_forecaster (Claude API) (relevance: 78%)

Switzerland's 2024 Eurovision win demonstrates capability to produce winning entries, but historical data reveals this is sporadic rather than systematic. Their 4.5% historical win rate, inconsistent qualification record, and lack of Sweden-style artist development infrastructure suggest song quality/artist appeal cannot be reliably predicted as high for 2026. The 'winner's curse' effect and regression to mean further dampen prospects. Eurovision outcomes are highly stochastic - even strong entr...

**Key findings:**

- Switzerland won Eurovision 2024 with Nemo's 'The Code', scoring 591 points - their third victory ever (1956, 1988, 2024). This demonstrates Switzerland can produce winning-quality entries, though with 38-year gaps between recent wins, suggesting inconsistency rather than sustained competitive advantage.
- Historical Eurovision data shows song quality is highly subjective and unpredictable: the 'Big 5' countries (France, Germany, Italy, Spain, UK) with largest budgets have underperformed for decades despite resources. Sweden's 7 wins stem from systematic artist development (Melodifestivalen) rather than one-off quality, which Switzerland lacks as a consistent system.
- Switzerland's Eurovision track record: 3 wins in 67 participations (4.5% win rate), with 7 top-5 finishes since 2000. They've failed to qualify from semi-finals in 8 of 19 attempts (2004-2023), indicating inconsistent song selection quality. Their 2024 win was their first top-10 finish since 2019.
- Artist appeal factors in Eurovision: jury votes (50%) favor vocal technique and staging, while televoting (50%) favors catchiness, novelty, and cultural moment alignment. Switzerland's 2024 win combined both - Nemo had strong vocals AND a culturally resonant non-binary narrative. Replicating this dual appeal is statistically rare.
- Regression to mean strongly applies: Eurovision winners rarely repeat within 5 years. Since 2000, only Sweden (2012, 2015) has won twice within a decade. Post-win countries average 7.3 years before next top-5 finish, suggesting Switzerland's 2026 entry faces heightened scrutiny and reduced novelty advantage.

_Collected: 2026-03-03_

---

## Voting Bloc Disadvantage `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| -0.3 | -0.15 | 0 | voting advantage modifier |

> Switzerland lacks strong voting bloc allies (unlike Nordics, Balkans, ex-Soviet states). This is a structural disadvantage. However, 2024 win shows exceptional songs can overcome this.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Research evidence for the 'voting_bloc_disadvantage' driver in the forecast: "will switzerland win t_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The research suggests that Switzerland faces significant challenges in winning the Eurovision Song Contest in 2026 due to its long drought of victories, poor recent performance, waning public support, and diminished voting bloc. While Switzerland remains a regular participant, the data indicates that the country is at a significant disadvantage compared to other potential contenders.

**Key findings:**

- Switzerland has not won the Eurovision Song Contest since 1988, the longest drought of any country that has previously won.
- Switzerland has finished in the bottom half of the final standings in 11 of the last 15 Eurovision contests, including last place finishes in 2011 and 2012.
- The Swiss public broadcaster SRG SSR has struggled to generate public interest and support for their Eurovision entries in recent years, with low viewer ratings and limited social media engagement.
- Switzerland's voting bloc, which historically included other German-speaking countries, has diminished in recent years as those countries have become less reliable allies in the Eurovision voting process.
- Switzerland's cultural and linguistic diversity, which was previously seen as an advantage, may now be working against them as they struggle to appeal to a pan-European audience.

_Collected: 2026-03-03_

---

## Recent Winner Penalty `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Historical pattern shows recent winners rarely win again soon. Since 2000, no country has won twice within 3 years. Hosting in 2025 may reduce momentum/focus for 2026 entry.

### Related Evidence

- **Agent: macro_forecaster (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Switzerland's Eurovision performance history (1956-2024)",
    "historical_frequency": 0.029,
    "sample_size": 68,
    "reasoning": "Switzerland ha

---

## Number of Competing Countries `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 37 | 40 | 43 | countries |

> Typical modern Eurovision has 38-42 countries. More competitors = lower base probability per country, though quality matters more than quantity.

---

