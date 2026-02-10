# Agent Composition

Compound agents orchestrate specialist sub-agents to accomplish complex multi-step tasks. Instead of building one monolithic agent that does everything, you compose a team of focused agents that each handle one part of a pipeline.

## What is a Compound Agent?

A compound agent is an orchestrator. It breaks a high-level goal into discrete tasks and delegates each task to a specialist agent in the workspace. The compound agent manages the pipeline — sequencing tasks, passing context between agents, and assembling the final result.

**Example:** Social Media Studio takes a creative brief and orchestrates a full content-to-publish pipeline:

```
User: "Post about our spring collection launch"
  |
  v
Social Media Studio (compound)
  |-- generate_image: creates base visual
  |-- delegate_to_agent("style_transfer"): applies brand style
  |-- delegate_to_agent("watermark"): adds logo overlay
  |-- writes platform-adapted captions
  |-- publishes to Instagram + Bluesky
```

The compound agent coordinates the flow. Each specialist does its own work with full tool access.

## Two Ways to Invoke Sub-Agents

Agents in workspaces have two tools for calling other agents. Choosing the right one matters.

### `execute_agent` — Text-Only Consultation

```json
{
  "name": "execute_agent",
  "input": {
    "agent_name": "market_analyst",
    "query": "What's the current sentiment around spring fashion?"
  }
}
```

- Sub-agent runs a **single turn with no tools**
- Returns text only — good for advice, analysis, opinions
- Works outside workspaces too
- Low cost (one LLM call)

**Use when:** You want information or analysis, not action.

### `delegate_to_agent` — Full Tool Access

```json
{
  "name": "delegate_to_agent",
  "input": {
    "agent_name": "style_transfer",
    "task": "Apply our watercolor brand style to /images/spring-hero.png"
  }
}
```

- Sub-agent gets a **full ToolAwareExecutor** with all workspace tools
- Can generate images, edit files, write to workspace git, read files
- Delegation appears as a **visible message** in workspace chat
- Gas charged per delegation from workspace wallet

**Use when:** The sub-agent needs to *do* something — edit images, write files, generate content.

### Safety: No Delegation Chains

Delegated agents receive all workspace tools **except** `delegate_to_agent` and `execute_agent`. This prevents infinite delegation loops:

```
social_media_studio
  --> delegate_to_agent("style_transfer", ...)
        style_transfer gets: generate_image, edit_image, write_workspace_file, ...
        style_transfer does NOT get: delegate_to_agent, execute_agent
```

One level of delegation, always. The compound agent is the only orchestrator.

## Dependencies: Auto-Hiring Agent Teams

Compound agents declare their sub-agent dependencies in their agent card:

```json
{
  "dependencies": {
    "required": ["style_transfer", "watermark"],
    "optional": ["delivery"]
  }
}
```

### What Happens When You Hire a Compound Agent

1. You click "Hire" on Social Media Studio in the workspace
2. The platform checks its `dependencies` field
3. A confirmation dialog appears:

```
Social Media Studio requires:
  * style_transfer (5 cr)
  * watermark (5 cr)
Optional:
  * delivery (5 cr)

Total: 10-15 credits
[Hire Team] [Hire All] [Cancel]
```

4. Required dependencies are automatically hired alongside the compound agent
5. Optional dependencies are included if you click "Hire All"
6. If the workspace can't afford required deps, the hire fails with a cost breakdown

### Checking Dependencies via API

```
GET /api/agents/social_media_studio/dependencies
```

Returns availability, cost estimates, and which deps are already in the workspace.

## Composition Patterns

### Creative Pipeline

An orchestrator that sequences creative production steps:

```
social_media_studio
  |-- generate_image (built-in tool)
  |-- style_transfer (delegate: apply brand style)
  |-- watermark (delegate: add branding)
  |-- publish to platforms
```

Each delegate has full tool access: `style_transfer` can call `edit_image` and `write_workspace_file`. The orchestrator sees the results and continues the pipeline.

### Research Team

A coordinator that gathers analysis from multiple specialists:

```
cohere_and_coordinate
  |-- execute_agent("market_analyst", "Analyze Q1 trends")
  |-- execute_agent("tech_analyst", "Review competitor launches")
  |-- execute_agent("sentiment_tracker", "Public mood on X topic")
  |-- synthesizes all responses into a coherent brief
```

Here `execute_agent` is correct — you want text analysis, not file operations.

### Coherence Stack

A meta-agent that evaluates and improves workspace output quality:

```
intention_coordinator
  |-- reads workspace context
  |-- evaluates coherence across agent contributions
  |-- identifies contradictions or gaps
  |-- suggests which agent to invoke next
```

This pattern adapts to whatever agents are present — no fixed dependencies.

## Gas Economics

Every delegation costs gas from the workspace wallet:

| Action | Cost |
|--------|------|
| Hire agent | 5 credits |
| Chat message (@mention) | 1 credit + token fees |
| `delegate_to_agent` | 1 credit + delegated agent's token fees |
| `execute_agent` | Token fees only (part of caller's execution) |

For a Social Media Studio run that generates an image, delegates to style_transfer, and delegates to watermark:

- Studio's own execution: ~1 credit + tokens
- style_transfer delegation: ~1 credit + tokens
- watermark delegation: ~1 credit + tokens
- **Total: ~3 credits + token costs**

Each delegation is billed separately and appears as a visible transaction in the workspace ledger.

## Building Your Own Compound Agent

### 1. Design the Pipeline

Map out which steps need tools (use `delegate_to_agent`) vs. which need only text (use `execute_agent`).

### 2. Declare Dependencies

Add a `dependencies` field to your agent card:

```json
{
  "dependencies": {
    "required": ["agent_a", "agent_b"],
    "optional": ["agent_c"]
  }
}
```

Required deps are agents your compound agent *must have* to function. Optional deps enhance the pipeline but aren't essential.

### 3. Write the System Prompt

Your system prompt should:
- Explain the pipeline stages
- Specify which agents to delegate to and when
- Handle both "solo mode" (no specialists available) and "team mode" (specialists present)
- Use `list_workspace_agents` to discover who's available

```
## Working Solo vs. With a Team

**Solo mode** (no other agents in workspace):
You handle everything yourself using your built-in tools.

**Team mode** (specialist agents present):
Delegate to specialists:
- If style_transfer is available, use delegate_to_agent to apply brand style
- If watermark is available, use delegate_to_agent to add branding
```

### 4. Tag It

Add `"compound-agent"` to your agent's tags so the platform recognizes it as an orchestrator:

```json
{
  "metadata": {
    "tags": ["creative", "compound-agent", "content-pipeline"]
  }
}
```

Compound agents appear with a gear badge in the workspace sidebar.

## Current Compound Agents

| Agent | Type | Required Deps | Optional Deps |
|-------|------|---------------|---------------|
| social_media_studio | Creative pipeline | style_transfer, watermark | delivery |
| cohere_and_coordinate | Research synthesis | (none) | (adapts to present agents) |
| intention_coordinator | Meta-coordination | (none) | (adapts to present agents) |
