# Fermi Agent Development Guide

**How to build, register, and deploy a domain agent for the Fermi forecasting platform.**

Version: 1.0.0  
Date: 2026-03-07  
Status: Living document — updated as the agent system evolves

---

## Overview

Fermi is a probabilistic forecasting platform where AI agents research evidence for forecast drivers. The **Fermi meta-agent** orchestrates research by recommending which specialist agent to assign to which driver based on the driver's domain.

Any agent tagged `fermi-orchestra` becomes available in the Fermi Console. When a user creates a forecast, the system automatically recommends your agent for drivers that match its skills and tags — no hardcoding required.

### Architecture

```
User types forecast question
  ↓
Fermi decomposes into drivers (base rate + probability multipliers)
  ↓
For each driver, Fermi recommends an agent based on skill/tag matching
  ↓
User assigns agent → agent executes via ABW API → evidence flows back
  ↓
Evidence updates driver confidence → Monte Carlo simulation → probability
```

**Key principle:** Agents execute through the ABW (Agent Bestiary World) API. The console never calls LLMs directly — ABW handles model selection, API keys, credit accounting, and execution. Your agent's system prompt, model, and tools are configured on ABW.

---

## Quick Start (30 minutes)

### 1. Create the agent card

```bash
mkdir -p agents/curated/my_agent
```

Create `agents/curated/my_agent/agent_card.json`:

```json
{
  "agent_id": "my_agent",
  "agent_type": "research",
  "version": "1.0.0",
  "tier": "curated",
  "system_prompt": "You are a specialist in [YOUR DOMAIN]. You are part of the Fermi research orchestra.\n\nYour role: provide evidence for probabilistic forecasts.\n\nIMPORTANT OUTPUT FORMAT:\n- Specific data points with sources\n- Key findings as a bulleted list\n- Relevance score (0.0-1.0)\n- Confidence in your findings (0.0-1.0)\n\nBe quantitative. Cite sources.",
  "capabilities": {
    "executor": "llm",
    "mcp_tools": [],
    "skills": ["skill-one", "skill-two", "domain-keyword"],
    "model": "claude-sonnet-4-5-20250929",
    "temperature": 0.3,
    "provider": "anthropic"
  },
  "metadata": {
    "created": "2026-03-07",
    "author": "Your Name",
    "description": "One-line description of what this agent does for forecasts.",
    "tags": ["domain-tag", "specific-tag", "fermi-orchestra"],
    "sample_queries": [
      "Example forecast question this agent can help with"
    ]
  },
  "accepts": ["query"],
  "produces": ["evidence"],
  "dependencies": {
    "required": [],
    "optional": []
  }
}
```

### 2. Register on ABW

```bash
ABW_TOKEN="your-token" ./scripts/sync-fermi-orchestra.sh
```

Or manually:

```bash
curl -X POST https://agent-bestiary.world/api/agents \
  -H "Authorization: Bearer $ABW_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "my_agent",
    "agent_type": "research",
    "description": "One-line description",
    "system_prompt": "Your full system prompt here...",
    "model": "claude-sonnet-4-5-20250929",
    "temperature": 0.3,
    "executor_type": "llm",
    "tags": ["domain-tag", "fermi-orchestra"],
    "visibility": "public",
    "llm_provider": "anthropic"
  }'
```

### 3. Test in the console

```bash
cargo run -p fermi-console
```

Sign in → type a forecast question in your agent's domain → your agent should appear in the agent picker and be recommended for relevant drivers.

### 4. Verify end-to-end

```bash
curl -X POST https://agent-bestiary.world/api/agents/my_agent/execute \
  -H "Authorization: Bearer $ABW_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "Research the probability of [domain-specific question]"}'
```

---

## Agent Card Specification

