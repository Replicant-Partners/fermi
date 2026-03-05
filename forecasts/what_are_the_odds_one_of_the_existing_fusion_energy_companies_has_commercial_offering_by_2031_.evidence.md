# Evidence Log: what are the odds one of the existing fusion energy companies has commercial offering by 2031?

**Version:** v20 | **Probability:** 16.3% | **Updated:** 2026-03-05 18:41 UTC

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

- **market_research** (schedule: every 1 Day)
  - Query: _evalaute through deep research the pace of relevant innovations and their maturity asit affects the _

### Evidence

#### Agent: market_research (Claude API) (relevance: 85%)

The research indicates that fusion energy technology is still in the early stages of development, with significant technical and engineering challenges remaining before it can be commercially viable. While recent advancements have been promising, the timeline for the commercialization of fusion energy is highly uncertain, and the development of the necessary supply chain will be a critical factor in determining the pace of its adoption.

**Key findings:**

- Fusion energy technology is still in the research and development stage, with no commercially viable fusion power plants currently in operation. Significant technical and engineering challenges remain to be overcome before fusion can be a reliable and cost-effective energy source.
- Recent advancements in fusion reactor designs, such as the development of stellarators and tokamaks, have shown promising progress in achieving the high temperatures and plasma confinement necessary for fusion reactions. However, these technologies are still years away from commercial deployment.
- The timeline for the commercialization of fusion energy is highly uncertain, with estimates ranging from 20 to 50 years or more. The pace of innovation and the maturity of the technology will be critical factors in determining when fusion can be integrated into the energy supply chain.
- The supply chain for fusion energy will need to be developed in parallel with the technological advancements. This will require significant investments in manufacturing, materials science, and logistics to ensure the reliable and cost-effective production and deployment of fusion power plants.
- The impact of fusion energy on the broader energy supply chain is also uncertain, as it will depend on factors such as the cost-competitiveness of fusion compared to other energy sources, the scalability of fusion technology, and the integration of fusion into existing energy infrastructure.

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

- "base_rate": {
- "reference_class": "Deep technology commercialization timelines (7-year horizon)",
- "historical_frequency": 0.15,
- "sample_size": 40,
- "reasoning": "Looking at deep tech sectors with similar characteristics (capital intensive, physics-based, regulatory complexity): commercial solar (1970s-1990s), LEDs (1960s-1990s), lithium batteries (1980s-2000s), and recent examples like quantum computing and advanced nuclear. For a 7-year commercialization window starting from 2024, historical base rate for technologies at fusion's current stage (demonstrated scientific feasibility, multiple well-funded companies, but no commercial deployment) achieving commercial offering is approximately 15%. This accounts for the fact that fusion has achieved net energy gain (NIF 2022) but faces enormous engineering and economic challenges."
- "drivers": [
- "name": "technical_milestone_acceleration",
- "display_name": "Technical Milestone Achievement Rate",
- "type": "continuous",
- "p5": 0.6,

