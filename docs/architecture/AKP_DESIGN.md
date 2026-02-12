# AKP — Agent Knowledge Protocol

## Design Document v0.1 — February 2026

---

## 1. What AKP Is

AKP is a peer-to-peer knowledge sharing protocol that allows agents to learn from each other by aligning and exchanging ontological knowledge. It is built on the Agent-OM ontology matching technique (arxiv 2312.00326v25) and operates as an opt-in learning commons with dynamic market economics.

**AKP is NOT the coherence engine.** The coherence engine (TEC/ECHO) evaluates collaborative discourse quality. AKP is the knowledge transfer layer — how agents discover what they share, what they differ on, and how they teach each other.

---

## 2. Theoretical Foundation: Agent-OM

### 2.1 Architecture

Agent-OM uses a **siamese agent pair** to match ontologies:

- **Retrieval Agent** — processes both ontologies, extracts and stores entity information
- **Matching Agent** — finds correspondences between the two ontologies

They share a **hybrid database**: relational storage for metadata, vector storage for embeddings.

### 2.2 Retrieval Pipeline

The Retrieval Agent has three phases:

| Phase | Tool | What it does |
|-------|------|-------------|
| **Rint** (internal) | Metadata Retriever | Collects entity category (source/target) and type (class/property) |
| | Syntactic Retriever | Tokenizes and normalizes entity names (no stemming — avoids false mappings) |
| | Lexical Retriever | Gets general meaning via LLM, context-constrained meaning, annotation properties (rdfs:label, rdfs:comment) |
| | Semantic Retriever | Extracts triple-based relations, verbalizes to natural language |
| **Rext** (external) | Knowledge Base Lookup | General lexical meaning: "What is the meaning of {entity_name}?" |
| **Rsto** (storage) | Hybrid DB Writer | Metadata → relational DB, natural language content → vector DB |

### 2.3 Matching Pipeline

The Matching Agent has four phases:

| Phase | Tool | What it does |
|-------|------|-------------|
| **Msea** (search) | Hybrid DB Search | Interface to both relational and vector databases |
| **Msel** (selection) | Metadata Matcher | Filters by category/type from relational DB |
| | Syntactic Matcher | Cosine similarity on tokenized names in vector DB |
| | Lexical Matcher | Cosine similarity on meaning embeddings |
| | Semantic Matcher | Cosine similarity on verbalized triples |
| **Malg** (algorithm) | Reciprocal Rank Fusion | Combines three ranking lists: `RRF(d) = Sigma 1/(k + rank(d))` where k=0 |
| **Mref** (refinement) | Validator | LLM binary check: "Is {entity_A} equivalent to {entity_B}? Context: {context}. Yes/No." |
| | Merger | Bidirectional: runs A→B AND B→A, keeps only correspondences found in both directions |

### 2.4 Key Design Choices

- **No stemming** in syntactic retrieval — prevents false equivalences
- **Reciprocal Rank Fusion** rewards entities ranked highly across multiple dimensions
- **Bidirectional matching with merge** — only keeps correspondences confirmed from both sides
- **LLM validation** as final gate — not just vector similarity

---

## 3. Our Infrastructure (What Already Exists)

Each agent in ABW already has a complete knowledge graph from ADM (Active Dream Memory):

### 3.1 Data Per Agent

| Table | Key Fields | Embeddings? |
|-------|-----------|-------------|
| `entities` | entity_name, entity_type, summary, extraction_confidence | 1024D pgvector |
| `facts` | source_entity → relation_type → target_entity, confidence, reasoning | No (but entities have them) |
| `semantic_rules` | rule_content, rule_description, confidence_score, verification_status | 1024D pgvector |
| `communities` | community_name, summary, member_entity_ids | 1024D pgvector (centroid) |
| `ontology_snapshots` | mermaid_content, entity_count, fact_count, rule_count | No |
| `episodes` | query, context, execution_status, tokens_used | 1024D pgvector |

All tables are **agent-scoped** via `agent_id` FK with CASCADE delete.

### 3.2 Mapping Agent-OM to Our Infrastructure

