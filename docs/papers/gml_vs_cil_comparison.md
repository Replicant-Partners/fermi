# Comparative Analysis: GML vs. Coherence Improvement Loop

**Papers compared:**
- **GML** — *Generalized Monad Logic: An Allosteric Framework for Conceptual Analysis* (hKask Research Group, May 2026)
- **CIL** — *Explanatory Coherence Modeling as an Improvement Loop in Agent-to-Agent and Agent-to-Human Collaboration* (Ivan Labra, February 2026)

**Question:** Which is better reasoned for development of a system?

---

## 0. The Prior Question: State Space Definition

Before comparing these frameworks on implementation specificity, parameter estimation, or protocol integration, there is a more fundamental question that must be answered first: **does the framework define a state space?**

A state space requires three things:
1. An enumerable set of states
2. A transition function between them
3. An observable that tells you which state you are in

CIL satisfies all three. States are coherence configurations over the utterance set $U$; transitions are ECHO activation updates; the observable is the conversation record itself. Every element of the formal model refers to something that exists in the world independently of the analyst's choices.

GML does not satisfy any of them. Its states ("freedom from interference" vs. "freedom to self-realize") are labels chosen by the analyst, not positions in a computable space. Its transition function — the MWC equation — requires parameters (L, c, n) that have no measurement procedure for abstract concepts. Its observable — R̄, the probability of being in the R-state — cannot be measured because the states themselves are not independently defined.

This is not a gap in GML's implementation. It is a consequence of a **category error in the founding analogy**: the MWC equation is valid for proteins because conformational states are physical facts with measurable energy differences. Language concepts are not physical systems. They have no conformational states, no free energy, no conservation laws, and no observer-independent equilibrium. Applying the MWC equation to concepts does not produce an approximation of something real — it produces a formally structured expression of the analyst's prior beliefs. The equation launders intuition into the appearance of quantitative output.

All subsequent comparisons should be read in light of this prior asymmetry: CIL has weaknesses that are engineering problems; GML has a weakness that is a logical one.

---

## 1. Summary of Core Claims

### GML Claims
1. Concepts exist as probability distributions over interpretive states analogous to allosteric protein conformations.
2. A six-operation algebra (`bind`, `equilibrium`, `cooperate`, `inhibit`, `activate`, `homeostasis`) provides a formal language for conceptual dynamics.
3. This algebra is implemented as a KnowAct cascade in the hKask agent system.
4. The framework is capability-gated via OCAP principles.
5. The MWC equation from biochemistry provides quantitative predictions about interpretive shift magnitude.

### CIL Claims
1. Collaboration quality is fundamentally a coherence problem, evaluable via Thagard's TEC model.
2. A coherence evaluator agent can score seven formal principles in real-time collaborative discourse.
3. Coherence scores fed back into agents' episodic memory create a self-improving collaboration loop.
4. Naive coherence optimization drives homophily; a taxonomy of incoherence types and an optimal tension model mitigate this.
5. The framework integrates with existing protocols (A2A, MCP) at defined protocol points.

---

## 2. Claim-by-Claim Comparison

### 2.1 Formal Foundations

**GML** borrows the MWC equation from biochemistry:
```
R̄ = (1 + α)ⁿ / ((1 + α)ⁿ + L·(1 + cα)ⁿ)
```
The equation is well-established in its home domain. The problem is the mapping: parameters L, c, n have no defined measurement procedure for abstract concepts, and the paper acknowledges this is open work. The analogy is structurally appealing but the load-bearing step — how do you assign L to "freedom"? — is unresolved.

**CIL** uses Thagard's ECHO model:
```
A_{t+1}(u_i) = clip_{[-1,1]}( (1-δ)·A_t(u_i) + η·∑_j w_{ij}·A_t(u_j) )
```
The equation is also borrowed from an established literature, but the mapping is much tighter. Utterances are the natural unit of discourse; excitatory/inhibitory links between them are interpretable; and convergence produces a scalar coherence score with a clear meaning. ECHO has been applied across domains (legal reasoning, scientific theory choice) so the framework's generality is validated by prior work, not asserted.

