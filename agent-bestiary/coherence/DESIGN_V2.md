# Coherence Evaluator v2 — Design & Assumptions

> Evolving from passive observer to invited participant and collaborative learning agent.

## 1. Role in the System Architecture

The coherence evaluator is an **invited participant** in conversations between agents, and between agents and humans. It is not a passive wiretap or infrastructure service — it joins when asked, provides evaluative feedback, and can be dismissed.

Its purpose is twofold:

1. **Evaluate** — Score the coherence of multi-party discourse using Thagard's TEC model (the deterministic core)
2. **Learn** — Accumulate knowledge about which agent-agent and agent-human pairings produce effective collaboration, and feed that knowledge back to participants

The feedback it provides to agent participants becomes material for their own dreaming cycles — a mechanism for agents to learn about effective collaboration through counterfactual reflection.

## 2. What We Keep from v1

The formal TEC model is sound and stays:

- **The tuple** `C = <U, E, R+, R-, A, sigma>` — utterance classification, coherence/incoherence relations, activation settling, principle scoring
- **The ECHO settling algorithm** — connectionist constraint satisfaction
- **The seven principles** — Symmetry, Explanation, Analogy, Data Priority, Contradiction, Competition, Acceptability
- **`coherence-core`** and **`coherence-engine`** crates — these are the formal engine

## 3. What Changes

### 3.1 From Passive Observer to Invited Participant

v1 design: "passive observer that periodically emits structured feedback without disrupting the conversation flow"

v2 design: The coherence evaluator is **invited into conversations**. It:
- Receives an invitation (via execution or tool call)
- Observes the conversation thread
- Provides evaluative feedback as a participant (not silently)
- Can be asked follow-up questions ("why is coherence dropping?")
- Can be dismissed when no longer needed

This means it operates through the **standard bestiary execution pipeline**: it gets invoked, produces output, that output becomes an episode, and it consolidates/dreams like any other agent.

### 3.2 From Deterministic-Only to Deterministic + LLM

The executor should be `llm` (not `rust` or `mcp`). The architecture becomes:

```
                        +------------------+
  Conversation  ------> | TEC Formal Model |  (deterministic)
  Thread                | ECHO Settling    |
                        +--------+---------+
                                 |
                          scores, structure
                                 |
                        +--------v---------+
                        | LLM Reasoning    |  (interpretive)
                        | - Explain why    |
                        | - Actionable     |
                        |   feedback       |
                        | - Historical     |
                        |   context        |
                        +------------------+
```

The deterministic TEC scoring informs the LLM's reasoning, but the agent needs language to:
- Explain *why* coherence is high or low
- Give actionable, contextual feedback to participants
- Draw on its accumulated experience ("In past collaborations between these agents, X pattern produced better outcomes")

### 3.3 Learning Through the ADM Pipeline

The coherence evaluator learns like any other bestiary agent:

- **Episodes** — each evaluation session produces an episode (the conversation, the scores, the feedback given, and ideally the *outcome quality* of the collaboration)
- **Consolidation** — dreaming cycles extract patterns: "which pairings cohere well on which tasks", "what feedback actually improved subsequent collaboration"
- **Knowledge graph** — entities are agents and humans; facts are relational coherence data; rules are learned heuristics about effective teaming

Key: its dreaming should produce **relational knowledge**, not just failure-pattern rules. The ontology evolves to model a graph of collaborative effectiveness.

## 4. The Homophily Problem

### 4.1 The Risk

If the coherence evaluator only optimizes for "things hang together," it will push agents toward:
- Agreement and consensus-seeking
- Similar reasoning styles
- Comfortable, low-friction interaction
- Echo chambers

This is antithetical to good forecasting, which often depends on productive friction.

### 4.2 Destructive vs. Productive Incoherence

The evaluator must distinguish between:

| Type | Characteristics | Desirable? |
|------|----------------|------------|
| **Destructive incoherence** | Talking past each other, contradictions nobody notices, fragmented reasoning that goes nowhere, no evidence engagement | No |
| **Productive incoherence** | Genuine disagreement that sharpens thinking, competing hypotheses that force evidence evaluation, analogical tension that reveals hidden assumptions | Yes |

### 4.3 How the Formal Model Handles This

Thagard's Principle 6 (Competition) already models rival explanations as incoherent — but the theory resolves competition through **evidence weight**, not by eliminating the competitor. A conversation where two well-supported rival hypotheses compete is *formally less coherent* but may be *epistemically superior*.

This means raw coherence score Gamma(C) is insufficient as a quality metric. We need additional dimensions.

### 4.4 Design Response: Optimal Tension

**Assumption 1: There exists an optimal level of productive disagreement for a given task type.**

- Pure consensus (Gamma -> 1.0) is suspicious for forecasting tasks — it suggests groupthink
- Pure fragmentation (Gamma -> 0.0) is unproductive
- The sweet spot depends on the task: exploratory research benefits from more friction than execution planning

**Assumption 2: The quality signal comes from outcomes, not from coherence alone.**

The evaluator should track:
- Coherence scores at conversation time
- Downstream outcome quality (forecast accuracy, task completion, decision quality)
- The relationship between the two

Over time, its dreaming should learn: "For agent X + agent Y on forecasting tasks, Gamma ~ 0.5 with high P6 (competition) produces better Brier scores than Gamma ~ 0.8."

**Assumption 3: Feedback should describe structure, not prescribe agreement.**

