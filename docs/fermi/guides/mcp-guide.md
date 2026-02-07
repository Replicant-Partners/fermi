# Fermi Agent Bestiary MCP Server - Quick Start

Access AI-powered forecasting agents directly from Zed editor!

## Quick Setup (3 steps)

### 1. Build the MCP Server

```bash
cd /home/ilabra/fermi
cargo build --release --bin agent-mcp-server
```

### 2. Configure Zed

Copy the Zed MCP configuration:

**Linux/macOS:**
```bash
# Create Zed config directory if it doesn't exist
mkdir -p ~/.config/zed

# Copy the sample config (edit with your API key)
cp zed-mcp-config.json ~/.config/zed/mcp.json

# Edit to add your API key
nano ~/.config/zed/mcp.json
```

**Or manually create** `~/.config/zed/mcp.json`:

```json
{
  "mcpServers": {
    "fermi-agent-bestiary": {
      "command": "/home/ilabra/fermi/target/release/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "sk-ant-api03-YOUR_API_KEY_HERE",
        "AGENTS_DIR": "/home/ilabra/fermi/agents/curated"
      }
    }
  }
}
```

Replace `YOUR_API_KEY_HERE` with your actual Anthropic API key.

### 3. Restart Zed

Close and reopen Zed. The Fermi Agent Bestiary tools will be available in the assistant!

## Using the Tools

Once configured, try these commands in Zed's AI assistant:

### List Available Agents
```
What forecasting agents are available?
```

### Get Agent Details
```
Tell me about the market_research agent
```

### Run a Research Query
```
Use the market_research agent to investigate: 
"What are the key trends in AI chip development for 2026?"
```

### Save Agent Stats
```
Save the market_research agent statistics
```

## Available Agents

The system comes with two curated agents:

1. **market_research** - Analyzes market trends, competitive landscapes, and industry forecasts
2. **sentiment_analyzer** - Evaluates sentiment and public opinion on topics

## Architecture

```
Zed Editor
    │
    │ (MCP Protocol - stdio)
    ▼
Fermi Agent Bestiary MCP Server
    │
    ├─► Agent Registry (in-memory + filesystem)
    ├─► LLM Executor (Claude API)
    └─► Git Integration (auto-commit stats)
```

## Tools Reference

| Tool | Description | Parameters |
|------|-------------|------------|
| `list_agents` | List all available agents | None |
| `get_agent` | Get detailed agent info | `agent_id` |
| `execute_agent` | Run a research query | `agent_id`, `query` |
| `save_agent` | Save stats to git | `agent_id` |

## Troubleshooting

**Server not appearing in Zed?**
- Check Zed logs: `~/.config/zed/logs/`
- Verify binary exists: `ls -l target/release/agent-mcp-server`
- Test manually: `ANTHROPIC_API_KEY="your-key" ./target/release/agent-mcp-server`

**"Using Mock Executor" message?**
- Your API key is not set or invalid
- Check the key in `~/.config/zed/mcp.json`

**No agents loaded?**
- Verify agent cards exist: `ls agents/curated/*/agent_card.json`
- Check `AGENTS_DIR` path in config

## Advanced Usage

### Custom Agents

Create your own agent by adding a new directory:

```bash
mkdir agents/curated/my_agent
```

Create `agents/curated/my_agent/agent_card.json`:

```json
{
  "agent_id": "my_agent",
  "agent_type": "research",
  "version": "0.1.0",
  "tier": "curated",
  "capabilities": {
    "executor": "LLM",
    "mcp_tools": [],
    "skills": ["analysis"],
    "model": "claude-3-haiku-20240307",
    "temperature": 0.7
  },
  "performance": {
    "forecasts_contributed": 0,
    "avg_brier_impact": 0.0,
    "avg_confidence": 0.0,
    "accuracy_rate": 0.0
  },
  "usage": {
    "total_executions": 0,
    "successful_executions": 0,
    "failed_executions": 0,
    "total_tokens_used": 0,
    "total_cost_usd": 0.0,
    "avg_execution_time_ms": 0,
    "last_30_days": {
      "executions": 0,
      "tokens": 0,
      "cost_usd": 0.0
    }
  },
  "wallet": null,
  "ontology_stats": {
    "entities": 0,
    "relationships": 0,
    "total_evidence": 0
  },
  "metadata": {
    "author": "Your Name",
    "created_at": "2026-02-05T00:00:00Z",
    "updated_at": "2026-02-05T00:00:00Z",
    "description": "My custom agent description",
    "tags": ["custom", "research"],
    "license": "MIT"
  }
}
```

Restart Zed to load the new agent.

### Git History

Agent statistics are automatically versioned:

```bash
# View agent update history
git log --oneline --grep="agent(" agents/curated/

# See specific agent changes
git log agents/curated/market_research/agent_card.json
```

## Documentation

For detailed documentation, see:
- [MCP Setup Guide](docs/MCP_SETUP.md) - Complete setup and troubleshooting
- [Agent Bestiary Design](docs/AGENT_BESTIARY_DESIGN.md) - Architecture and design
- [Model Context Protocol](https://modelcontextprotocol.io/) - MCP specification

## Example Session

Here's what a typical workflow looks like:

```
User: What agents do I have available?