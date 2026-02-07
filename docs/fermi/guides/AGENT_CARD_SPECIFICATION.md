# Agent Card Specification

**Version:** 1.1  
**Date:** 2026-02-06  
**Status:** Active

## Overview

Agent cards are JSON files that define an agent's metadata, capabilities, performance metrics, and configuration. Each agent in the Fermi Agent Bestiary has its own card stored in `agents/curated/{agent_name}/agent_card.json`.

## Complete Specification

```json
{
  "agent_id": "market_research",
  "agent_type": "research",
  "version": "1.2.0",
  "tier": "curated",
  
  "capabilities": {
    "executor": "llm",
    "model": "claude-sonnet-4",
    "temperature": 0.3,
    "mcp_servers": [
      {
        "name": "yahoo_finance",
        "command": "node",
        "args": ["/path/to/yahoo-finance-server.js"],
        "env": {
          "API_KEY": "${YAHOO_FINANCE_API_KEY}"
        }
      },
      {
        "name": "sec_api",
        "command": "python",
        "args": ["-m", "sec_api_server"],
        "env": {
          "SEC_API_KEY": "${SEC_API_KEY}"
        }
      }
    ],
    "skills": ["data_analysis", "trend_detection"]
  },
  
  "performance": {
    "forecasts_contributed": 47,
    "avg_brier_impact": 0.04,
    "avg_confidence": 0.82,
    "accuracy_rate": 0.89
  },
  
  "usage": {
    "total_executions": 152,
    "successful_executions": 144,
    "failed_executions": 8,
    "total_tokens_used": 2847392,
    "total_cost_usd": 142.37,
    "avg_execution_time_ms": 3420,
    "last_30_days": {
      "executions": 23,
      "tokens": 431045,
      "cost_usd": 21.55
    }
  },
  
  "wallet": {
    "primary": {
      "chain": "ethereum",
      "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb",
      "purpose": "Revenue share / payments"
    },
    "secondary": {
      "chain": "solana",
      "address": "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
      "purpose": "Micropayments / tips"
    },
    "payment_model": {
      "type": "usage_based",
      "rate_per_1k_tokens": 0.05,
      "revenue_share_pct": 0.15,
      "payment_threshold_usd": 10.0
    }
  },
  
  "ontology_stats": {
    "entities": 23,
    "relationships": 18,
    "last_updated": "2026-02-05T12:00:00Z",
    "evolution_commits": 15
  },
  
  "metadata": {
    "created": "2025-12-01",
    "author": "Fermi Team",
    "description": "Researches market trends and competitive dynamics",
    "tags": ["market", "research", "competitive-analysis"]
  }
}
```

## Field Descriptions

### Root Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `agent_id` | string | ✅ | Unique identifier for the agent (must match directory name) |
| `agent_type` | string | ✅ | Agent type: "research", "sentiment", "competitive", etc. |
| `version` | string | ✅ | Semantic version (e.g., "1.2.0") |
| `tier` | string | ✅ | "curated" or "community" |

### Capabilities

Configuration for how the agent executes tasks.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `executor` | string | ✅ | Execution type: "llm", "mcp", "manual", "skill" |
| `model` | string | ✅ | LLM model name (e.g., "claude-sonnet-4", "gpt-4") |
| `temperature` | number | ✅ | Temperature setting (0.0-1.0) |
| `mcp_servers` | array | ❌ | Array of MCP server configurations (see below) |
| `skills` | array | ❌ | Array of skill names this agent can invoke |

#### MCP Server Configuration

Each agent can specify its own MCP servers that it needs to function:

```json
{
  "name": "yahoo_finance",           // Unique name for this MCP server
  "command": "node",                  // Command to execute
  "args": [                           // Command arguments
    "/path/to/yahoo-finance-server.js"
  ],
  "env": {                            // Environment variables
    "API_KEY": "${YAHOO_FINANCE_API_KEY}",
    "TIMEOUT": "30000"
  }
}
```

**Key Points:**
- Each agent can have **multiple MCP servers**
- MCP servers are **agent-specific** (not global)
- Environment variables support `${VAR}` substitution
- Paths can be absolute or relative to agent directory

**Storage:**
- Database: `agents.mcp_servers` (JSONB column)
- Rust type: `Option<serde_json::Value>`
- Agent card: `capabilities.mcp_servers`

### Performance

Historical performance metrics for the agent.

| Field | Type | Description |
|-------|------|-------------|
| `forecasts_contributed` | integer | Number of forecasts this agent has contributed to |
| `avg_brier_impact` | number | Average impact on Brier score (positive = improvement) |
| `avg_confidence` | number | Average confidence score of agent's outputs |
| `accuracy_rate` | number | Percentage of successful executions |

### Usage

Cost and usage tracking.

| Field | Type | Description |
|-------|------|-------------|
| `total_executions` | integer | Total times agent has been executed |
| `successful_executions` | integer | Successful execution count |
| `failed_executions` | integer | Failed execution count |
| `total_tokens_used` | integer | Cumulative token usage |
| `total_cost_usd` | number | Cumulative cost in USD |
| `avg_execution_time_ms` | integer | Average execution time in milliseconds |
| `last_30_days` | object | Rolling 30-day window statistics |

### Wallet (Future)

Cryptocurrency wallet configuration for revenue sharing.

| Field | Type | Description |
|-------|------|-------------|
| `primary` | object | Primary wallet (e.g., Ethereum for revenue share) |
| `secondary` | object | Secondary wallet (e.g., Solana for micropayments) |
| `payment_model` | object | Payment configuration |

