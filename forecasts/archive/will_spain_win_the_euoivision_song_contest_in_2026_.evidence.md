# Evidence Log: will spain win the Euoivision song contest in 2026?

**Version:** v28 | **Probability:** 1.5% | **Updated:** 2026-03-05 22:46 UTC

---

## Outside View (Base Rate)

- **Reference class:** Eurovision Song Contest winners (1956-2024)
- **Historical frequency:** 1.5%
- **Sample size:** n=68
- **Source:** macro_forecaster

> Spain has won Eurovision 2 times (1968, 1969) out of 68 contests. However, the modern era (post-1999 with current voting system) shows 0 wins in 25 contests. Using the full historical rate of 2/68 = 0.0294, but adjusting for modern era performance gives ~0.015 as a reasonable base rate for any given year.

---

## recent_performance_trend `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.6 | 0.85 | 1.2 | multiplier |

> Spain's recent Eurovision results: 2024 (22nd), 2023 (3rd - best in decades), 2022 (3rd), 2021 (24th), 2019 (22nd). The 2022-2023 resurgence suggests improved competitiveness, but inconsistency remains. Median suggests 15% reduction from base, with upside if trend continues.

### Assigned Agents

- **entity_investigator** (schedule: once)
  - Query: _do an analysis of artist strength for spaiin vs top 5 _

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 45%)

Spain is a mid-tier music market (#10 globally) with strong cultural/linguistic advantages but weaker artist development infrastructure compared to top 5 markets (US, Japan, UK, Germany, France). Spanish-language artists achieve global success, but often through Latin American markets rather than Spain's domestic system.

**Key findings:**

- Spain ranks 10th globally in recorded music revenue ($467M in 2022 per IFPI), behind US, Japan, UK, Germany, France, China, South Korea, Canada, and Australia
- Spanish-language music has significant global reach: Latin music grew 15% globally in 2022, with Spanish artists like Rosalía, Bad Bunny (Puerto Rico), and Bizarrap achieving billions of streams
- Spain's domestic market is dominated by Latin/reggaeton genres (40%+ of top charts), but lacks the artist development infrastructure of top 5 markets
- Spotify data shows Spain has ~20M users but artist-per-capita success rate is lower than UK, US, or South Korea in terms of artists with 1M+ monthly listeners
- Spain's strength is in language/cultural reach (580M Spanish speakers globally) rather than domestic market size or artist development systems

_Collected: 2026-03-05_

---

## artist_selection_quality `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.7 | 1 | 1.5 | multiplier |

> Unknown until early 2026. Spain uses Benidorm Fest (since 2022) which has produced competitive entries. Quality variance is high - could select a winner-caliber act (1.5x) or mediocre entry (0.7x). Neutral median until selection known.

---

## voting_bloc_dynamics `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.8 | 0.95 | 1.1 | multiplier |

> Spain lacks strong voting blocs compared to Nordic, Balkan, or Eastern European countries. Limited diaspora voting advantage. Historical data shows Spain receives moderate but not exceptional neighbor votes (Portugal, France, Andorra when participating). Slight disadvantage.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Analyze Spain's Eurovision organizational structure and selection process changes since 2020. What s_

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

RTVE's reforms to Spain's Eurovision selection process, leadership changes, and increased investment in artist development and production quality have led to a marked improvement in Spain's Eurovision results in 2022 and 2023. The introduction of the Benidorm Fest, new leadership, and collaboration with top-tier international talent have all contributed to this turnaround.

**Key findings:**

- RTVE introduced the Benidorm Fest in 2022 as a new national selection process to choose Spain's Eurovision entry, replacing the previous internal selection method. This allowed for more public engagement and transparency in the selection.
- RTVE appointed Mikel Arriola as the new Head of Delegation for Eurovision in 2021, bringing in new leadership and a renewed focus on Spain's Eurovision strategy.
- Spain significantly increased investment in artist development, staging, and collaboration with international producers and choreographers for their 2022 and 2023 Eurovision entries. This included working with renowned producers like Leroy Sánchez and choreographers like Kyle Hanagami.
- The 2022 and 2023 Spanish entries, 'SloMo' by Chanel and 'Eaea' by Blanca Paloma, featured high-quality staging, choreography, and vocal performances that resonated with international audiences, leading to Spain's best Eurovision results since the 1990s.

_Collected: 2026-03-05_

---

## host_country_advantage `binary`

- **Probability:** 5%
- **Impact multiplier:** 1.3x

> Spain is unlikely to host 2026 (would need to win 2025 or have 2025 winner decline). If they did host, historical data shows modest advantage. Only 5% chance Spain hosts, but 30% boost if they do.

---

## competitive_field_strength `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.85 | 1 | 1.15 | multiplier |

> Unknown composition of 2026 field. Strong competitors (Sweden, Italy, Ukraine, UK with recent investments) will participate. If traditional powerhouses have weak years, Spain's chances improve. Neutral median with slight upside variance.

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 75%)

Spain's Eurovision prospects for 2026 show modest probability based on historical performance (2.9% win rate) tempered by modern era struggles (0 wins since 1969). Recent competitive entries in 2022-2023 suggest improved capability, but 2024's poor showing indicates inconsistency. Structural disadvantages in voting patterns and unknown 2026 entry quality create significant uncertainty. Base probability around 1.5% with multipliers for performance trends, selection quality, and competitive dynami...

- Spain has a 2.9% historical win rate (2/68 contests), but 0% in the modern era (2000-2024)
- Recent resurgence: 3rd place in both 2022 and 2023, but dropped to 22nd in 2024, showing high variance
- Spain lacks strong voting bloc advantages compared to Nordic, Balkan, or Eastern European countries
- Benidorm Fest selection process (since 2022) can produce competitive entries but quality varies significantly
- The 2026 artist and song are unknown, representing the largest source of uncertainty in the forecast

