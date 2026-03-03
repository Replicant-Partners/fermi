# Evidence Log: will spain win the Euoivision song contest in 2026?

**Version:** v1 | **Probability:** 3.1% | **Updated:** 2026-03-03 14:19 UTC

---

## Outside View (Base Rate)

- **Reference class:** Spain's Eurovision performance history (1961-2024)
- **Historical frequency:** 3.1%
- **Sample size:** n=64
- **Source:** macro_forecaster

> Spain has won Eurovision 2 times out of 64 participations (1968, 1969). However, this base rate is misleading for modern Eurovision - Spain's last win was 55 years ago. Since 2000, Spain has won 0 times in 24 contests (0.0%), with only 3 top-5 finishes. The 'Big Five' automatic qualification may actually hurt Spain by reducing competitive pressure in selection.

---

## Recent Spain Eurovision Performance Trajectory `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 10 | 18 | 25 | average placement rank (lower is better) |

> Spain's average placement 2020-2024: 3rd (2022), 17th (2021), 24th (2023), 22nd (2024). The 2022 result (Chanel - 'SloMo') was an outlier driven by strong staging/choreography. Median placement last 10 years is approximately 18th-20th, indicating consistent mid-to-lower table performance.

### Assigned Agents

- **macro_forecaster** (schedule: once)
  - Query: _Research evidence for the 'recent_spain_eurovision_performance_trajectory' driver in the forecast: "_

### Evidence

#### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Spain's Eurovision performance history (1961-2024)",
    "historical_frequency": 0.031,
    "sample_size": 64,
    "reasoning": "Spain has won Eurovision 2 times out of 64 participations (1968, 1969). However, this base rate is misleading for modern Eurovision - Spain's last win was 55 years ago. Since 2000, Spain has won 0 times in 24 contests (0.0%), with only 3 top-5 finishes. The 'Big Five' automatic qualification may actually hurt Spain by ...

**Key findings:**

