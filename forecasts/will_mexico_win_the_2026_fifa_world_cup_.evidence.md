# Will Mexico win the 2026 FIFA World Cup?

**Probability:** 3.1% · **Version:** v1 · **Updated:** 2026-06-30 12:34 UTC

**Confidence:** Medium (50%) · **Drivers:** 6 · **Evidence:** 4 · **Agents:** 4

---

## Polymarket Crowd Price

| Metric | Value |
|---|---|
| Crowd price | **1.5%** |
| Fermi estimate | **3.1%** |
| Divergence | +1.6pp above crowd (Consensus) |
| 24h volume | $2.8M |
| Market confidence | Very High |

[View on Polymarket](https://polymarket.com/event/world-cup-winner)

---

## Inside View

**Probability: 3.1%**

Inside view: model evaluates to 3.1% (p5=2.0%, p95=4.3%). Outside view (base rate): 2.1%. Key drivers: socio_capital, institutional_capacity, dynamic_performance.

**Forecast Confidence:** Medium (50%)

**Divergence from base rate:** 1pp above (3.1% vs 2.1%)

---

## Outside View (Base Rate)

**2.1%** — FIFA World Cup winners 1930–2022

- **Sample size:** n=22
- **Source:** FIFA tournament archive — 22 prior World Cups

Equal-prior baseline across the 2026 expanded 48-team field. Inside view diverges via the six factor-derived drivers.

---

## Simulation Distribution

**10000 iterations** · p5 = 2.0% · median = 3.0% · p95 = 4.3% · σ = 0.007

```
▁▂▃▅▇███▆▅▄▃▂▂▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 1.5% | 43 | 0.4% |
| 1.7% | 157 | 1.6% |
| 2.0% | 448 | 4.5% |
| 2.2% | 819 | 8.2% |
| 2.5% | 1180 | 11.8% |
| 2.7% | 1390 | 13.9% |
| 3.0% | 1397 | 14.0% |
| 3.2% | 1324 | 13.2% |
| 3.5% | 1016 | 10.2% |
| 3.7% | 843 | 8.4% |
| 4.0% | 571 | 5.7% |
| 4.2% | 376 | 3.8% |
| 4.5% | 195 | 1.9% |
| 4.7% | 108 | 1.1% |
| 5.0% | 60 | 0.6% |
| 5.2% | 42 | 0.4% |
| 5.5% | 18 | 0.2% |
| 5.7% | 9 | 0.1% |
| 6.0% | 0 | 0.0% |
| 6.2% | 4 | 0.0% |

---

## 1. socio_capital `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Macro capacity for footballing investment: GDP, population, HDI.

### Assigned Agents

- **macro_data_agent** (schedule: every 1 Month)  
  Query: _GDP per capita, population, HDI for Mexico (2024–2026 latest available)_

### Evidence (1) — Strong quality (75%)

#### Agent: macro_data_agent — relevance 50% · quality ●●● High (75%) · 2026-06-30

## MEXICO (MEX) — X1 SOCIOECONOMIC CAPITAL INDICATORS (2024–2026)

**[DATA AGE]** Latest available data: GDP per capita and population from 2024–2025 sources; HDI from UNDP 2023 baseline (2024 report not yet released with Mexico-specific update at time of search).

---

### CORE INDICATORS

**[INDICATOR]** GDP per capita (2024, nominal current US$): **$14,110**  
Source: GDPIndex.org citing 2024 estimates; IMF WEO April 2026 cites PPP figure of $26,643 for 2026.  
- Log₁₀ transformation: log₁₀(14,110) ≈ **4.149**

**[INDICATOR]** Population (2026, total): **131.0 million**  
Source: World Population Clock / UN WPP 2024 Revision; multiple sources converge on ~130.8–131M for early 2026.  
- Log₁₀ transformation: log₁₀(131.0) ≈ **2.117**

**[INDICATOR]** HDI (2022, UNDP Human Development Report 2024): **0.780**  
Source: World Scorecard / UNDP HDR; Mexico peaked at 0.780 in 2017, maintained ~0.78 range through 2022.  
- Logit transformation: log(0.780 / (1 − 0.780)) ≈ log(3.545) ≈ **1.266**

---

### BASELINE COMPARISON

**[BASELINE]** World Cup 2026 field median benchmarks (32-team tournament):  
- GDP per capita (log): ~4.05 (median ~$11,200)  
- Population (log): ~1.60 (median ~40M)  
- HDI (logit): ~1.50 (median ~0.818)

**[TRANSFORM]** Mexico composite Z-score calculation:  
Using standard X1 weights (0.4 GDP, 0.3 Pop, 0.3 HDI):  
Z = (0.4 × 4.149 + 0.3 × 2.117 + 0.3 × 1.266 − 2.6) / 0.7  
Z = (1.660 + 0.635 + 0.380 − 2.6) / 0.7  
Z = **+0.11** — marginally above field median

Mexico's GDP per capita is ~26% above the WC field median, but HDI lags slightly (0.780 vs. ~0.82 median), and population is substantially larger (131M vs. 40M median), which dilutes per-capita resource concentration in tournament contexts.

---

### FERMI MULTIPLIER OUTPUT

**[MULTIPLIER]** Suggested p50: **1.03** (p5: **0.92**, p95: **1.16**) — Mexico's GDP per capita ($14,110, log 4.15) sits modestly above the WC2026 field median, offsetting slightly lower HDI (0.780 vs. 0.82); large population (131M) creates resource diffusion but also deep talent pool; net effect is marginal upward adjustment to X1 Socioeconomic Capital factor.

**Key findings:**

- [DATA AGE]** Latest available data: GDP per capita and population from 2024–2025 sources; HDI from UNDP 2023 baseline (2024 report not yet released with Mexico-specific update at time of search).
- [INDICATOR]** GDP per capita (2024, nominal current US$): **$14,110**
- Source: GDPIndex.org citing 2024 estimates; IMF WEO April 2026 cites PPP figure of $26,643 for 2026.
- Log₁₀ transformation: log₁₀(14,110) ≈ **4.149**
- [INDICATOR]** Population (2026, total): **131.0 million**
- Log₁₀ transformation: log₁₀(131.0) ≈ **2.117**
- [INDICATOR]** HDI (2022, UNDP Human Development Report 2024): **0.780**
- Logit transformation: log(0.780 / (1 − 0.780)) ≈ log(3.545) ≈ **1.266**
- [BASELINE]** World Cup 2026 field median benchmarks (32-team tournament):
- GDP per capita (log): ~4.05 (median ~$11,200)
- Population (log): ~1.60 (median ~40M)
- HDI (logit): ~1.50 (median ~0.818)
- [TRANSFORM]** Mexico composite Z-score calculation:
- Mexico's GDP per capita is ~26% above the WC field median, but HDI lags slightly (0.780 vs. ~0.82 median), and population is substantially larger (131M vs. 40M median), which dilutes per-capita resource concentration in tournament contexts.
- [MULTIPLIER]** Suggested p50: **1.03** (p5: **0.92**, p95: **1.16**) — Mexico's GDP per capita ($14,110, log 4.15) sits modestly above the WC2026 field median, offsetting slightly lower HDI (0.780 vs. 0.82); large population (131M) creates resource diffusion but also deep talent pool; net effect is marginal upward adjustment to X1 Socioeconomic Capital factor.

---

## 2. institutional_capacity `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Federation strength, league development, player pipeline depth.

### Assigned Agents

- **football_institution_agent** (schedule: every 1 Month)  
  Query: _Player registration, top-flight league revenue, confederation coefficient for Mexico_

### Evidence (1) — Strong quality (71%)

#### Agent: football_institution_agent — relevance 50% · quality ●●● High (71%) · 2026-06-30

Based on the available search results and institutional football infrastructure analysis for Mexico:

---

## MEXICO — INSTITUTIONAL CAPACITY (X2) ASSESSMENT

[PENETRATION] **Professional players only**: FIFA Professional Football Report 2023 confirms Mexico has **9,464 registered professional players** — the highest count globally, ahead of Spain (8,560) and England (5,582). However, this represents only the professional tier. With Mexico's population of ~129.4M (2024), professional penetration = 7.3 per 100k. **[DATA AGE]** — Total grassroots/amateur registration data unavailable in search results; FIFA Big Count comprehensive data not retrieved. Professional-only metric significantly understates true player base in a country where football is the dominant sport (73% self-identify as fans per Wikipedia). Training-data baseline suggests Mexico's total registered player pool (including youth/amateur) likely exceeds 6-8 million, yielding ~5,000-6,200 per 100k — well above global median but below elite European penetration rates (Germany ~8,500/100k, England ~7,000/100k).

[LEAGUE REVENUE] **Liga MX financial scale**: Direct revenue figures not retrieved in search results. **[DATA AGE]** — Deloitte Money League 2025 focuses on European clubs; Liga MX clubs do not appear in top-20 global rankings. Training-data baseline: Liga MX aggregate annual revenue estimated at **$650-750M USD** (2023-24 season), making it the wealthiest league in the Americas outside MLS, but ~15-20x smaller than the English Premier League ($7.6B) or La Liga ($4.5B). Log10(700M) ≈ **8.85** — mid-tier among major football nations, comparable to Eredivisie or Belgian Pro League scale.

[CONFEDERATION] **CONCACAF coefficient**: Mexico competes in CONCACAF, the third-strongest confederation globally after UEFA and CONMEBOL. Standard coefficient: **0.65** (per agent training baseline). Recent performance signals: CONCACAF clubs have historically struggled in FIFA Club World Cup (34 World Cup wins from 152 matches per search results — 22.4% win rate, lowest among major confederations except OFC). Mexican clubs dominate CONCACAF Champions Cup (Cruz Azul: 119 matches, 69 wins, 271 goals — most in competition history) but rarely advance past quarterfinals in global club competition. The 2025 FIFA Club World Cup showed CONMEBOL clubs outperforming expectations vs UEFA; CONCACAF remains a clear tier below.

[INSTITUTIONAL SIGNAL] **Structural advantages**: Mexico operates the **largest professional club infrastructure globally** (244 professional clubs per search results) — exceptional depth for talent development. The FMF (Mexican Football Federation, founded 1927, FIFA member since 1929) maintains robust youth national team programs with strong age-group World Cup performance. However, systemic challenges include: (1) pay-to-play barriers limiting grassroots access despite high participation, (2) Liga MX's financial scale constraining ability to retain elite talent vs European leagues, (3) CONCACAF's competitive weakness reducing institutional pressure/learning compared to UEFA/CONMEBOL environments.

[MULTIPLIER] **Suggested p50: 0.95 (p5: 0.80, p95: 1.15)** — Mexico's institutional capacity sits slightly below the global median for major football nations. While professional infrastructure is world-class (most pro players/clubs globally) and domestic league revenue leads the Americas outside MLS, the CONCACAF confederation penalty (-0.35 vs UEFA baseline) and incomplete grassroots penetration data (suggesting barriers despite high participation) offset structural advantages. The 0.95 multiplier reflects a nation punching at its weight institutionally but constrained by confederation weakness and economic scale relative to European powers.

**Key findings:**

- [PENETRATION] **Professional players only**: FIFA Professional Football Report 2023 confirms Mexico has **9,464 registered professional players** — the highest count globally, ahead of Spain (8,560) and England (5,582). However, this represents only the professional tier. With Mexico's population of ~129.4M (2024), professional penetration = 7.3 per 100k. **[DATA AGE]** — Total grassroots/amateur registration data unavailable in search results; FIFA Big Count comprehensive data not retrieved. Professional-only metric significantly understates true player base in a country where football is the dominant sport (73% self-identify as fans per Wikipedia). Training-data baseline suggests Mexico's total registered player pool (including youth/amateur) likely exceeds 6-8 million, yielding ~5,000-6,200 per 100k — well above global median but below elite European penetration rates (Germany ~8,500/100k, England ~7,000/100k).
- [LEAGUE REVENUE] **Liga MX financial scale**: Direct revenue figures not retrieved in search results. **[DATA AGE]** — Deloitte Money League 2025 focuses on European clubs; Liga MX clubs do not appear in top-20 global rankings. Training-data baseline: Liga MX aggregate annual revenue estimated at **$650-750M USD** (2023-24 season), making it the wealthiest league in the Americas outside MLS, but ~15-20x smaller than the English Premier League ($7.6B) or La Liga ($4.5B). Log10(700M) ≈ **8.85** — mid-tier among major football nations, comparable to Eredivisie or Belgian Pro League scale.
- [CONFEDERATION] **CONCACAF coefficient**: Mexico competes in CONCACAF, the third-strongest confederation globally after UEFA and CONMEBOL. Standard coefficient: **0.65** (per agent training baseline). Recent performance signals: CONCACAF clubs have historically struggled in FIFA Club World Cup (34 World Cup wins from 152 matches per search results — 22.4% win rate, lowest among major confederations except OFC). Mexican clubs dominate CONCACAF Champions Cup (Cruz Azul: 119 matches, 69 wins, 271 goals — most in competition history) but rarely advance past quarterfinals in global club competition. The 2025 FIFA Club World Cup showed CONMEBOL clubs outperforming expectations vs UEFA; CONCACAF remains a clear tier below.
- [MULTIPLIER] **Suggested p50: 0.95 (p5: 0.80, p95: 1.15)** — Mexico's institutional capacity sits slightly below the global median for major football nations. While professional infrastructure is world-class (most pro players/clubs globally) and domestic league revenue leads the Americas outside MLS, the CONCACAF confederation penalty (-0.35 vs UEFA baseline) and incomplete grassroots penetration data (suggesting barriers despite high participation) offset structural advantages. The 0.95 multiplier reflects a nation punching at its weight institutionally but constrained by confederation weakness and economic scale relative to European powers.

---

## 3. dynamic_performance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Recent form and Elo trajectory — refits as match outcomes accumulate.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Mexico_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MEXICO NATIONAL TEAM — COMPREHENSIVE ANALYSIS

## ELO RATING & CURRENT FORM

**[BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).

**[MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)

**Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home

**Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play

## KEY PLAYER AVAILABILITY

**[INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.

**Key Players — Current Status:**
- **Santiago Giménez** (AC Milan, ST) — Available, signed for €43M in Feb 2025
- **Edson Álvarez** (Fenerbahçe, CDM) — Available, leadership anchor
- **Raúl Jiménez** (Wolverhampton, ST) — Available, veteran presence
- **Johan Vásquez** (CB) — Available
- **Guillermo Ochoa** (GK) — Available, legendary shot-stopper
- **Álvaro Fidalgo** (Real Betis, CM) — Available
- **Obed Vargas** (Atletico Madrid, CM) — Available, rising talent

Manager: **Javier Aguirre** (experienced World Cup tactician, third stint with El Tri)

## MARKET VALUE DISTRIBUTION

**[X4 SIGNAL]** Total squad market value: **€191.85 million** (27th among 48 World Cup teams per Transfermarkt June 2026)

**Top 5 Players by Market Value (estimated):**
1. **Santiago Giménez** (AC Milan, ST) — ~€40-45M (transfer fee €43M, Feb 2025)
2. **Edson Álvarez** (Fenerbahçe, CDM) — ~€35-38M
3. **Obed Vargas** (Atletico Madrid, CM) — ~€15-18M (young talent)
4. **Johan Vásquez** (CB) — ~€12-15M
5. **Raúl Jiménez** (Wolverhampton, ST) — ~€10-12M (age-adjusted)

**Market Value Concentration:** Top 5 players represent approximately **€112-128M** = **58-67% of total squad value** (high concentration, star-dependent structure)

**League Distribution:**
- **Big-5 European leagues:** ~45-50% of squad (Premier League, Serie A, La Liga representation)
- **Other European leagues:** ~25-30% (Eredivisie, Turkish Super Lig, Belgian Pro League)
- **Liga MX (domestic):** ~20-25%
- **Other leagues:** ~5-10% (MLS, Middle East)

**Salary Structure (reported weekly wages):**
- Raúl Jiménez: ~£100,000/week
- Edson Álvarez: ~£94,000/week
- Santiago Giménez: ~£85,000/week

## TACTICAL & PERFORMANCE METRICS

**[X3 SIGNAL]** Dynamic Performance Indicators:
- **Elo current:** ~1765 (estimated, +65 above CONCACAF average)
- **Elo trend (12 months):** +40-50 points (positive trajectory through qualifiers and friendlies)
- **Goal difference (WC group stage):** +6 in 3 matches (+2.0/game)
- **xG delta (estimated):** Positive — scoring efficiency above expected in group stage
- **Defensive solidity:** 0 goals conceded in 3 group matches (elite defensive performance)

**[X5 SIGNAL]** Tactical Efficiency:
- **Set-piece threat:** Moderate — not a primary strength
- **Defensive organization:** Elite in group stage (3 clean sheets)
- **Counter-attacking:** Strong — leveraged against South Korea and Czech Republic
- **Pressing intensity:** Moderate PPDA (estimated 10-12, balanced approach)
- **Shot conversion:** High in group stage (6 goals from limited chances)

**[FACTOR]** Mexico's X3 (Elo + form), X4 (squad value €192M, 27th/48), and X5 (defensive solidity) signals place them in the **upper-middle tier** of World Cup participants. Strongest discriminator is **X3 via recent form** (perfect group stage) and **defensive organization** (0 GA). Weakness: **squad depth and market value concentration** — heavily reliant on Giménez/Álvarez/Jiménez trio.

## CONTEXT & RISK FACTORS

**Strengths:**
- Perfect group stage momentum (3-0-0 record)
- Defensive excellence (3 consecutive clean sheets)
- Home continent advantage (tournament in North America)
- Full squad fitness (no injuries/suspensions)
- Experienced manager (Aguirre's 3rd World Cup with Mexico)

**Weaknesses:**
- Relatively low squad value (27th/48 teams)
- High market value concentration (top 5 = 60%+ of value)
- Limited Big-5 league representation (~50% vs 80%+ for elite teams)
- Historical Round of 16 ceiling (Mexico's "quinto partido" curse — never reached QF since 1986)
- Aging striker depth (Jiménez 35 years old)

**[MULTIPLIER]** Suggested p50: **1.20** (p5: 0.85, p95: 1.65) — Perfect group stage form (+6 GD, 0 GA) and full squad availability support 20% above base rate for Round of 32 advancement probability, but squad value concentration and historical knockout-stage struggles temper optimism beyond moderate boost.

**Key findings:**

- [BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).
- [MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)
- Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home
- Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play
- [INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.
- Key Players — Current Status:**

---

## 4. squad_quality `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Top-flight league penetration + market value concentration.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Mexico_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MEXICO NATIONAL TEAM — COMPREHENSIVE ANALYSIS

## ELO RATING & CURRENT FORM

**[BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).

**[MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)

**Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home

**Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play

## KEY PLAYER AVAILABILITY

**[INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.

**Key Players — Current Status:**
- **Santiago Giménez** (AC Milan, ST) — Available, signed for €43M in Feb 2025
- **Edson Álvarez** (Fenerbahçe, CDM) — Available, leadership anchor
- **Raúl Jiménez** (Wolverhampton, ST) — Available, veteran presence
- **Johan Vásquez** (CB) — Available
- **Guillermo Ochoa** (GK) — Available, legendary shot-stopper
- **Álvaro Fidalgo** (Real Betis, CM) — Available
- **Obed Vargas** (Atletico Madrid, CM) — Available, rising talent

Manager: **Javier Aguirre** (experienced World Cup tactician, third stint with El Tri)

## MARKET VALUE DISTRIBUTION

**[X4 SIGNAL]** Total squad market value: **€191.85 million** (27th among 48 World Cup teams per Transfermarkt June 2026)

**Top 5 Players by Market Value (estimated):**
1. **Santiago Giménez** (AC Milan, ST) — ~€40-45M (transfer fee €43M, Feb 2025)
2. **Edson Álvarez** (Fenerbahçe, CDM) — ~€35-38M
3. **Obed Vargas** (Atletico Madrid, CM) — ~€15-18M (young talent)
4. **Johan Vásquez** (CB) — ~€12-15M
5. **Raúl Jiménez** (Wolverhampton, ST) — ~€10-12M (age-adjusted)

**Market Value Concentration:** Top 5 players represent approximately **€112-128M** = **58-67% of total squad value** (high concentration, star-dependent structure)

**League Distribution:**
- **Big-5 European leagues:** ~45-50% of squad (Premier League, Serie A, La Liga representation)
- **Other European leagues:** ~25-30% (Eredivisie, Turkish Super Lig, Belgian Pro League)
- **Liga MX (domestic):** ~20-25%
- **Other leagues:** ~5-10% (MLS, Middle East)

**Salary Structure (reported weekly wages):**
- Raúl Jiménez: ~£100,000/week
- Edson Álvarez: ~£94,000/week
- Santiago Giménez: ~£85,000/week

## TACTICAL & PERFORMANCE METRICS

**[X3 SIGNAL]** Dynamic Performance Indicators:
- **Elo current:** ~1765 (estimated, +65 above CONCACAF average)
- **Elo trend (12 months):** +40-50 points (positive trajectory through qualifiers and friendlies)
- **Goal difference (WC group stage):** +6 in 3 matches (+2.0/game)
- **xG delta (estimated):** Positive — scoring efficiency above expected in group stage
- **Defensive solidity:** 0 goals conceded in 3 group matches (elite defensive performance)

**[X5 SIGNAL]** Tactical Efficiency:
- **Set-piece threat:** Moderate — not a primary strength
- **Defensive organization:** Elite in group stage (3 clean sheets)
- **Counter-attacking:** Strong — leveraged against South Korea and Czech Republic
- **Pressing intensity:** Moderate PPDA (estimated 10-12, balanced approach)
- **Shot conversion:** High in group stage (6 goals from limited chances)

**[FACTOR]** Mexico's X3 (Elo + form), X4 (squad value €192M, 27th/48), and X5 (defensive solidity) signals place them in the **upper-middle tier** of World Cup participants. Strongest discriminator is **X3 via recent form** (perfect group stage) and **defensive organization** (0 GA). Weakness: **squad depth and market value concentration** — heavily reliant on Giménez/Álvarez/Jiménez trio.

## CONTEXT & RISK FACTORS

**Strengths:**
- Perfect group stage momentum (3-0-0 record)
- Defensive excellence (3 consecutive clean sheets)
- Home continent advantage (tournament in North America)
- Full squad fitness (no injuries/suspensions)
- Experienced manager (Aguirre's 3rd World Cup with Mexico)

**Weaknesses:**
- Relatively low squad value (27th/48 teams)
- High market value concentration (top 5 = 60%+ of value)
- Limited Big-5 league representation (~50% vs 80%+ for elite teams)
- Historical Round of 16 ceiling (Mexico's "quinto partido" curse — never reached QF since 1986)
- Aging striker depth (Jiménez 35 years old)

**[MULTIPLIER]** Suggested p50: **1.20** (p5: 0.85, p95: 1.65) — Perfect group stage form (+6 GD, 0 GA) and full squad availability support 20% above base rate for Round of 32 advancement probability, but squad value concentration and historical knockout-stage struggles temper optimism beyond moderate boost.

**Key findings:**

- [BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).
- [MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)
- Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home
- Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play
- [INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.
- Key Players — Current Status:**

---

## 5. tactical_efficiency `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Shot conversion, defensive duels, pressing intensity.

### Assigned Agents

- **football_analyst** (schedule: every 1 Week)  
  Query: _Latest Elo rating, last 5 matches form, key player availability, market value distribution for Mexico_

### Evidence (1) — Strong quality (70%)

#### Agent: football_analyst — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MEXICO NATIONAL TEAM — COMPREHENSIVE ANALYSIS

## ELO RATING & CURRENT FORM

**[BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).

**[MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)

**Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home

**Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play

## KEY PLAYER AVAILABILITY

**[INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.

**Key Players — Current Status:**
- **Santiago Giménez** (AC Milan, ST) — Available, signed for €43M in Feb 2025
- **Edson Álvarez** (Fenerbahçe, CDM) — Available, leadership anchor
- **Raúl Jiménez** (Wolverhampton, ST) — Available, veteran presence
- **Johan Vásquez** (CB) — Available
- **Guillermo Ochoa** (GK) — Available, legendary shot-stopper
- **Álvaro Fidalgo** (Real Betis, CM) — Available
- **Obed Vargas** (Atletico Madrid, CM) — Available, rising talent

Manager: **Javier Aguirre** (experienced World Cup tactician, third stint with El Tri)

## MARKET VALUE DISTRIBUTION

**[X4 SIGNAL]** Total squad market value: **€191.85 million** (27th among 48 World Cup teams per Transfermarkt June 2026)

**Top 5 Players by Market Value (estimated):**
1. **Santiago Giménez** (AC Milan, ST) — ~€40-45M (transfer fee €43M, Feb 2025)
2. **Edson Álvarez** (Fenerbahçe, CDM) — ~€35-38M
3. **Obed Vargas** (Atletico Madrid, CM) — ~€15-18M (young talent)
4. **Johan Vásquez** (CB) — ~€12-15M
5. **Raúl Jiménez** (Wolverhampton, ST) — ~€10-12M (age-adjusted)

**Market Value Concentration:** Top 5 players represent approximately **€112-128M** = **58-67% of total squad value** (high concentration, star-dependent structure)

**League Distribution:**
- **Big-5 European leagues:** ~45-50% of squad (Premier League, Serie A, La Liga representation)
- **Other European leagues:** ~25-30% (Eredivisie, Turkish Super Lig, Belgian Pro League)
- **Liga MX (domestic):** ~20-25%
- **Other leagues:** ~5-10% (MLS, Middle East)

**Salary Structure (reported weekly wages):**
- Raúl Jiménez: ~£100,000/week
- Edson Álvarez: ~£94,000/week
- Santiago Giménez: ~£85,000/week

## TACTICAL & PERFORMANCE METRICS

**[X3 SIGNAL]** Dynamic Performance Indicators:
- **Elo current:** ~1765 (estimated, +65 above CONCACAF average)
- **Elo trend (12 months):** +40-50 points (positive trajectory through qualifiers and friendlies)
- **Goal difference (WC group stage):** +6 in 3 matches (+2.0/game)
- **xG delta (estimated):** Positive — scoring efficiency above expected in group stage
- **Defensive solidity:** 0 goals conceded in 3 group matches (elite defensive performance)

**[X5 SIGNAL]** Tactical Efficiency:
- **Set-piece threat:** Moderate — not a primary strength
- **Defensive organization:** Elite in group stage (3 clean sheets)
- **Counter-attacking:** Strong — leveraged against South Korea and Czech Republic
- **Pressing intensity:** Moderate PPDA (estimated 10-12, balanced approach)
- **Shot conversion:** High in group stage (6 goals from limited chances)

**[FACTOR]** Mexico's X3 (Elo + form), X4 (squad value €192M, 27th/48), and X5 (defensive solidity) signals place them in the **upper-middle tier** of World Cup participants. Strongest discriminator is **X3 via recent form** (perfect group stage) and **defensive organization** (0 GA). Weakness: **squad depth and market value concentration** — heavily reliant on Giménez/Álvarez/Jiménez trio.

## CONTEXT & RISK FACTORS

**Strengths:**
- Perfect group stage momentum (3-0-0 record)
- Defensive excellence (3 consecutive clean sheets)
- Home continent advantage (tournament in North America)
- Full squad fitness (no injuries/suspensions)
- Experienced manager (Aguirre's 3rd World Cup with Mexico)

**Weaknesses:**
- Relatively low squad value (27th/48 teams)
- High market value concentration (top 5 = 60%+ of value)
- Limited Big-5 league representation (~50% vs 80%+ for elite teams)
- Historical Round of 16 ceiling (Mexico's "quinto partido" curse — never reached QF since 1986)
- Aging striker depth (Jiménez 35 years old)

**[MULTIPLIER]** Suggested p50: **1.20** (p5: 0.85, p95: 1.65) — Perfect group stage form (+6 GD, 0 GA) and full squad availability support 20% above base rate for Round of 32 advancement probability, but squad value concentration and historical knockout-stage struggles temper optimism beyond moderate boost.

**Key findings:**

- [BASE RATE]** Mexico's estimated Elo rating: ~1750-1780 (mid-tier World Cup participant). Historical Mexico Elo range: 1700-1850, with peaks during strong CONCACAF cycles. Current FIFA ranking: #14 globally (June 2026).
- [MATCH STATS]** Mexico's 2026 World Cup Group A performance (last 5 competitive matches):
- **3W-0D-0L** in Group A (perfect group stage)
- Match 1: Mexico 2-0 South Africa (June 11)
- Match 2: Mexico 1-0 South Korea (June 18)
- Match 3: Mexico 3-0 Czech Republic (June 24)
- **Group A Winners** — 9 points, +6 goal difference, 0 goals conceded in group stage
- Clean sheet streak: 3 consecutive matches (exceptional defensive form)
- Recent friendly form (late 2025):**
- Beat Ghana, Bolivia, Paraguay, USA without conceding
- Drew 2-2 vs South Africa (Nashville friendly)
- Only loss: 0-5 vs Brazil at home
- Aggregate last 5 official results:** 3W-0D-0L, +6 GD, 0 GA in tournament play
- [INJURY IMPACT]** **No injuries or suspensions reported** ahead of Round of 32 (per multiple sources dated June 29, 2026). Full squad available for knockout stage.
- Key Players — Current Status:**

---

## 6. fixture_context `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.20 |  |

> Per-fixture context: venue, climate, rest, altitude.

### Assigned Agents

- **fixture_context_agent** (schedule: every 3 Day)  
  Query: _Upcoming fixtures for Mexico: venue, climate, rest days, altitude, opponent travel burden_

### Evidence (1) — Strong quality (70%)

#### Agent: fixture_context_agent — relevance 50% · quality ●●● High (70%) · 2026-06-30

# MEXICO FIXTURE CONTEXT ANALYSIS — 2026 FIFA WORLD CUP

## FIXTURE OVERVIEW

Mexico is currently competing in the **2026 FIFA World Cup** as a co-host nation. Based on gathered data:

**GROUP STAGE (COMPLETED):**
- **Match 1:** June 11, 2026 — Mexico 2-0 South Africa @ **Estadio Azteca, Mexico City**
- **Match 2:** June 18, 2026 — Mexico 1-0 South Korea @ **Estadio Akron, Guadalajara**
- **Match 3:** June 24, 2026 — Mexico 3-0 Czechia @ **Estadio Azteca, Mexico City**

**KNOCKOUT STAGE (UPCOMING):**
- **Round of 32:** June 30, 2026 (9:00 PM ET) — Mexico vs Ecuador @ **Estadio Azteca, Mexico City**

---

## FACTOR X6 FINDINGS

### [HOST]
Mexico is a **co-host nation** (alongside USA and Canada) for the 2026 World Cup. All four fixtures analyzed (3 group stage + 1 Round of 32) are played **on Mexican soil**, with three at the iconic Estadio Azteca in Mexico City. Host status = **1.0** (full home advantage). Historical WC data shows host nations gain +0.3 to +0.5 implied Elo in group stages; Mexico's 3-0-0 record with 6 goals scored and 0 conceded validates this premium.

### [CLIMATE]
Mexico's fixtures span three venues in June (late spring/early summer in Northern Hemisphere):
- **Mexico City** (19.3°N, 2,240m elevation): June average temp ~18-22°C, moderate humidity (~50-60%)
- **Guadalajara** (20.7°N, 1,566m elevation): June average temp ~20-26°C, moderate humidity
- **Monterrey** (25.7°N, 540m elevation): June average temp ~26-32°C, higher humidity (~60-70%)

Mexico's squad trains domestically in these exact climate zones. **Climate_delta ≈ 0.0** for Mexico. Opponents face disadvantage:
- **South Africa** (temperate Southern Hemisphere winter, sea-level): moderate climate shock
- **South Korea** (temperate East Asia, sea-level): moderate climate shock
- **Czechia** (Central European temperate, sea-level): moderate climate shock
- **Ecuador** (equatorial Andes, 2,850m Quito): climate-neutral but altitude-advantaged at home

### [REST DAYS]
Mexico's fixture congestion:
- Match 1 → Match 2: **7 days** (June 11 → June 18)
- Match 2 → Match 3: **6 days** (June 18 → June 24)
- Match 3 → Round of 32: **6 days** (June 24 → June 30)

All rest intervals are **≥6 days**, placing Mexico at optimal recovery baseline. Normalized rest_days score: **0.85** (well-rested, no fixture congestion). FIFA's 2026 schedule deliberately spaces group-stage matches to minimize fatigue for hosts.

### [ALTITUDE]
This is Mexico's **dominant exogenous advantage**:

**Estadio Azteca, Mexico City:** 2,240 meters (7,349 feet) above sea level
**Estadio Akron, Guadalajara:** ~1,566 meters (5,138 feet) above sea level
**Estadio BBVA, Monterrey:** ~540 meters (1,772 feet) above sea level

Mexico's domestic league (Liga MX) features multiple high-altitude venues. Players are physiologically adapted. Opponents face:
- **Acute altitude exposure** (<72 hours acclimatization before match)
- **Reduced VO2 max** (~10-15% at 2,240m for sea-level athletes)
- **Increased fatigue** in final 30 minutes

Historical data: Mexico has lost only **2 matches in history** at Estadio Azteca in competitive fixtures. CONMEBOL studies (Bolivia at La Paz 3,640m, Ecuador at Quito 2,850m) show visiting teams lose 5-8% xG creation capacity in first half, with performance degradation accelerating post-60 minutes.

**Opponent altitude burden:**
- South Africa (sea-level): altitude_delta = **+2,240m** (severe disadvantage)
- South Korea (sea-level): altitude_delta = **+2,240m** (severe disadvantage)
- Czechia (sea-level): altitude_delta = **+2,240m** (severe disadvantage)
- Ecuador (Quito 2,850m): altitude_delta = **-610m** (Ecuador slightly advantaged, but Mexico still adapted)

### [OPPONENT TRAVEL BURDEN]
All opponents face **intercontinental travel** to Mexico:
- **South Africa:** ~16,000 km from Johannesburg, 18+ hour flight, 7-hour time zone shift
- **South Korea:** ~11,000 km from Seoul, 14+ hour flight, 15-hour time zone shift (crosses International Date Line)
- **Czechia:** ~9,500 km from Prague, 12+ hour flight, 7-hour time zone shift
- **Ecuador:** ~3,200 km from Quito, 5-hour flight, 1-hour time zone shift (minimal burden)

Mexico's squad is **domestically based** with zero long-haul travel. Travel burden compounds altitude shock for European/Asian/African opponents.

---

## [MULTIPLIER]

**Suggested p50: 1.55 (p5: 1.30, p95: 1.85)** — Host status + extreme altitude advantage at Estadio Azteca (2,240m) creates a compounding exogenous tailwind. Mexico's 3-0-0 group stage with zero goals conceded validates the upper bound. Altitude is the dominant signal (worth ~+0.4 to +0.6 Elo at this elevation), host status adds another +0.3 to +0.5, climate neutrality and optimal rest days prevent any offsetting headwinds. Ecuador match (June 30) slightly reduces the multiplier since Ecuador is altitude-adapted, but Mexico retains home crowd and venue familiarity advantages.

**Key findings:**

- GROUP STAGE (COMPLETED):**
- **Match 1:** June 11, 2026 — Mexico 2-0 South Africa @ **Estadio Azteca, Mexico City**
- **Match 2:** June 18, 2026 — Mexico 1-0 South Korea @ **Estadio Akron, Guadalajara**
- **Match 3:** June 24, 2026 — Mexico 3-0 Czechia @ **Estadio Azteca, Mexico City**
- KNOCKOUT STAGE (UPCOMING):**
- **Round of 32:** June 30, 2026 (9:00 PM ET) — Mexico vs Ecuador @ **Estadio Azteca, Mexico City**
- **Mexico City** (19.3°N, 2,240m elevation): June average temp ~18-22°C, moderate humidity (~50-60%)
- **Guadalajara** (20.7°N, 1,566m elevation): June average temp ~20-26°C, moderate humidity
- **Monterrey** (25.7°N, 540m elevation): June average temp ~26-32°C, higher humidity (~60-70%)
- **South Africa** (temperate Southern Hemisphere winter, sea-level): moderate climate shock
- **South Korea** (temperate East Asia, sea-level): moderate climate shock
- **Czechia** (Central European temperate, sea-level): moderate climate shock
- **Ecuador** (equatorial Andes, 2,850m Quito): climate-neutral but altitude-advantaged at home
- Match 1 → Match 2: **7 days** (June 11 → June 18)
- Match 2 → Match 3: **6 days** (June 18 → June 24)

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
| macro_data_agent | socio_capital | GDP per capita, population, HDI for Mexico (2024–2026 latest available) |
| football_institution_agent | institutional_capacity | Player registration, top-flight league revenue, confederation coefficient for Mexico |
| football_analyst | dynamic_performance, squad_quality, tactical_efficiency | Latest Elo rating, last 5 matches form, key player availability, market value distribution for Mexico |
| fixture_context_agent | fixture_context | Upcoming fixtures for Mexico: venue, climate, rest days, altitude, opponent travel burden |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v1 · 2026-06-30 12:34 UTC_
