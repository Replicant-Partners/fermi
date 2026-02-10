# Zed Extension & MCP Setup

Use Agent Bestiary directly from your editor. The MCP (Model Context Protocol) server exposes all platform agents as tools you can invoke from Zed's assistant panel.

## Prerequisites

- [Zed editor](https://zed.dev) installed
- [Rust toolchain](https://rustup.rs) installed
- The Fermi repository cloned locally

## Build the MCP Server

```bash
cd fermi
cargo build --bin agent-mcp-server
```

This produces the binary at `target/debug/agent-mcp-server` (or `target/release/agent-mcp-server` with `--release`).

## Configure Zed

Add the MCP server to your Zed settings. Open Zed settings (`Cmd+,` on macOS) and add to the `context_servers` section:

```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/path/to/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "your-api-key",
        "AGENTS_DIR": "/path/to/fermi/agents/curated",
        "RUST_LOG": "info"
      }
    }
  }
}
```

Replace `/path/to/fermi` with your actual clone path, and set your Anthropic API key.

### Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes (for execution) | Your Anthropic API key for running agents |
| `AGENTS_DIR` | Yes | Path to the agent cards directory |
| `RUST_LOG` | No | Log level: `error`, `warn`, `info`, `debug`, `trace` |

## Available MCP Tools

Once connected, these tools appear in Zed's assistant panel:

| Tool | Description |
|------|-------------|
| `list_agents` | List all agents with metadata (name, type, description, tags) |
| `get_agent` | Get detailed info for a specific agent including capabilities and performance |
| `execute_agent` | Run an agent with a query and receive evidence-based insights |
| `save_agent` | Save an agent's updated performance statistics to disk and commit to git |
| `search_agents` | Search agents by keyword, tag, type, or tier |
| `get_catalogue` | Full catalogue grouped by category with composition patterns |
| `ask_xaman_ek` | Ask the platform navigator anything about the bestiary |

### Example: List All Agents

In Zed's assistant panel, ask Claude to use the `list_agents` tool:

```
Show me all available research agents
```

The assistant calls `list_agents` and filters the results for you.

### Example: Execute an Agent

```
Use the market_sentiment agent to analyze current AI chip market trends
```

The assistant calls `execute_agent` with:
```json
{
  "agent_id": "market_sentiment",
  "query": "Analyze current AI chip market trends"
}
```

You get back the agent's analysis with confidence scores and evidence.

### Example: Search by Tags

```
Find all creative agents that can generate images
```

The assistant calls `search_agents` with relevant keywords and returns matching agents.

### Example: Ask Xaman Ek

```
Ask Xaman Ek how to build a compound agent
```

The platform navigator responds with contextual guidance about the bestiary.

## Using with Zed Assistant

The MCP tools integrate naturally with Zed's AI assistant:

1. Open the assistant panel (`Cmd+?` or `Ctrl+?`)
2. Ask questions — the assistant can call MCP tools automatically
3. Results from agent executions appear inline in the conversation
4. You can chain queries: research with one agent, then create with another

### Workflow: Research then Create

```
1. "Use search_agents to find agents related to market analysis"
2. "Execute the tech_analysis agent with: Compare NVIDIA and AMD datacenter revenue"
3. "Save the tech_analysis agent stats"
```

Each step uses a different MCP tool, building on the previous result.

## Adding the Neon Database Server

For database diagnostics and direct SQL access, you can also add the Neon MCP server:

```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/path/to/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "your-api-key",
        "AGENTS_DIR": "/path/to/fermi/agents/curated",
        "RUST_LOG": "info"
      }
    },
    "neon": {
      "command": "npx",
      "args": ["@neondatabase/mcp-server-neon"],
      "env": {
        "NEON_API_KEY": "your-neon-api-key"
      }
    }
  }
}
```

## Troubleshooting

### MCP Server Not Connecting

**Check the binary exists:**
```bash
ls -la target/debug/agent-mcp-server
```

If missing, rebuild: `cargo build --bin agent-mcp-server`

**Check the agents directory:**
```bash
ls agents/curated/
```

You should see directories like `market_sentiment/`, `tech_analysis/`, `social_media_studio/`, etc.

**View MCP logs in Zed:**

Go to `View > Debug > Language Server Logs` and look for the `fermi-agent-bestiary` server.

### Agent Execution Fails

**No API key:** Listing and searching agents works without an API key, but `execute_agent` requires `ANTHROPIC_API_KEY`.

**Agent not found:** Check the agent name matches a directory in `AGENTS_DIR`. Names are lowercase with underscores (e.g., `market_sentiment`, not `Market Sentiment`).

**Timeout:** Agent execution can take 10-30 seconds depending on the model. Haiku agents are fastest; Opus agents are slowest.

### Rebuild After Updates

After pulling new code:
```bash
git pull
cargo build --bin agent-mcp-server
```

Restart Zed to pick up the new binary. New agents added to `agents/curated/` are available immediately without rebuilding (the server reads agent cards from disk at runtime).

## FPL Forecasting in Zed

Beyond MCP tools, the Fermi repo includes a Zed extension for the FPL forecasting language:

### Install the Extension

```bash
./install-zed-extension.sh
killall zed && zed
```

This builds the tree-sitter parser, compiles the LSP, and links the extension.

### FPL Features in Zed

- **Syntax highlighting** for keywords, distributions, operators, numbers, strings, comments
- **Real-time diagnostics** — lexical, syntax, and semantic errors
- **Auto-indentation** and bracket matching

### Quick FPL Test

Create a `.fpl` file and type:

```fpl
forecast "Revenue Estimate" {
    driver sales triangular(1000, 2000, 5000)
    driver margin normal(0.3, 0.05)
    estimate sales * margin
}
```

You should see color-coded syntax highlighting and real-time error checking.

### Running Forecasts

```bash
./run-forecast.sh your_forecast.fpl
```

Output includes mean, standard deviation, percentiles, and an ASCII histogram.
