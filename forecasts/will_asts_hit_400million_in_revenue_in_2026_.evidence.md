# Will ASTS hit 400Million in revenue in 2026?

**Probability:** 69.9% · **Version:** v9 · **Updated:** 2026-07-22 16:05 UTC

**Confidence:** Low (28%) · **Drivers:** 5 · **Evidence:** 3 · **Agents:** 5

---

## Inside View

**Probability: 69.9%**

Inside view: model evaluates to 69.9% (p5=34.4%, p95=116.8%). Outside view (base rate): 15.0%. Key drivers: satellite_deployment_execution, carrier_partnership_monetization, market_demand_adoption. Most influential: capital_availability_execution (43%), carrier_partnership_monetization (21%), satellite_deployment_execution (20%).

**Forecast Confidence:** Low (28%)

**Divergence from base rate:** 55pp above (69.9% vs 15.0%)

---

## Outside View (Base Rate)

**15.0%** — High-growth satellite/space infrastructure companies reaching $400M revenue within 3-4 years of commercial service launch

- **Sample size:** n=20
- **Source:** macro_forecaster

AST SpaceMobile (ASTS) is attempting to build a space-based cellular broadband network. Reference class includes companies like Iridium (took 6+ years post-bankruptcy restructuring), Globalstar (never reached $400M in comparable timeframe), OneWeb (still ramping), and high-growth space/telecom infrastructure plays. Of ~20 comparable satellite/space infrastructure ventures in the past 25 years, only ~3 (15%) hit $400M revenue within their first 3-4 years of commercial operations. ASTS reported ~$1.8M revenue in 2023, projects commercial service starting 2024-2025. Reaching $400M by end of 2026 requires ~200x growth in 2-3 years - exceptionally rare even for successful space ventures.

---

## Simulation Distribution

**10000 iterations** · p5 = 34.4% · median = 66.5% · p95 = 116.8% · σ = 0.257

```
▁▃▅▇██▇▆▅▃▃▂▂▁▁▁▁▁▁▁
```

| Bin center | Count | % of sims |
|---|---|---|
| 18.3% | 49 | 0.5% |
| 27.7% | 338 | 3.4% |
| 37.1% | 853 | 8.5% |
| 46.5% | 1339 | 13.4% |
| 55.9% | 1500 | 15.0% |
| 65.3% | 1450 | 14.5% |
| 74.7% | 1276 | 12.8% |
| 84.1% | 1021 | 10.2% |
| 93.5% | 773 | 7.7% |
| 102.9% | 533 | 5.3% |
| 112.3% | 375 | 3.8% |
| 121.7% | 201 | 2.0% |
| 131.1% | 134 | 1.3% |
| 140.5% | 74 | 0.7% |
| 149.9% | 48 | 0.5% |
| 159.2% | 19 | 0.2% |
| 168.6% | 10 | 0.1% |
| 178.0% | 4 | 0.0% |
| 187.4% | 1 | 0.0% |
| 196.8% | 2 | 0.0% |

---

## Forecast Index (version history)

| v | timestamp | model | base | Δ(model−base) | note |
|---|---|---|---|---|---|
| v1 | 2026-07-22 15:57 | 15.0% | 15.0% | +0.0pp | Initial: 15.0% base=15%, 5 drivers, 1 evidence |
| v2 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v3 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v4 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v5 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v6 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v7 | 2026-07-22 15:58 | 15.0% | 15.0% | +0.0pp | 15.0% (→), 5 drivers, 1 evidence, 1 agents |
| v8 | 2026-07-22 15:58 | 97.8% | 15.0% | +82.8pp | 97.8% (+83pp), 5 drivers, 1 evidence, 1 agents |
| v9 | 2026-07-22 16:05 | 69.9% | 15.0% | +54.9pp | 69.9% (-28pp), 5 drivers, 3 evidence, 3 agents |

**Model line:** ```▁▁▁▁▁▁▁█▆``` (range 15.0% – 97.8%)

---

## 1. satellite_deployment_execution `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.60 | 1.00 | 1.40 | multiplier |

> ASTS must successfully launch and operationalize its Block 1 BlueBird satellites (5 planned for 2024-2025) to enable commercial service. Historical satellite deployment has 70-80% on-time success rate for new constellations. Delays of 6-12 months are common (Starlink, OneWeb both experienced). Early deployment accelerates revenue ramp; delays compress the 2026 revenue window. Technical risk includes in-orbit performance validation of novel large-aperture phased array technology - no direct precedent at this scale.

