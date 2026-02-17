# Specialization-Preserving Agent Ecology Architecture
## Design Document v1.0

---

## 1. Vision & Guiding Principle

This architecture defines a multi-agent knowledge system where heterogeneous specialist agents maintain and deepen their domain expertise while participating in a coordinated ecology. The meta-agent provides ecological insight — understanding the topology, dynamics, and health of the agent ecosystem — without subsuming or homogenizing the specialists it coordinates.

**Core invariant:** Domain expert agents are never flattened into the meta-graph. Specialization diversity is the system's primary asset. Teaching enriches boundaries between specialists; it does not erode cores.

---

## 2. System Overview

The architecture is organized into three computational levels, supported by a feedback system and a meta-agent coordination layer.

| Layer | Purpose | Primary Model | Optimizes For |
|-------|---------|---------------|---------------|
| Level 1 | Individual agent knowledge representation | Agent-Conditioned CompGCN | Intra-agent link prediction, node classification |
| Level 2 | Cross-agent characterization & bounded teaching | Contrastive objectives + Bounded NBFNet | Specialization profiling, boundary enrichment |
| Level 3 | Ecological awareness & evolution tracking | TGN + Spectral Analysis | Topology understanding, trajectory prediction |
| Feedback | Teaching quality & drift monitoring | Drift detection metrics | Specialization preservation |
| Meta-Agent | Orchestration & insight | Reads from L3 outputs | Interaction guidance, vulnerability detection |

---

## 3. Level 1: Individual Agent Knowledge Graphs

### 3.1 Storage & Versioning

Each agent's knowledge graph is encoded in **Mermaid diagram syntax** and version-controlled via **Git**. This provides:

- **Human-readable and LLM-parseable** graph serialization — agents and humans can both inspect and reason about the graph natively.
- **Immutable temporal versioning** — every commit is a snapshot of the agent's knowledge state. History is preserved and replayable.
- **Diff-based change detection** — git diffs on Mermaid files provide structural change signals (delta detection) that trigger downstream recomputation.
- **Auditability** — full provenance of how each agent's knowledge evolved, including which teaching interactions contributed to changes.

### 3.2 Graph Neural Network: Agent-Conditioned CompGCN

Each agent's KG is processed by a **CompGCN** (Composition-based Graph Convolutional Network) that is conditioned on the agent's embedding vector.

**Why CompGCN:** CompGCN jointly embeds entities and relations through composition operations (subtraction, multiplication, circular correlation). This is more parameter-efficient than R-GCN for heterogeneous graphs and naturally handles diverse relation types within each agent's KG.

**Agent conditioning:** The agent's embedding vector modulates the CompGCN weights via a hypernetwork. This means the same relation type in two different agents' graphs can be processed differently based on the agent's identity and characteristics. The agent embedding captures the agent's role, capabilities, and position in the ecology.

### 3.3 Core/Boundary Separation

The CompGCN output for each agent is partitioned into two zones:

- **Core Specialization (Protected):** The deep interior of the agent's knowledge graph — densely connected concepts that define the agent's expertise. Core representations are never directly modified by teaching signals. They evolve only through the agent's own learning and graph updates.

- **Boundary Zone (Teachable):** Concepts at the periphery of the agent's knowledge — where its expertise interfaces with other domains. Boundary representations are eligible for enrichment through cross-agent teaching. Only boundary representations flow upward to Level 2.

**Partitioning method:** Core vs. boundary is determined by graph topology — concepts with high local clustering coefficient and low cross-agent OM alignment are core; concepts with OM alignments to other agents' concepts are boundary. This partition is recomputed as the graph evolves.

### 3.4 Agent Embeddings

Each agent has an embedding vector in a shared embedding space. There is a one-to-one correspondence between agents and embedding vectors. These embeddings capture agent identity, capability profile, and ecological position. They serve as:

- Conditioning signal for the CompGCN (agent-specific graph processing)
- Features in the Level 3 meta-graph (ecological reasoning)
- Basis for FAISS indexing (scalable candidate selection for OM)

---

## 4. Level 2: Cross-Agent Characterization

### 4.1 AGENT-OM Ontology Matching

**AGENT-OM (AKP)** provides structured ontology matching between agent KGs, producing:

- **Pairwise Specialization Profiles:** For each pair of agents, a structured description of which concepts align (shared knowledge), which align weakly (related but different), and which have no alignment (unique specialization).
- **Alignment Confidence Scores:** Per-concept-pair confidence values that weight all downstream cross-agent computations.