**Advantage: CIL.** The formal mapping is closer, prior cross-domain application exists, and parameter interpretation is cleaner.

---

### 2.2 Parameter Estimation

**GML** requires estimating L (default bias), c (selectivity), n (cooperativity dimensionality), and α (contextual pressure) for each concept. The paper provides no method for doing this and identifies it as a critical open question. In the worked examples, parameters are chosen by the author to produce the desired conclusion.

**CIL** requires: (a) classifying utterances into TEC categories (Claim, Evidence, Explanation, Analogy, Question), (b) assigning coherence/incoherence links between utterance pairs, and (c) setting ECHO hyperparameters δ and η. The paper acknowledges that utterance classification is imprecise and may require LLM assistance — but it also notes this as a known limitation with a known mitigation path. Critically, δ and η are **model hyperparameters** rather than concept-specific parameters: they can be set once and held constant across conversations. The concept-specific work (link assignment) is done by the evaluator agent as part of its operation, not by the system designer in advance.

**Advantage: CIL.** The parameter problem is scoped more tractably: classify utterances and assign links per conversation, rather than characterize abstract concepts in advance.

---

### 2.3 Implementation Specificity

**GML** presents:
- Rust struct sketches with fields but no methods or trait implementations
- A YAML cascade referencing an undefined `hKask` system
- Five Jinja2 templates described but not provided
- CNS instrumentation spans listed but not defined

The implementation section describes an architecture that cannot be built from the paper alone, referencing systems with no public existence.

**CIL** presents:
- A tuple definition `C = ⟨U, E, R⁺, R⁻, A, σ, τ⟩` that is fully specified
- An ASCII architecture diagram showing information flow through the improvement loop
- Explicit protocol integration points for MCP and A2A (including a concrete tool call: `evaluate_coherence(conversation_id)`)
- A phased cold-start model with defined episode thresholds (0-50, 50-200, 200+)
- A knowledge graph schema for the evaluator's relational memory

CIL does not provide source code either, but the specification is complete enough that an implementation could be designed from it without requiring access to undisclosed internal systems.

**Advantage: CIL.** The specification is buildable. GML's is not.

---

### 2.4 Handling of the Core Failure Mode

Every evaluation system risks optimizing the wrong thing. Both papers identify their respective failure modes.

**GML** identifies parameter subjectivity as its key limitation (Section 8) but does not provide structural mitigations. The framework has no mechanism for detecting or correcting when parameters encode the analyst's conclusion rather than an independently measured property.

**CIL** identifies the homophily trap as its key failure mode — coherence optimization converging on agreement and suppressing productive disagreement — and devotes an entire section (Section 5) to it. The mitigation is formal: a taxonomy of four incoherence types with defined signatures (combinations of principle scores), an optimal tension model with a bounded coherence range per task type, and explicit feedback design rules (structural not prescriptive, anti-convergence alerts). This is not a perfect solution, but it is a proposed mechanism with testable predictions.

**Advantage: CIL.** The failure mode is anticipated, formalized, and partially mitigated. GML's failure mode is acknowledged but unaddressed.

---

### 2.5 Agent Interaction Architecture

**GML** claims to be a framework for agent interactions but its operations are all applied to individual concepts, not to agents communicating. The `homeostasis` operation assesses a concept network's coherence but provides no feedback loop back to agents. The OCAP capability model names real security principles but provides no mechanism, threat model, or enforcement specification. There is no defined protocol for how agents exchange allosteric signals with each other.

**CIL** is explicitly an agent interaction architecture. The improvement loop diagram specifies:
- How the evaluator participates in sessions
- How feedback is delivered to participating agents
- How episodes are stored and consolidated
- How counterfactual reflection by agents generates learning
- How the evaluator's own knowledge graph evolves

The protocol integration table (Section 6.3) maps the evaluator's role across MCP, A2A, and REST, which means the framework can be adopted incrementally in existing multi-agent stacks.