The agent card (`agent_card.json`) is the single source of truth for your agent's identity, capabilities, and behavior. It lives in `agents/curated/<agent_id>/agent_card.json` locally and is registered on ABW for execution.

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `agent_id` | string | Unique identifier. Snake_case, lowercase. Must be unique across ABW. |
| `agent_type` | string | `"research"` for forecast agents. Other types: `"system"`, `"creative"`, `"infrastructure"`. |
| `version` | string | Semver. Bump when you change the system prompt or capabilities. |
| `tier` | string | `"curated"` (team-built), `"community"` (third-party), `"system"` (platform). |
| `system_prompt` | string | The full system prompt sent to the LLM. **This is the most important field.** |
| `capabilities` | object | Model, tools, skills. See below. |
| `metadata` | object | Description, tags, samples. See below. |

### System Prompt Guidelines

The system prompt is what makes your agent good or bad at forecasting. Follow these principles:

**1. Identity and orchestra context**
```
You are the [Agent Name], a specialist in [domain]. You are part of the Fermi
research orchestra — designed to be called alongside macro_forecaster,
market_research, sentiment_analyzer, and entity_investigator to build
evidence for probabilistic forecasts.
```

**2. Specific domain expertise**
```
You have deep expertise in:
- [Domain area 1]
- [Domain area 2]
- [Specific knowledge that helps with forecasting]
```

**3. Role in the forecasting workflow**
```
Your role in the forecasting workflow:
1. [What you research for the forecast]
2. [What base rates you can provide]
3. [What competitive/contextual analysis you do]
```

**4. Output format (critical)**
```
IMPORTANT OUTPUT FORMAT:
Always structure your response as evidence for a forecast. Include:
- Specific data points with sources
- Key findings as a bulleted list
- Relevance score (0.0-1.0) indicating how directly this evidence
  bears on the forecast question
- Confidence in your findings (0.0-1.0)

Be quantitative. Instead of "[vague statement]", say "[specific data
with source and date]".
```

**5. Domain-specific base rates (if applicable)**
```
When providing base rates, use domain-specific historical data:
- [Example: "Phase 2→3 oncology success rate is ~28%"]
- [Example: "Series A biotech companies have a 12% IPO rate within 5 years"]
```

### Capabilities Object

```json
{
  "capabilities": {
    "executor": "llm",
    "mcp_tools": [],
    "skills": ["keyword-one", "keyword-two"],
    "model": "claude-sonnet-4-5-20250929",
    "temperature": 0.3,
    "provider": "anthropic"
  }
}
```