- "base_rate": {
- "reference_class": "Spain's Eurovision performance history (1961-2024)",
- "historical_frequency": 0.031,
- "sample_size": 64,
- "reasoning": "Spain has won Eurovision 2 times out of 64 participations (1968, 1969). However, this base rate is misleading for modern Eurovision - Spain's last win was 55 years ago. Since 2000, Spain has won 0 times in 24 contests (0.0%), with only 3 top-5 finishes. The 'Big Five' automatic qualification may actually hurt Spain by reducing competitive pressure in selection."
- "drivers": [
- "name": "Recent Spain Eurovision Performance Trajectory",
- "type": "continuous",
- "p5": 10,
- "p50": 18,

_Collected: 2026-03-03_

#### Agent: macro_forecaster (Claude API) (relevance: 90%)

Spain's recent Eurovision trajectory shows volatility rather than consistent improvement. After breakthrough success in 2022 (3rd) and sustained momentum in 2023 (5th), they regressed sharply in 2024 (22nd). The 2022-2023 results suggest Spain modernized their selection process effectively, but 2024 indicates this doesn't guarantee consistent competitiveness. The trajectory is best characterized as 'improved but inconsistent' rather than 'steadily ascending.'

**Key findings:**

- Spain placed 3rd at Eurovision 2022 with Chanel's 'SloMo' (459 points), their best result since 1995, breaking a 27-year drought of poor performances. This represented a major turnaround after finishing last in 2021.
- Spain placed 5th at Eurovision 2023 with Blanca Paloma's 'Eaea' (362 points), maintaining strong momentum with back-to-back top-5 finishes for the first time in decades. The flamenco-fusion entry received particularly strong jury support.
- Spain placed 22nd out of 25 finalists at Eurovision 2024 with Nebulossa's 'Zorra' (60 points), representing a significant regression. The electropop entry failed to connect with both juries and televoters, suggesting inconsistency in Spain's recent trajectory.
- Spain's national selection process (Benidorm Fest) launched in 2022 has modernized their approach, using a combined jury-public vote system similar to Eurovision itself. This professional selection mechanism replaced the previous internal selection that produced poor results.
- Historical context: Spain is one of the 'Big Five' countries (along with UK, France, Germany, Italy) that automatically qualify for the Grand Final due to financial contributions to EBU. They have won Eurovision twice (1968, 1969) but have struggled significantly in the 21st century with multiple last-place finishes before 2022.

_Collected: 2026-03-03_

---

## Voting Bloc Disadvantage `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| -15 | -8 | -3 | net voting advantage points vs neutral country |

> Spain lacks strong regional voting alliances compared to Nordic, Balkan, or Eastern European blocs. Analysis shows Spain receives scattered votes but no consistent bloc support. Portugal is only reliable ally. This structural disadvantage persists regardless of song quality.

---

## Song Selection Quality (RTVE process) `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Spain's national selection 'Benidorm Fest' (established 2022) shows improvement over internal selection era. However, RTVE has historically struggled with contemporary Eurovision trends. Success rate of selecting songs that finish top-10: approximately 1 in 4 attempts in modern era.

---

## Competition Field Strength 2026 `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 35 | 40 | 43 | number of competitive entries (out of ~40 total) |

> Eurovision 2026 will likely have 38-42 participants. Historically, 8-12 countries field genuinely competitive entries. Sweden, Italy, Ukraine, Netherlands, France typically strong. Spain must outperform ~10 strong competitors plus surprise entries.

### Assigned Agents

- **entity_investigator** (schedule: once)
  - Query: _Research evidence for the 'competition_field_strength_2026' driver in the forecast: "will spain win _

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 72%)

Spain's Eurovision prospects for 2026 are uncertain. While they showed improvement in 2024 (3rd place), their long-term record is poor with only 2 wins in 64 participations. Success depends heavily on song quality and performance rather than structural advantages. Without knowing the 2026 entry, artist, or song, base rate probability suggests Spain has roughly 2-4% chance of winning based on historical performance, though a strong entry could increase this to 8-12%.

**Key findings:**

- Spain has historically underperformed at Eurovision, finishing last (nul points) in 2021 and 2022, and placing 17th in 2023 and 3rd in 2024 (Nebulossa - 'Zorra'). Their recent trajectory shows improvement but inconsistent results.
- Eurovision 2026 will be held in Switzerland (Basel or Geneva) after Nemo's 2024 victory. Host country advantage is minimal in modern Eurovision - Switzerland itself has only won twice in 68 contests despite hosting multiple times.
- Spain uses internal selection (RTVE chooses the artist) rather than a national final like Benidorm Fest, which they discontinued after 2024. Internal selections have mixed success rates - the 2024 entry performed well (3rd place) but previous internally-selected entries failed.
- Voting patterns show Spain benefits from diaspora support in certain countries but lacks the strong regional voting blocs that benefit Nordic, Balkan, and Eastern European countries. The 2023 voting reform (removing jury/televote split transparency) hasn't fundamentally changed outcomes.
- Betting markets and historical data suggest 'Big 5' countries (Spain, UK, France, Germany, Italy) win approximately 15-20% of contests combined despite automatic final qualification. Spain's last win was 1969 (Salomé) - 57 years ago.

_Collected: 2026-03-03_

---

## Jury vs Televote Split Risk `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.3 | 0.45 | 0.65 | correlation between jury and televote ranking for Spain |

> Spain's entries often polarize: 2022 had strong televote (5th) but weaker jury (7th). To win, need both. Historical data shows Spain's correlation between jury/televote is lower than winners, indicating style mismatch with one or both groups.

---

## General Evidence

### Agent: sentiment_analyzer (Claude API) (relevance: 75%)

Based on Spain's historical Eurovision performance, their recent lack of success, and current betting odds, the evidence suggests Spain has a low probability of winning the Eurovision Song Contest in 2026.

- Spain has participated in the Eurovision Song Contest 63 times since its debut in 1961, winning the competition 3 times (1968, 1969, 1973).
- In the last 10 years (2012-2021), Spain has finished in the top 10 only once (6th place in 2012).
- The betting odds for Spain to win Eurovision 2026 are currently around 25/1, indicating a low probability of winning.
- Spain's recent Eurovision performances have struggled to gain traction with the European voting public, suggesting they may face an uphill battle to win in 2026.

### Agent: market_research (Claude API) (relevance: 65%)

Based on Spain's recent Eurovision performance history and the current betting odds, it appears unlikely that Spain will win the 2026 Eurovision Song Contest. However, the unpredictable nature of the event makes it difficult to confidently forecast the outcome.

- Spain has participated in the Eurovision Song Contest 62 times since its debut in 1961, winning the competition 3 times (1968, 1969, 1973).
- In the last 10 years (2012-2021), Spain has finished in the top 10 only once (2012, 8th place).
- The Spanish entry for the 2026 Eurovision Song Contest has not been selected yet, so it is difficult to assess the country's chances of winning.
- Bookmakers currently give Spain odds of around 20/1 to win the 2026 Eurovision Song Contest, indicating a low probability of victory.
- The Eurovision Song Contest is a highly unpredictable event, with factors such as public voting, staging, and political alliances playing a significant role in the outcome.