**Advantage: CIL.** It is actually an agent interaction architecture. GML describes a conceptual analysis tool that has agent-adjacent vocabulary but no agent interaction protocol.

---

### 2.6 Temporal Dynamics

**GML** treats temporal dynamics as an open question (Section 7.4). The framework operates at equilibrium and says nothing about the order of operations, settling time, or path-dependence. This is a significant limitation for any system where agents interact in sequence.

**CIL** explicitly builds time into its formal model via `τ: U → ℝ`, the temporal ordering of utterances. This enables coherence trajectory analysis, phase transition detection, and repair sequence identification. The improvement loop architecture inherits temporal structure: episodes are ordered, consolidation happens after sessions, and feedback is delivered at defined points during or after a conversation.

**Advantage: CIL.** Time is a first-class element of the model.

---

### 2.7 Empirical Grounding

**GML** cites no empirical work beyond the original MWC biochemistry paper. The worked examples are analytical demonstrations with no ground truth. There is no reference to corpus data, user studies, or computational experiments validating any prediction of the model.

**CIL** grounds its central claim (diversity beats homogeneity) in Page's Diversity Prediction Theorem, Surowiecki on crowd wisdom, and Sunstein on group polarization — all empirical or theoretically proven results. The organizational theory analog to its incoherence taxonomy (task conflict vs. relationship conflict, Jehn 1995) is backed by experimental data. The framework's predictions about optimal coherence ranges are stated as empirically testable, and future work explicitly includes "controlled experiments comparing collaboration outcomes with and without coherence evaluation feedback."

**Advantage: CIL.** Claims are grounded in existing empirical literature and the paper articulates a validation path.

---

### 2.8 Scope Honesty

**GML** names itself "Generalized Monad Logic" (a mathematical structure it has not verified), claims a "formal framework" (whose parameters are unmeasurable), an "implementation" (in a system that does not publicly exist), and "capability-gated operations" (with no defined capability types). The gap between what is claimed and what is delivered is substantial.

**CIL** is more measured. It proposes a framework, identifies limitations forthrightly (utterance classification imprecision, optimal tension parameters need empirical estimation, human pragmatics are poorly captured), and positions future work honestly. The three stated contributions — formal model, improvement loop architecture, homophily trap analysis — are all delivered in the body of the paper at the level of specificity claimed.

**Advantage: CIL.** The paper delivers what it promises.

---

## 3. What GML Offers (and What It Does Not)

GML is not without value, but its value must be characterised precisely to avoid overstating it.

### 3.1 Vocabulary, Not Mechanism

GML's T/R state distinction, the L/c/n parameter vocabulary, and the five questions method provide a **structured language for human analysts** thinking about contested concepts. They are closer in kind to a well-designed interview protocol or a qualitative codebook than to a computational model. Used as such — as a thinking tool for a human trying to map the interpretive landscape of a debate — they can be genuinely useful.

The important constraint: this value exists entirely at the level of human interpretation. It does not transfer to system design because none of GML's vocabulary generates computable inputs. An agent system cannot use "the allosteric port for security threats has c=0.1" because there is no procedure for arriving at that value independently of the person who typed it.

### 3.2 The Allosteric Port as Intuition Pump

The idea that a concept has discrete contextual handles — specific framings that, when activated, shift its interpretive equilibrium — is an evocative and potentially productive intuition. It might, in the future, motivate a genuinely formal model: perhaps a learned attention map over contextual features, or a topic model where document context shifts the probability distribution over interpretive clusters.

But the paper does not build that model. The allosteric port is a metaphor that points toward where a formal model might live, not the model itself. Crediting it as a "design concept for agent systems" overstates what is currently available.

---

## 4. Can They Be Combined? The Grounding Problem

The previous section identified GML's allosteric port model as a potential complement to CIL — a way to characterize each agent's conceptual commitments before evaluation begins. This deserves harder scrutiny, because the combination only works if GML can actually provide something CIL could use.