| Field | Description |
|-------|-------------|
| `executor` | Always `"llm"` for research agents. |
| `mcp_tools` | MCP server tools this agent can call. See [MCP Tools](#mcp-tools). |
| `skills` | **Critical for auto-recommendation.** Hyphenated keywords that describe what this agent knows. The console matches these against driver names to suggest your agent. |
| `model` | LLM model identifier. Use `claude-sonnet-4-5-20250929` for quality. Use `claude-3-haiku-20240307` for speed/cost. |
| `temperature` | 0.0-1.0. Use 0.2-0.4 for factual research agents. |
| `provider` | `"anthropic"`, `"openai"`, `"mistral"`, `"openrouter"`. |

### Metadata Object

```json
{
  "metadata": {
    "created": "2026-03-07",
    "author": "Your Name",
    "description": "One-line description shown in the agent picker.",
    "tags": ["domain", "specific-topic", "fermi-orchestra"],
    "sample_queries": [
      "Example forecast question this agent can help with"
    ]
  }
}
```

| Field | Description |
|-------|-------------|
| `description` | Shown in the console's agent picker. Keep it concise and action-oriented. |
| `tags` | **Must include `"fermi-orchestra"` to appear in the console.** Other tags drive search and recommendation. |
| `sample_queries` | Example forecast questions. Shown to users considering your agent. |

### How Auto-Recommendation Works

When a user creates a forecast, Fermi decomposes it into drivers (e.g., `clinical_trial_success`, `market_competition`, `regulatory_risk`). For each driver, the console scores every `fermi-orchestra` agent by matching:

1. **Skills** (2 points per match) — `capabilities.skills` words matched against driver name + rationale
2. **Tags** (1 point per match) — `metadata.tags` words matched against driver name + rationale
3. **Description** (1 point per match) — `metadata.description` keywords matched against driver words

The highest-scoring agent is recommended for each driver. **This means your agent's `skills` and `tags` directly determine when it gets recommended.** Choose them carefully.

Example: For a driver named `drug_pipeline_value`, an agent with skills `["drug-development", "pipeline-valuation"]` scores 4 points (2 matches × 2 points), beating a generic `market_research` agent that scores 0.

---

## MCP Tools

Agents can call MCP (Model Context Protocol) servers for external data. This is how domain agents access specialized APIs.

### Declaring MCP Tools

```json
{
  "mcp_tools": [
    {
      "name": "search_ontology_terms",
      "description": "Search BioPortal for biomedical ontology terms",
      "input_schema": {
        "type": "object",
        "properties": {
          "query": {
            "type": "string",
            "description": "The biomedical term to search"
          }
        },
        "required": ["query"]
      },
      "server": "bioportal-mcp"
    }
  ]
}
```

### Available MCP Servers

MCP tools are executed server-side by ABW. The MCP server must be:
- Registered with ABW (contact platform admins)
- Accessible via HTTP streaming (`streamable_url`) or as a subprocess (`command`)

Current MCP servers available to agents:
- `bioportal-mcp` — BioPortal biomedical ontology search (800+ ontologies)
- Additional servers can be proposed via the ABW admin process

### Tool Execution Flow

```
Agent prompt includes tool descriptions
  ↓
LLM decides to call a tool (e.g., search_ontology_terms)
  ↓
ABW's ToolAwareExecutor intercepts the tool call
  ↓
ABW calls the MCP server with the tool parameters
  ↓
MCP server returns results
  ↓
Results injected back into the LLM conversation
  ↓
LLM incorporates tool results into its evidence response
```

---

## Worked Example: biotech_analyst

This section walks through building the `biotech_analyst` agent — a domain expert in biotechnology, clinical trials, and drug development that integrates with BioPortal ontologies.

### Step 1: Identify the domain gap

**Question:** "What forecast domains are underserved by the current orchestra?"

Current agents cover:
- `macro_forecaster` — economics, policy, GDP, inflation
- `market_research` — competitive dynamics, market sizing, trends
- `sentiment_analyzer` — news/social sentiment, public perception
- `entity_investigator` — OSINT, company research, ownership structures

**Gap:** Life sciences, biotech, pharma, clinical trials. A question like "Will Eli Lilly's donanemab get full FDA approval?" has no specialist agent.

### Step 2: Define domain expertise

For biotech forecasting, the agent needs to know:
- Clinical trial phases and historical success rates by therapeutic area
- FDA regulatory pathways (standard, accelerated, breakthrough)
- Drug pipeline valuation drivers
- Disease biology and mechanism of action
- Biomedical ontologies for precise terminology

**Domain-specific base rates** (the most valuable contribution):
```
Phase 1 → Approval (all): 7.9%
Phase 1 → Approval (oncology): 5.7%
Phase 2 → Approval (all): 15.4%
Phase 3 → Approval (all): 57.9%
Phase 1 → Approval (rare disease): 17.2%
```

These come from published meta-analyses (BIO/QLS Advisors) and are embedded in the agent card's `metadata.domain_knowledge` for reference.

### Step 3: Design the system prompt

Key decisions:
- **Model:** Sonnet (not Haiku) — domain expertise requires reasoning ability
- **Temperature:** 0.3 — factual, not creative
- **Output format:** Evidence-structured with quantitative base rates and ontology references

The full prompt is ~2,400 chars. See `agents/curated/biotech_analyst/agent_card.json`.

### Step 4: Choose skills and tags for auto-recommendation

```json
"skills": [
  "clinical-trials", "drug-development", "biomedical-ontologies",
  "disease-biology", "regulatory-analysis", "pipeline-valuation",
  "genomics", "proteomics", "pharmacology", "biotech-valuation"
],
"tags": [
  "biotech", "pharma", "clinical-trials", "drug-development",
  "life-sciences", "ontology", "bioportal", "fermi-orchestra"
]
```

Now when a forecast has a driver named `clinical_trial_success` or `drug_pipeline`, this agent scores highest and gets recommended automatically.

### Step 5: Add MCP tools (optional)

The biotech_analyst declares BioPortal MCP tools:
- `search_ontology_terms` — look up diseases, drugs, genes in standard vocabularies
- `search_ontology_properties` — find relationships (treats, causes, associated_with)
- `get_ontology_analytics` — identify which ontologies are most active

These tools let the agent ground its evidence in standardized biomedical terminology rather than free-texting disease names.

### Step 6: Register on ABW

```bash
ABW_TOKEN="your-token" ./scripts/sync-fermi-orchestra.sh
```

Or test directly:
```bash
curl -X POST https://agent-bestiary.world/api/agents/biotech_analyst/execute \
  -H "Authorization: Bearer $ABW_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "What is the Phase 3 success rate for GLP-1 agonists in NASH?"}'
```

### Step 7: Verify in the console

1. Launch: `cargo run -p fermi-console`
2. Sign in via Dashboard → Google/GitHub
3. Type: "Will a GLP-1 drug get FDA approval for NASH by 2027?"
4. Press Ctrl+Enter
5. Check: `biotech_analyst` should appear in the agent picker
6. Assign it to a relevant driver (e.g., `clinical_trial_success`)
7. Verify evidence flows back with domain-specific data

---

## Updating an Existing Agent

To update an agent's system prompt, model, or tags on ABW:

```bash
curl -X PUT https://agent-bestiary.world/api/agents/<agent_id> \
  -H "Authorization: Bearer $ABW_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "system_prompt": "Updated prompt...",
    "model": "claude-sonnet-4-5-20250929",
    "tags": ["tag1", "tag2", "fermi-orchestra"]
  }'
```

Only include the fields you want to change. See `AgentUpdate` struct for all updatable fields:
- `description`
- `system_prompt`
- `visibility` (`"public"` or `"private"`)
- `tags`
- `model`
- `temperature`
- `status` (`"published"`, `"draft"`)
- `display_alias`
- `accepts`
- `produces`

**Always update both** the local `agent_card.json` and ABW to keep them in sync.

---

## Testing Your Agent

### Unit test: Direct execution

```bash
# Execute via ABW and check the response structure
curl -s -X POST "https://agent-bestiary.world/api/agents/my_agent/execute" \
  -H "Authorization: Bearer $ABW_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "Your test query"}' | python3 -m json.tool
```

Verify the response has:
- `evidence` array with at least one item
- Each evidence item has `summary`, `key_findings`, `relevance`
- `confidence` score (0.0-1.0)
- `metadata.reasoning` with substantive analysis

### Integration test: Console workflow

1. Create a forecast in the agent's domain
2. Verify the agent is recommended for relevant drivers
3. Assign the agent and check evidence quality
4. Run simulation (Ctrl+R) and verify the evidence affects probability
5. Save (Ctrl+S) and reload — verify evidence persists

### Quality checklist

- [ ] System prompt instructs evidence-structured output
- [ ] Agent provides specific data points, not vague statements
- [ ] Agent cites sources (publications, databases, official statistics)
- [ ] Agent provides domain-specific base rates when relevant
- [ ] Relevance and confidence scores are reasonable (not always 0.5)
- [ ] Agent handles unclear queries gracefully (asks for clarification or provides general domain context)
- [ ] Skills and tags cover the agent's key domains for auto-recommendation
- [ ] Model is appropriate for the task (Sonnet for reasoning, Haiku for simple retrieval)

---

## Agent Economics

Agents on ABW participate in a credit economy:

- **Execution cost:** Each agent call costs credits (proportional to tokens used)
- **User pays:** The forecaster's wallet is charged when they assign an agent
- **Agent earns:** Agent creators earn a percentage of execution credits (configurable via `auto_collect_pct`)

### Pricing your agent

- Simple research agents (Haiku, no tools): ~3-5 credits per call
- Domain experts (Sonnet, no tools): ~8-15 credits per call
- Tool-augmented agents (Sonnet + MCP): ~15-30 credits per call (depends on tool usage)

Credit costs are set by the platform based on the model used. You don't set prices directly.

---

## Reference: Fermi Orchestra Agents

Current fermi-orchestra agents as of 2026-03-07:

| Agent | Domain | Skills | Model |
|-------|--------|--------|-------|
| `fermi` | Meta-orchestration | tetlock-methodology, forecast-decomposition, agent-orchestration | Sonnet |
| `macro_forecaster` | Economics / policy | macroeconomics, scenario-forecasting, base-rates | Sonnet |
| `market_research` | Markets / competition | market-analysis, competitive-intelligence, trend-forecasting | Sonnet |
| `sentiment_analyzer` | News / social | sentiment-analysis, social-listening, emotion-detection | Sonnet |
| `entity_investigator` | OSINT / due diligence | osint, investigation, entity-resolution, knowledge-graph | Sonnet |
| `biotech_analyst` | Life sciences / pharma | clinical-trials, drug-development, biomedical-ontologies, genomics | Sonnet |

### Domain gaps (agents we'd love to see built)

- **Geopolitics:** International relations, conflict risk, diplomatic history, treaty analysis
- **Climate/Energy:** Emissions forecasting, energy transition, policy impact, IPCC data
- **Technology adoption:** S-curves, diffusion models, tech readiness levels, patent analysis
- **Legal/Regulatory:** Case law analysis, regulatory precedent, legislative tracking
- **Sports analytics:** Player performance, team dynamics, historical matchup data
- **Real estate:** Market cycles, demographic trends, zoning/development analysis
- **Crypto/DeFi:** On-chain analytics, protocol metrics, governance tracking

---

## Troubleshooting

### Agent doesn't appear in picker

- **Missing tag:** Ensure `"fermi-orchestra"` is in `metadata.tags`
- **Not on ABW:** Run `./scripts/sync-fermi-orchestra.sh` or register manually
- **Private visibility:** Set `"visibility": "public"` so other users can discover it

### Agent produces poor quality output

- **No system prompt on ABW:** Check with `curl /api/agents?limit=100` — the `system_prompt` field should have content
- **Wrong model:** Haiku is too weak for domain reasoning. Upgrade to Sonnet via `PUT /api/agents/<id>`
- **Prompt too vague:** Add the `IMPORTANT OUTPUT FORMAT` section. Agents need explicit structure

### Agent not recommended for relevant drivers

- **Skills don't match:** Add hyphenated skills that contain keywords likely to appear in driver names
- **Too few skills:** More skills = more chances to match. Add 5-10 relevant skills
- **Tag overlap:** If your agent competes with `market_research` for generic terms, add more specific skills

### MCP tools not working

- **Tool not registered on ABW:** Contact platform admins to register the MCP server
- **Tool schema mismatch:** The `input_schema` in your card must match what the MCP server expects
- **API key needed:** Some MCP servers need API keys set as environment variables on the ABW server

---

## File Reference

```
agents/curated/<agent_id>/
  └── agent_card.json          # Agent definition (system prompt, skills, tools)

scripts/
  ├── sync-fermi-orchestra.sh  # Register all fermi-orchestra agents on ABW
  └── package-console.sh       # Package console for testers

docs/fermi/guides/
  └── AGENT_DEVELOPMENT.md     # This file
```

---

## Changelog

- **2026-03-07:** Initial version. Covers agent card spec, ABW registration, auto-recommendation, MCP tools, worked example (biotech_analyst).

---

## Contact

- **Platform:** [Agent Bestiary World](https://agent-bestiary.world)
- **Repository:** [github.com/Replicant-Partners/fermi](https://github.com/Replicant-Partners/fermi)
- **Agent sync:** `./scripts/sync-fermi-orchestra.sh`