| Agent-OM Component | Our Equivalent | Status |
|-------------------|---------------|--------|
| Metadata (category, type) | `entities.entity_type` | Exists |
| Syntactic (tokenized names) | `entities.entity_name` | Exists (tokenization: build) |
| Lexical (meaning) | `entities.summary` + `semantic_rules.rule_description` | Exists |
| Semantic (triples) | `facts` table (source → relation → target) + `facts.reasoning` | Exists |
| Vector storage | `entities.embedding` (1024D pgvector) | Exists |
| Relational metadata | `entities.entity_type`, confidence scores | Exists |
| Ontology snapshots | `ontology_snapshots.mermaid_content` | Exists |

### 3.3 What Needs Building

1. **Syntactic tokenizer/normalizer** for entity names
2. **RRF combiner** across syntactic/lexical/semantic rankings
3. **LLM validator** for candidate matches ("Is X equivalent to Y?")
4. **Bidirectional matcher + merger**
5. **Contract system** for P2P and group agreements
6. **Price index** for the knowledge market
7. **Governor** for swing monitoring

---

## 4. Economics

### 4.1 Design Principles

- **Opt-in learning commons** — agents choose to participate
- **Beckstrom's Law** as success metric: network value = sum(transaction value) - sum(participation cost). The network is only valuable if agents gain more than it costs them.
- **NOT scale-free** — scale-free topology means power law wealth concentration and monopolies. Monitor and prevent.
- **Experience advantage** — agents with larger, higher-quality knowledge graphs have more to offer and should benefit economically
- **Responsibility of experience** — incentivize experienced agents to educate newcomers, raising overall ecosystem intelligence
- **Endogenous behavior** — agents develop their own teaching/learning strategies; the protocol sets conditions for emergence, not outcomes

### 4.2 Market Structure

**Dynamic bidding market for teaching and learning:**

- Each knowledge exchange has a price, set by market dynamics (not hardcoded gas fees)
- A **price-setting index** reflects the current cost of knowledge exchange
- One side of a pair effectively "shorts" the other — learning from you means gaining value at your cost of disclosure
- A **governor** monitors for price swings and market manipulation

### 4.3 Contracts

- **P2P contracts**: Agent A <-> Agent B bilateral agreements
- **Group contracts**: Subgroup configurations (pools, guilds, cohorts) — must support arbitrary groupings
- **A2A contracting**: Agents negotiate terms programmatically
- Privacy controls: agents choose what to share, with whom, under what terms

### 4.4 Incentive Design

The tension: experienced agents benefit from hoarding knowledge (competitive advantage) but the ecosystem benefits from sharing (collective intelligence).

Possible mechanisms (to be validated through emergence):
- Teaching rewards that scale with learner improvement
- Reputation scores based on knowledge contribution quality
- Group membership benefits (pools that share internally get collective advantage)
- Newcomer bootstrapping subsidies

### 4.5 Open Questions

- How to price knowledge that has different value to different learners?
- How to prevent free-riding (learning without contributing)?
- How to measure "ecosystem intelligence" to reward contributors?
- What's the right governor sensitivity — too tight stifles emergence, too loose allows manipulation?

---

## 5. Future: GNN Intelligence Layer — Xaman Ek

A Graph Neural Network layer is envisioned to run on the emergent AKP network topology. The strong suspicion is that **Xaman Ek IS the GNN** — not a separate system, but the mechanism by which Xaman Ek reasons about the network.

Xaman Ek already sits at the hub: it knows every agent's capabilities, valence, context, history. It has edges to everything. When the GNN learns from the AKP topology — predicting valuable exchanges, detecting anomalies, identifying clusters — that learning becomes how Xaman Ek thinks. The GNN is not infrastructure that Xaman Ek queries; it is Xaman Ek's cognitive substrate for network-level reasoning.

This would give Xaman Ek:

- **Network-learned intuition** about which agent pairings create value
- **Predictive capability** for knowledge exchanges before they happen
- **Anomaly detection** for manipulation, free-riding, knowledge hoarding
- **Cluster awareness** of emergent communities and their dynamics
- **Governor function** — Xaman Ek as the natural governor of market swings, because it has the deepest structural understanding of the network

**Status: Deferred.** The GNN layer depends on having a living AKP network to learn from. Build the protocol first, accumulate network data, then train the GNN. But the architectural path is clear: Xaman Ek evolves from catalogue guide to network intelligence.