**Key property:** OM captures both similarity AND difference. High-confidence alignments indicate shared conceptual ground. Absence of alignment indicates unique specialization. Both signals are equally valuable to the system.

### 4.2 Contrastive Objectives

The system is trained with contrastive objectives that explicitly reward accurate characterization of inter-agent relationships, rather than rewarding homogenization.

- **Overlap Characterization:** Given two agent representations, predict which concepts align and with what strength. The model is rewarded for accurately mapping shared knowledge.
- **Divergence Characterization:** Given two agent representations, predict where their ontologies diverge. The model is rewarded for accurately identifying unique specializations.
- **OM Evolution Prediction:** Given the current OM alignment profile and agent trajectories, predict how the alignment will change at the next time step. This forces the model to learn the dynamics of specialization — which alignments are strengthening, weakening, or emerging.

**Why contrastive rather than reconstructive:** A reconstructive objective (e.g., standard link prediction across the ecology) would incentivize the model to make agent representations more similar, eroding specialization. The contrastive objective rewards understanding the pattern of difference, preserving it.

### 4.3 Boundary-Focused Teaching

Teaching between agents operates exclusively at the boundary zone through a three-stage process:

**Stage 1 — Interface Identification:** The meta-agent's orchestration layer (Level 3) identifies productive teaching opportunities: pairs of agents whose boundary zones have OM alignments that suggest mutual enrichment potential. Priority is given to pairs where teaching would strengthen the interface between specializations without eroding either core.

**Stage 2 — Bounded NBFNet Propagation:** For each identified teaching opportunity, a query-conditioned propagation (following the NBFNet paradigm) runs from the knowledge gap in one agent's boundary zone, across OM alignment edges, into the other agent's boundary zone. 

**Critical constraint: propagation is hard-limited to 1-2 OM hops.** This structural bound ensures that teaching signals can enrich the interface between two specializations but cannot propagate deep into an agent's core. The propagation uses learned operators parameterized by relation type and OM confidence, producing a teaching signal that encodes cross-agent evidence.

**Stage 3 — Selective Transfer Filter:** The teaching signal is evaluated before being applied. The filter checks:

- Does this signal enrich the boundary without shifting the core?
- Is the OM alignment confidence above threshold?
- Has the receiving agent's specialization drift metric remained stable?

Only signals passing all checks are applied as boundary enrichment.

**Integration into the agent's KG:** Accepted teaching signals propose graph updates — new triples, reclassified nodes, new edges — in the agent's boundary zone. These are committed to the agent's git repo with full provenance metadata: source agent, OM alignment used, confidence score, and teaching rationale. The commit becomes part of the immutable history.

---

## 5. Level 3: Ecological Awareness

### 5.1 Temporal Graph Network (TGN)

The TGN operates over the agent meta-graph, where nodes are agents and edges represent relationships derived from OM alignments, teaching history, and embedding proximity.

**Agent Memory Modules:** Each agent has a compressed memory vector maintained by the TGN that encodes its evolutionary trajectory — not just its current state, but a compressed history of how it has changed. This enables the meta-agent to distinguish between: agents that are deepening their specialization, agents that are broadening, agents that are shifting domains, and agents that are stagnating.

**Event-Driven Message Passing:** Rather than running on fixed intervals, the TGN processes events: git commits (agent KG changes), OM alignment updates, teaching interactions, and feedback signals. Each event triggers local memory updates and message passing in the affected neighborhood. This is computationally efficient and naturally handles the asynchronous evolution of agents.

**Ecological Evolution Predictor:** Trained to predict future states of the ecology — which agents will change, how OM alignments will shift, where new specializations will emerge. This predictive capability gives the meta-agent anticipatory awareness rather than purely reactive monitoring.

### 5.2 Spectral Analysis

Operating on the OM alignment graph (agents as nodes, OM alignment strength as edge weights), spectral analysis provides structural insight:

**OM Graph Laplacian:** Computed from the weighted adjacency matrix of OM alignments. Captures the global structure of how agents relate to each other.

**Top-k Eigenvalues + Eigenvectors:** The spectral decomposition reveals:

- **Specialization Cluster Detection:** Clusters of agents with strong mutual OM alignments correspond to specialization communities. The number of significant eigenvalue gaps indicates the number of distinct specialization clusters.
- **Boundary & Bridge Identification:** Agents with high components in multiple eigenvectors span specialization boundaries — they are the bridges between communities. These agents are strategically important for cross-cluster knowledge flow.
- **Ecological Gap Detection:** Regions of the spectral embedding with low agent density indicate areas of the problem space that no agent currently covers — potential vulnerabilities.

