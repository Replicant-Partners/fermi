# Evidence Log: will austria win the eurovision song contest in 2026?

**Version:** v1 | **Probability:** 2.9% | **Updated:** 2026-03-03 19:01 UTC

---

## Outside View (Base Rate)

- **Reference class:** Austria's Eurovision performance history (1957-2024)
- **Historical frequency:** 2.9%
- **Sample size:** n=58
- **Source:** macro_forecaster

> Austria has participated in Eurovision 58 times since 1957 and won twice (1966 with Udo Jürgens and 2014 with Conchita Wurst). This gives a historical win rate of 2/58 = 3.4%. However, the modern era (post-1998 with semi-finals) shows 1 win in 26 participations = 3.8%. The reference class of 'mid-sized European countries' shows similar win rates: Switzerland (2 wins/62 entries = 3.2%), Netherlands (5 wins/64 entries = 7.8%), Denmark (3 wins/53 entries = 5.7%).

---

## Song Quality Percentile `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 20 | 50 | 85 | percentile_rank |

> Austria's recent performance shows high variance. Since 2014 win: 0th (2015, didn't qualify), 13th (2016), DNQ (2017), 3rd (2018), DNQ (2019), cancelled (2020), DNQ (2021), DNQ (2022), 15th (2023), DNQ (2024). This shows 5 non-qualifications in last 9 attempts, suggesting median performance around 50th percentile with occasional excellence.

---

## Voting Bloc Strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 30 | 55 | 75 | expected_points_from_neighbors |

> Austria benefits from Central European voting patterns (Germany, Switzerland, Czech Republic typically friendly) but lacks the strong diaspora voting of countries like Greece-Cyprus or Nordic bloc. Historical analysis shows Austria receives moderate regional support but not dominant bloc advantage.

---

## Big 5 Competition `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> The Big 5 (UK, France, Germany, Italy, Spain) auto-qualify and can dominate when they send strong entries. Italy has been particularly strong recently (2021 win, multiple top 5s). This creates additional competition beyond the ~26 finalists.

---

## Qualification Success `binary`

- **Probability:** 50%
- **Impact multiplier:** 1.3x

> Austria must first qualify from semi-finals. Recent record: 5 non-qualifications in last 9 attempts (44% qualification rate vs ~50% baseline). Without reaching the final, win probability is zero. This is a critical gate.

---

## Number of Competitive Entries `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 22 | 26 | 28 | finalists |

> Eurovision finals typically have 26 entries (Big 5 + host + 20 qualifiers). The competitive field size directly affects win probability. In recent years, 5-8 entries are typically considered 'competitive' for the win.

---

## General Evidence

### Agent: macro_forecaster (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Austria's Eurovision performance history (1957-2024)",
    "historical_frequency": 0.029,
    "sample_size": 58,
    "reasoning": "Austria has participated in Eurovision 58 times since 1957 and won twice (1966 with Udo Jürgens and 2014 with Conchita Wurst). This gives a historical win rate of 2/58 = 3.4%. However, the modern era (post-1998 with semi-finals) shows 1 win in 26 participations = 3.8%. The reference class of 'mid-sized European coun...

- "base_rate": {
- "reference_class": "Austria's Eurovision performance history (1957-2024)",
- "historical_frequency": 0.029,
- "sample_size": 58,
- "reasoning": "Austria has participated in Eurovision 58 times since 1957 and won twice (1966 with Udo Jürgens and 2014 with Conchita Wurst). This gives a historical win rate of 2/58 = 3.4%. However, the modern era (post-1998 with semi-finals) shows 1 win in 26 participations = 3.8%. The reference class of 'mid-sized European countries' shows similar win rates: Switzerland (2 wins/62 entries = 3.2%), Netherlands (5 wins/64 entries = 7.8%), Denmark (3 wins/53 entries = 5.7%)."
- "drivers": [
- "name": "Song Quality Percentile",
- "type": "continuous",
- "p5": 20,
- "p50": 50,

