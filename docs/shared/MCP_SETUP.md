# Fermi Agent Bestiary MCP Server Setup

This guide shows how to integrate the Fermi Agent Bestiary with Zed editor via Model Context Protocol (MCP).

## Overview

The Fermi Agent Bestiary MCP Server exposes AI-powered forecasting agents as tools that can be used directly from Zed. This allows you to:

- List available forecasting agents
- Get detailed agent information
- Execute research queries through agents
- Save agent statistics with automatic git commits

## Prerequisites

- Rust toolchain installed
- Zed editor (version with MCP support)
- ANTHROPIC_API_KEY environment variable set (for LLM execution)

## Building the MCP Server

```bash
cd /home/ilabra/fermi
cargo build --release --bin agent-mcp-server
```

The binary will be available at: `target/release/agent-mcp-server`

## Configuring Zed

### 1. Locate Zed's MCP Configuration

Zed's MCP servers are configured in one of these locations:
- `~/.config/zed/mcp.json` (Linux)
- `~/Library/Application Support/Zed/mcp.json` (macOS)
- `%APPDATA%\Zed\mcp.json` (Windows)

### 2. Add Fermi Agent Bestiary Server

Create or edit the `mcp.json` file:

```json
{
  "mcpServers": {
    "fermi-agent-bestiary": {
      "command": "/home/ilabra/fermi/target/release/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "your-api-key-here",
        "AGENTS_DIR": "/home/ilabra/fermi/agents/curated"
      }
    }
  }
}
```

**Important Configuration:**
- `command`: Full path to the compiled MCP server binary
- `ANTHROPIC_API_KEY`: Your Anthropic API key for Claude access
- `AGENTS_DIR`: Path to the curated agents directory (optional, defaults to `agents/curated`)

### 3. Restart Zed

After saving the configuration, restart Zed to load the MCP server.

## Available Tools

Once configured, you'll have access to these tools in Zed's assistant:

### 1. `list_agents`
List all available forecasting agents with their capabilities and stats.

**Usage:**
```
Can you list the available forecasting agents?
```

**Returns:**
- Agent IDs and types
- Capabilities and specializations
- Performance metrics (accuracy, confidence)
- Usage statistics (executions, cost)

### 2. `get_agent`
Get detailed information about a specific agent.

**Parameters:**
- `agent_id`: The ID of the agent (e.g., "market_research", "sentiment_analyzer")

**Usage:**
```
Tell me about the market_research agent
```

**Returns:**
- Complete agent capabilities
- Detailed performance metrics
- Usage history and costs
- Configuration (model, temperature, tools)

### 3. `execute_agent`
Execute a forecasting agent with a research query.

**Parameters:**
- `agent_id`: The agent to execute
- `query`: Your research question

**Usage:**
```
Use the market_research agent to find information about AI chip market trends in 2026
```

**Returns:**
- Evidence-based insights
- Key findings
- Confidence scores
- Execution metrics (time, tokens, cost)
- Updated agent statistics

### 4. `save_agent`
Save an agent's updated statistics and commit to git.

**Parameters:**
- `agent_id`: The agent to save

**Usage:**
```
Save the market_research agent stats
```

**Returns:**
- Confirmation message
- Git commit details

## Example Workflow

Here's a typical workflow using the Agent Bestiary from Zed:

1. **Discover agents:**
   ```
   What forecasting agents are available?
   ```

2. **Learn about an agent:**
   ```
   Show me details about the market_research agent
   ```

3. **Execute research:**
   ```
   Use market_research to investigate: "What are the growth projections for cloud computing infrastructure in 2026?"
   ```

4. **Save results:**
   ```
   Save the market_research agent's updated statistics
   ```

## Troubleshooting

### MCP Server Not Appearing in Zed

1. Check Zed's logs for MCP server errors:
   - Linux: `~/.config/zed/logs/`
   - macOS: `~/Library/Logs/Zed/`

2. Verify the binary path is correct:
   ```bash
   ls -l /home/ilabra/fermi/target/release/agent-mcp-server
   ```

3. Test the server manually:
   ```bash
   ANTHROPIC_API_KEY="your-key" ./target/release/agent-mcp-server
   ```
   The server should start and output initialization messages to stderr.

### API Key Issues

If you see "using Mock Executor" in logs:
- Verify `ANTHROPIC_API_KEY` is set in `mcp.json`
- The key should start with `sk-ant-api03-`

### No Agents Loaded

If "No agents found" appears:
- Check the `AGENTS_DIR` path in `mcp.json`
- Verify agent card JSON files exist:
  ```bash
  ls /home/ilabra/fermi/agents/curated/*/agent_card.json
  ```

## Development Mode

For development, you can run the server with debug output:

```bash
RUST_LOG=debug ANTHROPIC_API_KEY="your-key" cargo run --bin agent-mcp-server
```

## Agent Management

### Adding New Agents

1. Create a directory in `agents/curated/`:
   ```bash
   mkdir agents/curated/my_agent
   ```

2. Create an `agent_card.json`:
   ```json
   {
     "agent_id": "my_agent",
     "agent_type": "research",
     "version": "0.1.0",
     "tier": "curated",
     ...
   }
   ```

3. Restart the MCP server (restart Zed)

### Version Control

Agent statistics are automatically committed to git when you use the `save_agent` tool. This creates an audit trail of agent performance over time.

View git history:
```bash
git log --oneline --grep="agent(" agents/curated/
```

## Architecture

```
┌─────────────┐
│  Zed Editor │
└──────┬──────┘
       │ MCP Protocol (stdio)
       │
┌──────▼────────────────────┐
│ Fermi Agent Bestiary      │
│ MCP Server                │
│                           │
│ Tools:                    │
│  - list_agents           │
│  - get_agent             │
│  - execute_agent         │
│  - save_agent            │
└──────┬────────────────────┘
       │
       ├──► Agent Registry
       ├──► LLM Executor (Claude API)
       └──► Git Integration
```

## Security Notes

- API keys are passed via environment variables (never hardcoded)
- MCP server runs as a subprocess of Zed
- All file I/O is restricted to the agents directory
- Git commits use your configured git identity

## Resources

- [Model Context Protocol Specification](https://modelcontextprotocol.io/specification/2025-11-25)
- [rust-mcp-sdk Documentation](https://docs.rs/rust-mcp-sdk)
- [Fermi Agent Bestiary Design](./AGENT_BESTIARY_DESIGN.md)

## Support

If you encounter issues:
1. Check Zed's MCP logs
2. Test the server standalone
3. Verify agent card JSON format
4. Ensure API key is valid

For questions or bug reports, see the project README.
