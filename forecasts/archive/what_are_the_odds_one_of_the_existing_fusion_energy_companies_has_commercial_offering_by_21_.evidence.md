# Evidence Log: what are the odds one of the existing fusion energy companies has commercial offering by 21?

**Version:** v1 | **Probability:** 50.0% | **Updated:** 2026-03-05 12:38 UTC

---

## Primary Factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 10 | 50 | 100 |  |

> The main driver — adjust the range to match your question

---

## Secondary Factor `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.7 | 1 | 1.5 | multiplier |

> A modifying factor (1.0 = neutral, >1 = amplifying, <1 = dampening)

---

## Disruption Event `binary`

- **Probability:** 15%
- **Impact multiplier:** 1.5x

> Probability of a disruptive event that amplifies the outcome by 50%

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Major energy technology commercialization timelines (nuclear, solar PV, wind, shale gas)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking at transformative energy technologies from first demonstration to commercial viability: nuclear (1942 demo → 1956 commercial: 14 years), solar PV (1954 → 1980s: ~30 years), wind (1970s → 1990s: ~20 years), shale gas (1980s → 2005: ~20 years). Fusion has had demonstra...

- "base_rate": {
- "reference_class": "Major energy technology commercialization timelines (nuclear, solar PV, wind, shale gas)",
- "historical_frequency": 0.15,
- "sample_size": 20,
- "reasoning": "Looking at transformative energy technologies from first demonstration to commercial viability: nuclear (1942 demo → 1956 commercial: 14 years), solar PV (1954 → 1980s: ~30 years), wind (1970s → 1990s: ~20 years), shale gas (1980s → 2005: ~20 years). Fusion has had demonstration since 1950s but net energy gain only achieved in 2022 (NIF). Given 2021 as target year is in the PAST, and we're now in 2024, this question asks about historical outcome. However, interpreting as '2031' (10 years from 2021), the base rate for a 10-year commercialization window after breakthrough is ~15% based on energy tech history."
- "drivers": [
- "name": "technical_readiness_multiplier",
- "display_name": "Technical Readiness & Recent Breakthroughs",
- "type": "continuous",
- "p5": 0.6,