It cannot. Here is why.

### 4.1 The grounding gap is not a detail — it is the whole problem

For GML to contribute agent-level modeling to a CIL architecture, it would need to supply, for each agent and each concept, values for L, c, and n *before the conversation begins*. Without those values, there is no allosteric model — just the metaphor of one.

GML provides no mechanism for producing those values. Its parameter estimation section (7.2) lists four approaches (elicitation, behavioral inference, LLM estimation, Bayesian updating) but specifies none of them. An approach is not a mechanism. The allosteric port abstraction — the idea that concepts have specific contextual handles — is linguistically generative: it lets you *talk* about why a concept shifts. It does not let you *compute* anything about it, because the port parameters are never defined.

This is not a solvable gap by stitching the two papers together. It is a research gap that requires independent work.

### 4.2 What GML's "richness" actually is

The T/R state distinction, the L/c/n vocabulary, the five questions method — these are **interpretive scaffolding**, not formal model components. They help a human analyst structure their thinking about a contested concept. They are closer to a well-designed interview protocol than to a computational model.

This is genuinely useful for some purposes. It is not useful as a foundation for agent modeling, because:

- Agents cannot introspect their own L values — those would have to be assigned externally
- External assignment requires the parameter estimation methodology GML explicitly does not have
- Even if you forced an LLM to produce L estimates, there is no validation that those estimates are consistent, stable, or predictive
- The resulting numbers would be **laundered intuition**: the analyst's prior beliefs passed through an equation and returned as quantitative output

CIL avoids this problem entirely because it does not try to model agents before they speak. It observes what they actually say and evaluates the relational structure of those utterances. The data is the conversation, not a pre-specified model of each participant.

### 4.3 The honest table

| Dimension | GML | CIL |
|---|---|---|
| Unit of analysis | Individual concept (pre-specified) | Utterance in conversation (observed) |
| Data source | Analyst's parameter assignments | The conversation itself |
| What it can compute | R̄ given parameters you chose | Coherence score from observable structure |
| Agent modeling | Not possible without grounded parameters | Not needed — emerges from discourse |
| Buildable from the paper? | No | Yes |

The combination framing was too generous. GML does not describe agents — it describes a vocabulary for discussing agents. CIL describes a mechanism for evaluating them. Only one of those is a foundation for system development.

---

## 5. Verdict

**For system development, CIL is the only viable foundation. GML is not a system.**

The decisive issue is not mathematical elegance or scope of ambition — it is whether the framework provides a mechanism that generates observable, consistent outputs from real inputs. CIL does. GML does not.

More precisely:

- **CIL's inputs are observable**: utterances in a conversation, classifiable by an LLM, linkable by a coherence evaluator agent operating at runtime.
- **GML's inputs are unobservable**: L, c, n for abstract concepts, requiring a parameter estimation methodology the paper explicitly does not provide.

This means GML's richest-seeming feature — the ability to model *why* a concept shifts — is not a computational feature at all. It is a post-hoc description. You can apply the MWC equation to explain a shift that already happened, choosing parameters that fit your account. You cannot use it to predict or constrain what will happen next, because the parameters are not grounded in anything independent of the analyst's judgment.

GML is best understood as an **interpretive vocabulary** — a structured way of thinking and talking about conceptual dynamics, closer in kind to frame semantics or conceptual blending than to a computational framework. Used as such, it has real value. The five questions method is a legitimate elicitation tool. The T/R state distinction is a useful analytical lens.

But vocabulary does not compose with mechanism. You cannot wire a dictionary to a database and call it a knowledge graph. Similarly, you cannot attach GML's conceptual labels to CIL's coherence engine and get agent-level modeling. The missing piece — grounded parameter estimation — is not a small addition. It is an independent research programme.

**For development: build on CIL. Use GML's vocabulary, if at all, as a qualitative lens for interpreting what CIL's evaluator reports — not as a formal input to the system.**

---

*Internal comparative analysis. Not for external distribution without author consent.*