The evaluator's feedback to participants should NOT be:
- "You're incoherent, fix it"
- "You should agree with agent X"

It SHOULD be:
- "Here's the structure of your disagreement"
- "These two explanations compete — here's what evidence would resolve it"
- "You're building on each other's reasoning well in area A but talking past each other in area B"
- "Historical pattern: this kind of tension tends to produce better outcomes when resolved through [specific mechanism]"

### 4.5 Protecting Necessary Friction

**Assumption 4: The evaluator should actively protect productive disagreement, not smooth it away.**

If it detects that agents are converging too quickly (rising Gamma without evidence resolution of competing hypotheses), it should flag that as a potential problem — not celebrate it.

This inverts the naive coherence optimization: sometimes the evaluator's job is to say "this conversation is too coherent — you're not challenging each other enough."

## 5. Integration Architecture

### 5.1 Invocation

The coherence evaluator is invoked like any bestiary agent:

```
POST /api/agents/coherence_evaluator/execute
{
  "query": "<conversation thread or session reference>"
}
```

It can also be invoked as an MCP tool by other agents during their own execution:
```
evaluate_coherence(conversation_id: "...")
```

### 5.2 Feedback as Dreaming Material

When the evaluator provides feedback to agent participants:
1. The feedback is delivered as part of the evaluator's output
2. Agent participants receive this feedback as input to their next execution or as context
3. When those agents dream, the evaluator's feedback becomes material for counterfactual reflection: "The coherence evaluator said my reasoning was fragmented — what would I do differently?"

This creates a learning loop:
```
Agent executes  --->  Coherence evaluator scores  --->  Feedback delivered
      ^                                                        |
      |                                                        v
      +----  Agent dreams on feedback  <----  Agent's next episode includes feedback
```

### 5.3 What the Evaluator Doesn't Do

- It does NOT run as a separate service (no standalone coherence-api server)
- It does NOT silently monitor all conversations
- It does NOT have authority to modify other agents' behavior
- It does NOT optimize for maximum coherence

## 6. Crate Architecture (Simplified)

v1 had 5 crates. v2 simplifies:

| Keep | Purpose |
|------|---------|
| `coherence-core` | Formal model, types, relations, principles — the math |
| `coherence-engine` | ECHO settling algorithm, principle scoring — the computation |

| Remove/Defer | Reason |
|--------------|--------|
| `coherence-observer` | Heuristic classification — replaced by LLM-based understanding |
| `coherence-api` | Standalone REST server — evaluator uses the bestiary's standard execution pipeline instead |
| `coherence-protocols` | Premature abstraction — MCP integration happens through the standard bestiary MCP tools |

The evaluator's system prompt instructs the LLM to:
1. Parse the conversation into TEC structures (replacing the heuristic classifier)
2. Invoke the deterministic engine for scoring
3. Interpret the scores with historical context from its accumulated episodes
4. Generate actionable, structure-describing feedback

## 7. Open Questions

1. **How does the evaluator access conversation threads?** Does it receive the full transcript, or a session ID it can query? What's the data model for multi-party conversations in the bestiary?

2. **How is outcome quality measured?** For forecasting tasks, Brier scores are natural. For other collaboration types, what's the ground truth signal that lets the evaluator learn whether high or low coherence was better?

3. **Temporal dynamics** — Should the evaluator provide feedback in real-time (after each message) or in batch (at conversation end)? Real-time feedback changes the conversation; batch feedback only informs future interactions.

4. **How does "too coherent" feedback land?** Telling agents "you agree too much" is a novel kind of feedback. How should agents' dreaming processes handle it? Does it need special treatment in the consolidation pipeline?

5. **Multi-evaluator scenarios** — If multiple evaluators are invited (e.g., a coherence evaluator and a bias detector), how do their feedback streams interact? Could they evaluate each other?

6. **Cold start** — Before the evaluator has accumulated experience, its feedback is purely formal (TEC scores). How quickly can it learn useful relational patterns? What's the minimum episode count before its historical context adds value?

## 8. Assumptions Summary

| # | Assumption | Implication |
|---|-----------|-------------|
| A1 | Optimal tension exists and varies by task | Evaluator needs task-type awareness |
| A2 | Quality signal comes from outcomes | Need to connect coherence episodes to downstream results |
| A3 | Feedback describes structure, doesn't prescribe agreement | LLM prompt engineering must enforce this |
| A4 | Productive friction should be protected | Evaluator may flag *too much* coherence as a problem |
| A5 | The evaluator learns through standard ADM | No special infrastructure — episodes, consolidation, dreaming |
| A6 | Feedback to agents seeds their counterfactual reasoning | Requires the feedback delivery mechanism to be well-defined |
| A7 | The homophily risk is real and must be actively mitigated | Can't just optimize Gamma; need multi-dimensional quality model |

## 9. References

- Thagard, P. (1989). Explanatory Coherence. _Behavioral and Brain Sciences_, 12(3), 435-467.
- Thagard, P. & Verbeurgt, K. (1998). Coherence as constraint satisfaction. _Cognitive Science_, 22(1), 1-24.
- Sunstein, C. (2002). The Law of Group Polarization. _Journal of Political Philosophy_, 10(2), 175-195.
- Page, S. (2007). _The Difference: How the Power of Diversity Creates Better Groups_. Princeton University Press.
- Surowiecki, J. (2004). _The Wisdom of Crowds_. Doubleday.