### Assigned Agents

- **entity_investigator** (schedule: once)  
  Query: _AST SpaceMobile (ASTS ticker) satellite deployment track record and risk factors: analyze Block 1 BlueBird launch schedule adherence, technical readiness of phased array technology, manufacturing capacity at MDA Space, historical on-time delivery rate for comparable first-generation satellite constellations. Return probability distribution of deployment timeline as multiplier (1.0 = on schedule, <1.0 = delays reduce 2026 revenue, >1.0 = early deployment accelerates)._

### Evidence (1) — Weak quality (25%)

#### Agent: entity_investigator — relevance 50% · quality ●○○ Low (25%) · 2026-07-22



---

## 2. carrier_partnership_monetization `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.10 | 1.60 | multiplier |

> ASTS has announced partnerships with AT&T, Verizon, Vodafone, Rakuten, and others covering ~1.8B subscribers. Revenue depends on converting these MOUs into binding commercial agreements with meaningful revenue share or capacity payments. Comparable wholesale satellite-to-telco deals (Ligado, Globalstar-Apple) took 2-4 years to monetize and often at lower-than-projected rates. Upside if ASTS captures premium pricing for unique direct-to-device capability; downside if carriers delay commercial deployment or negotiate lower rates.

### Assigned Agents

- **market_research** (schedule: once)  
  Query: _Satellite-to-mobile network operator partnership monetization: analyze historical revenue conversion rates and timelines for wholesale satellite capacity agreements (Ligado, Globalstar, Iridium carrier deals). For ASTS with partnerships covering 1.8B subscribers targeting $400M revenue by 2026, estimate probability distribution of revenue per subscriber and partnership activation rate as multiplier relative to management guidance. Include competitive pressure from Starlink direct-to-cell and Apple satellite services._

_No evidence collected yet. Assign an agent to research this driver._

---

## 3. market_demand_adoption `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.80 | 1.00 | 1.30 | multiplier |

> Revenue depends on end-user adoption of space-based cellular service for coverage gaps, IoT, and emergency connectivity. Market sizing estimates vary widely ($10B-$100B TAM by 2030). Adoption drivers include pricing relative to terrestrial alternatives, device compatibility (ASTS claims standard smartphones work), and use case validation. Starlink, Apple Emergency SOS, and Lynk Global are competing. Faster-than-expected enterprise IoT adoption or regulatory mandates (e.g., FirstNet requirements) could accelerate; consumer price sensitivity or technical limitations could dampen.

### Assigned Agents

- **market_research** (schedule: once)  
  Query: _Direct-to-device satellite connectivity market adoption 2024-2026: estimate market size growth, enterprise vs consumer split, pricing elasticity, and competitive positioning of AST SpaceMobile vs Starlink direct-to-cell and Apple satellite services. For a target of $400M revenue by end 2026, return probability distribution of market penetration rate and ARPU as multiplier. Include regulatory tailwinds (FirstNet, rural broadband mandates) and technology substitution risk._

_No evidence collected yet. Assign an agent to research this driver._

---

## 4. capital_availability_execution `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.25 | 0.65 | 0.95 | multiplier |

> ASTS is pre-revenue and capital-intensive. Company has raised ~$500M+ but will need additional funding to complete Block 1 and scale to Block 2 constellation. Equity dilution, debt financing, or strategic investment required. Execution risk includes manufacturing scale-up, ground infrastructure deployment, and operational burn rate management. Comparable space ventures (OneWeb bankruptcy 2020, Virgin Orbit failure 2023) show capital constraints can halt operations. Median multiplier <1.0 reflects elevated execution risk for cash-burning pre-revenue space company.

### Assigned Agents

- **equity_analyst** (schedule: once)  
  Query: _AST SpaceMobile (ticker ASTS) financial health and capital runway: analyze current cash position, quarterly burn rate, debt obligations, and equity dilution risk. Compare to capital requirements for completing Block 1 satellite deployment and scaling to commercial operations by 2026. Assess probability of successful capital raise vs operational constraints impacting revenue ramp. Return as multiplier where 1.0 = adequate capital and execution, <1.0 = capital/execution constraints delay revenue, >1.0 = better-than-expected funding or cost efficiency._