**Incremental updates:** Full spectral decomposition is expensive. The system uses incremental spectral methods that update eigenvalues/eigenvectors as the OM graph evolves, rather than recomputing from scratch.

### 5.3 Ecology Health Monitor

Integrates signals from the TGN and spectral analysis to maintain a dashboard of ecological health:

- **Specialization Diversity Index:** A scalar measure (analogous to biodiversity indices like Shannon entropy) that captures how diverse and balanced the specialization landscape is. A healthy ecology has high diversity — many distinct specializations with balanced representation.
- **Cluster Trajectory Tracking:** For each specialization cluster, tracks whether it is: growing (more agents specializing in this area), stable, fragmenting (splitting into sub-specializations), or declining.
- **Vulnerability Assessment:** Identifies risks: single points of failure (a specialization covered by only one agent), widening gaps (uncovered problem space), echo chambers (over-connected clusters that reinforce each other), and excessive teaching pressure (agents at risk of losing specialization).

---

## 6. Feedback & Validation

### 6.1 Teaching Interaction Quality Score

After each teaching interaction, the system measures whether the taught knowledge improved the receiving agent's downstream task performance (link prediction accuracy, classification quality). This score feeds back into the meta-graph as an edge weight on the teaching relationship.

**Positive feedback:** Strengthens the teaching relationship in the meta-graph, making future teaching between this pair more likely.

**Negative feedback:** Weakens the relationship and may trigger the Selective Transfer Filter to raise its threshold for this pair.

### 6.2 Specialization Drift Detection

Monitors each agent's core/boundary ratio and the stability of its core representations over time. Drift is detected when:

- The boundary zone expands significantly relative to the core (the agent is becoming a generalist)
- Core representations shift in directions correlated with teaching inputs (teaching is leaking into the core)
- The agent's spectral position shifts toward another cluster (it's losing its distinct specialization)

**Response to drift:** Drift alerts propagate to the Ecology Health Monitor, which can instruct the Orchestrator to reduce teaching pressure on the affected agent, increase the Selective Transfer Filter threshold, or temporarily exclude the agent from teaching interactions until its specialization stabilizes.

### 6.3 Boundary Productivity

Measures whether boundary zones are generating productive cross-agent interactions. A healthy boundary is one that facilitates knowledge exchange without expanding uncontrollably. Unproductive boundaries (low teaching quality, high drift) may indicate a poor OM alignment that should be deprecated.

---

## 7. Meta-Agent Ecological Insight

The meta-agent does not attempt to know everything every agent knows. Its awareness is ecological — it understands the shape, dynamics, and health of the agent ecosystem.

### 7.1 Topology of Specialization

Derived from spectral analysis, the meta-agent maintains a map of: how many specialization clusters exist, their relative sizes and densities, which agents bridge between clusters, where gaps and vulnerabilities exist, and how the topology compares to previous states.

### 7.2 Evolution Dynamics

Derived from the TGN, the meta-agent tracks: the trajectory of each specialization cluster, emerging patterns (new specializations forming, old ones declining), the rate and direction of ecological change, and predicted future states.

### 7.3 Interaction Orchestration

Using topology and dynamics, the meta-agent makes orchestration decisions:

- **Teaching priorities:** Which agent pairs should interact, and in which direction? Priority goes to pairs where boundary enrichment would strengthen a productive interface or fill a detected vulnerability.
- **Teaching restrictions:** Which agents should be temporarily shielded from teaching to preserve their specialization? Applied when drift is detected or when a specialization is at risk.
- **Ecological interventions:** In extreme cases, the meta-agent may signal that a new specialist agent should be spawned to cover a detected gap, or that redundant specialists should be encouraged to differentiate.

---

## 8. Scaling Strategy

### 8.1 Current Scale: Dozens of Agents

At this scale, the full architecture operates without approximation. All-pairs OM matching is feasible. The spectral decomposition is exact. The TGN processes all events. The meta-agent has direct awareness of every agent.

### 8.2 Target Scale: Hundreds of Thousands of Agents

**OM Bottleneck Mitigation:**
- FAISS indexing over agent embeddings for candidate pair selection — only run OM on agents whose embeddings suggest potential alignment.
- Incremental OM — when an agent's KG changes (git commit), match only the delta against existing alignments rather than recomputing full OM.
- Transitive alignment inference — if A aligns with B and B aligns with C, approximate A-C alignment without running OM directly.

