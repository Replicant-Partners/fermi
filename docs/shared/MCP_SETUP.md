# Fermi Agent Bestiary MCP Server

Interact with 27 AI agents directly from Zed editor via Model Context Protocol.

## Quick Setup

### 1. Build

```bash
cd /path/to/fermi
cargo build --bin agent-mcp-server
```

### 2. Configure Zed

Open Zed settings (`Ctrl+,` or `Cmd+,`) and add to `context_servers`:

```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/home/ilabra/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-api03-YOUR_KEY_HERE",
        "AGENTS_DIR": "/home/ilabra/fermi/agents/curated",
        "RUST_LOG": "info"
      }
    }
  }
}
```

### 3. Restart Zed

After saving settings, restart Zed. Check the Agent Panel settings for a green dot next to "fermi-agent-bestiary" confirming the server is active.

## How to Invoke Tools in Zed

Open the **Agent Panel** (`Ctrl+Shift+A` / `Cmd+Shift+A`). Then just talk to it naturally — mention the server name or tool name to help Zed route to the right tool.

**Tip**: Mentioning "fermi" or "bestiary" in your prompt helps Zed select the right MCP tools. You can also create a custom profile that enables only the Bestiary tools.

**Tool approval**: By default, Zed asks you to approve each tool call. Set `"agent.always_allow_tool_actions": true` in settings to auto-approve (useful during development).

---

## 7 Available Tools

### `list_agents`
List all agents with metadata, skills, and performance stats.

```
List all the agents in the bestiary
```

### `get_agent`
Get detailed info on a specific agent — capabilities, model, tools, performance.

```
Show me everything about the coherence_evaluator agent
```

### `execute_agent`
Run any agent with a query. Returns evidence, confidence scores, and execution metrics.

```
Execute macro_forecaster with: "What are the growth projections for cloud infrastructure in 2026?"
```

### `save_agent`
Save an agent's updated stats to disk and auto-commit to git.

```
Save the macro_forecaster agent stats
```

### `search_agents`
Search by keyword, tag, type, or tier. Matches against agent ID, description, tags, and skills.

```
Search for agents tagged with "social-media"
Search for agents with "coherence" capabilities
Find all system-tier agents
```

### `get_catalogue`
Get the complete catalogue organized by category with composition patterns.

```
Show me the full agent catalogue
What composition patterns are available?
```

### `ask_xaman_ek`
Ask the platform navigator anything. Xaman Ek knows every agent, every composition pattern, and every platform feature. This is the power tool — it reasons about the full bestiary.

```
Ask Xaman Ek: What agents do I need for a social media content pipeline?
Ask Xaman Ek: Compare macro_forecaster vs monte_carlo_sim for economic analysis
Ask Xaman Ek: How does the coherence system work?
```

---

## Example Workflows

### Discovering agents for a task

```
You: I need to do competitive intelligence on the fintech sector. What agents should I use?

# Zed calls search_agents("fintech") or ask_xaman_ek
# Returns: macro_forecaster, entity_investigator, sentiment_analyzer, market_research

You: Tell me more about entity_investigator

# Zed calls get_agent("entity_investigator")
# Returns: full capabilities, model, tools, sample queries

You: Execute entity_investigator with: "Map the key players in embedded finance 2025-2026"

# Zed calls execute_agent with agent_id + query
# Returns: evidence, key findings, confidence score
```

### Designing a compound agent

```
You: Show me the catalogue

# Zed calls get_catalogue
# Returns: 27 agents in 7 categories + 5 composition patterns

You: Ask Xaman Ek: I want to build a content pipeline that generates
     branded images and cross-posts to Instagram and Bluesky

# Xaman Ek reasons about the full bestiary and responds:
#
# You need the Social Media Studio composition:
# - social_media_studio (compound orchestrator, Sonnet)
# - style_transfer (brand-consistent visual styling)
# - watermark (logo/branding overlay)
# - instagram_publisher (Graph API: images, carousels, reels, scheduling)
# - bluesky_publisher (AT Protocol: posts, threads, quote posts)
#
# social_media_studio can work solo (it has generate_image built in)
# or delegate to the specialists when they're in the workspace.
# Hire cost: 5cr each = 25cr for the full deck.

You: What about just Bluesky for now?

# Xaman Ek: Hire bluesky_publisher standalone (5cr).
# It handles posts, threads, image uploads (up to 4, max 1MB each),
# rich text facets (mentions, links, hashtags), and quote posts.
# Add social_media_studio later when you want image generation + cross-posting.
```

### Checking coherence in a workspace

```
You: Ask Xaman Ek: My research workspace feels fragmented.
     The agents aren't building on each other's work. What should I do?

# Xaman Ek:
# Your workspace needs the Coherence Stack:
# - coherence_evaluator: runs TEC (7-principle constraint satisfaction)
#   on your workspace messages, scores alignment
# - coherence_consultant: interprets the scores, diagnoses which
#   principles are failing (Symmetry? Explanation? Data Priority?)
# - intention_coordinator: prevents future conflicts by having agents
#   declare intentions before acting
#
# Or hire cohere_and_coordinate (compound agent) which combines
# retrospective coherence analysis with prospective coordination
# in a single agent. Cheaper and simpler for most cases.
#
# Start with: evaluate the workspace coherence, diagnose the weak
# principles, then assign specific roles to each agent.
```

### Building a new agent from the editor

