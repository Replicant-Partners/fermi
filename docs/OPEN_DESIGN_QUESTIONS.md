# Open Design Questions & Conceptual Clarifications

Captured 2026-02-08 after MVP Sprint 1-5 deployment. These need resolution before next iteration.

---

## 1. Hire vs Execute — Agent Lifecycle

**Current state:** "Hire Agent" button was replaced by "Execute Agent" — but these are different concepts.

**Correct model:**
- **Hire** = bring an agent into a team/workspace. This is an organizational action, not a computation.
  - For agents you DON'T own: hiring means adding them to your workspace (like recruiting)
  - For agents you DO own: hiring to a workspace means assigning them to a shared context
- **Execute** = run a query against an agent. Can only happen AFTER the agent is in your workspace (or is your own personal agent).
- **Flow:** Browse bestiary -> Hire agent to workspace -> Execute within workspace context

**Action needed:** Restore "Hire" as a distinct concept. Execute should require the agent to be "hired" (associated with a workspace/team the caller belongs to, or owned by the caller directly).

---

## 2. Gas Fees Apply to ALL A2A Transactions

**Current state:** Gas fees only apply to execution (10% surcharge on token cost).

**Correct model:** Gas should apply to ALL agent-to-agent transactions regardless of ownership:
- Agent execution (already implemented)
- Hiring an agent to a workspace
- Agent-to-agent communication (future AKP)
- Education/consolidation cycles
- Any computational action that touches the platform

**Rationale:** Gas fees fund the platform. Even your own agents consume infrastructure. This creates a uniform economic model where every transaction has a cost, encouraging efficiency.

**Action needed:** Define gas fee schedule for each transaction type. Current: 10% on execution. Need rates for: hire, transfer, consolidation, A2A messages.

---

## 3. Agentified Agent Creation (MCP Coach)

**Current state:** Agent creation is a dumb HTML form that posts JSON to an API.

**Correct model:** Agent creation should be an agentified process with an MCP-type coaching server that:
- Steers users toward high-quality agent design
- References practices from `/agents/templates/` (design checklist, prompt engineering guide, ontology patterns)
- Coaches through the 11-point design checklist interactively
- Validates agent cards against schema in real-time
- Suggests improvements to system prompts
- Recommends appropriate executor types, models, and temperature settings
- Warns about common pitfalls (overly broad scope, missing error handling, ontology over-engineering)
- Can be "deep and efficient" — thorough when needed, streamlined when the user knows what they want

**Architecture options:**
1. MCP server that the creation page talks to via WebSocket
2. Conversational UI in the creation page that calls an agent-creation-coach agent
3. Zed extension integration (the original DSL vision)
4. All of the above — web UI for casual users, Zed for power users

**Action needed:** Design the agent-creation-coach agent. It needs access to the template library and design practices as context.

---

## 4. Model Support — Not Just Claude

**Current state:** Everything is hardcoded to Claude models (Haiku, Sonnet, Opus). The creation form and executor only reference Anthropic.

**Correct model:** Support multiple model providers:
- **Anthropic** (Claude): Haiku, Sonnet, Opus — current default
- **Mistral**: Open-weight models, self-hostable
- **Qwen**: Open-weight models, self-hostable
- **OpenRouter**: Meta-router for many models (Llama, Gemma, etc.)
- **Bring Your Own**: User-provided API endpoints

**Self-hosting note:** Replicant Partners will likely host and run own Mistral and Qwen instances for:
- Testing custom embeddings
- Running proprietary workloads without external API dependency
- Cost optimization for high-volume operations

**Action needed:**
- Abstract LLM executor to support multiple backends (not just Anthropic API)
- Update agent_card schema: model field should reference provider + model_id
- Update creation form with model provider dropdown + model selector
- Define embedding provider options (currently hardcoded to Voyage/Anthropic)

---

## 5. Executor Types & Agent Taxonomy

**Current state:** Executor types are: `llm`, `mcp`, `manual`, `skill`. But deterministic agents don't fit cleanly.

**The Monte Carlo question:** Fermi currently has a Monte Carlo simulation model. As a standalone agent, it would be "deterministic" — no LLM involved. Where does it sit in the taxonomy?

**Proposed taxonomy:**

| Executor | Description | Example |
|----------|-------------|---------|
| `llm` | LLM-powered, single model call | sentiment_analyzer |
| `mcp` | LLM + external tools via MCP | market_research |
| `deterministic` | Algorithmic, no LLM | monte_carlo_sim, coherence_evaluator (TEC core) |
| `hybrid` | Deterministic core + LLM interpretation layer | coherence_evaluator (TEC + LLM narration) |
| `skill` | Orchestrates multiple sub-agents | risk_assessment_pipeline |
| `manual` | Human-in-the-loop | expert_review |
| `custom` | User-provided execution endpoint | bring-your-own |

**Key insight:** The coherence evaluator already demonstrates the `hybrid` pattern — deterministic TEC computation with an LLM layer for interpretation. This is likely a common pattern.

**Action needed:** Formalize the taxonomy. Update AgentCard executor enum. Ensure the execution pipeline can route to the right backend based on executor type.

---

## 6. Workspace Draft Implementation

**Current state:** Workspaces are thin wrappers around teams with budget fields. The concept needs more substance.

**Open questions:**
- What does a workspace "look like" to a user? Is it a persistent environment where agents run?
- Do agents in a workspace share context/memory? (Shared ontology? Shared episodes?)
- Can a workspace have its own ontology that emerges from the collective activity of its agents?
- How do workspace-level gas fees differ from individual agent gas fees?
- What's the relationship between workspace budget and individual agent education budgets?

---

## Priority Order

1. **Hire vs Execute** — Conceptual fix, affects UX flow
2. **Gas on all transactions** — Economic model foundation
3. **Model support** — Technical prerequisite for diverse agents
4. **Executor taxonomy** — Needed before agent creation coaching makes sense
5. **Agentified creation** — HIGH value but depends on 3 & 4
6. **Workspace substance** — Ongoing design conversation

---

## References

- Agent templates: `/home/ilabra/fermi/agents/templates/`
- Design checklist: `/home/ilabra/fermi/agents/templates/DESIGN_CHECKLIST.md`
- Prompt engineering guide: `/home/ilabra/fermi/agents/templates/PROMPT_ENGINEERING_GUIDE.md`
- Coherence evaluator design (hybrid pattern): `/home/ilabra/fermi/agent-bestiary/coherence/DESIGN_V2.md`
- AKP whitepaper: `/home/ilabra/fermi/docs/papers/coherence_improvement_loop.md`