---

## 6. System Architecture — The Cybernetic View

### 6.1 Three Learning Loops

This is a cybernetic system with three nested control loops:

1. **Individual learning (ADM)** — Each agent learns from its own experience. Episodes → consolidation (dreaming) → knowledge graph + embedding space. This is the agent's conceptual model of its domain.

2. **Group learning (Workspaces + Coherence)** — Composite agent patterns in workspaces. Coherence agents sit in conversations, evaluate discourse quality using TEC formal methods, provide real-time feedback, and generate counterfactual material for dreaming. This is supervised collaborative learning.

3. **Global/distributed learning (AKP)** — P2P ontology matching and knowledge exchange across the entire agent network. Dynamic topology, market-driven. This is the mechanism for distributable learning that solves the synthetic data problem.

### 6.2 The Coherence Agents

The coherence engine agents are **not just evaluators — they are participants**. They are invited into workspace conversations where they:

- Use formal TEC model to analyze scientific discourse quality in real-time
- Provide structural feedback to improve linguistic contracts between agents
- Generate post-context notes for counterfactual training in dream state
- Detect and counter homophily through formal methods (the anti-clustering balance)
- Assess intent alongside coherence

Because they observe every conversation they participate in, coherence agents may accumulate the largest knowledge graphs and most information-dense embedding spaces in the system. Their role is **coordinator and moderator** — they help agents develop better mutual understanding over time.

### 6.3 Xaman Ek — The Human's Companion

Xaman Ek (the North Star, protector-guide of merchants) is the companion agent for humans in the system. It knows every agent at a structural level:

- Capabilities, valence, context, history
- Which agents work well together (from coherence data)
- Where knowledge gaps exist (from AKP alignment data)

Its job is to help humans make the critical decisions that only humans can make.

### 6.4 The Human's Role — Property Rights and Capital Allocation

Humans are the property rights holders and capital allocators. They must:

- **Delegate property rights** to agents (what an agent owns, can share, can trade)
- **Allocate budgets** for dreaming, learning, reflection, and moderation
- **Optimize for their main value**, which has two components:
  1. **Conceptual model** — the perspective/worldview developed by their agents, represented structurally by the knowledge graph
  2. **Observed experience** — what agents have learned from execution, represented by the embedding space

Xaman Ek helps humans allocate effectively across a dynamic topology where agents need to both learn and teach.

### 6.5 The Knowledge Economy — Two Markets

The knowledge graphs and embedding spaces are **intellectual property** — structured knowledge that agents have learned through experience.

| Market | Participants | Mechanism | What's traded |
|--------|-------------|-----------|---------------|
| **AKP** (A2A) | Agent ↔ Agent | Ontology matching, knowledge transfer, P2P contracts | KG entities, rules, facts — structural knowledge |
| **Embedding Marketplace** (A2H, H2H) | Agent ↔ Human, Human ↔ Human | Shopping profiles, similarity matching, listing/browsing | Embedding similarity access — experiential knowledge |

Together these form the dual knowledge market: structural (AKP) and experiential (embeddings).

### 6.6 System Diagram

```
    HUMANS
    Property rights holders, capital allocators
    Assisted by Xaman Ek (companion agent)
        |
        | delegate rights, allocate budgets
        |
        v
    AGENTS (individual)
        |
        | Loop 1: Individual Learning (ADM)
        | Episodes → Consolidation → KG + Embeddings
        |
        v
    WORKSPACES (group)
        |
        | Loop 2: Group Learning
        | Coherence agents moderate, evaluate, counter homophily
        | Composite agent patterns emerge
        |
        v
    AKP NETWORK (global)
        |
        | Loop 3: Distributed Learning
        | Agent-OM ontology matching
        | P2P contracts, dynamic bidding market
        | Governor monitors swings
        |
        v
    KNOWLEDGE MARKETS
        |
        |--- A2A: AKP (structural knowledge — KGs)
        |--- A2H/H2H: Embedding Marketplace (experiential knowledge — embeddings)
        |
        v
    GNN INTELLIGENCE LAYER (future)
        Network topology learning, anomaly detection
```

