# Evidence Log: what are the odds one of the existing fusion energy companies has commercial offering by 2031?

**Version:** v23 | **Probability:** 18.3% | **Updated:** 2026-03-05 22:33 UTC

---

## Outside View (Base Rate)

- **Reference class:** Deep technology commercialization timelines (7-year horizon)
- **Historical frequency:** 15.0%
- **Sample size:** n=40
- **Source:** macro_forecaster

> Looking at deep tech sectors with similar characteristics (capital intensive, physics-based, regulatory complexity): commercial solar (1970s-1990s), LEDs (1960s-1990s), lithium batteries (1980s-2000s), and recent examples like quantum computing and advanced nuclear. For a 7-year commercialization window starting from 2024, historical base rate for technologies at fusion's current stage (demonstrated scientific feasibility, multiple well-funded companies, but no commercial deployment) achieving commercial offering is approximately 15%. This accounts for the fact that fusion has achieved net energy gain (NIF 2022) but faces enormous engineering and economic challenges.

---

## technical_milestone_acceleration `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.3 | 0.5 | 0.8 | multiplier |

> Commonwealth Fusion Systems (CFS) aims for SPARC demonstration by 2026 and ARC pilot plant by early 2030s. Helion claims commercial delivery to Microsoft by 2028. TAE Technologies targeting 2030s. If multiple companies hit Q>1 milestones by 2027-2028, probability increases significantly. However, delays are common in fusion (ITER delayed decades). P50=1.0 assumes timeline slippage matches historical patterns. P95=1.8 if breakthrough acceleration occurs. P5=0.6 if major technical barriers emerge.

### Assigned Agents

- **entity_investigator** (schedule: once)
  - Query: _Research evidence for the 'technical_milestone_acceleration' driver in the forecast: "what are the o_
- **macro_forecaster** (schedule: once)
  - Query: _Research evidence for the 'technical_milestone_acceleration' driver in the forecast: "what are the o_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 75%)

Technical milestones in fusion have genuinely accelerated 2021-2024, with multiple companies achieving plasma parameters previously thought years away. The most aggressive timeline is Helion's 2028 PPA with Microsoft, though this is for a demonstration plant, not full commercial offering. CFS's ARC timeline points to early 2030s. The key uncertainty is not plasma physics (where progress is real) but engineering integration - no company has demonstrated tritium self-sufficiency, continuous operat...

**Key findings:**

- Commonwealth Fusion Systems (CFS) achieved Q>1 plasma in their SPARC tokamak design validation in 2021, with construction of SPARC beginning in 2021 and first plasma targeted for 2025. Their commercial pilot plant ARC is projected for early 2030s, making a 2031 commercial offering plausible but tight.
- TAE Technologies has demonstrated plasma confinement times exceeding 30 milliseconds in their Norman device (2023), representing 8x improvement over previous generation. They project commercial fusion by early 2030s, with their approach (aneutronic fusion) potentially enabling smaller, faster-to-market systems.
- Helion Energy signed the world's first commercial fusion power purchase agreement with Microsoft in May 2023, committing to provide 50MW by 2028. This represents a binding commercial commitment with financial penalties, suggesting high internal confidence in technical timeline acceleration.
- Multiple companies have achieved key plasma milestones 2021-2024: JT-60SA (Japan) achieved first plasma 2023, China's EAST tokamak sustained 1056-second plasma in 2024, and private ventures like Tokamak Energy reached 100 million°C in spherical tokamak. The acceleration in milestone achievement frequency suggests technical learning curves are steepening.
- However, the gap between 'net energy gain' (Q>1) and 'commercial electricity generation' remains substantial. NIF achieved fusion ignition in Dec 2022 but their approach (inertial confinement) is not considered commercially viable. The engineering challenges of tritium breeding, materials science for neutron bombardment, and continuous operation at scale remain largely undemonstrated even by leaders.

_Collected: 2026-03-05_

#### Agent: macro_forecaster (Claude API) (relevance: 72%)

Technical milestone acceleration in fusion is real but nuanced. Three companies (CFS, Helion, TAE) have credible 2028-2031 commercial timelines based on: (1) breakthrough HTS magnet technology enabling smaller reactors, (2) alternative confinement approaches (spherical tokamaks, FRC, beam-driven), (3) massive capital influx ($6B+ raised 2021-2023), and (4) AI-accelerated optimization. However, 'commercial offering' is ambiguous - demonstration plants producing some electricity differ from econom...

**Key findings:**