```
You: I want to create a new agent for regulatory compliance monitoring.
     What should it look like?

# Zed calls ask_xaman_ek with the question

You: Execute publish_coach with: "Review this agent card for
     publication readiness: [paste your draft agent_card.json]"

# publish_coach scores the card 1-10, identifies strengths,
# suggests improvements, and flags quick wins

You: Save the agent stats after the review

# Zed calls save_agent("publish_coach")
```

### Quick reference lookups

```
You: Search for all creative agents
# search_agents("creative") → style_transfer, watermark, delivery,
#   instagram_publisher, bluesky_publisher, social_media_studio

You: Search for system tier
# search_agents("system") → stripe_billing, intention_coordinator, xaman_ek

You: What composition patterns exist?
# get_catalogue → Artist Deck, Social Media Studio, Research Team,
#   Coherence Stack, Full Coordination
```

---

## Agent Catalogue (27 agents)

### Research & Analysis
| Agent | What it does | Model |
|---|---|---|
| macro_forecaster | Geopolitical and economic trend analysis | Sonnet |
| market_research | Competitive intelligence and market sizing | Sonnet |
| monte_carlo_sim | Probabilistic simulation engine | Sonnet |
| sentiment_analyzer | Text sentiment and opinion mining | Haiku |
| entity_investigator | Entity relationship mapping and OSINT | Sonnet |
| video_analyst | Video content analysis via Twelve Labs | Haiku |

### Creative & Visual
| Agent | What it does | Model |
|---|---|---|
| style_transfer | Apply artistic styles to images (Gemini) | Haiku |
| watermark | Add branding overlays to images | Haiku |
| delivery | Serve completed artwork from workspace | Haiku |

### Social Media & Publishing
| Agent | What it does | Model |
|---|---|---|
| instagram_publisher | Instagram Graph API (images, carousels, reels, scheduling) | Haiku |
| bluesky_publisher | Bluesky AT Protocol (posts, threads, quote posts) | Haiku |
| social_media_studio | Compound: generate, style, brand, publish across platforms | Sonnet |

### Coherence & Coordination
| Agent | What it does | Model |
|---|---|---|
| coherence_evaluator | TEC engine: 7-principle constraint satisfaction scoring | Haiku |
| coherence_consultant | Interprets coherence scores, diagnoses principle failures | Sonnet |
| cohere_and_coordinate | Compound: coherence analysis + prospective coordination | Sonnet |
| intention_coordinator | Prospective coordination via intention declaration | Haiku |

### Billing & Economics
| Agent | What it does | Model |
|---|---|---|
| stripe_billing | System: Stripe Connect payments, usage metering, payouts | Sonnet |
| stripe_connect_advisor | Stripe Connect architecture guidance | Sonnet |

### Meta & Platform
| Agent | What it does | Model |
|---|---|---|
| xaman_ek | Platform navigator — knows everything, guides composition | Haiku |
| publish_coach | Reviews agent cards for publication readiness | Sonnet |
| companion_builder_coach | Guides creation of companion-style agents | Sonnet |
| embedding_projector_guide | Interprets PCA/t-SNE memory projections | Haiku |
| dream_narrator | Generates narrative synopses from consolidation | Haiku |

### Games & Engagement
| Agent | What it does | Model |
|---|---|---|
| daily_puzzle | Fermi estimation puzzles with scoring | Haiku |
| performance_coach | Tracks and improves agent performance metrics | Sonnet |
| micro_patron_template | Template for patronage-model agents | Haiku |
| ar_avatar_renderer | AR avatar generation (runtime deferred) | Haiku |

### Composition Patterns

| Pattern | Agents | Use case |
|---|---|---|
| **Artist Deck** | style_transfer + watermark + delivery | Generate and brand visual content |
| **Social Media Studio** | social_media_studio + instagram_publisher + bluesky_publisher | Content-to-publish pipeline |
| **Research Team** | macro_forecaster + entity_investigator + sentiment_analyzer + monte_carlo_sim | Deep research |
| **Coherence Stack** | coherence_evaluator + coherence_consultant + intention_coordinator | Workspace alignment |
| **Full Coordination** | cohere_and_coordinate | Single compound agent for coherence + coordination |

---

## Architecture

```
┌─────────────────────┐
│  Zed Editor         │
│  (Agent Panel)      │
└─────────┬───────────┘
          │ MCP Protocol (stdio)
          │
┌─────────▼───────────────────────┐
│  Fermi Agent Bestiary           │
│  MCP Server (7 tools)           │
│                                 │
│  list_agents    search_agents   │
│  get_agent      get_catalogue   │
│  execute_agent  ask_xaman_ek    │
│  save_agent                     │
└─────────┬───────────────────────┘
          │
          ├──► Agent Registry (27 agent cards from filesystem)
          ├──► LLM Executor (Claude API via ANTHROPIC_API_KEY)
          └──► Git Integration (auto-commit on save_agent)
```

## Troubleshooting

**Server not appearing?**
- Check Agent Panel settings for green/red indicator dot
- Check Zed logs: `~/.config/zed/logs/`
- Test manually: `ANTHROPIC_API_KEY="your-key" AGENTS_DIR="agents/curated" ./target/debug/agent-mcp-server`

**"Using Mock Executor" in stderr?**
- ANTHROPIC_API_KEY not set or invalid in settings.json

**No agents loaded?**
- Verify: `ls agents/curated/*/agent_card.json | wc -l` (should be 27)
- Check AGENTS_DIR path in settings

**Tool calls not routing?**
- Mention "fermi" or "bestiary" in your prompt
- Create a custom Zed profile with only Bestiary tools enabled
- Set `"agent.always_allow_tool_actions": true` to skip approval dialogs