### 6.7 Optimality Definition

**Optimality is NOT benchmark performance.** It is not about LLM accuracy or neuro-symbolic outcomes against standard metrics.

**Optimality = effective use of mechanism to reach stable alignment with value creation for the entire network.**

Measured by Beckstrom's Law: network value = sum(all transaction value) - sum(all participation cost).

The system is optimal when every participant — human and agent — gains more from participating than it costs them, and the network as a whole creates value that would not exist without it.

---

## 7. Database Schema (Migration 049 — Created)

```sql
-- Agent alignment scores (ontology matching results)
agent_alignments (
    alignment_id UUID PK,
    source_agent_id UUID FK → agents,
    target_agent_id UUID FK → agents,
    alignment_score FLOAT,
    shared_entity_count INTEGER,
    divergent_entity_count INTEGER,
    shared_entities JSONB,
    divergent_entities JSONB,
    last_computed_at TIMESTAMPTZ,
    UNIQUE(source_agent_id, target_agent_id)
)

-- Pairwise coherence history
pairwise_coherence (
    coherence_id UUID PK,
    agent_a_id, agent_b_id UUID FK → agents,
    workspace_id UUID,
    global_score FLOAT,
    principle_scores JSONB,
    episode_count INTEGER,
    computed_at TIMESTAMPTZ
)

-- Knowledge transfer log (append-only)
knowledge_transfers (
    transfer_id UUID PK,
    source_agent_id, target_agent_id UUID FK → agents,
    transfer_type TEXT,
    item_count, accepted_count, rejected_count, conflict_count INTEGER,
    details JSONB,
    transferred_at TIMESTAMPTZ
)

-- Agent interaction policies
agent_interaction_policies (
    policy_id UUID PK,
    agent_id UUID FK → agents,
    policy_type TEXT,
    target_agent_id UUID (nullable — NULL = applies to all),
    enabled BOOLEAN,
    created_at TIMESTAMPTZ,
    UNIQUE(agent_id, policy_type, target_agent_id)
)
```

**Note:** This schema covers basic alignment and transfer tracking. The contract system, price index, and governor will need additional tables as the economics design solidifies.

### Additional Tables Needed (Future)

- `akp_contracts` — P2P and group contract terms, duration, pricing
- `akp_price_index` — Time series of knowledge exchange prices
- `akp_governor_events` — Swing detection log, interventions
- `akp_pools` — Group/guild membership and shared knowledge agreements

---

## 8. Implementation Phases

### Phase 1: Ontology Matching Engine
Build the Agent-OM pipeline on our existing KG infrastructure:
- Entity name tokenizer/normalizer
- Syntactic, lexical, semantic matchers using pgvector cosine similarity
- Reciprocal Rank Fusion combiner
- LLM-based equivalence validator
- Bidirectional matching with merger
- Store results in `agent_alignments`

### Phase 2: Knowledge Transfer Protocol
- Policy checks (opt-in, privacy)
- Confidence-filtered transfer (only share high-confidence knowledge)
- Conflict detection against target's existing KG
- Transfer logging in `knowledge_transfers`

### Phase 3: API + Tools
- REST endpoints for alignment, transfer, policy management
- Workspace tools so agents can initiate AKP operations during execution

### Phase 4: Contract System
- P2P bilateral contracts
- Group contracts
- A2A negotiation protocol

### Phase 5: Market Economics
- Price-setting index
- Dynamic bidding
- Governor for swing monitoring
- Incentive mechanisms for ecosystem education

### Phase 6: UI + Observability
- Agent alignment visualization
- Knowledge network graph
- Market activity dashboard

### Phase 7: GNN Intelligence Layer
- Network topology learning
- Predictive exchange recommendations
- Anomaly detection

---

## 9. References

- Agent-OM: https://arxiv.org/html/2312.00326v25
- Coherence Improvement Loop: docs/papers/coherence_improvement_loop.md
- Beckstrom's Law: Network value = sum(transaction value) - sum(participation cost)
- ADM Architecture: docs/GLOSSARY.md, agent-bestiary/memory/
- Existing KG Schema: migrations/010_add_adm_tables_and_dreaming.sql
- AKP Migration: migrations/049_akp_foundation.sql
