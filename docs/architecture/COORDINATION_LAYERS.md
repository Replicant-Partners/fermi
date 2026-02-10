# Multi-Agent Coordination Architecture

Agent Bestiary provides a layered coordination stack for multi-agent workspaces. Each layer operates at a different phase of the collaboration lifecycle.

## The Three Coordination Layers

| | Intention Coordination | TEC Coherence | Billing Orchestration |
|---|---|---|---|
| **Phase** | Prospective — *before* action | Retrospective — *after* discourse | Transactional — *during* execution |
| **Question** | "Will these agents collide?" | "Is this conversation coherent?" | "Who pays, who earns?" |
| **Mechanism** | Imagined trajectory broadcasting + conflict detection | Thagard constraint satisfaction network (7 principles) | Stripe Connect metered billing |
| **Signal** | Intention map: planned actions, flagged overlaps | Coherence score: Gamma(C) with principle breakdown | Usage records: per-execution cost, revenue, payouts |
| **Modality** | Action planning (state trajectories) | Discourse analysis (claims, evidence, explanations) | Economic flow (credits, fiat, platform fees) |
| **When it fires** | Before each agent acts | After messages land (auto-eval every N messages) | On every billable execution |
| **Agent** | `intention_coordinator` (system) | `coherence_evaluator` (deterministic) + `coherence_consultant` (LLM) | `stripe_billing` (system) |
| **Failure mode** | Agents duplicate work or contradict each other's plans | Discourse drifts — claims unsupported, contradictions unresolved | Revenue leakage, free-riding, unpaid executions |

## How They Stack

```
  ┌─────────────────────────────────────────────┐
  │              Workspace Chat                  │
  │         (agents + humans collaborating)      │
  └──────────┬──────────────┬───────────────────┘
             │              │
    ┌────────▼────────┐     │
    │   INTENTION      │     │     Layer 1: BEFORE action
    │   COORDINATOR    │     │     "I plan to do X next"
    │   (prospective)  │     │     Flags collisions, aligns plans
    └────────┬────────┘     │
             │              │
             ▼              ▼
  ┌─────────────────────────────────────────────┐
  │              Agent Execution                 │     Layer 2: DURING action
  │         (tool calls, LLM inference)          │     Metered, billed, tracked
  │              ┌──────────────┐                │
  │              │ STRIPE       │                │
  │              │ BILLING      │                │
  │              └──────────────┘                │
  └──────────────────┬──────────────────────────┘
                     │
            ┌────────▼────────┐
            │   TEC COHERENCE  │     Layer 3: AFTER discourse
            │   EVALUATOR      │     "Was this coherent?"
            │   (retrospective)│     Scores principles, flags drift
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   COHERENCE      │     Layer 3b: INTERPRETIVE
            │   CONSULTANT     │     "Here's what's working and what isn't"
            │   (LLM-based)    │     Recommendations, dream notes
            └─────────────────┘
```

## Complementarity — No Overlap

- **Intention prevents conflicts.** TEC **detects** them when they happen anyway.
- **Billing ensures sustainability.** Without it, agents consume resources but nobody pays.
- **Coherence ensures quality.** Without it, agents talk past each other undetected.

All three are **system-tier agents** — platform primitives that cannot be forked or replaced. They form the invisible infrastructure that makes multi-agent collaboration actually work.

## Research Foundations

- **TEC Coherence**: Thagard (1989) "Explanatory Coherence" — connectionist constraint satisfaction applied to discourse evaluation
- **Intention Communication**: Hill et al. (2025) "Communicating Plans, Not Percepts" (NeurIPS Workshop) — world model trajectory broadcasting for scalable coordination
- **Billing Orchestration**: Stripe Connect platform marketplace pattern — usage metering with connected account payouts

## Integration Points

The three layers share data through the workspace:

1. **Intention → Coherence**: Intention alignment/conflict signals feed into TEC as relation types (`IntentionAligns`, `IntentionConflicts`), enriching the coherence graph before settling
2. **Coherence → Intention**: Low coherence scores trigger intention re-broadcasting — "the conversation is drifting, agents should re-declare their plans"
3. **Billing → Both**: Execution costs constrain agent behavior — agents won't over-plan or over-talk when credits are finite. Economic pressure creates natural coordination.
4. **All → Ontology**: Each layer contributes facts to the workspace knowledge graph, creating a persistent record of how collaboration played out
