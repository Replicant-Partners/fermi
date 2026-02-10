# Fermi Agent Bestiary MCP Server - Quick Start

Access 27 AI agents directly from Zed editor.

## Setup (3 steps)

### 1. Build

```bash
cd /path/to/fermi
cargo build --bin agent-mcp-server
```

### 2. Configure Zed

Open Zed settings (`Ctrl+,`) and add to `context_servers`:

```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/home/ilabra/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-api03-YOUR_KEY_HERE",
        "AGENTS_DIR": "/home/ilabra/fermi/agents/curated"
      }
    }
  }
}
```

### 3. Restart Zed

Green dot next to "fermi-agent-bestiary" in Agent Panel settings = working.

## Tools Reference

| Tool | Description | Example prompt |
|------|-------------|----------------|
| `list_agents` | List all 27 agents | "List the bestiary agents" |
| `get_agent` | Detailed agent info | "Tell me about coherence_evaluator" |
| `execute_agent` | Run an agent query | "Execute macro_forecaster: cloud growth projections 2026" |
| `save_agent` | Save stats + git commit | "Save macro_forecaster stats" |
| `search_agents` | Search by keyword/tag/tier | "Search for social-media agents" |
| `get_catalogue` | Full catalogue by category | "Show the agent catalogue" |
| `ask_xaman_ek` | Ask the navigator anything | "Ask Xaman Ek: design a research workspace" |

## 5-Minute Walkthrough

**1. Browse the catalogue**
```
Show me the full agent catalogue with composition patterns
```

**2. Find agents for your task**
```
Search for agents that handle coherence
```

**3. Get details on one**
```
Tell me about the cohere_and_coordinate agent
```

**4. Run a query**
```
Execute sentiment_analyzer with: "What is public opinion on AI regulation in the EU?"
```

**5. Ask the navigator**
```
Ask Xaman Ek: I need a workspace team for competitive market research on fintech.
What agents should I hire and how do they compose?
```

## Composition Patterns

These are pre-designed agent teams for common tasks:

- **Artist Deck**: style_transfer + watermark + delivery
- **Social Media Studio**: social_media_studio + instagram_publisher + bluesky_publisher
- **Research Team**: macro_forecaster + entity_investigator + sentiment_analyzer + monte_carlo_sim
- **Coherence Stack**: coherence_evaluator + coherence_consultant + intention_coordinator
- **Full Coordination**: cohere_and_coordinate (single compound agent)

## Full Documentation

See [MCP_SETUP.md](../../shared/MCP_SETUP.md) for the complete guide with all example workflows, troubleshooting, and the full 27-agent catalogue.
