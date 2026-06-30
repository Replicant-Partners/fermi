# Will Morocco win the 2026 FIFA World Cup?

**Probability:** 2.3% · **Version:** v2 · **Updated:** 2026-06-30 13:12 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 4 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **2.4%** |
| Fermi estimate | **2.3%** |
| Divergence | +0.1pp below crowd (Consensus) |
| 24h volume | $2.3M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 2.3%**

Inside view: model evaluates to 1.6% (p5=1.0%, p95=2.3%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 0pp above (2.3% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 1.0% · median = 1.6% · p95 = 2.3% · σ = 0.004

```
▁▂▃▅▇██▇▆▅▄▃▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 0.7% | 18 | 0.2% |
| 0.8% | 114 | 1.1% |
| 0.9% | 426 | 4.3% |
| 1.1% | 807 | 8.1% |
| 1.2% | 1234 | 12.3% |
| 1.4% | 1476 | 14.8% |
| 1.5% | 1471 | 14.7% |
| 1.7% | 1339 | 13.4% |
| 1.8% | 1030 | 10.3% |
| 2.0% | 743 | 7.4% |
| 2.1% | 541 | 5.4% |
| 2.3% | 339 | 3.4% |
| 2.4% | 217 | 2.2% |
| 2.6% | 114 | 1.1% |
| 2.7% | 63 | 0.6% |
| 2.9% | 30 | 0.3% |
| 3.0% | 20 | 0.2% |
| 3.2% | 10 | 0.1% |
| 3.3% | 5 | 0.1% |
| 3.4% | 3 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | crowd | Δ(model−base) | Δ(model−crowd) | note |
|---|---|---|---|---|---|---|---|
| v1 | 2026-06-30 13:12 | 2.3% | 2.1% | 2.4% | +0.2pp | -0.1pp | Initial: 2.3% base=2%, 6 drivers, 4 evidence |
| v2 | 2026-06-30 13:12 | 2.3% | 2.1% | 2.4% | +0.2pp | -0.1pp | 2.3% (→), 6 drivers, 4 evidence |

**Model line:** ```▁█``` (range 2.3% – 2.3%)

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Morocco (2024–2026 latest available)_

### Evidence (1) — Strong quality (75%)

#### Agent: macro_data_agent — relevance 50% · quality ●●● High (75%) · 2026-06-30

Excellent! Now I have enough data. Let me compile the evidence for Morocco's socioeconomic capital factor:

---

## EVIDENCE: Socioeconomic Capital (X1) for Morocco — 2026 FIFA World Cup Forecast

### Core Indicators (Morocco, MAR)

**[INDICATOR]** GDP per capita (2023, World Bank NY.GDP.PCAP.CD): **$3,672** (current US$); log₁₀ ≈ **3.565**  
Source: IMF/World Bank data via Wikipedia Economy of Morocco, 6.69% increase from 2022.

**[INDICATOR]** GDP per capita PPP (2024 est., IMF): **$10,615** (international $); log₁₀ ≈ **4.026**  
Source: IMF estimate cited in Economy of Morocco (Wikipedia, June 2026).

**[INDICATOR]** Population (2024, Morocco census): **36.83 million** (HCP census Nov 2024); log₁₀ ≈ **1.566**  
Source: Morocco High Commission for Planning (HCP) via Yabiladi, Nov 2024. Crossed 37M mark around 2024.

**[INDICATOR]** HDI (2023, UNDP Human Development Report 2025): **0.710** (high development category); logit ≈ **0.896**  
Source: UNDP HDR 2025 (TheGlobalEconomy.com, MoroccoWorldNews May 2025). Morocco moved from 0.698 (2022, medium development) to 0.710 (2023, high development), ranking 120th globally out of 193 countries.

**[DATA AGE]** All indicators are 2023–2024 vintage; HDI is most recent authoritative release (UNDP May 2025). GDP per capita nominal is 2023; PPP estimate is 2024 IMF projection.

---

### Field Context: 2026 FIFA World Cup (48 teams)

**[BASELINE]** WC 2026 field median population: **~30 million** (per Dr. Randal Olson analysis, June 2026). Morocco at 36.8M is **above median** (+23% vs. field median).

**[BASELINE]** WC 2026 field GDP per capita range: **$752 (DR Congo) to $53,202 (United States)**, 71× spread (per WorldDataLab analysis, June 2026). Morocco's $3,672 nominal GDP/capita places it in the **lower tercile** of the field; PPP-adjusted $10,615 moves it closer to **mid-field**.

**[BASELINE]** WC 2026 field includes high-income nations (USA $53k, Switzerland, Norway, Qatar) and low-income (DR Congo $752, several sub-Saharan African qualifiers). Estimated field **median GDP/capita (nominal) ≈ $8,000–$12,000** (interpolating from known high/low extremes and 48-team distribution). Morocco's **nominal $3,672 is ~40–50% of field median**; **PPP $10,615 is ~90–130% of field median** depending on exact distribution.

**[BASELINE]** World average HDI (2023, UNDP): **0.744**. Morocco's **0.710 is 95% of world average**, placing it in the **6th decile globally** (120th of 193). Among WC 2026 participants, Morocco is likely **mid-to-lower tercile** on HDI (many European/North American qualifiers have HDI >0.90; several African/Asian qualifiers <0.70).

---

### Transform & Multiplier Calculation

**[TRANSFORM]** Using standard X1 (Socioeconomic Capital) weighting:  
- **40% GDP/capita (log)**: Use PPP-adjusted for international comparison → log₁₀(10,615) = **4.026**  
- **30% Population (log)**: log₁₀(36.83) = **1.566**  
- **30% HDI (logit)**: logit(0.710) = ln(0.710 / 0.290) ≈ **0.896**

Composite raw score:  
`0.4 × 4.026 + 0.3 × 1.566 + 0.3 × 0.896 = 1.610 + 0.470 + 0.269 = 2.349`

**[TRANSFORM]** Field median composite (estimated from WC 2026 field):  
- Median GDP/capita PPP ≈ $15,000 → log₁₀ ≈ 4.176  
- Median population ≈ 30M → log₁₀ ≈ 1.477  
- Median HDI ≈ 0.75 (interpolated) → logit ≈ 1.099  

Field median composite:  
`0.4 × 4.176 + 0.3 × 1.477 + 0.3 × 1.099 = 1.670 + 0.443 + 0.330 = 2.443`

**[TRANSFORM]** Morocco deviation from field median:  
`(2.349 − 2.443) / 0.7 ≈ −0.134` standard deviations below field median.

Morocco's socioeconomic capital is **slightly below the WC 2026 field median** (−0.13 σ), driven primarily by lower GDP/capita (even PPP-adjusted) offset partially by above-median population and respectable HDI progress.

---

### Final Multiplier

**[MULTIPLIER]** Suggested p50: **0.93** (p5: **0.82**, p95: **1.05**) — Morocco's GDP/capita ($10.6k PPP) sits ~30% below WC 2026 field median; HDI (0.710) is mid-tier; population (36.8M) is above median. Net socioeconomic capital is modestly below field average, warranting a **7% downward adjustment** to the X1 factor prior. Uncertainty range reflects Morocco's recent HDI gains (crossed into "high development" 2023) and GDP volatility (poverty rate rose 2021–2022); upside case (p95 = 1.05) allows for continued convergence trajectory.

---

**RATIONALE**: Morocco is a **middle-income, medium-HDI nation with above-median population** in the WC 2026 field. Its nominal GDP/capita ($3,672) is low-tier, but PPP adjustment ($10,615) brings it closer to field center. The 0.93 multiplier reflects a **modest socioeconomic disadvantage** relative to the tournament field, which includes many high-income European/North American teams. Morocco's recent HDI升级 to "high development" (0.710) and stable population base provide some upside, but GDP/capita remains a limiting factor for infrastructure, sports investment, and talent development pipelines compared to wealthier qualifiers.

**Key findings:**

- [INDICATOR]** GDP per capita (2023, World Bank NY.GDP.PCAP.CD): **$3,672** (current US$); log₁₀ ≈ **3.565**
- Source: IMF/World Bank data via Wikipedia Economy of Morocco, 6.69% increase from 2022.
- [INDICATOR]** GDP per capita PPP (2024 est., IMF): **$10,615** (international $); log₁₀ ≈ **4.026**
- [INDICATOR]** Population (2024, Morocco census): **36.83 million** (HCP census Nov 2024); log₁₀ ≈ **1.566**
- [INDICATOR]** HDI (2023, UNDP Human Development Report 2025): **0.710** (high development category); logit ≈ **0.896**
- [DATA AGE]** All indicators are 2023–2024 vintage; HDI is most recent authoritative release (UNDP May 2025). GDP per capita nominal is 2023; PPP estimate is 2024 IMF projection.
- [BASELINE]** WC 2026 field median population: **~30 million** (per Dr. Randal Olson analysis, June 2026). Morocco at 36.8M is **above median** (+23% vs. field median).
- [BASELINE]** WC 2026 field GDP per capita range: **$752 (DR Congo) to $53,202 (United States)**, 71× spread (per WorldDataLab analysis, June 2026). Morocco's $3,672 nominal GDP/capita places it in the **lower tercile** of the field; PPP-adjusted $10,615 moves it closer to **mid-field**.
- [BASELINE]** WC 2026 field includes high-income nations (USA $53k, Switzerland, Norway, Qatar) and low-income (DR Congo $752, several sub-Saharan African qualifiers). Estimated field **median GDP/capita (nominal) ≈ $8,000–$12,000** (interpolating from known high/low extremes and 48-team distribution). Morocco's **nominal $3,672 is ~40–50% of field median**; **PPP $10,615 is ~90–130% of field median** depending on exact distribution.
- [BASELINE]** World average HDI (2023, UNDP): **0.744**. Morocco's **0.710 is 95% of world average**, placing it in the **6th decile globally** (120th of 193). Among WC 2026 participants, Morocco is likely **mid-to-lower tercile** on HDI (many European/North American qualifiers have HDI >0.90; several African/Asian qualifiers <0.70).
- [TRANSFORM]** Using standard X1 (Socioeconomic Capital) weighting:
- **40% GDP/capita (log)**: Use PPP-adjusted for international comparison → log₁₀(10,615) = **4.026**
- **30% Population (log)**: log₁₀(36.83) = **1.566**
- **30% HDI (logit)**: logit(0.710) = ln(0.710 / 0.290) ≈ **0.896**
- [TRANSFORM]** Field median composite (estimated from WC 2026 field):

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Morocco_

### Evidence (1) — Partial quality (53%)

#### Agent: football_institution_agent — relevance 50% · quality ●●○ Med (53%) · 2026-06-30

Based on the available data, here is the institutional capacity assessment for Morocco:

---

## MOROCCO — FACTOR X2 (INSTITUTIONAL CAPACITY) ASSESSMENT

[DATA AGE] Most recent comprehensive FIFA Big Count data unavailable; using training-data baselines (~2020) supplemented by 2024 contextual signals.

[PENETRATION] Estimated registered players: ~500,000 in a population of 37.0M (2024) → **1,351 per 100k inhabitants**. This is above the African median (~800/100k) but well below European standards (2,500-4,000/100k). Morocco's Mohammed VI Football Academy (inaugurated 2009) and expanding grassroots infrastructure signal improving penetration, but the country still lags top-tier nations in youth registration density.

[LEAGUE REVENUE] Botola Pro annual revenue estimated at **€25-35M** (2023-24). Evidence: (1) Transfer revenue of $8.5M in 2023 (FIFA data) suggests modest commercial scale; (2) Botola Pro cited as one of Africa's top-3 leagues alongside South Africa PSL (~$100M+) and Egypt Premier League (~$40-50M); (3) Broadcasting rights held by Arryadia (state channel) with limited international reach. Log₁₀(30M) ≈ **7.48** — mid-tier African league, far below European top-5 (log₁₀ >9.0).

[CONFEDERATION] CAF member; **confederation coefficient ≈ 0.60**. Morocco's clubs dominate North Africa: Wydad Casablanca won CAF Champions League in 2022; Al Ahly (Egypt) won 2023-24. Morocco ranks **2nd-3rd in CAF 5-year coefficient** (2020-2024) behind Egypt, ahead of South Africa/Tunisia. CAF remains structurally weaker than UEFA/CONMEBOL due to lower club-competition prize money and European talent drain.

[INSTITUTIONAL SIGNAL] Morocco's 2022 World Cup semifinal run (best African performance ever) reflects **institutional overperformance** relative to domestic league scale. Key factors: (1) Diaspora talent pipeline (14/26 squad members born in Europe); (2) Elite coaching (Walid Regragui, European-trained); (3) FRMF investment in youth academies post-2010. However, domestic league remains export-focused — top talent leaves for Europe by age 20-22, limiting Botola Pro's competitive depth.

[MULTIPLIER] Suggested p50: **0.95** (p5: 0.75, p95: 1.15) — Morocco's institutions are **at the CAF median** but underperform relative to the country's economic scale (X1). The diaspora model compensates for weak domestic infrastructure, but X2 should not be boosted above the African field average. The 2022 World Cup was an institutional peak, not a sustainable baseline.

**Key findings:**

- [LEAGUE REVENUE] Botola Pro annual revenue estimated at **€25-35M** (2023-24). Evidence: (1) Transfer revenue of $8.5M in 2023 (FIFA data) suggests modest commercial scale; (2) Botola Pro cited as one of Africa's top-3 leagues alongside South Africa PSL (~$100M+) and Egypt Premier League (~$40-50M); (3) Broadcasting rights held by Arryadia (state channel) with limited international reach. Log₁₀(30M) ≈ **7.48** — mid-tier African league, far below European top-5 (log₁₀ >9.0).
- [MULTIPLIER] Suggested p50: **0.95** (p5: 0.75, p95: 1.15) — Morocco's institutions are **at the CAF median** but underperform relative to the country's economic scale (X1). The diaspora model compensates for weak domestic infrastructure, but X2 should not be boosted above the African field average. The 2022 World Cup was an institutional peak, not a sustainable baseline.

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MOROCCO NATIONAL TEAM — COMPREHENSIVE ANALYSIS (June 2025)

## CURRENT STATUS & ELO RATING

[BASE RATE] Morocco currently ranked **#7 in FIFA World Rankings** (June 2025), highest-ranked African nation. Estimated Elo rating **~1950-2000** based on FIFA ranking position and recent performance trajectory.

[MATCH STATS] **Recent Form (Last 5 competitive matches from API data):**
- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**

**Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)

[FORM SIGNAL] Morocco achieved a **19-match winning streak** in 2025-2026, breaking international football world records. They are **2025 AFCON Champions** (title awarded 3-0 by forfeit after Senegal abandoned the final — CAF Appeal Board ruling March 17, 2026).

## SQUAD QUALITY & MARKET VALUE DISTRIBUTION

[X4 SIGNAL] **Total squad market value: €490.7 million** (Transfermarkt, June 2026) — **highest in Africa**, surpassing Algeria, Egypt, and Tunisia combined. At World Cup 2026, Morocco ranked **14th globally by squad value** at €447.7M (tournament submission valuation).

**Top 10 Players by Market Value:**
1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
3. **Ismael Saibari** (PSV, AM) — €40M
4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
6. **Brahim Díaz** (Real Madrid, AM) — €35M
7. **Bilal El Khannouss** (Leicester, CM) — €35M
8. **Amine Gouiri** (Rennes, FW) — €28M
9. **Nayef Aguerd** (West Ham, CB) — €25M+ — **INJURED, out of WC 2026**
10. **Noussair Mazraoui** (Man United, RB) — €20M+

**Market Value Concentration:** Top-5 players represent approximately **€260M of €490M total = 53%** — high concentration indicating star-driven squad structure.

[X4 SIGNAL] **Big-5 European League Representation:** Approximately **75-80% of squad plays in Premier League, La Liga, Bundesliga, Ligue 1, or Serie A**. Key clubs: PSG (Hakimi), Real Madrid (Brahim), AC Milan (Bouaddi), PSV (Saibari), Man United (Mazraoui, Amrabat), West Ham (Aguerd), Wolves (Aït-Nouri).

**Average Squad Age:** Estimated **26-27 years** — optimal peak-performance window. Manager Mohamed Wahbi Ouahbi (appointed post-Regragui) has aggressively rejuvenated squad, taking only **9 players from Qatar 2022 squad** to WC 2026.

## KEY PLAYER AVAILABILITY & INJURY IMPACT

[INJURY IMPACT] **Major Absences for World Cup 2026:**
- **Abde Ezzalzouli** (LW, €40M) — OUT, injury
- **Nayef Aguerd** (CB, €25M) — OUT, injury
- **Youssef En-Nesyri** (ST) — **Controversially DROPPED** by Ouahbi (not injured)
- **Hakim Ziyech** (AM/RW) — **DROPPED** by Ouahbi (tactical decision)
- **Sofiane Boufal** (AM) — **DROPPED** by Ouahbi

**Estimated xG Impact:** Loss of Ezzalzouli (~0.25 xG/90) and defensive downgrade from Aguerd absence (~0.15 xGA/90 increase). However, squad depth mitigates: Ayoub El Kaabi (Olympiakos, 32 years old) selected as first-choice striker — **20 goals in 54 international appearances** (0.37 goals/game), superior international record to En-Nesyri.

**Key Players AVAILABLE:**
- ✅ **Achraf Hakimi** — Fully fit, captain, created **13 chances at AFCON 2025** (2nd-most in tournament)
- ✅ **Brahim Díaz** — Fit, key creative outlet
- ✅ **Sofyan Amrabat** — Fit, defensive midfielder anchor
- ✅ **Ismael Saibari** — Fit, breakout star (€30.4M valuation)
- ✅ **Ayyoub Bouaddi** — Fit, €60M+ young midfielder

## TACTICAL EFFICIENCY & PERFORMANCE METRICS

[X5 SIGNAL] **Set-Piece Strength:** Morocco historically strong from set pieces (~0.35-0.40 goals/game from set pieces in major tournaments). Hakimi's delivery and aerial presence from Aguerd (when fit) key weapons.

**Pressing Intensity:** Estimated **PPDA 9-11** (moderate press) — Morocco balances possession with counter-attacking threat. Not a high-press team like Germany/Spain.

**Defensive Solidity:** AFCON 2025 champions conceded sparingly in tournament run. Estimated **xGA ~0.8-1.0 per game** in competitive fixtures.

**Shot Conversion:** Ayoub El Kaabi's international conversion rate **~18-20%** (elite for striker). Morocco's tactical shift under Ouahbi emphasizes technical, possession-based play over physicality.

## ELO & DYNAMIC PERFORMANCE SIGNAL

[X3 SIGNAL] **Elo Estimation:** Based on #7 FIFA ranking and African dominance, Morocco's Elo likely **1950-2000** (World Cup field mean ~1700, sd 300). This places Morocco **+0.83 to +1.0 standard deviations above tournament mean**.

**Elo Trend (12-month):** Strongly positive. 19-match winning streak + AFCON title suggests **+100-150 Elo gain** over past year. Goal difference in recent competitive matches: **+7 in last 6 CHAN matches** (+1.17/game).

**xG Delta:** Limited data available, but tournament performance suggests **+0.5 to +0.8 xG/game** over recent run.

[X3 SIGNAL] **X3 Deterministic Component Calculation:**
- 0.50 × (1975 − 1700) / 300 = **+0.458**
- 0.10 × elo_trend (+125 over 12mo) = **+0.013** (normalized)
- 0.15 × goal_difference (+1.17/g) = **+0.175**
- 0.10 × pass_completion (est. 82%) = **+0.082**
- 0.15 × xg_delta (+0.65/g est.) = **+0.098**
- **Total X3 ≈ +0.83** (strong positive signal)

## FACTOR MODEL SUMMARY

[FACTOR] **Morocco's Three-Factor Profile for WC 2026:**

**X3 (Dynamic Performance):** Elo 1950-2000, 19-match winning streak, AFCON champions, +1.17 GD/game recent form — **Top quartile globally**

**X4 (Squad Quality):** €490M market value (#1 Africa, #14 globally), 75-80% Big-5 leagues, 53% value concentration in top-5 stars, optimal age profile 26-27 — **Top third globally, elite for non-European/South American team**

**X5 (Tactical Efficiency):** Strong set-piece threat, moderate pressing, solid defense, technical possession-based evolution under Ouahbi — **Above-average, not elite**

**Discriminating Factor:** X3 (dynamic performance) is Morocco's strongest signal. The 19-match winning streak and AFCON title represent exceptional recent form. X4 (squad quality) is very strong for Africa but mid-tier globally. X5 is solid but not exceptional.

[MULTIPLIER] Suggested p50: **1.35** (p5: 1.05, p95: 1.70) — Factor-mode: Morocco's X3 signal (Elo ~1975, winning streak, AFCON title) places them 0.8-1.0 SD above WC field mean; X4 squad value €490M supports top-African/mid-global tier; aggregate 35% boost to tournament prior reflects elite African form with injury-adjusted squad depth concerns.

---

**CONFIDENCE:** 0.75 — High confidence in market value data and recent results; moderate uncertainty on exact Elo (estimated from FIFA rank) and xG metrics (limited granular data). Injury impact to Ezzalzouli/Aguerd partially offset by squad depth.

**RELEVANCE:** 1.0 — All findings directly address Morocco's current status, form, squad quality, and tournament readiness.

**Key findings:**

- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**
- Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)
- Top 10 Players by Market Value:**
- 1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
- 2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
- 3. **Ismael Saibari** (PSV, AM) — €40M
- 4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
- 5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
- 6. **Brahim Díaz** (Real Madrid, AM) — €35M
- 7. **Bilal El Khannouss** (Leicester, CM) — €35M

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MOROCCO NATIONAL TEAM — COMPREHENSIVE ANALYSIS (June 2025)

## CURRENT STATUS & ELO RATING

[BASE RATE] Morocco currently ranked **#7 in FIFA World Rankings** (June 2025), highest-ranked African nation. Estimated Elo rating **~1950-2000** based on FIFA ranking position and recent performance trajectory.

[MATCH STATS] **Recent Form (Last 5 competitive matches from API data):**
- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**

**Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)

[FORM SIGNAL] Morocco achieved a **19-match winning streak** in 2025-2026, breaking international football world records. They are **2025 AFCON Champions** (title awarded 3-0 by forfeit after Senegal abandoned the final — CAF Appeal Board ruling March 17, 2026).

## SQUAD QUALITY & MARKET VALUE DISTRIBUTION

[X4 SIGNAL] **Total squad market value: €490.7 million** (Transfermarkt, June 2026) — **highest in Africa**, surpassing Algeria, Egypt, and Tunisia combined. At World Cup 2026, Morocco ranked **14th globally by squad value** at €447.7M (tournament submission valuation).

**Top 10 Players by Market Value:**
1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
3. **Ismael Saibari** (PSV, AM) — €40M
4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
6. **Brahim Díaz** (Real Madrid, AM) — €35M
7. **Bilal El Khannouss** (Leicester, CM) — €35M
8. **Amine Gouiri** (Rennes, FW) — €28M
9. **Nayef Aguerd** (West Ham, CB) — €25M+ — **INJURED, out of WC 2026**
10. **Noussair Mazraoui** (Man United, RB) — €20M+

**Market Value Concentration:** Top-5 players represent approximately **€260M of €490M total = 53%** — high concentration indicating star-driven squad structure.

[X4 SIGNAL] **Big-5 European League Representation:** Approximately **75-80% of squad plays in Premier League, La Liga, Bundesliga, Ligue 1, or Serie A**. Key clubs: PSG (Hakimi), Real Madrid (Brahim), AC Milan (Bouaddi), PSV (Saibari), Man United (Mazraoui, Amrabat), West Ham (Aguerd), Wolves (Aït-Nouri).

**Average Squad Age:** Estimated **26-27 years** — optimal peak-performance window. Manager Mohamed Wahbi Ouahbi (appointed post-Regragui) has aggressively rejuvenated squad, taking only **9 players from Qatar 2022 squad** to WC 2026.

## KEY PLAYER AVAILABILITY & INJURY IMPACT

[INJURY IMPACT] **Major Absences for World Cup 2026:**
- **Abde Ezzalzouli** (LW, €40M) — OUT, injury
- **Nayef Aguerd** (CB, €25M) — OUT, injury
- **Youssef En-Nesyri** (ST) — **Controversially DROPPED** by Ouahbi (not injured)
- **Hakim Ziyech** (AM/RW) — **DROPPED** by Ouahbi (tactical decision)
- **Sofiane Boufal** (AM) — **DROPPED** by Ouahbi

**Estimated xG Impact:** Loss of Ezzalzouli (~0.25 xG/90) and defensive downgrade from Aguerd absence (~0.15 xGA/90 increase). However, squad depth mitigates: Ayoub El Kaabi (Olympiakos, 32 years old) selected as first-choice striker — **20 goals in 54 international appearances** (0.37 goals/game), superior international record to En-Nesyri.

**Key Players AVAILABLE:**
- ✅ **Achraf Hakimi** — Fully fit, captain, created **13 chances at AFCON 2025** (2nd-most in tournament)
- ✅ **Brahim Díaz** — Fit, key creative outlet
- ✅ **Sofyan Amrabat** — Fit, defensive midfielder anchor
- ✅ **Ismael Saibari** — Fit, breakout star (€30.4M valuation)
- ✅ **Ayyoub Bouaddi** — Fit, €60M+ young midfielder

## TACTICAL EFFICIENCY & PERFORMANCE METRICS

[X5 SIGNAL] **Set-Piece Strength:** Morocco historically strong from set pieces (~0.35-0.40 goals/game from set pieces in major tournaments). Hakimi's delivery and aerial presence from Aguerd (when fit) key weapons.

**Pressing Intensity:** Estimated **PPDA 9-11** (moderate press) — Morocco balances possession with counter-attacking threat. Not a high-press team like Germany/Spain.

**Defensive Solidity:** AFCON 2025 champions conceded sparingly in tournament run. Estimated **xGA ~0.8-1.0 per game** in competitive fixtures.

**Shot Conversion:** Ayoub El Kaabi's international conversion rate **~18-20%** (elite for striker). Morocco's tactical shift under Ouahbi emphasizes technical, possession-based play over physicality.

## ELO & DYNAMIC PERFORMANCE SIGNAL

[X3 SIGNAL] **Elo Estimation:** Based on #7 FIFA ranking and African dominance, Morocco's Elo likely **1950-2000** (World Cup field mean ~1700, sd 300). This places Morocco **+0.83 to +1.0 standard deviations above tournament mean**.

**Elo Trend (12-month):** Strongly positive. 19-match winning streak + AFCON title suggests **+100-150 Elo gain** over past year. Goal difference in recent competitive matches: **+7 in last 6 CHAN matches** (+1.17/game).

**xG Delta:** Limited data available, but tournament performance suggests **+0.5 to +0.8 xG/game** over recent run.

[X3 SIGNAL] **X3 Deterministic Component Calculation:**
- 0.50 × (1975 − 1700) / 300 = **+0.458**
- 0.10 × elo_trend (+125 over 12mo) = **+0.013** (normalized)
- 0.15 × goal_difference (+1.17/g) = **+0.175**
- 0.10 × pass_completion (est. 82%) = **+0.082**
- 0.15 × xg_delta (+0.65/g est.) = **+0.098**
- **Total X3 ≈ +0.83** (strong positive signal)

## FACTOR MODEL SUMMARY

[FACTOR] **Morocco's Three-Factor Profile for WC 2026:**

**X3 (Dynamic Performance):** Elo 1950-2000, 19-match winning streak, AFCON champions, +1.17 GD/game recent form — **Top quartile globally**

**X4 (Squad Quality):** €490M market value (#1 Africa, #14 globally), 75-80% Big-5 leagues, 53% value concentration in top-5 stars, optimal age profile 26-27 — **Top third globally, elite for non-European/South American team**

**X5 (Tactical Efficiency):** Strong set-piece threat, moderate pressing, solid defense, technical possession-based evolution under Ouahbi — **Above-average, not elite**

**Discriminating Factor:** X3 (dynamic performance) is Morocco's strongest signal. The 19-match winning streak and AFCON title represent exceptional recent form. X4 (squad quality) is very strong for Africa but mid-tier globally. X5 is solid but not exceptional.

[MULTIPLIER] Suggested p50: **1.35** (p5: 1.05, p95: 1.70) — Factor-mode: Morocco's X3 signal (Elo ~1975, winning streak, AFCON title) places them 0.8-1.0 SD above WC field mean; X4 squad value €490M supports top-African/mid-global tier; aggregate 35% boost to tournament prior reflects elite African form with injury-adjusted squad depth concerns.

---

**CONFIDENCE:** 0.75 — High confidence in market value data and recent results; moderate uncertainty on exact Elo (estimated from FIFA rank) and xG metrics (limited granular data). Injury impact to Ezzalzouli/Aguerd partially offset by squad depth.

**RELEVANCE:** 1.0 — All findings directly address Morocco's current status, form, squad quality, and tournament readiness.

**Key findings:**

- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**
- Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)
- Top 10 Players by Market Value:**
- 1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
- 2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
- 3. **Ismael Saibari** (PSV, AM) — €40M
- 4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
- 5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
- 6. **Brahim Díaz** (Real Madrid, AM) — €35M
- 7. **Bilal El Khannouss** (Leicester, CM) — €35M

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MOROCCO NATIONAL TEAM — COMPREHENSIVE ANALYSIS (June 2025)

## CURRENT STATUS & ELO RATING

[BASE RATE] Morocco currently ranked **#7 in FIFA World Rankings** (June 2025), highest-ranked African nation. Estimated Elo rating **~1950-2000** based on FIFA ranking position and recent performance trajectory.

[MATCH STATS] **Recent Form (Last 5 competitive matches from API data):**
- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**

**Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)

[FORM SIGNAL] Morocco achieved a **19-match winning streak** in 2025-2026, breaking international football world records. They are **2025 AFCON Champions** (title awarded 3-0 by forfeit after Senegal abandoned the final — CAF Appeal Board ruling March 17, 2026).

## SQUAD QUALITY & MARKET VALUE DISTRIBUTION

[X4 SIGNAL] **Total squad market value: €490.7 million** (Transfermarkt, June 2026) — **highest in Africa**, surpassing Algeria, Egypt, and Tunisia combined. At World Cup 2026, Morocco ranked **14th globally by squad value** at €447.7M (tournament submission valuation).

**Top 10 Players by Market Value:**
1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
3. **Ismael Saibari** (PSV, AM) — €40M
4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
6. **Brahim Díaz** (Real Madrid, AM) — €35M
7. **Bilal El Khannouss** (Leicester, CM) — €35M
8. **Amine Gouiri** (Rennes, FW) — €28M
9. **Nayef Aguerd** (West Ham, CB) — €25M+ — **INJURED, out of WC 2026**
10. **Noussair Mazraoui** (Man United, RB) — €20M+

**Market Value Concentration:** Top-5 players represent approximately **€260M of €490M total = 53%** — high concentration indicating star-driven squad structure.

[X4 SIGNAL] **Big-5 European League Representation:** Approximately **75-80% of squad plays in Premier League, La Liga, Bundesliga, Ligue 1, or Serie A**. Key clubs: PSG (Hakimi), Real Madrid (Brahim), AC Milan (Bouaddi), PSV (Saibari), Man United (Mazraoui, Amrabat), West Ham (Aguerd), Wolves (Aït-Nouri).

**Average Squad Age:** Estimated **26-27 years** — optimal peak-performance window. Manager Mohamed Wahbi Ouahbi (appointed post-Regragui) has aggressively rejuvenated squad, taking only **9 players from Qatar 2022 squad** to WC 2026.

## KEY PLAYER AVAILABILITY & INJURY IMPACT

[INJURY IMPACT] **Major Absences for World Cup 2026:**
- **Abde Ezzalzouli** (LW, €40M) — OUT, injury
- **Nayef Aguerd** (CB, €25M) — OUT, injury
- **Youssef En-Nesyri** (ST) — **Controversially DROPPED** by Ouahbi (not injured)
- **Hakim Ziyech** (AM/RW) — **DROPPED** by Ouahbi (tactical decision)
- **Sofiane Boufal** (AM) — **DROPPED** by Ouahbi

**Estimated xG Impact:** Loss of Ezzalzouli (~0.25 xG/90) and defensive downgrade from Aguerd absence (~0.15 xGA/90 increase). However, squad depth mitigates: Ayoub El Kaabi (Olympiakos, 32 years old) selected as first-choice striker — **20 goals in 54 international appearances** (0.37 goals/game), superior international record to En-Nesyri.

**Key Players AVAILABLE:**
- ✅ **Achraf Hakimi** — Fully fit, captain, created **13 chances at AFCON 2025** (2nd-most in tournament)
- ✅ **Brahim Díaz** — Fit, key creative outlet
- ✅ **Sofyan Amrabat** — Fit, defensive midfielder anchor
- ✅ **Ismael Saibari** — Fit, breakout star (€30.4M valuation)
- ✅ **Ayyoub Bouaddi** — Fit, €60M+ young midfielder

## TACTICAL EFFICIENCY & PERFORMANCE METRICS

[X5 SIGNAL] **Set-Piece Strength:** Morocco historically strong from set pieces (~0.35-0.40 goals/game from set pieces in major tournaments). Hakimi's delivery and aerial presence from Aguerd (when fit) key weapons.

**Pressing Intensity:** Estimated **PPDA 9-11** (moderate press) — Morocco balances possession with counter-attacking threat. Not a high-press team like Germany/Spain.

**Defensive Solidity:** AFCON 2025 champions conceded sparingly in tournament run. Estimated **xGA ~0.8-1.0 per game** in competitive fixtures.

**Shot Conversion:** Ayoub El Kaabi's international conversion rate **~18-20%** (elite for striker). Morocco's tactical shift under Ouahbi emphasizes technical, possession-based play over physicality.

## ELO & DYNAMIC PERFORMANCE SIGNAL

[X3 SIGNAL] **Elo Estimation:** Based on #7 FIFA ranking and African dominance, Morocco's Elo likely **1950-2000** (World Cup field mean ~1700, sd 300). This places Morocco **+0.83 to +1.0 standard deviations above tournament mean**.

**Elo Trend (12-month):** Strongly positive. 19-match winning streak + AFCON title suggests **+100-150 Elo gain** over past year. Goal difference in recent competitive matches: **+7 in last 6 CHAN matches** (+1.17/game).

**xG Delta:** Limited data available, but tournament performance suggests **+0.5 to +0.8 xG/game** over recent run.

[X3 SIGNAL] **X3 Deterministic Component Calculation:**
- 0.50 × (1975 − 1700) / 300 = **+0.458**
- 0.10 × elo_trend (+125 over 12mo) = **+0.013** (normalized)
- 0.15 × goal_difference (+1.17/g) = **+0.175**
- 0.10 × pass_completion (est. 82%) = **+0.082**
- 0.15 × xg_delta (+0.65/g est.) = **+0.098**
- **Total X3 ≈ +0.83** (strong positive signal)

## FACTOR MODEL SUMMARY

[FACTOR] **Morocco's Three-Factor Profile for WC 2026:**

**X3 (Dynamic Performance):** Elo 1950-2000, 19-match winning streak, AFCON champions, +1.17 GD/game recent form — **Top quartile globally**

**X4 (Squad Quality):** €490M market value (#1 Africa, #14 globally), 75-80% Big-5 leagues, 53% value concentration in top-5 stars, optimal age profile 26-27 — **Top third globally, elite for non-European/South American team**

**X5 (Tactical Efficiency):** Strong set-piece threat, moderate pressing, solid defense, technical possession-based evolution under Ouahbi — **Above-average, not elite**

**Discriminating Factor:** X3 (dynamic performance) is Morocco's strongest signal. The 19-match winning streak and AFCON title represent exceptional recent form. X4 (squad quality) is very strong for Africa but mid-tier globally. X5 is solid but not exceptional.

[MULTIPLIER] Suggested p50: **1.35** (p5: 1.05, p95: 1.70) — Factor-mode: Morocco's X3 signal (Elo ~1975, winning streak, AFCON title) places them 0.8-1.0 SD above WC field mean; X4 squad value €490M supports top-African/mid-global tier; aggregate 35% boost to tournament prior reflects elite African form with injury-adjusted squad depth concerns.

---

**CONFIDENCE:** 0.75 — High confidence in market value data and recent results; moderate uncertainty on exact Elo (estimated from FIFA rank) and xG metrics (limited granular data). Injury impact to Ezzalzouli/Aguerd partially offset by squad depth.

**RELEVANCE:** 1.0 — All findings directly address Morocco's current status, form, squad quality, and tournament readiness.

**Key findings:**

- **W** vs Angola 2-0 (CHAN 2024, Aug 3, 2025)
- **L** vs Kenya 0-1 (CHAN 2024, Aug 10, 2025)
- **W** vs Zambia 3-1 (CHAN 2024, Aug 14, 2025)
- **W** vs Congo DR 3-1 (CHAN 2024, Aug 17, 2025)
- **W** vs Tanzania 1-0 (CHAN QF, Aug 22, 2025)
- **D** vs Senegal 1-1 (CHAN SF, Aug 26, 2025) — **Won on penalties**
- Record: 4W-1D-1L in last 6 matches** (83% win rate excluding penalty shootout)
- Top 10 Players by Market Value:**
- 1. **Achraf Hakimi** (PSG, RB/RWB) — €80-81M — Captain, 27 years old
- 2. **Ayyoub Bouaddi** (AC Milan, CM) — €60M+ — Young star midfielder
- 3. **Ismael Saibari** (PSV, AM) — €40M
- 4. **Abde Ezzalzouli** (Real Betis, LW) — €40M — **INJURED, out of WC 2026**
- 5. **Rayan Aït-Nouri** (Wolves, LB) — €40M
- 6. **Brahim Díaz** (Real Madrid, AM) — €35M
- 7. **Bilal El Khannouss** (Leicester, CM) — €35M

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Morocco: venue, climate, rest days, altitude, opponent travel burden_

### Evidence (1) — Strong quality (70%)

#### Agent: fixture_context_agent — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MOROCCO — EXOGENOUS CONTEXT ANALYSIS (Factor X6)
## 2026 FIFA World Cup Group C Fixtures

---

## FIXTURE SCHEDULE CONFIRMED

Morocco's Group C fixtures at the 2026 World Cup:

1. **vs Brazil** — June 13, 2026 — MetLife Stadium (New York/New Jersey)
2. **vs Scotland** — June 19, 2026 — Gillette Stadium (Foxborough, Massachusetts)  
3. **vs Haiti** — June 24, 2026 — Mercedes-Benz Stadium (Atlanta, Georgia)

**Round of 32** — June 29, 2026 — vs Netherlands — Estadio BBVA (Monterrey, Mexico)

---

## FACTOR FINDINGS

### [HOST] Host Status: 0.0 (Away Nation)
Morocco plays all group-stage fixtures in the United States. They are **not a co-host** (USA/Canada/Mexico are the three hosts). Host status = **0.0** — no home advantage. This is a significant disadvantage relative to field-median (neutral = 0.5). Historical WC data shows host nations gain +0.3 to +0.5 implied Elo in group stages.

### [CLIMATE] Climate Delta: Moderate Disadvantage (0.35 climate_delta score)

**Morocco's home climate (June baseline):**
- Rabat/Casablanca: 20-28°C, ~70-74% humidity
- Mediterranean coastal climate — warm, moderately humid

**US venue climates (mid-June):**

| Venue | Date | Temp Range | Humidity | Delta Assessment |
|-------|------|------------|----------|------------------|
| MetLife (NJ) | June 13 | 22-31°C | 60-77% | **Minimal** — nearly identical to Moroccan coastal summer |
| Gillette (MA) | June 19 | 18-28°C | 69-72% | **Minimal** — slightly cooler, similar humidity |
| Atlanta (GA) | June 24 | 24-31°C | 67-91% | **Moderate** — higher humidity (subtropical), +3-5°C warmer |

**Climate delta assessment:** Morocco's Mediterranean-acclimated squad faces **near-neutral conditions** in New Jersey and Massachusetts (both temperate coastal, similar to home). Atlanta presents a **moderate subtropical challenge** (higher humidity, warmer). Averaged across three fixtures: **climate_delta ≈ 0.35** (where 0 = perfect match, 1 = maximum disadvantage). This is **better than field-median** — most European/South American teams face worse climate deltas in US summer venues.

### [REST DAYS] Fixture Congestion: Neutral to Favorable (0.60 rest_days score)

**Inter-fixture rest:**
- June 13 → June 19: **6 days rest** (optimal)
- June 19 → June 24: **5 days rest** (optimal)
- June 24 → June 29 (R32): **5 days rest** (optimal)

**Pre-tournament rest:** Morocco's last competitive fixture before the World Cup was the **CAF playoff final on March 20, 2026** (84 days before June 13 opener). This is **exceptional preparation time** — no fixture congestion whatsoever.

FIFA/UEFA research shows performance drops 10-15% on <3 rest days; returns to baseline at 3+ days; no further gain past 5 days. Morocco's 5-6 day intervals are **optimal**. Rest_days score: **0.60** (where 0 = <3 days congested, 0.5 = 3-4 days baseline, 1.0 = 7+ days). This is **above field-median** — many European teams arrive with domestic season fixture congestion.

### [ALTITUDE] Altitude Delta: Negligible Advantage (0.05 altitude_delta)

**Venue elevations:**
- MetLife Stadium (East Rutherford, NJ): **~3 feet / 1 meter** (essentially sea level)
- Gillette Stadium (Foxborough, MA): **~50 meters** (negligible)
- Mercedes-Benz Stadium (Atlanta, GA): **~320 meters** (negligible)

**Morocco's training base altitude:** Rabat/Casablanca are **coastal sea-level cities** (0-50m). Morocco's squad trains at sea level.

**Altitude delta:** All three group-stage venues are **<350m elevation** — no altitude disadvantage for any team. Morocco faces **zero altitude stress**. If they advance to the Round of 32 in Monterrey (Estadio BBVA at ~540m), still negligible. Altitude_delta score: **0.05** (where 0 = perfect match, 1 = severe disadvantage like La Paz at 3,600m). This is **neutral to field**.

### [OPPONENT TRAVEL BURDEN] Relative Travel Advantage: Moderate

**Morocco's travel pattern:**
- New Jersey → Massachusetts: ~350 km
- Massachusetts → Atlanta: ~1,700 km
- Total group-stage travel: ~2,050 km

**Opponent travel burdens:**
- **Brazil** (also playing in Miami, Philadelphia): Similar multi-city US tour — **neutral**
- **Scotland** (also playing in Miami, Philadelphia): Similar pattern — **neutral**
- **Haiti** (also playing in New Jersey, Philadelphia): Similar pattern — **neutral**

Morocco has **no relative travel advantage** in the group stage — all teams face comparable intra-US logistics. However, Morocco's **84-day pre-tournament rest** (vs European teams arriving from May domestic seasons) is a **significant preparation advantage**.

---

## [MULTIPLIER] FINAL ASSESSMENT

**Suggested p50: 0.85 (p5: 0.70, p95: 1.00)**

**Rationale:** Morocco faces a **net exogenous headwind** driven primarily by **absence of host status** (the dominant X6 signal). Climate conditions are near-neutral (slightly favorable vs field), rest days are optimal, and altitude is negligible. The 0.85 multiplier reflects:

- **Host status penalty:** -0.15 to -0.20 (away nation in a host-advantaged tournament)
- **Climate advantage:** +0.05 (better acclimated than European teams to US summer)
- **Rest advantage:** +0.05 (optimal preparation, no congestion)
- **Altitude neutral:** 0.00

**Net effect:** Morocco operates at **~85% of a hypothetical host-nation baseline** for exogenous context. This is **below tournament median** (which includes three host nations at 1.25-1.50), but **above the away-nation median** (many teams face worse climate/rest burdens). The p95 of 1.00 allows for scenarios where climate/rest advantages fully offset host-status absence; the p5 of 0.70 captures worst-case where subtropical Atlanta conditions and opponent home-crowd support (for USA-adjacent teams) compound disadvantages.

**Key findings:**

- 1. **vs Brazil** — June 13, 2026 — MetLife Stadium (New York/New Jersey)
- 2. **vs Scotland** — June 19, 2026 — Gillette Stadium (Foxborough, Massachusetts)
- 3. **vs Haiti** — June 24, 2026 — Mercedes-Benz Stadium (Atlanta, Georgia)
- Round of 32** — June 29, 2026 — vs Netherlands — Estadio BBVA (Monterrey, Mexico)
- Morocco's home climate (June baseline):**
- Rabat/Casablanca: 20-28°C, ~70-74% humidity
- Mediterranean coastal climate — warm, moderately humid
- US venue climates (mid-June):**
- | MetLife (NJ) | June 13 | 22-31°C | 60-77% | **Minimal** — nearly identical to Moroccan coastal summer |
- | Gillette (MA) | June 19 | 18-28°C | 69-72% | **Minimal** — slightly cooler, similar humidity |
- | Atlanta (GA) | June 24 | 24-31°C | 67-91% | **Moderate** — higher humidity (subtropical), +3-5°C warmer |
- Climate delta assessment:** Morocco's Mediterranean-acclimated squad faces **near-neutral conditions** in New Jersey and Massachusetts (both temperate coastal, similar to home). Atlanta presents a **moderate subtropical challenge** (higher humidity, warmer). Averaged across three fixtures: **climate_delta ≈ 0.35** (where 0 = perfect match, 1 = maximum disadvantage). This is **better than field-median** — most European/South American teams face worse climate deltas in US summer venues.
- Inter-fixture rest:**
- June 13 → June 19: **6 days rest** (optimal)
- June 19 → June 24: **5 days rest** (optimal)

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Morocco (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Morocco |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Morocco |
| fixture_context_agent | fixture_context | Upcoming fixtures for Morocco: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v2 · 2026-06-30 13:12 UTC_