### Evidence (1) — Strong quality (75%)

#### Agent: equity_analyst — relevance 50% · quality ●●● High (75%) · 2026-07-22

# AST SPACEMOBILE (ASTS) FINANCIAL HEALTH & CAPITAL RUNWAY ANALYSIS

## COMPANY SNAPSHOT
**Current Position (July 2026):**
- Market Cap: $25.7B
- Current Price: $63.25 (Range: $36.08-$133.86)
- Beta: 2.677 (highly volatile)
- Sector: Communication Services / Telecommunications
- Stage: Pre-commercial revenue (satellite deployment phase)

## CAPITAL POSITION DASHBOARD

```
METRIC                          Q1 2026    POST-FINANCING   TREND
Cash & Equivalents              $3.5B      $3.8B+          ↑ Strong
Quarterly Revenue               $14.7M     —               Early stage
Q1 Operating Expenses           $164.1M    —               ↑ Scaling
Q1 Adjusted OpEx (ex-COGS)      $79.8M     —               ↑ 19% QoQ
Q1 Net Loss                     ($191M)    —               High burn
Quarterly Cash Burn (implied)   ~$175M+    —               Accelerating
```

## KEY FINDINGS

**[BASE RATE]** Pre-revenue space infrastructure companies completing capital-intensive deployment phases: historical success rate ~40-50% reaching commercial scale without significant delays or additional dilution (SpaceX, Iridium precedents; OneWeb bankruptcy/restructure as counter-example).

**[CAPITAL POSITION]** Pro forma cash position $3.8B as of June 30, 2026 following $1.15B convertible notes offering (1.625% interest, $149.20 conversion price = 20% premium). Previous position was $3.5B at March 31, 2026. This represents substantial liquidity cushion (BusinessWire July 21, 2026).

**[BURN RATE]** Q1 2026 operating expenses $164.1M with adjusted OpEx (excluding cost of revenue) $79.8M, up 19% from Q4 2025's $66.8M. Net loss $191M in Q1 2026. Implied quarterly burn rate $175-200M and accelerating as satellite production scales. At current burn, $3.8B provides ~5-6 quarters of runway without revenue growth (TipRanks Q1 2026 earnings; MerlinTrader July 2026).

**[DEPLOYMENT STATUS]** BlueBirds 8-10 launched and activated June 2026; BlueBirds 11-33 in advanced production with assembly lines processing through BlueBird 37. Target: ~45 satellites in orbit by end of 2026, scaling to 6 satellites/month manufacturing capacity. However, **commercial service launch delayed from late 2026 to early 2027** per regulatory filing (OuterSpaceToday July 17, 2026).

**[REVENUE TRAJECTORY]** Q1 2026 revenue only $14.7M (primarily U.S. government contracts, not commercial). Management guidance: $150-200M total 2026 revenue, with potential $1B revenue run rate in 2027. Consensus projects $170M (2026) → $2.84B (2029). However, commercial launch delay pushes meaningful revenue 6+ months further out (Yahoo Finance, Simply Wall St July 2026).

**[DILUTION RISK]** Recent $1.15B convertible notes offering triggered 11-13% stock decline on dilution concerns. Conversion price $149.20 vs current $63.25 = significant underwater position, but capped call transactions provide some protection. Share count already elevated; further equity raises likely needed if commercial ramp slower than projected or if satellite costs exceed estimates (Simply Wall St, TIKR July 2026).

**[CAPITAL ADEQUACY ASSESSMENT]** 
- **Positive factors:** $3.8B cash provides multi-quarter runway; 95% vertically integrated manufacturing (500K+ sq ft Texas facilities) controls costs; $1.2B+ in contracted revenue commitments from 60 global carriers; successful satellite activations demonstrate technical viability.
- **Negative factors:** Commercial launch delayed 6+ months to early 2027; burn rate accelerating ($175-200M/quarter and rising); no meaningful commercial revenue yet; Q1 loss included $155-160M BlueBird 7 write-off (insurance offset); capital intensity requires ~$2-3B more to reach full Block 1 deployment based on current trajectory.

**[EXECUTION RISK]** Timeline slippage already evident (commercial launch pushed to early 2027). At $200M/quarter burn scaling to $250M+ as production ramps, company burns through ~$800M-1B annually. With $3.8B cash, this provides ~3-4 years runway IF no further delays and IF revenue ramps as projected. However, space infrastructure projects historically experience 12-24 month delays and 30-50% cost overruns. Additional capital raise likely needed in 2027-2028 timeframe.