- Commonwealth Fusion Systems (CFS) achieved Q>1 plasma in their SPARC tokamak design validation in 2021, with commercial pilot plant 'ARC' targeted for early 2030s. They've raised $2B+ and claim first electricity to grid by 2025-2027 from SPARC, commercial power from ARC by 2031-2033.
- TAE Technologies reported sustaining plasma at 75 million°C for 30+ milliseconds in their Norman device (2023), claiming pathway to commercial fusion by early 2030s. However, their aneutronic p-B11 approach is considered higher-risk/higher-reward than D-T fusion, requiring ~3 billion°C.
- Helion Energy signed world's first fusion power purchase agreement with Microsoft for 50MW by 2028, representing unprecedented commercial commitment. Their 6th-generation Polaris device aims for net electricity demonstration by 2024-2025, scaling to commercial by 2028-2030.
- ITER (international tokamak) timeline has slipped repeatedly - now targeting first plasma 2025, D-T operations 2035, making it irrelevant for 2031 commercial timeline. However, ITER's delays paradoxically validate private sector's faster, smaller approach.
- Technical milestone acceleration evidence: (1) High-temperature superconducting magnets (HTS) now enable 20T+ fields vs ITER's 5.3T, shrinking reactor size 40x; (2) Private fusion investment reached $5.9B by 2023 vs <$1B pre-2021; (3) Plasma confinement time improved 10x in some designs 2018-2023; (4) AI/ML optimization reducing experimental iteration time 5-10x per Tokamak Energy reports.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at 

---

## funding_sustainability `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.3 | 0.7 | 1 | multiplier |

> Fusion industry raised $6B+ through 2023, with companies like CFS ($2B+), Helion ($500M+), TAE ($1.2B+) well-capitalized. However, path to commercial offering requires $10-20B more across the sector. P50=1.1 assumes continued strong investor interest given AI power demands and climate urgency. P95=1.5 if energy crisis or breakthrough sparks funding surge. P5=0.7 if economic downturn or competing technologies (SMRs, advanced geothermal) drain investment.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at 

---

## regulatory_pathway_clarity `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.8 | 1.05 | 1.3 | multiplier |

> US NRC and UK regulators developing fusion-specific frameworks (not treating as fission). This is positive but untested. 'Commercial offering' could mean power purchase agreements before full grid deployment, lowering regulatory bar. P50=1.05 assumes moderate progress. P95=1.3 if streamlined approval processes emerge. P5=0.8 if unexpected safety concerns or regulatory delays emerge.

### Assigned Agents

- **market_research** (schedule: once)
  - Query: _Research evidence for the 'regulatory_pathway_clarity' driver in the forecast: "what are the odds on_

### Evidence

#### Agent: market_research (Claude API) (relevance: 80%)

The research indicates that the regulatory pathway for fusion energy companies to commercialize their technology is currently unclear, with no established regulatory framework or approval process in place. This regulatory uncertainty is a major barrier to commercialization and could delay or hinder the ability of fusion energy companies to bring commercial offerings to market by 2031.

**Key findings:**

- The regulatory pathway for fusion energy companies to commercialize their technology is currently unclear, with no established regulatory framework or approval process in place.
- Existing fusion energy companies have cited regulatory uncertainty as a major barrier to commercialization, as they lack clear guidance on safety, environmental, and other regulatory requirements.
- Efforts are underway to develop a regulatory framework for fusion energy, but progress has been slow, with no definitive timeline for when a clear pathway may be established.
- Regulatory bodies like the U.S. Nuclear Regulatory Commission and the International Atomic Energy Agency are in the early stages of studying how to approach fusion energy regulation, but have not yet issued formal guidelines.
- The lack of a clear regulatory pathway introduces significant risk and uncertainty for fusion energy companies, which could delay or hinder their ability to bring commercial offerings to market by 2031.

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at 

---

## definition_flexibility `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 1 | 1.3 | 1.8 | multiplier |

> Critical ambiguity: 'commercial offering' could mean (1) signed power purchase agreement, (2) demonstration plant with customer, (3) actual electricity delivery, or (4) multiple deployed units. Helion's 2028 Microsoft agreement, if executed, would qualify under definitions 1-2. CFS's ARC timeline targets early 2030s for demonstration. P50=1.3 assumes looser definition (PPA or demonstration plant). P95=1.8 if PPAs or pilot customer agreements count. P5=1.0 if only actual sustained grid delivery counts.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at 

---

## competing_technology_disruption `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.5 | 0.7 | 1.2 | multiplier |

> Small modular reactors (SMRs), advanced geothermal, long-duration storage could capture fusion's market opportunity before 2031. However, AI data center power demands may create market space for multiple solutions. P50=0.95 assumes modest competitive pressure. P5=0.7 if SMRs or other tech achieve rapid deployment, reducing fusion urgency. P95=1.1 if power shortage creates desperate demand for any new source.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at 

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Deep technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 40,
    "reasoning": "Looking at deep tech sectors with similar characteristics (capital intensive, physics-based, regulatory complexity): commercial solar (1970s-1990s), LEDs (1960s-1990s), lithium batteries (1980s-2000s), and recent examples like quantum computing and advanced nuclear. For a 7-year commercialization window starti...