### Ontology Stats

Statistics about the agent's learned knowledge.

| Field | Type | Description |
|-------|------|-------------|
| `entities` | integer | Number of entities in agent's ontology |
| `relationships` | integer | Number of relationships |
| `last_updated` | timestamp | When ontology was last updated |
| `evolution_commits` | integer | Number of git commits for ontology evolution |

### Metadata

General information about the agent.

| Field | Type | Description |
|-------|------|-------------|
| `created` | date | When agent was created |
| `author` | string | Agent creator/maintainer |
| `description` | string | Human-readable description |
| `tags` | array | Searchable tags for categorization |

## Database Mapping

The agent card maps to the `agents` table in the database:

```sql
CREATE TABLE agents (
    agent_id UUID PRIMARY KEY,
    agent_name TEXT UNIQUE NOT NULL,
    agent_type TEXT NOT NULL,
    version TEXT NOT NULL,
    tier TEXT NOT NULL,
    
    -- Capabilities
    executor_type TEXT NOT NULL,
    model TEXT NOT NULL,
    temperature FLOAT NOT NULL,
    mcp_servers JSONB,  -- NEW: Agent-specific MCP servers
    
    -- Metadata
    description TEXT,
    author TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    
    -- Ontology
    current_ontology_commit TEXT,
    current_ontology_snapshot_id UUID,
    last_consolidated_at TIMESTAMPTZ,
    
    -- Performance (cached)
    total_executions INTEGER,
    successful_executions INTEGER,
    failed_executions INTEGER,
    total_cost_usd DECIMAL(10, 6),
    avg_execution_time_ms BIGINT
);
```

## Examples

### Research Agent with MCP Tools

```json
{
  "agent_id": "market_research",
  "agent_type": "research",
  "version": "1.2.0",
  "tier": "curated",
  "capabilities": {
    "executor": "llm",
    "model": "claude-sonnet-4",
    "temperature": 0.3,
    "mcp_servers": [
      {
        "name": "yahoo_finance",
        "command": "node",
        "args": ["servers/yahoo-finance/index.js"],
        "env": {
          "API_KEY": "${YAHOO_FINANCE_API_KEY}"
        }
      }
    ]
  },
  "metadata": {
    "author": "Fermi Team",
    "description": "Market research specialist with access to financial APIs"
  }
}
```

### Sentiment Analyzer (No MCP)

```json
{
  "agent_id": "sentiment_analyzer",
  "agent_type": "sentiment",
  "version": "1.0.0",
  "tier": "curated",
  "capabilities": {
    "executor": "llm",
    "model": "claude-haiku-4",
    "temperature": 0.1,
    "mcp_servers": null
  },
  "metadata": {
    "author": "Fermi Team",
    "description": "Analyzes sentiment from text without external tools"
  }
}
```

### MCP-First Agent

```json
{
  "agent_id": "web_scraper",
  "agent_type": "research",
  "version": "1.0.0",
  "tier": "community",
  "capabilities": {
    "executor": "mcp",
    "model": "n/a",
    "temperature": 0.0,
    "mcp_servers": [
      {
        "name": "puppeteer",
        "command": "node",
        "args": ["servers/puppeteer/server.js"],
        "env": {
          "BROWSER_PATH": "/usr/bin/chromium"
        }
      },
      {
        "name": "html_parser",
        "command": "python",
        "args": ["-m", "html_parser_mcp"],
        "env": {}
      }
    ]
  },
  "metadata": {
    "author": "Community",
    "description": "Web scraping agent using Puppeteer and HTML parsing"
  }
}
```

## MCP Server Best Practices

### 1. Use Environment Variables

Don't hardcode secrets in agent cards:

```json
// ❌ Bad
"env": {
  "API_KEY": "sk-1234567890abcdef"
}

// ✅ Good
"env": {
  "API_KEY": "${YAHOO_FINANCE_API_KEY}"
}
```

### 2. Specify Relative Paths

Use paths relative to the agent directory when possible:

```json
"args": ["./servers/custom-mcp.js"]  // Relative to agent directory
```

### 3. Document Required MCP Servers

In the agent's README, document which MCP servers are required:

```markdown
## Required MCP Servers

This agent requires the following MCP servers:

- **yahoo_finance**: Financial data API
  - Installation: `npm install -g yahoo-finance-mcp-server`
  - API Key: Set `YAHOO_FINANCE_API_KEY` environment variable
```

### 4. Test MCP Server Availability

Before executing, verify MCP servers are available and responsive.

### 5. Handle MCP Failures Gracefully

If an MCP server is unavailable, the agent should:
- Log the error clearly
- Fall back to alternative methods if possible
- Return a partial result with reduced confidence

## Version History

### 1.1 (2026-02-06)
- Added `mcp_servers` field to capabilities
- Each agent can now specify its own MCP server configuration
- MCP servers stored in database as JSONB
- Updated examples with MCP configurations

### 1.0 (2025-12-01)
- Initial specification
- Core fields: capabilities, performance, usage, metadata

## Related Documentation

- [Agent Bestiary Design](../AGENT_BESTIARY_DESIGN.md)
- [MCP Integration Guide](./MCP_INTEGRATION.md)
- [Database Schema](../MEMORY_SCHEMA.sql)
- [Agent Development Guide](./AGENT_DEVELOPMENT.md)