**[DEBT OBLIGATIONS]** $1.15B convertible notes at 1.625% interest = ~$19M annual interest expense (manageable). However, conversion at $149.20 vs current $63.25 means debt likely stays as debt unless stock more than doubles, adding refinancing risk at 2034 maturity.

## FORECAST IMPACT ASSESSMENT

**Capital Runway Adequacy:** Current $3.8B position provides 15-19 months of runway at current/projected burn rates before requiring additional capital. Commercial launch delay to early 2027 means revenue ramp pushed 6+ months, compressing the timeline between cash depletion and revenue inflection.

**Probability Assessment:**
- **Adequate capital scenario (1.0x):** Company completes Block 1 deployment on revised timeline, achieves $150-200M 2026 revenue, scales to $1B run rate by late 2027, requires one more capital raise in 2027 but from position of strength with commercial traction.
- **Constraint scenario (<1.0x):** Further delays push commercial launch to mid-2027 or later, burn rate exceeds $250M/quarter, revenue ramp slower than projected, requires emergency capital raise from weakened position causing significant dilution, operational constraints delay full deployment.
- **Upside scenario (>1.0x):** Commercial launch accelerates back to late 2026, early revenue traction exceeds guidance, manufacturing efficiencies reduce burn rate, strategic partnerships provide non-dilutive capital.

**Relevance Score:** 0.95 — Financial health and capital runway directly determine ability to complete deployment and reach commercial operations.

**Confidence:** 0.75 — High-quality data on cash position and recent financing, but uncertainty around actual deployment costs, revenue ramp timing, and execution risk in novel space infrastructure.

**[MULTIPLIER]** Suggested p50: 0.65 (p5: 0.35, p95: 1.10) — Commercial launch delay to early 2027, accelerating burn rate, and 15-19 month runway before next capital raise create 35% below-base probability that capital constraints delay revenue ramp versus adequate execution scenario.

**Key findings:**

- Current Position (July 2026):**
- Market Cap: $25.7B
- Current Price: $63.25 (Range: $36.08-$133.86)
- Beta: 2.677 (highly volatile)
- Sector: Communication Services / Telecommunications
- Stage: Pre-commercial revenue (satellite deployment phase)
- Cash & Equivalents              $3.5B      $3.8B+          ↑ Strong
- Quarterly Revenue               $14.7M     —               Early stage
- Q1 Operating Expenses           $164.1M    —               ↑ Scaling
- Q1 Adjusted OpEx (ex-COGS)      $79.8M     —               ↑ 19% QoQ
- Q1 Net Loss                     ($191M)    —               High burn
- Quarterly Cash Burn (implied)   ~$175M+    —               Accelerating
- [BASE RATE]** Pre-revenue space infrastructure companies completing capital-intensive deployment phases: historical success rate ~40-50% reaching commercial scale without significant delays or additional dilution (SpaceX, Iridium precedents; OneWeb bankruptcy/restructure as counter-example).
- [CAPITAL POSITION]** Pro forma cash position $3.8B as of June 30, 2026 following $1.15B convertible notes offering (1.625% interest, $149.20 conversion price = 20% premium). Previous position was $3.5B at March 31, 2026. This represents substantial liquidity cushion (BusinessWire July 21, 2026).
- [BURN RATE]** Q1 2026 operating expenses $164.1M with adjusted OpEx (excluding cost of revenue) $79.8M, up 19% from Q4 2025's $66.8M. Net loss $191M in Q1 2026. Implied quarterly burn rate $175-200M and accelerating as satellite production scales. At current burn, $3.8B provides ~5-6 quarters of runway without revenue growth (TipRanks Q1 2026 earnings; MerlinTrader July 2026).

---

## 5. regulatory_spectrum_clearance `continuous`

| p5 | p50 | p95 | unit |
|---:|---:|---:|---|
| 0.70 | 1.00 | 1.20 | multiplier |

