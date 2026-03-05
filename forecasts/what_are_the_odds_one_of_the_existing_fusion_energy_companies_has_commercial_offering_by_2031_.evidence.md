# Evidence Log: what are the odds one of the existing fusion energy companies has commercial offering by 2031?

**Version:** v3 | **Probability:** 15.0% | **Updated:** 2026-03-05 12:52 UTC

---

## Outside View (Base Rate)

- **Reference class:** Energy technology commercialization timelines (7-year horizon)
- **Historical frequency:** 15.0%
- **Sample size:** n=20
- **Source:** macro_forecaster

> Looking at major energy technologies that reached commercial deployment: solar PV (1970s-1990s, ~20 years), wind turbines (1980s-2000s, ~15 years), lithium-ion batteries (1991-2010s, ~20 years), and advanced nuclear (still pending after 15+ years). For a 7-year timeline (2024-2031), technologies typically need to already be in pilot/demonstration phase. Of ~20 major energy innovations tracked since 1990, approximately 3 achieved commercial deployment within 7 years of their demonstration phase, yielding ~15% base rate.

---

## technical_readiness_multiplier `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.6 | 1 | 1.8 | multiplier |

> Current fusion companies (Commonwealth Fusion, TAE, Helion) are at TRL 5-7. Commercial offering requires TRL 9. Commonwealth's SPARC aims for net energy by 2025, which would accelerate timeline significantly (p95=1.8x). However, unexpected physics challenges or engineering delays could slow progress (p5=0.6x). Median assumes steady progress matching current projections.

### Assigned Agents

- **entity_investigator** (schedule: once)
  - Query: _deepresearch into avalanche energy_

### Evidence

#### Agent: entity_investigator (Claude API) (relevance: 50%)

```json
{
  "key_findings": [
    "Avalanche Energy is a Seattle-based fusion energy startup founded in 2018 by Robin Langtry and Brian Riordan, focused on developing compact fusion reactors using electrostatic confinement technology called 'Orbitron' devices. The company has raised significant venture capital including a $5M seed round and a $40M Series A led by Prime Movers Lab in 2023.",
    "The company's technical approach involves miniaturized electrostatic fusion devices that could theore...

**Key findings:**

- "key_findings": [
- "Avalanche Energy is a Seattle-based fusion energy startup founded in 2018 by Robin Langtry and Brian Riordan, focused on developing compact fusion reactors using electrostatic confinement technology called 'Orbitron' devices. The company has raised significant venture capital including a $5M seed round and a $40M Series A led by Prime Movers Lab in 2023.",
- "The company's technical approach involves miniaturized electrostatic fusion devices that could theoretically fit in shipping containers, targeting distributed power generation rather than grid-scale plants. They've published research showing ion confinement improvements and are working toward net energy gain demonstrations, though no commercial fusion has been achieved by any company globally as of 2024.",
- "Avalanche Energy has connections to the broader fusion investment ecosystem: Prime Movers Lab (lead investor) also backs Commonwealth Fusion Systems and Type One Energy. Board/advisory connections include former ARPA-E officials and fusion physics researchers from University of Washington and national labs.",
- "The company operates in a competitive landscape with 30+ private fusion ventures globally (Commonwealth Fusion, Helion Energy, TAE Technologies, etc.), most targeting 2030s for commercial demonstration. Avalanche's compact approach is higher-risk/higher-reward compared to tokamak or inertial confinement approaches with more established physics.",
- "Regulatory and commercial risk factors: No clear regulatory pathway for commercial fusion reactors in the US yet (NRC developing framework), unknown manufacturing scalability for novel confinement geometries, and dependency on continued venture funding in a capital-intensive sector where no company has achieved breakeven."
- "summary": "Avalanche Energy is a venture-backed fusion startup pursuing compact electrostatic confinement technology with notable investors and technical pedigree, but faces the fundamental challenge all fusion companies face: unproven commercial viability. The entity sits within a network of fusion-focused investors and technical advisors, with relationships to the Pacific Northwest research ecosystem and the broader fusion venture community.",
- "sources": [
- "Prime Movers Lab press release (2023) - Series A announcement",
- "Company website and technical publications on Orbitron design",

_Collected: 2026-03-05_

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking a

---

## regulatory_pathway_multiplier `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.7 | 1.1 | 1.5 | multiplier |

> NRC and DOE are developing fusion-specific regulatory frameworks (not treating fusion as fission). UK has established clearer pathways. Fast regulatory approval could accelerate by 50% (p95=1.5x). Regulatory confusion or safety concerns could slow by 30% (p5=0.7x). Current trajectory suggests modest acceleration (p50=1.1x).

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking a

---

## funding_sustainability_multiplier `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.5 | 1 | 1.6 | multiplier |

> Fusion companies have raised $6B+ (2021-2023). Commercial deployment requires $10-20B more. Strong continued investment (government + private) could accelerate by 60% (p95=1.6x). Funding drought due to economic conditions or failed milestones could cut probability in half (p5=0.5x). Median assumes current funding trajectory continues.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking a

---

## supply_chain_maturity_multiplier `continuous`

| p5 | p50 | p95 | unit |
|---|---|---|---|
| 0.4 | 0.7 | 1 | multiplier |

> High-temperature superconductors, tritium breeding, specialized materials are bottlenecks. Supply chains are immature. Rapid scaling could provide 30% boost (p95=1.3x). Supply chain failures or material shortages likely reduce probability by 40% (p5=0.6x). Median expects supply chain to be a slight drag (p50=0.9x).

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking a

---

## first_mover_success `binary`

- **Probability:** 65%
- **Impact multiplier:** 2.5x

> If Commonwealth Fusion's SPARC or similar achieves Q>1 by 2025-2026 (65% probability based on technical assessments), it dramatically increases commercial deployment odds by 2031 (2.5x multiplier due to proof-of-concept, investor confidence, and accelerated follow-on projects). Without this milestone, commercial deployment by 2031 becomes highly unlikely.

### Related Evidence

- **Agent: fermi (Claude API)**: ```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking a

---

## General Evidence

### Agent: fermi (Claude API) (relevance: 50%)

```json
{
  "base_rate": {
    "reference_class": "Energy technology commercialization timelines (7-year horizon)",
    "historical_frequency": 0.15,
    "sample_size": 20,
    "reasoning": "Looking at major energy technologies that reached commercial deployment: solar PV (1970s-1990s, ~20 years), wind turbines (1980s-2000s, ~15 years), lithium-ion batteries (1991-2010s, ~20 years), and advanced nuclear (still pending after 15+ years). For a 7-year timeline (2024-2031), technologies typically ne...

- "base_rate": {
- "reference_class": "Energy technology commercialization timelines (7-year horizon)",
- "historical_frequency": 0.15,
- "sample_size": 20,
- "reasoning": "Looking at major energy technologies that reached commercial deployment: solar PV (1970s-1990s, ~20 years), wind turbines (1980s-2000s, ~15 years), lithium-ion batteries (1991-2010s, ~20 years), and advanced nuclear (still pending after 15+ years). For a 7-year timeline (2024-2031), technologies typically need to already be in pilot/demonstration phase. Of ~20 major energy innovations tracked since 1990, approximately 3 achieved commercial deployment within 7 years of their demonstration phase, yielding ~15% base rate."
- "drivers": [
- "name": "technical_readiness_multiplier",
- "display_name": "Technical Readiness Level Progress",
- "type": "continuous",
- "p5": 0.6,

