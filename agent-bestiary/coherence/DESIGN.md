# Collaboration Coherence Evaluator — Formal Design

> Grounded in Thagard (1989), "Explanatory Coherence," _Behavioral and Brain Sciences_.

## 1. Overview

This agent observes **multi-party conversations** in real time and evaluates how well
the participants' contributions "hang together" — that is, how **coherent** the
collaborative discourse is. It adapts Thagard's Theory of Explanatory Coherence
(TEC) from its original domain of scientific belief revision into a tool for
assessing group collaboration quality.

The agent is a **passive observer** that periodically emits structured feedback
(scores, diagnostics, targeted suggestions) without disrupting the conversation flow.

---

## 2. Theoretical Foundation: TEC → Collaboration

Thagard's TEC evaluates how well a set of propositions cohere via constraint
satisfaction over a connectionist network. We map every TEC concept onto
collaborative discourse:

| TEC Concept       | Collaboration Mapping                                  |
|--------------------|--------------------------------------------------------|
| Propositions       | Utterances / claims made by participants               |
| Explanations       | Justifications & reasoning chains offered              |
| Data (observations)| Shared evidence, agreed-upon facts                     |
| Contradictions     | Conflicting claims between participants                |
| Analogy            | Structural parallels drawn across domains              |
| Competition        | Rival explanations for the same phenomenon             |

---

## 3. Formal Model

A **Collaboration Coherence System** is the tuple:

```
C = ⟨U, E, R⁺, R⁻, A, σ⟩
```

where:

| Symbol | Definition |
|--------|-----------|
| **U** | Set of *utterance-propositions*, each classified as one of: `Claim`, `Evidence`, `Explanation`, `Analogy`, `Question` |
| **E ⊆ U** | The *evidence subset* — utterances with intrinsic acceptability (Thagard's Principle 4: Data Priority) |
| **R⁺** | Symmetric *coherence relation* — explanation links, acknowledgments, analogies |
| **R⁻** | Symmetric *incoherence relation* — contradictions, unresolved competition |
| **A : U → [−1, 1]** | *Activation function* settled via connectionist constraint satisfaction |
| **σ : {P₁, …, P₇} → [0, 1]** | *Principle-level scoring function* (one score per TEC principle) |

### 3.1 Thagard's Seven Principles (adapted)

| # | Principle | Collaboration Interpretation |
|---|-----------|------------------------------|
| P₁ | Symmetry | If u₁ coheres with u₂, then u₂ coheres with u₁ |
| P₂ | Explanation | Utterances that jointly explain data cohere with each other and with the data |
| P₃ | Analogy | Analogical mappings between utterances create coherence |
| P₄ | Data Priority | Evidence utterances have a degree of intrinsic acceptability |
| P₅ | Contradiction | Contradictory utterances are incoherent |
| P₆ | Competition | If two explanations both account for the same data, they compete (incohere) unless one subsumes the other |
| P₇ | Acceptability | The acceptability of an utterance depends on its coherence with the overall system |

---

## 4. Settling Mechanism

The network uses Thagard's connectionist settling rule (as implemented in ECHO):

```
A_{t+1}(uᵢ) = clip[-1,1]( (1 − δ) · Aₜ(uᵢ)  +  η · Σⱼ wᵢⱼ · Aₜ(uⱼ) )
```

where:
- **δ** is the decay parameter (typically 0.05)
- **η** is the learning rate (typically 0.05)
- **wᵢⱼ** is the weight between nodes i and j:
  - positive for R⁺ pairs (coherence)
  - negative for R⁻ pairs (incoherence)
- **clip** bounds the result to [−1, 1]

Iteration continues until the network converges (max activation change < ε) or
a maximum number of cycles is reached.

### 4.1 Global Coherence Score

```
Γ(C) = (1 / |U|) · Σᵢ max(0, A(uᵢ))
```

This is the mean positive activation across all utterances, normalized to [0, 1].

---

## 5. Agent Decision Rules

The evaluator agent emits feedback based on threshold comparisons:

| Condition | Action |
|-----------|--------|
| **Γ(C) < θ_critical** (e.g. 0.3) | Full intervention — flag systemic incoherence |
| **Any σ(Pₖ) < θ_principle** (e.g. 0.4) | Targeted feedback for that principle |
| **Γ(C) ≥ θ_good** (e.g. 0.7) | Positive reinforcement — conversation is coherent |
| **Rapid Γ decline** | Alert — coherence is degrading |

---

## 6. Integration Points

The agent is designed to integrate into multi-agent workflows through:

| Protocol | Purpose |
|----------|---------|
| **REST API** | Direct HTTP integration for web services and dashboards |
| **MCP** (Model Context Protocol) | Tool-use integration with LLM agents (Claude, etc.) |
| **A2A** (Agent-to-Agent) | Peer communication with other evaluator or facilitator agents |
| **AKP** (to be defined) | Custom protocol for domain-specific workflow integration |

---

## 7. Scenarios

### 7.1 High-Coherence Team
All participants build on each other's points, evidence is shared freely, no
unresolved contradictions. Expected: Γ > 0.7, all σ(Pₖ) > 0.6.

### 7.2 Fragmented Discussion
Participants talk past each other, minimal acknowledgment, low explanation density.
Expected: Γ ≈ 0.3–0.5, low σ(P₂) and σ(P₇).

### 7.3 Competing Hypotheses
Two subgroups champion rival explanations for the same data. Healthy if both sides
engage with each other's evidence; unhealthy if they ignore it.
Expected: moderate Γ, low σ(P₆), potentially low σ(P₅).

---

## 8. References

- Thagard, P. (1989). Explanatory Coherence. _Behavioral and Brain Sciences_, 12(3), 435–467.
- Thagard, P. (1992). _Conceptual Revolutions_. Princeton University Press.
- Thagard, P. & Verbeurgt, K. (1998). Coherence as constraint satisfaction. _Cognitive Science_, 22(1), 1–24.