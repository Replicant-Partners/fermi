# 🚀 Quick Test: Fermi MCP Server in Zed

## TL;DR - 3 Steps to Test

### 1. Set Your API Key (30 seconds)
```bash
nano ~/.config/zed/settings.json

# Find this line:
"ANTHROPIC_API_KEY": "REPLACE_WITH_YOUR_KEY",

# Replace with your actual key:
"ANTHROPIC_API_KEY": "sk-ant-api03-XXXXX",

# Save and exit (Ctrl+X, Y, Enter)
```

### 2. Restart Zed (10 seconds)
```bash
pkill zed && zed
```

### 3. Test in Assistant (1 minute)

Open Zed assistant (`Ctrl+Shift+A`) and try:

```
What forecasting agents are available?
```

**Expected Result**: Should list `market_research` and `sentiment_analyzer` agents

---

## Quick Verification Checklist

Before opening Zed, verify:

```bash
# ✅ MCP server binary exists
ls -l /home/ilabra/fermi/target/debug/agent-mcp-server

# ✅ Agents are present
ls /home/ilabra/fermi/agents/curated/

# ✅ Config is valid JSON
cat ~/.config/zed/settings.json | jq . > /dev/null && echo "✅ Valid JSON"

# ✅ MCP config is present
grep "context_servers" ~/.config/zed/settings.json
```

All checks should pass before testing in Zed.

---

## What You Should See

### ✅ Success Indicators:

1. **In Zed Assistant**:
   - Context tools icon (🔌) appears
   - "fermi-agent-bestiary" listed as available server
   - 4 tools shown: `list_agents`, `get_agent`, `execute_agent`, `save_agent`

2. **When asking about agents**:
   - Zed calls `list_agents` tool
   - Returns agent data with stats
   - Shows execution counts and costs

3. **When executing an agent**:
   - Takes 2-5 seconds (real API call)
   - Returns evidence and key findings
   - Shows confidence score and token usage

### ❌ Failure Indicators:

1. **No context tools appear**: MCP server didn't start
   - Check Zed logs: `tail ~/.local/share/zed/logs/*.log`
   - Verify API key is set correctly

2. **Tools appear but don't work**: MCP server started but has issues
   - Check if using Mock Executor (no real results)
   - Verify API key format

3. **"No agents found"**: Directory path wrong
   - Check `AGENTS_DIR` in settings.json
   - Should be `/home/ilabra/fermi/agents/curated`

---

## Quick Test Queries

Copy-paste these into Zed assistant:

### Query 1: List Agents
```
What forecasting agents do I have available?
```
**Should see**: 2 agents with their stats

### Query 2: Agent Details
```
Show me details about the market_research agent
```
**Should see**: Full capabilities, performance metrics, usage stats

### Query 3: Execute Agent
```
Use market_research to investigate: "What are the key AI infrastructure trends for 2026?"
```
**Should see**: 
- Evidence from "Claude API"
- Key findings (3-5 bullet points)
- Confidence score (~0.85)
- Tokens used (~700-900)
- Execution time (~3-5 seconds)

### Query 4: Save Stats
```
Save the market_research agent statistics
```
**Should see**: Confirmation with git commit message

---

## Debugging One-Liners

```bash
# Test MCP server manually
echo '{"jsonrpc":"2.0","method":"tools/list","id":1}' | \
  ANTHROPIC_API_KEY="your-key" \
  /home/ilabra/fermi/target/debug/agent-mcp-server

# Watch Zed logs live
tail -f ~/.local/share/zed/logs/*.log | grep -i "mcp\|fermi\|context"

# Verify MCP server can start
ANTHROPIC_API_KEY="your-key" \
  AGENTS_DIR="/home/ilabra/fermi/agents/curated" \
  /home/ilabra/fermi/target/debug/agent-mcp-server 2>&1 | head -5

# Check what tools are registered
ANTHROPIC_API_KEY="your-key" \
  /home/ilabra/fermi/target/debug/agent-mcp-server 2>&1 | \
  grep -A 5 "Tools:"
```

---

## Success!

If you see agents listed and can execute queries, **congratulations!** 🎉

You now have:
- ✅ MCP server running in Zed
- ✅ Access to forecasting agents from your editor
- ✅ Real-time research capabilities
- ✅ Automatic stat tracking and git commits

## What's Next?

Try advanced queries:
- "Compare execution stats between agents"
- "Use sentiment_analyzer to evaluate public opinion on [topic]"
- "What's the total cost across all agent executions?"
- "Show me the most expensive agent by cost"

The MCP server provides a natural language interface to your Agent Bestiary! 🚀
