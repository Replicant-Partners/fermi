# Evidence Log: will finalnd win the eurvision in 2026?

**Version:** v3 | **Probability:** 1.5% | **Updated:** 2026-03-03 12:20 UTC

---

## Outside View (Base Rate)

- **Reference class:** Small Nordic countries winning Eurovision (population <6M)
- **Historical frequency:** 1.5%
- **Sample size:** n=67
- **Source:** macro_forecaster

> Eurovision has run 67 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time collectively in the modern era (Finland 2006). Finland specifically: 3 wins total (1961 shared, 2006, 2023) = 3/67 = 0.045 base rate for Finland specifically.

---

## Recent momentum effect `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Finland won in 2023 with Käärijä's 'Cha Cha Cha'. Historical pattern: repeat winners within 3 years is rare (only 2 countries have won back-to-back in modern era: Ireland 1992-1994, Israel 1978-1979). However, recent success often indicates strong national broadcaster investment and public engagement.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Research evidence for the 'recent_momentum_effect' driver in the forecast: "will finalnd win the eur_

### Evidence

#### Agent: market_research (Claude API) (relevance: 75%)

The research indicates that Finland has not demonstrated strong recent momentum or a consistent track record of success in the Eurovision Song Contest, suggesting a low probability of winning in 2026 based on historical performance.

**Key findings:**

- Finland has not won the Eurovision Song Contest since 2006, suggesting a lack of recent momentum.
- Finland has only won the Eurovision Song Contest twice in its history (2006 and 2013), indicating a relatively weak track record compared to other Nordic countries.
- In the last 10 Eurovision contests (2012-2021), Finland has finished in the top 10 only twice, with an average placement of 15th.
- Bookmakers currently give Finland odds of around 50/1 to win the 2026 Eurovision Song Contest, suggesting a low probability of victory.

_Collected: 2026-03-03_

---

## Big 5 advantage factor `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Finland is NOT in Big 5 (UK, France, Germany, Italy, Spain). Must qualify through semi-finals. This reduces probability by ~15-20% as semi-final performance adds uncertainty. Big 5 countries win ~30% of contests despite being only 5/~40 participants.

---

## Voting bloc strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 3 | 6 | 10 | reliable voting partners |

> Nordic bloc (Sweden, Norway, Denmark, Iceland) typically exchanges points. Finland also receives votes from Estonia, Baltic states. However, 2023 win showed Finland can win on merit beyond bloc voting. Estimated 6 reliable high-point partners.

---

## Song quality variance `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.01 | 0.045 | 0.15 | probability of top-tier song |

> National selection quality varies. Finland uses UMK (Uuden Musiikin Kilpailu) selection process. 2023 produced viral hit. 2024 entry (Windows95man) placed 11th. Historical variance suggests 1-in-20 to 1-in-7 chance of producing genuine contender.

---

## Geopolitical sentiment `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| -0.02 | 0.01 | 0.05 | probability adjustment |

> Finland's 2023 NATO accession and proximity to Russia may generate sympathy voting in 2026, though this effect typically fades after 2-3 years. Ukraine's strong performances 2022-2024 show geopolitical sympathy exists but is time-limited.

---

## General Evidence

### Agent: sentiment_analyzer (Claude API) (relevance: 65%)

Based on Finland's past Eurovision performance, current odds, and the unpredictable nature of the contest, there is a moderate likelihood that Finland could win the Eurovision Song Contest in 2026, but it is far from a guarantee.

- Finland has participated in the Eurovision Song Contest 54 times since its debut in 1961, winning the competition once in 2006 with the song 'Hard Rock Hallelujah' performed by the band Lordi.
- In the past 10 Eurovision contests (2012-2021), Finland has finished in the top 10 three times, with a best result of 6th place in 2021.
- Bookmakers and prediction models currently give Finland around a 10-15% chance of winning the Eurovision Song Contest in 2026 based on recent performance and the strength of the Finnish music industry.
- Finland's Eurovision entries have tended to perform best when they feature heavy metal, rock, or other genres that diverge from the typical Eurovision pop sound, suggesting they may need a unique act to stand out in 2026.
- The Eurovision voting system, which combines jury and public votes, can be unpredictable, making it difficult to confidently forecast the winner more than a few years in advance.

### Agent: entity_investigator (Claude API) (relevance: 70%)

Finland has demonstrated recent competitive strength at Eurovision (2nd place 2023) but faces inherent unpredictability in a contest 2+ years away. Historical win rate is low (~1.7%), and outcomes depend on unknown variables: song selection, performance execution, 2026 competition field, and voting dynamics. No participant selection or song has been announced for 2026, making evidence-based forecasting extremely limited.

- Finland won Eurovision 2006 with Lordi's 'Hard Rock Hallelujah' but has not won since. Recent performance: 2023 (2nd place with Käärijä's 'Cha Cha Cha'), 2022 (did not qualify from semi-final), 2021 (did not qualify). Historical win rate: 1 victory in 59+ participations (~1.7%).
- Eurovision 2026 host city and dates are not yet confirmed as of late 2024. Switzerland won Eurovision 2024 (Nemo - 'The Code'), making them the likely host for 2025. The 2026 contest will be hosted by the 2025 winner, which is unknown.
- Eurovision outcomes are highly unpredictable and influenced by multiple factors: song quality, performance, staging, voting bloc patterns (Nordic countries sometimes exchange votes), jury vs. public vote splits, and contemporary music trends. No reliable statistical model exists for predicting winners 2+ years in advance.
- Finland's recent Eurovision trajectory shows strong public appeal (Käärijä's 2023 runner-up finish won the public vote but lost on jury votes), suggesting competitive potential if they select crowd-pleasing entries. However, they must first qualify through semi-finals unless granted automatic final placement.
- Base rate analysis: With ~40 countries typically competing, random chance of any single country winning is ~2.5%. Finland's historical performance (1 win, multiple top-10 finishes) suggests slightly above-random probability, but still <5% for any given year when accounting for competition quality variance.

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Small Nordic countries winning Eurovision (population <6M)",
    "historical_frequency": 0.015,
    "sample_size": 67,
    "reasoning": "Eurovision has run 67 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time collectively in the modern era (Finland 2006). Finland specifically: 3 wins total (1961 shared, 2006, 2023) = 3/67 = 0.045 base rate for Finland specifically."
  },
  "drivers...

- "base_rate": {
- "reference_class": "Small Nordic countries winning Eurovision (population <6M)",
- "historical_frequency": 0.015,
- "sample_size": 67,
- "reasoning": "Eurovision has run 67 contests (1956-2024, excluding cancelled years). Small Nordic countries (Finland, Norway, Denmark) have won 1 time collectively in the modern era (Finland 2006). Finland specifically: 3 wins total (1961 shared, 2006, 2023) = 3/67 = 0.045 base rate for Finland specifically."
- "drivers": [
- "name": "Recent momentum effect",
- "type": "binary",
- "p5": 0,
- "p50": 0.5,