> ASTS requires FCC approval in the US and regulatory clearance in international markets (EU, Japan, etc.) to operate commercially. FCC granted initial authorization but full commercial license and spectrum coordination with terrestrial operators is ongoing. International approvals vary by jurisdiction - some fast (UK), others slow (EU fragmentation). Delays in key markets (US, EU represent ~40% of revenue potential) would compress 2026 revenue. Faster-than-expected approvals or spectrum allocation priority could accelerate. Median 1.0 assumes on-track regulatory path per company guidance.

### Assigned Agents

- **macro_forecaster** (schedule: once)  
  Query: _Satellite spectrum regulatory approval timeline and risk for AST SpaceMobile: analyze FCC commercial license process for space-based cellular (S-band coordination with terrestrial operators), international regulatory approval timelines in EU, Japan, and other key markets. Historical base rate for spectrum approval delays in satellite-to-mobile services. Return probability distribution of regulatory clearance timing as multiplier (1.0 = on schedule for 2025-2026 commercial service, <1.0 = delays, >1.0 = expedited approvals)._

_No evidence collected yet. Assign an agent to research this driver._

---

## General Evidence (1)

_Evidence not linked to a specific driver._

### Agent: fermi (Claude API) — relevance 50% · quality ●○○ Low (35%)



---

## Methodology

This forecast uses a **Fermi decomposition** approach based on Tetlock superforecasting methodology:

1. **Outside view** — anchor to a base rate from a relevant reference class
2. **Inside view** — decompose into independent drivers, each represented as a probability multiplier
3. **Monte Carlo simulation** — run 10,000 iterations sampling from driver distributions
4. **Normalization** — `P = base_rate × (simulation_mean / baseline_mean)` clamped to [1%, 99%]

### Model

```
model: satellite_deployment_execution * carrier_partnership_monetization * market_demand_adoption * capital_availability_execution * regulatory_spectrum_clearance
```

### Research Agents

| Agent | Driver | Query |
|---|---|---|
| entity_investigator | satellite_deployment_execution | AST SpaceMobile (ASTS ticker) satellite deployment track record and risk factors: analyze Block 1 BlueBird launch schedule adherence, technical readiness of phased array technology, manufacturing capacity at MDA Space, historical on-time delivery rate for comparable first-generation satellite constellations. Return probability distribution of deployment timeline as multiplier (1.0 = on schedule, <1.0 = delays reduce 2026 revenue, >1.0 = early deployment accelerates). |
| market_research | carrier_partnership_monetization | Satellite-to-mobile network operator partnership monetization: analyze historical revenue conversion rates and timelines for wholesale satellite capacity agreements (Ligado, Globalstar, Iridium carrier deals). For ASTS with partnerships covering 1.8B subscribers targeting $400M revenue by 2026, estimate probability distribution of revenue per subscriber and partnership activation rate as multiplier relative to management guidance. Include competitive pressure from Starlink direct-to-cell and Apple satellite services. |
| market_research | market_demand_adoption | Direct-to-device satellite connectivity market adoption 2024-2026: estimate market size growth, enterprise vs consumer split, pricing elasticity, and competitive positioning of AST SpaceMobile vs Starlink direct-to-cell and Apple satellite services. For a target of $400M revenue by end 2026, return probability distribution of market penetration rate and ARPU as multiplier. Include regulatory tailwinds (FirstNet, rural broadband mandates) and technology substitution risk. |
| equity_analyst | capital_availability_execution | AST SpaceMobile (ticker ASTS) financial health and capital runway: analyze current cash position, quarterly burn rate, debt obligations, and equity dilution risk. Compare to capital requirements for completing Block 1 satellite deployment and scaling to commercial operations by 2026. Assess probability of successful capital raise vs operational constraints impacting revenue ramp. Return as multiplier where 1.0 = adequate capital and execution, <1.0 = capital/execution constraints delay revenue, >1.0 = better-than-expected funding or cost efficiency. |
| macro_forecaster | regulatory_spectrum_clearance | Satellite spectrum regulatory approval timeline and risk for AST SpaceMobile: analyze FCC commercial license process for space-based cellular (S-band coordination with terrestrial operators), international regulatory approval timelines in EU, Japan, and other key markets. Historical base rate for spectrum approval delays in satellite-to-mobile services. Return probability distribution of regulatory clearance timing as multiplier (1.0 = on schedule for 2025-2026 commercial service, <1.0 = delays, >1.0 = expedited approvals). |


---

_Generated by [Fermi Console](https://agent-bestiary.world) · v9 · 2026-07-22 16:05 UTC_
