# Testing Fermi Agent Bestiary MCP Server in Zed

## ✅ Configuration Complete

The MCP server has been added to Zed's configuration at:
- **Config File**: `~/.config/zed/settings.json`
- **Backup**: `~/.config/zed/settings.json.mcp-backup`

## Configuration Details

```json
"context_servers": {
  "fermi-agent-bestiary": {
    "command": "/home/ilabra/fermi/target/debug/agent-mcp-server",
    "args": [],
    "env": {
      "ANTHROPIC_API_KEY": "REPLACE_WITH_YOUR_KEY",
      "AGENTS_DIR": "/home/ilabra/fermi/agents/curated"
    }
  }
}
```

## ⚠️ Before Testing - Set Your API Key

**IMPORTANT**: You need to replace `REPLACE_WITH_YOUR_KEY` with your actual Anthropic API key!

### Option 1: Edit settings.json directly
```bash
# Open in your editor
nano ~/.config/zed/settings.json

# Replace this line:
"ANTHROPIC_API_KEY": "REPLACE_WITH_YOUR_KEY",

# With your actual key:
"ANTHROPIC_API_KEY": "sk-ant-api03-YOUR_ACTUAL_KEY_HERE",
```

### Option 2: Use environment variable (if Zed supports it)
```bash
export ANTHROPIC_API_KEY="sk-ant-api03-YOUR_ACTUAL_KEY_HERE"
```

## How to Test in Zed

### Step 1: Restart Zed
After updating the configuration, completely quit and restart Zed:
```bash
# Kill any running Zed processes
pkill -9 zed

# Start Zed
zed
```

### Step 2: Open the Assistant
- Press `Ctrl+Shift+A` (or `Cmd+Shift+A` on Mac) to open the AI assistant
- Or use the command palette: `Ctrl+Shift+P` → "assistant: toggle focus"

### Step 3: Check MCP Server Status
In the assistant panel, you should see:
- A "🔌" or context icon indicating MCP servers are available
- The server name "fermi-agent-bestiary" should appear in available tools

### Step 4: Test the Tools

The MCP server provides 4 tools:

#### 1. **list_agents** - List all available agents
Try asking:
```
What forecasting agents are available?
```

Expected: You should see a list with `market_research` and `sentiment_analyzer`

#### 2. **get_agent** - Get detailed agent info
Try asking:
```
Tell me about the market_research agent
```

Expected: Detailed information about capabilities, performance, and usage stats

#### 3. **execute_agent** - Run a research query
Try asking:
```
Use the market_research agent to research: "What are the top AI trends in 2026?"
```

Expected: 
- Evidence-based insights
- Key findings
- Confidence scores
- Token usage and cost

#### 4. **save_agent** - Save stats and commit to git
Try asking:
```
Save the market_research agent statistics
```

Expected: Confirmation that stats were saved and committed to git

## Troubleshooting

### MCP Server Not Appearing

1. **Check Zed logs**:
   ```bash
   tail -f ~/.local/share/zed/logs/*.log
   ```

2. **Verify binary exists**:
   ```bash
   ls -l /home/ilabra/fermi/target/debug/agent-mcp-server
   ```

3. **Test server manually**:
   ```bash
   ANTHROPIC_API_KEY="your-key" \
   /home/ilabra/fermi/target/debug/agent-mcp-server
   ```
   
   Should output:
   ```
   ✓ Using LLM Executor (Claude API)
   ✓ Loaded 2 agent(s) from agents/curated
   🚀 Fermi Agent Bestiary MCP Server started
   ```

4. **Check JSON syntax**:
   ```bash
   cat ~/.config/zed/settings.json | jq .
   ```
   Should not have any JSON errors.

### "Invalid API Key" Error

- Double-check your API key in `~/.config/zed/settings.json`
- Make sure it starts with `sk-ant-api03-`
- No quotes or spaces around the key

### "No agents found" Warning

- Verify agents directory exists:
  ```bash
  ls /home/ilabra/fermi/agents/curated/
  ```
  
- Should see: `market_research/` and `sentiment_analyzer/`

### Tools Not Working

1. **Try rebuilding the MCP server**:
   ```bash
   cd /home/ilabra/fermi
   cargo build --bin agent-mcp-server
   ```

2. **Check if server is using Mock Executor**:
   - If API key is wrong, it falls back to mock mode
   - Mock mode won't give real results

## Expected Behavior

### Successful Connection
When everything works, you'll see:

1. **In Zed Assistant**: Tools appear with 🔧 icon
2. **On first use**: Server starts and loads agents
3. **Agent execution**: Real Claude API calls with evidence
4. **Stats tracking**: Each execution updates agent stats
5. **Git commits**: Save operations create git commits

### Example Session

```
You: What forecasting agents do I have?