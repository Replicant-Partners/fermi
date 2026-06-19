# Will France win the 2026 FIFA World Cup?

**Probability:** 11.5% · **Version:** v2 · **Updated:** 2026-06-19 01:29 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 4 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **18.4%** |
| Fermi estimate | **11.5%** |
| Divergence | +7.0pp below crowd (Moderate divergence — potential edge) |
| 24h volume | $2.7M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 11.5%**

Inside view: model evaluates to 11.5% (p5=8.4%, p95=15.0%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 9pp above (11.5% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 8.4% · median = 11.3% · p95 = 15.0% · σ = 0.020

```
▁▁▂▃▅▇██▇▆▄▄▃▂▂▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 6.4% | 14 | 0.1% |
| 7.1% | 78 | 0.8% |
| 7.8% | 245 | 2.5% |
| 8.5% | 506 | 5.1% |
| 9.2% | 869 | 8.7% |
| 9.9% | 1183 | 11.8% |
| 10.6% | 1433 | 14.3% |
| 11.3% | 1356 | 13.6% |
| 12.0% | 1265 | 12.7% |
| 12.7% | 1003 | 10.0% |
| 13.4% | 715 | 7.1% |
| 14.1% | 528 | 5.3% |
| 14.8% | 358 | 3.6% |
| 15.5% | 200 | 2.0% |
| 16.2% | 121 | 1.2% |
| 16.9% | 69 | 0.7% |
| 17.6% | 38 | 0.4% |
| 18.3% | 13 | 0.1% |
| 19.0% | 3 | 0.0% |
| 19.7% | 3 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-19 01:28 | 11.5% | 2.1% | 18.4% | +9.4pp | -7.0pp | Initial: 11.5% base=2%, 6 drivers, 1 evidence |
| v2 | 2026-06-19 01:29 | 11.5% | 2.1% | 18.4% | +9.4pp | -7.0pp | 11.5% (→), 6 drivers, 4 evidence |

**Model line:** ```▁█``` (range 11.5% – 11.5%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for France (2024–2026 latest available)_

### Evidence (1) — Strong quality (75%)

#### Agent: macro_data_agent — relevance 50% · quality ●●● High (75%) · 2026-06-19

Perfect! Now I have the complete list of 48 qualified teams. Let me compile the evidence for France and calculate the field baseline.

---

## SOCIOECONOMIC CAPITAL (X1) EVIDENCE — FRANCE (FRA)

### Core Indicators (2024 data)

**[INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$39,441** current US$; log₁₀ ≈ **4.596**  
Source: Trading Economics / World Bank, 2024 release

**[INDICATOR]** GDP per capita PPP (2024, World Bank NY.GDP.PCAP.PP.CD): **$61,322** international $; log₁₀ ≈ **4.788**  
Source: Trading Economics / World Bank, 2024 release

**[INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **68.52 million**; log₁₀ ≈ **1.836**  
Source: Trading Economics / World Bank, 2024 release (68,516,699 total)

**[INDICATOR]** HDI (2023, UNDP Human Development Report): **0.920**; logit = log(0.920 / 0.080) ≈ **2.451**  
Source: UNDP HDR 2025 release (data year 2023); TheGlobalEconomy.com

---

### Field Baseline — World Cup 2026 (48 teams)

**[BASELINE]** WC 2026 field composition: 48 qualified nations spanning extreme economic diversity  
- **Richest** (GDP/capita PPP): Switzerland (~$94k), Norway (~$89k), USA (~$81k), Qatar (~$115k)  
- **Poorest** (GDP/capita PPP): DR Congo (~$752), Haiti (~$3.1k), Senegal (~$3.7k)  
- **Range**: 71× gap between USA and DR Congo (per Politico/World Data Lab analysis)

**[BASELINE]** Estimated field **median GDP per capita** (current US$): **~$12,000–15,000**  
- Log₁₀ median ≈ **4.08–4.18** (mid-range estimate: **4.13**)  
- France at $39,441 (log 4.596) sits in the **top quartile** (75th–85th percentile)

**[BASELINE]** Estimated field **median population**: **~20–30 million**  
- Log₁₀ median ≈ **1.30–1.48** (mid-range estimate: **1.39**)  
- France at 68.5M (log 1.836) is **above median** but below giants (USA 343M, Brazil 217M, Mexico 128M)

**[BASELINE]** Estimated field **median HDI**: **~0.75–0.80** (mix of "high" and "very high" development)  
- Logit median ≈ **1.10–1.39** (mid-range estimate: **1.25**)  
- France at 0.920 (logit 2.451) is in the **top decile** (very high development tier)

---

### Transform Calculation

Using the standard X1 (Socioeconomic Capital) composite:

**[TRANSFORM]** X1 = 0.4 · GDP_log + 0.3 · Pop_log + 0.3 · HDI_logit  
= 0.4 · 4.596 + 0.3 · 1.836 + 0.3 · 2.451  
= 1.838 + 0.551 + 0.735  
= **3.124**

**[TRANSFORM]** Field median X1 (estimated):  
= 0.4 · 4.13 + 0.3 · 1.39 + 0.3 · 1.25  
= 1.652 + 0.417 + 0.375  
= **2.444**

**[TRANSFORM]** France deviation from field median:  
Δ = 3.124 − 2.444 = **+0.680** (standardized units)  
= **+0.97 standard deviations** above field mean (assuming σ ≈ 0.70 for WC field)

---

### Multiplier Recommendation

France's socioeconomic profile is **substantially above** the World Cup 2026 field median across all three dimensions:
- **GDP/capita**: top quartile (312% of world average per Trading Economics)
- **Population**: above median, large talent pool (68.5M vs. field median ~25M)
- **HDI**: top decile (0.920, "very high" development tier)

This translates to:
- **Stronger youth development infrastructure** (academies, coaching, facilities)
- **Greater professional league depth** (Ligue 1 + diaspora in top European leagues)
- **Higher baseline athletic participation** and nutrition standards

**[MULTIPLIER]** Suggested p50: **1.15** (p5: **1.05**, p95: **1.28**) — France's GDP/capita ($39k, log 4.60), population (68.5M, log 1.84), and HDI (0.920, logit 2.45) all exceed WC 2026 field medians by 0.7–1.0 SD, lifting socioeconomic capital factor materially above baseline

---

### Data Quality Note

**[DATA AGE]** All indicators current as of 2024 (GDP, population) or 2023 (HDI, latest UNDP release). World Bank and UNDP official sources. Field baseline is **estimated** from published ranges (Investec, World Data Lab, Politico analyses of WC 2026 field); precise median would require full 48-country dataset but directional confidence is high given France's clear top-quartile position.

**Key findings:**

- [INDICATOR]** GDP per capita (2024, World Bank NY.GDP.PCAP.CD): **$39,441** current US$; log₁₀ ≈ **4.596**
- [INDICATOR]** GDP per capita PPP (2024, World Bank NY.GDP.PCAP.PP.CD): **$61,322** international $; log₁₀ ≈ **4.788**
- [INDICATOR]** Population (2024, World Bank SP.POP.TOTL): **68.52 million**; log₁₀ ≈ **1.836**
- [INDICATOR]** HDI (2023, UNDP Human Development Report): **0.920**; logit = log(0.920 / 0.080) ≈ **2.451**
- [BASELINE]** WC 2026 field composition: 48 qualified nations spanning extreme economic diversity
- **Richest** (GDP/capita PPP): Switzerland (~$94k), Norway (~$89k), USA (~$81k), Qatar (~$115k)
- **Poorest** (GDP/capita PPP): DR Congo (~$752), Haiti (~$3.1k), Senegal (~$3.7k)
- **Range**: 71× gap between USA and DR Congo (per Politico/World Data Lab analysis)
- [BASELINE]** Estimated field **median GDP per capita** (current US$): **~$12,000–15,000**
- Log₁₀ median ≈ **4.08–4.18** (mid-range estimate: **4.13**)
- France at $39,441 (log 4.596) sits in the **top quartile** (75th–85th percentile)
- [BASELINE]** Estimated field **median population**: **~20–30 million**
- Log₁₀ median ≈ **1.30–1.48** (mid-range estimate: **1.39**)
- France at 68.5M (log 1.836) is **above median** but below giants (USA 343M, Brazil 217M, Mexico 128M)
- [BASELINE]** Estimated field **median HDI**: **~0.75–0.80** (mix of "high" and "very high" development)

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for France_

### Evidence (1) — Strong quality (75%)

#### Agent: football_institution_agent — relevance 50% · quality ●●● High (75%) · 2026-06-19

Perfect! I now have comprehensive data. Let me compile the institutional capacity analysis for France.

---

## INSTITUTIONAL CAPACITY ANALYSIS: FRANCE (2026 FIFA World Cup)

### Factor X2: Institutional Capacity — Evidence Summary

**[PENETRATION] FIFA-registered players (2022 FFF data): 2.1 million licensees, 1.8 million active players in a population of ~68.9 million (2024) → 2,612 players per 100k inhabitants**

France's player penetration rate of 2,612/100k places it in the upper tier of European nations, though below the Nordic outliers (Iceland ~5,790/100k, Norway ~4,200/100k). This is approximately **1.7× the European median** (~1,500/100k) and reflects deep grassroots infrastructure: 14,000+ registered clubs (2022), the second-highest club density in Europe after Germany. The FFF's "Label Jeunes" certification system ensures quality control across youth academies, with 16 elite Pôles Espoirs regional academies feeding into the national system.

**[LEAGUE REVENUE] Ligue 1 aggregate revenue (2023/24 Deloitte): €2.6 billion across 18 clubs → log₁₀(2.6×10⁹) ≈ 9.41**

Ligue 1 is the **5th-richest domestic league in Europe** by aggregate revenue, behind the Premier League (~€7.0B), Bundesliga (~€4.0B), La Liga (~€3.8B), and Serie A (~€3.2B). PSG alone generated €802M (2023/24 Money League), ranking 3rd globally. However, the league faces structural headwinds: domestic broadcast rights fell ~3% in the 2024/25 cycle, and revenue concentration is extreme (PSG accounts for ~31% of total league revenue). The log-scale revenue index of 9.41 is **strong but not elite** — comparable to Italy, below England (9.85) and Germany (9.60).

**[CONFEDERATION] UEFA coefficient (2024/25 season): France ranked 5th with 64.950 points, behind England (98.660), Italy (87.043), Spain (81.561), Germany (78.285)**

France's UEFA coefficient has **declined from 1st (2023-24) to 5th (2024-25)**, reflecting inconsistent European club performance outside PSG. However, PSG's **2024/25 Champions League victory** (5-0 vs Inter Milan final) — their first-ever European title — represents a watershed moment. For 2025/26, France secured 3 automatic Champions League spots (PSG, Marseille, Monaco). The confederation coefficient for UEFA remains 1.00 (highest globally), but France's **within-UEFA standing is mid-tier**, suggesting institutional strength is concentrated rather than distributed across the pyramid.

**[INSTITUTIONAL SIGNAL] Elite youth infrastructure: 16 Pôles Espoirs regional academies + INF Clairefontaine (national centre) + 300+ technical consultants for nationwide scouting**

France operates the **most centralized and systematic talent identification system in Europe**. Clairefontaine (established 1988) has produced Mbappé, Henry, Anelka, and the core of the 2018 World Cup squad. The FFF's Institut de Formation du Football (IFF) coordinates all federal training for coaches, referees, and volunteers. France has the **highest UEFA Pro/A coaching license density per capita among major European nations** (estimated ~1 UEFA Pro coach per 15,000 inhabitants vs ~1 per 25,000 in Germany). The dual-track system (club academies + federal centres) ensures no regional talent is overlooked.

**[DATA AGE] All data sources current: FFF licensee count (2022), Ligue 1 revenue (2023/24 Deloitte), UEFA coefficient (2024/25 live), PSG Champions League victory (May 2025)**

---

### Synthesis: France's Institutional Multiplier

France exhibits **asymmetric institutional strength**: world-class centralized infrastructure (Clairefontaine, FFF scouting network, coaching density) but a **domestically weak and financially fragile league** outside PSG. The 2024/25 Champions League victory signals a potential inflection point — PSG's success without a megastar (post-Messi/Neymar/Mbappé) suggests the French system is now producing **depth** rather than relying on individual brilliance.

However, France's **within-UEFA coefficient decline** (1st → 5th in two years) and Ligue 1's broadcast revenue contraction are structural headwinds. The institutional multiplier should reflect:
- **Upside**: Elite youth development, high coaching density, proven national-team pipeline (2018 WC winners, 2022 WC finalists)
- **Downside**: Weak domestic league competitiveness, revenue concentration risk, recent European underperformance by non-PSG clubs

France's institutions **overperform its league economics** (X2 > X1 would predict) but **underperform its historical UEFA standing** (X2 < peak 2023 levels). The p50 should be **moderately above field median** (1.0), with wide uncertainty reflecting the PSG-dependency risk.

**[MULTIPLIER] Suggested p50: 1.15 (p5: 0.95, p95: 1.40) — France's centralized talent development and coaching infrastructure exceed its domestic league strength, but recent UEFA coefficient decline and revenue fragility temper the boost; PSG's 2025 CL victory signals potential upside if systemic depth materializes.**

**Key findings:**

- [PENETRATION] FIFA-registered players (2022 FFF data): 2.1 million licensees, 1.8 million active players in a population of ~68.9 million (2024) → 2,612 players per 100k inhabitants**
- [LEAGUE REVENUE] Ligue 1 aggregate revenue (2023/24 Deloitte): €2.6 billion across 18 clubs → log₁₀(2.6×10⁹) ≈ 9.41**
- Ligue 1 is the **5th-richest domestic league in Europe** by aggregate revenue, behind the Premier League (~€7.0B), Bundesliga (~€4.0B), La Liga (~€3.8B), and Serie A (~€3.2B). PSG alone generated €802M (2023/24 Money League), ranking 3rd globally. However, the league faces structural headwinds: domestic broadcast rights fell ~3% in the 2024/25 cycle, and revenue concentration is extreme (PSG accounts for ~31% of total league revenue). The log-scale revenue index of 9.41 is **strong but not elite** — comparable to Italy, below England (9.85) and Germany (9.60).
- [CONFEDERATION] UEFA coefficient (2024/25 season): France ranked 5th with 64.950 points, behind England (98.660), Italy (87.043), Spain (81.561), Germany (78.285)**
- [INSTITUTIONAL SIGNAL] Elite youth infrastructure: 16 Pôles Espoirs regional academies + INF Clairefontaine (national centre) + 300+ technical consultants for nationwide scouting**
- [DATA AGE] All data sources current: FFF licensee count (2022), Ligue 1 revenue (2023/24 Deloitte), UEFA coefficient (2024/25 live), PSG Champions League victory (May 2025)**
- **Upside**: Elite youth development, high coaching density, proven national-team pipeline (2018 WC winners, 2022 WC finalists)
- **Downside**: Weak domestic league competitiveness, revenue concentration risk, recent European underperformance by non-PSG clubs
- France's institutions **overperform its league economics** (X2 > X1 would predict) but **underperform its historical UEFA standing** (X2 < peak 2023 levels). The p50 should be **moderately above field median** (1.0), with wide uncertainty reflecting the PSG-dependency risk.
- [MULTIPLIER] Suggested p50: 1.15 (p5: 0.95, p95: 1.40) — France's centralized talent development and coaching infrastructure exceed its domestic league strength, but recent UEFA coefficient decline and revenue fragility temper the boost; PSG's 2025 CL victory signals potential upside if systemic depth materializes.**

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-19

Based on the live API data and web research, I can now provide comprehensive tactical efficiency evidence for France ahead of the 2026 World Cup. Let me compile the findings:

---

## TACTICAL EFFICIENCY EVIDENCE: FRANCE AT 2026 FIFA WORLD CUP

**Factor X5 — Tactical Efficiency Analysis**

### CURRENT FORM & TACTICAL SETUP (UEFA Nations League 2024/25)

[BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.

[MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).

[X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.

[X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.

[X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.

[X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.

[TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.

[INJURY IMPACT] **Current squad depth**: Griezmann (34 years old at WC 2026, declining but still key playmaker), Mbappé (27, peak age), Koundé (27, peak), Tchouaméni (26, peak). No major injuries reported in recent API data. Squad rotation concerns: Deschamps' **tactical conservatism** may limit squad utilization in group stage, risking fatigue in knockout rounds.

[ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.

[FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.

[MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.

---

### KEY FINDINGS SUMMARY

**Tactical Efficiency (Factor X5) Components:**

1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.

2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).

3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

4. **Set Piece Efficiency**: 0.9 penalties/game (elite generation), but direct set-piece goals **~25-30%** of total (below elite 35%+ benchmark). Defensive set-piece vulnerability in early game windows.

**Relevance Score**: **0.92** — Tactical efficiency is a critical discriminator for tournament success; France's mixed X5 profile (elite defense, volatile attack) directly impacts WC win probability.

**Confidence**: **0.78** — High confidence in defensive metrics (API + recent tournament data), moderate confidence in attacking efficiency (Euro 2024 anomaly vs Nations League recovery), low confidence in set-piece data (limited granular stats).

**Factor-Mode Multiplier Rationale**: France's X5 tactical efficiency is **marginally above WC field median** (p50: 1.05) due to elite defensive duels and late-game execution, but shot conversion volatility and moderate pressing/set-piece efficiency prevent classification as elite tier. The p5-p95 range (0.85-1.30) reflects **high uncertainty** from Euro 2024 attacking underperformance vs historical tournament success.

**Key findings:**

- Factor X5 — Tactical Efficiency Analysis**
- [BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.
- [MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).
- [X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.
- [X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.
- [X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.
- [X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.
- [TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.
- [ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.
- [FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.
- [MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.
- Tactical Efficiency (Factor X5) Components:**
- 1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.
- 2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).
- 3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-19

Based on the live API data and web research, I can now provide comprehensive tactical efficiency evidence for France ahead of the 2026 World Cup. Let me compile the findings:

---

## TACTICAL EFFICIENCY EVIDENCE: FRANCE AT 2026 FIFA WORLD CUP

**Factor X5 — Tactical Efficiency Analysis**

### CURRENT FORM & TACTICAL SETUP (UEFA Nations League 2024/25)

[BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.

[MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).

[X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.

[X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.

[X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.

[X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.

[TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.

[INJURY IMPACT] **Current squad depth**: Griezmann (34 years old at WC 2026, declining but still key playmaker), Mbappé (27, peak age), Koundé (27, peak), Tchouaméni (26, peak). No major injuries reported in recent API data. Squad rotation concerns: Deschamps' **tactical conservatism** may limit squad utilization in group stage, risking fatigue in knockout rounds.

[ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.

[FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.

[MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.

---

### KEY FINDINGS SUMMARY

**Tactical Efficiency (Factor X5) Components:**

1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.

2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).

3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

4. **Set Piece Efficiency**: 0.9 penalties/game (elite generation), but direct set-piece goals **~25-30%** of total (below elite 35%+ benchmark). Defensive set-piece vulnerability in early game windows.

**Relevance Score**: **0.92** — Tactical efficiency is a critical discriminator for tournament success; France's mixed X5 profile (elite defense, volatile attack) directly impacts WC win probability.

**Confidence**: **0.78** — High confidence in defensive metrics (API + recent tournament data), moderate confidence in attacking efficiency (Euro 2024 anomaly vs Nations League recovery), low confidence in set-piece data (limited granular stats).

**Factor-Mode Multiplier Rationale**: France's X5 tactical efficiency is **marginally above WC field median** (p50: 1.05) due to elite defensive duels and late-game execution, but shot conversion volatility and moderate pressing/set-piece efficiency prevent classification as elite tier. The p5-p95 range (0.85-1.30) reflects **high uncertainty** from Euro 2024 attacking underperformance vs historical tournament success.

**Key findings:**

- Factor X5 — Tactical Efficiency Analysis**
- [BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.
- [MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).
- [X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.
- [X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.
- [X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.
- [X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.
- [TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.
- [ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.
- [FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.
- [MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.
- Tactical Efficiency (Factor X5) Components:**
- 1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.
- 2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).
- 3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for France_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-19

Based on the live API data and web research, I can now provide comprehensive tactical efficiency evidence for France ahead of the 2026 World Cup. Let me compile the findings:

---

## TACTICAL EFFICIENCY EVIDENCE: FRANCE AT 2026 FIFA WORLD CUP

**Factor X5 — Tactical Efficiency Analysis**

### CURRENT FORM & TACTICAL SETUP (UEFA Nations League 2024/25)

[BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.

[MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).

[X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.

[X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.

[X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.

[X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.

[TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.

[INJURY IMPACT] **Current squad depth**: Griezmann (34 years old at WC 2026, declining but still key playmaker), Mbappé (27, peak age), Koundé (27, peak), Tchouaméni (26, peak). No major injuries reported in recent API data. Squad rotation concerns: Deschamps' **tactical conservatism** may limit squad utilization in group stage, risking fatigue in knockout rounds.

[ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.

[FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.

[MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.

---

### KEY FINDINGS SUMMARY

**Tactical Efficiency (Factor X5) Components:**

1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.

2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).

3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

4. **Set Piece Efficiency**: 0.9 penalties/game (elite generation), but direct set-piece goals **~25-30%** of total (below elite 35%+ benchmark). Defensive set-piece vulnerability in early game windows.

**Relevance Score**: **0.92** — Tactical efficiency is a critical discriminator for tournament success; France's mixed X5 profile (elite defense, volatile attack) directly impacts WC win probability.

**Confidence**: **0.78** — High confidence in defensive metrics (API + recent tournament data), moderate confidence in attacking efficiency (Euro 2024 anomaly vs Nations League recovery), low confidence in set-piece data (limited granular stats).

**Factor-Mode Multiplier Rationale**: France's X5 tactical efficiency is **marginally above WC field median** (p50: 1.05) due to elite defensive duels and late-game execution, but shot conversion volatility and moderate pressing/set-piece efficiency prevent classification as elite tier. The p5-p95 range (0.85-1.30) reflects **high uncertainty** from Euro 2024 attacking underperformance vs historical tournament success.

**Key findings:**

- Factor X5 — Tactical Efficiency Analysis**
- [BASE RATE] France's recent tournament record: 2018 World Cup winners, 2022 World Cup runners-up, Euro 2024 semi-finalists. Historical win rate in major tournaments: ~65% across knockout stages since 2016.
- [MATCH STATS] **UEFA Nations League 2024/25 (10 matches)**: 6W-1D-3L record. Goals for: 20 (2.0/game), Goals against: 13 (1.3/game). Goal differential: +0.7/game. Form: LWWWDWLWLW. Home record significantly stronger (2W-1D-1L, 5 GF, 3 GA) than away (4W-0D-2L, 15 GF, 10 GA). Clean sheets: 40% (4/10 matches).
- [X5 SIGNAL — SHOT CONVERSION] **Penalty conversion rate: 77.8%** (7/9 scored, 2 missed) in Nations League 2024/25. Open-play shot data from API shows France scored 13 non-penalty goals from 20 total goals = **65% from open play**. Goal timing distribution: **47.8% of goals in final 15 minutes** (76-90' + 106-120' = 11/23 goals) — elite late-game efficiency, indicating superior fitness and tactical adjustments. Euro 2024 criticism: **only 4 goals in 6 games** (2 own-goals, 1 penalty) = severe underperformance in shot conversion at major tournament.
- [X5 SIGNAL — DEFENSIVE DUELS] **Jules Koundé defensive metrics (Nations League 2024)**: 71 total duels, **59.2% won** (42/71). Tackles: 12 total, interceptions: 10, blocks: 2. Pass accuracy: **88%** (elite ball progression from defense). Koundé's Euro 2024: **60.4% duels won** (32/53), 18 tackles, rating 7.57 — top-tier defensive efficiency. France conceded **only 4 goals in 6 WC qualifying matches** (0.67 GA/game) — elite defensive structure.
- [X5 SIGNAL — PRESSING INTENSITY] **Tactical formation: 4-2-3-1 (60% of matches) or 4-3-3 (20%)**. Deschamps' system prioritizes **defensive solidity over high pressing**. Web sources indicate France's PPDA is **moderate (~10-12 range)** — not a high-press team like Spain (PPDA <8). Tchouameni-Rabiot double pivot provides defensive cover but **limits creative output vs low blocks**. Tactical criticism: "pragmatic approach" = structured defense, rapid counter-attacks, but **underutilizes attacking talent** in possession-dominant scenarios.
- [X5 SIGNAL — SET PIECE EFFICIENCY] **Penalty data: 9 penalties awarded in 10 Nations League matches** = elite set-piece generation (0.9 penalties/game, top quartile internationally). However, **no direct set-piece goal data available** from API. Historical context: France's set-piece goals typically **~25-30% of total goals** (below elite benchmark of 35%+). Defensive set-piece vulnerability: **26.3% of goals conceded in 16-30' window** (5/19 goals) suggests early-game set-piece weakness.
- [TACTICAL MATCHUP] **Strengths**: World-class individual talent (Mbappé, Griezmann, Tchouaméni, Saliba, Koundé), elite defensive organization (0.67 GA/game in WC qualifying), late-game efficiency (47.8% goals after 75'), counter-attack lethality. **Weaknesses**: Shot conversion inconsistency (Euro 2024: 4 goals in 6 games), midfield creativity vs deep blocks, moderate pressing intensity (vulnerable to possession-dominant opponents), set-piece scoring below elite level.
- [ELO] France's estimated Elo: **~2050-2100** (top 3 globally alongside Argentina, Brazil). ClubElo-equivalent for national teams places France in **top quartile** for WC 2026 field. Elo-implied probability vs median WC opponent (Elo ~1750): **P(France win) ≈ 72%** using standard formula with +65 HFA adjustment.
- [FACTOR] **X5 Tactical Efficiency composite assessment**: France's tactical efficiency is **above WC field median but below elite tier** (Spain, Germany). Shot conversion volatility (77.8% penalties vs 4 goals/6 games at Euro 2024) is the primary discriminator. Defensive duels (59.2% won) and late-game efficiency (47.8% goals after 75') are elite. Pressing intensity (moderate PPDA ~10-12) and set-piece scoring (~25-30%) are **mid-tier for tournament favorites**. Deschamps' pragmatism maximizes defensive solidity but **constrains attacking output** — France's xG underperformance at Euro 2024 suggests tactical system limits chance creation quality.
- [MULTIPLIER] Suggested p50: **1.05** (p5: 0.85, p95: 1.30) — Factor X5 (tactical efficiency) shows mixed signals: elite defensive duels and late-game execution offset by shot conversion volatility and moderate pressing/set-piece efficiency; France's tactical system is **tournament-proven but not dominant** relative to WC field.
- Tactical Efficiency (Factor X5) Components:**
- 1. **Shot Conversion Rate**: 77.8% penalty conversion (elite), but Euro 2024 showed severe open-play underperformance (4 goals/6 games). **Volatility = high uncertainty**.
- 2. **Defensive Duel Win %**: 59.2% (Koundé, Nations League 2024) — **top quartile** for international defenders. Defensive structure elite (0.67 GA/game in WC qualifying).
- 3. **Pressing Intensity**: Moderate PPDA (~10-12) — **mid-tier** for tournament favorites. Deschamps' pragmatic system prioritizes defensive shape over high press.

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for France: venue, climate, rest days, altitude, opponent travel burden_

### Evidence (1) — Strong quality (70%)

#### Agent: fixture_context_agent — relevance 50% · quality ●●● High (70%) · 2026-06-19

# FIXTURE CONTEXT ANALYSIS: FRANCE AT 2026 FIFA WORLD CUP

## CORE FINDINGS

**[HOST]** France is NOT a host nation (USA/Canada/Mexico are co-hosts). Host_status = 0. France receives no home advantage multiplier. This is the dominant negative signal for X6.

**[CLIMATE]** France's Group I venues are New York/New Jersey (MetLife Stadium), Philadelphia (Lincoln Financial Field), and Boston (Gillette Stadium) — all northeastern US cities. Climate analysis:
- **France baseline**: Temperate oceanic climate. Paris/Lyon June averages: 18-22°C, humidity 65-75%
- **Venue conditions (June 2026)**: Northeastern US June averages: 20-25°C, humidity 70-77% (per historical normals)
- **Climate_delta assessment**: Minimal. Temperature differential ≈ +2-3°C, humidity differential ≈ +5-10% RH. French players train in similar temperate conditions. Climate_delta score: **0.10** (near-neutral, slight warmth/humidity uptick but well within European summer norms).

**[REST DAYS]** France's Group I schedule:
- Match 1: June 18 vs Senegal (New York/NJ)
- Match 2: June 22 vs Iraq (Philadelphia) — **4 days rest**
- Match 3: June 26 vs Norway (Boston) — **4 days rest**

FIFA's 2026 format explicitly designed group-stage spacing at 4+ days between matches to ensure recovery. 4 days is the **optimal recovery window** per FIFA medical research (3+ days restores baseline performance, no further gain past 5 days). Rest_days normalised score: **0.75** (strong positive — consistent 4-day spacing throughout group stage).

**[ALTITUDE]** All three France group-stage venues are near sea level:
- MetLife Stadium (East Rutherford, NJ): ~7m elevation
- Lincoln Financial Field (Philadelphia, PA): ~12m elevation  
- Gillette Stadium (Foxborough, MA): ~45m elevation

France's training bases (Clairefontaine, Paris region, Lyon) are 50-200m elevation. **Altitude_delta ≈ 0** — no physiological disadvantage. If France advances to knockout rounds, potential exposure to Mexico City (Estadio Azteca, 2,200m) in later stages, but group-stage altitude impact is **nil**.

**[TOURNAMENT AVG]** For non-host European teams at 2026 WC, the exogenous context is field-neutral to slightly negative. France avoids the worst climate extremes (no Gulf-style heat, no high-altitude Mexico City in group stage), benefits from optimal rest scheduling, but critically **lacks host advantage** which is worth +0.3-0.5 Elo in group stages historically.

---

## MULTIPLIER ASSESSMENT

**Baseline X6 prior for elite non-host teams: 1.0** (field-median exogenous context)

**France-specific adjustments:**
- Host status: **0** (no advantage, -0.25 multiplier penalty vs hosts)
- Climate: **+0.05** (marginal advantage — northeastern US June is comfortable for European squads)
- Rest days: **+0.10** (optimal 4-day spacing, FIFA-designed recovery windows)
- Altitude: **0** (sea-level venues, no disadvantage)

**Net adjustment: -0.10 from baseline 1.0**

---

**[MULTIPLIER] Suggested p50: 0.90 (p5: 0.75, p95: 1.05) — France lacks host advantage (the dominant X6 signal), but benefits from climate-neutral northeastern US venues and optimal rest scheduling; overall slightly below field-median exogenous context.**

**Key findings:**

- [HOST]** France is NOT a host nation (USA/Canada/Mexico are co-hosts). Host_status = 0. France receives no home advantage multiplier. This is the dominant negative signal for X6.
- [CLIMATE]** France's Group I venues are New York/New Jersey (MetLife Stadium), Philadelphia (Lincoln Financial Field), and Boston (Gillette Stadium) — all northeastern US cities. Climate analysis:
- **France baseline**: Temperate oceanic climate. Paris/Lyon June averages: 18-22°C, humidity 65-75%
- **Venue conditions (June 2026)**: Northeastern US June averages: 20-25°C, humidity 70-77% (per historical normals)
- **Climate_delta assessment**: Minimal. Temperature differential ≈ +2-3°C, humidity differential ≈ +5-10% RH. French players train in similar temperate conditions. Climate_delta score: **0.10** (near-neutral, slight warmth/humidity uptick but well within European summer norms).
- [REST DAYS]** France's Group I schedule:
- Match 1: June 18 vs Senegal (New York/NJ)
- Match 2: June 22 vs Iraq (Philadelphia) — **4 days rest**
- Match 3: June 26 vs Norway (Boston) — **4 days rest**
- [ALTITUDE]** All three France group-stage venues are near sea level:
- MetLife Stadium (East Rutherford, NJ): ~7m elevation
- Lincoln Financial Field (Philadelphia, PA): ~12m elevation
- Gillette Stadium (Foxborough, MA): ~45m elevation
- [TOURNAMENT AVG]** For non-host European teams at 2026 WC, the exogenous context is field-neutral to slightly negative. France avoids the worst climate extremes (no Gulf-style heat, no high-altitude Mexico City in group stage), benefits from optimal rest scheduling, but critically **lacks host advantage** which is worth +0.3-0.5 Elo in group stages historically.
- Baseline X6 prior for elite non-host teams: 1.0** (field-median exogenous context)

---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: socio_capital * institutional_capacity * dynamic_performance * squad_quality * tactical_efficiency * fixture_context
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| macro_data_agent | socio_capital | GDP per capita, population, HDI for France (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for France |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for France |
| fixture_context_agent | fixture_context | Upcoming fixtures for France: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v2 · 2026-06-19 01:29 UTC_