**Hierarchical Coordination:**
- Agents are grouped into communities based on spectral clustering.
- Community-level coordinators (smaller TGN instances) handle intra-community orchestration.
- A global coordinator operates over community-level summaries.
- Teaching within a community is frequent and lightweight. Cross-community teaching is rarer and requires global coordinator approval.

**Computation Management:**
- Level 1 representations are cached and recomputed only on git triggers.
- Transitive cache invalidation — when Agent A's graph changes, agents OM-aligned to A are marked for potential recomputation at Level 2.
- Event-driven TGN updates — no fixed-interval full passes over the ecology.
- Neighborhood sampling (GraphSAGE-style) for mini-batch training at all levels.

### 8.3 Cold-Start Handling

New agents with small graphs operate in a **listen-only mode**: they receive ecological context and can observe teaching interactions in their spectral neighborhood, but they do not contribute to cross-agent message passing or teaching until their graphs reach a structural threshold (minimum node count, minimum clustering coefficient). This prevents noisy representations from corrupting the ecology.

---

## 9. Technology Stack

| Component | Technology |
|-----------|-----------|
| Agent KG Storage | Mermaid diagram syntax |
| Temporal Versioning | Git (immutable commit history) |
| Change Detection | Git diffs |
| Intra-Agent GNN | CompGCN (agent-conditioned via hypernetwork) |
| Ontology Matching | AGENT-OM (AKP) |
| Cross-Agent Teaching | Bounded NBFNet propagation |
| Ecology Temporal Model | TGN (Temporal Graph Network) |
| Spectral Analysis | Incremental eigendecomposition on OM Laplacian |
| Embedding Index | FAISS (for scalable candidate selection) |
| Training Objectives | Contrastive (overlap/divergence characterization, OM evolution prediction) |

---

## 10. Key Design Decisions & Rationale

**Decision: Contrastive objectives over reconstructive objectives.**
Rationale: Reconstructive objectives (standard link prediction across the full ecology) would incentivize homogenization. Contrastive objectives reward the model for understanding the pattern of differences, preserving specialization diversity.

**Decision: Hard-bounded propagation (1-2 OM hops) for teaching.**
Rationale: Without a structural bound, teaching signals could propagate deep into an agent's core specialization, gradually eroding it. The hop limit ensures teaching enriches interfaces without reaching cores. This is a design constraint, not a limitation.

**Decision: TGN over static GNN at the ecology level.**
Rationale: The ecology is fundamentally dynamic. A static GNN would require periodic recomputation over the full graph. TGN processes events incrementally, maintains agent memories that encode evolution history, and naturally handles the asynchronous nature of agent updates.

**Decision: Spectral analysis alongside (not instead of) TGN.**
Rationale: Spectral methods give global structural insight (cluster count, boundaries, gaps) that local message passing misses. TGN gives temporal dynamics that spectral snapshots miss. They are complementary — spectral analysis answers "what is the shape of the ecology" while TGN answers "how is the ecology changing."

**Decision: Core/boundary separation within agent KGs.**
Rationale: Without explicit separation, teaching signals would modify whatever the GNN's gradient flow touches. The core/boundary partition creates a structural firewall that protects deep specialization while allowing productive boundary interaction. The partition is topologically derived and evolves with the graph.

**Decision: Git for versioning rather than a temporal graph database.**
Rationale: Git provides immutability, diff-based change detection, full audit trail, and compatibility with existing tooling. Combined with Mermaid's human/LLM-readable format, this gives both machine-processable and human-inspectable graph history. The tradeoff is that Git is not optimized for graph queries, but the GNN layers handle graph computation while Git handles versioning and provenance.

---

## 11. Open Questions & Future Work

- **Automated core/boundary repartitioning:** As agents evolve, the boundary between core and boundary shifts. The current design recomputes based on topology, but a learned partitioning (perhaps trained on drift detection signals) may be more robust.
- **Cross-ecology federation:** If multiple independent ecologies exist, how do they interact? The same principles (characterize differences, teach at boundaries) could apply at the ecology-to-ecology level.
- **Agent spawning/retirement:** The meta-agent can detect gaps and redundancies. A natural extension is automated spawning of new specialist agents to fill gaps and graceful retirement of redundant agents, managed by the orchestration layer.
- **Uncertainty quantification:** Adding explicit uncertainty estimates to agent KG nodes/edges would improve teaching signal quality — the system could teach not just what agents know but how confident they are.
- **Adversarial robustness:** At scale, a malicious or malfunctioning agent could inject bad knowledge through teaching. The feedback loop provides some defense, but explicit adversarial detection at the OM layer may be needed.
