# Quick Start Guide

Get Agent Bestiary integrated with your AI agents in 5 minutes.

## Prerequisites

- An AI agent (LangChain, AutoGPT, CrewAI, or custom)
- HTTP client (curl, Python requests, etc.)
- Agent Bestiary account (sign up at https://agent-bestiary.world)

## Step 1: Get Your API Key

1. Sign up at https://agent-bestiary.world
2. Go to Settings → API Keys
3. Create a new API key
4. Save it securely (shown only once)

```bash
export AGENT_BESTIARY_API_KEY="your-api-key-here"
```

## Step 2: Create an Agent

```bash
curl -X POST https://agent-bestiary.world/api/agents \
  -H "Authorization: Bearer $AGENT_BESTIARY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "my-assistant",
    "agent_type": "personal-assistant",
    "version": "1.0.0"
  }'
```

Response:
```json
{
  "agent_id": "550e8400-e29b-41d4-a716-446655440000",
  "agent_name": "my-assistant",
  "agent_type": "personal-assistant",
  "created_at": "2026-02-07T12:00:00Z",
  "github_url": "https://github.com/YourOrg-Agents/my-assistant"
}
```

Save the `agent_id` - you'll use it for all subsequent requests.

## Step 3: Store Episodes

As your agent interacts with users, store episodes:

```bash
curl -X POST https://agent-bestiary.world/api/agents/{agent_id}/episodes \
  -H "Authorization: Bearer $AGENT_BESTIARY_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "episode": "User asked about our pricing. I explained that Pro is $20/month. User said that seems expensive and asked if there are discounts. I offered 20% off for annual plans.",
    "metadata": {
      "user_id": "user-123",
      "conversation_id": "conv-456",
      "timestamp": "2026-02-07T12:30:00Z"
    }
  }'
```

Response:
```json
{
  "episode_id": "ep-789",
  "stored_at": "2026-02-07T12:30:05Z"
}
```

## Step 4: Trigger Consolidation

After accumulating 20-50 episodes, trigger consolidation:

```bash
curl -X POST https://agent-bestiary.world/api/agents/{agent_id}/consolidate \
  -H "Authorization: Bearer $AGENT_BESTIARY_API_KEY"
```

Response:
```json
{
  "job_id": "job-abc123",
  "status": "processing",
  "estimated_time": "2-5 minutes"
}
```

Check consolidation status:
```bash
curl https://agent-bestiary.world/api/consolidation/jobs/{job_id} \
  -H "Authorization: Bearer $AGENT_BESTIARY_API_KEY"
```

## Step 5: Query Semantic Rules

Once consolidation completes, query the extracted rules:

```bash
curl https://agent-bestiary.world/api/agents/{agent_id}/rules \
  -H "Authorization: Bearer $AGENT_BESTIARY_API_KEY"
```

Response:
```json
{
  "rules": [
    {
      "rule_id": "rule-1",
      "rule": "Users often find $20/mo pricing expensive. Proactively mention annual discount when discussing pricing.",
      "confidence": 0.85,
      "evidence_count": 12,
      "created_at": "2026-02-07T12:35:00Z"
    },
    {
      "rule_id": "rule-2",
      "rule": "When users ask about discounts, annual plans with 20% off are most appealing.",
      "confidence": 0.78,
      "evidence_count": 8,
      "created_at": "2026-02-07T12:35:00Z"
    }
  ]
}
```

## Step 6: Use Rules in Your Agent

Incorporate semantic rules into your agent's prompts:

```python
import requests

# Get semantic rules
response = requests.get(
    f"https://agent-bestiary.world/api/agents/{agent_id}/rules",
    headers={"Authorization": f"Bearer {api_key}"}
)
rules = response.json()["rules"]

# Add to agent prompt
system_prompt = f"""
You are a helpful assistant.

Based on past interactions, here are patterns you've learned:
{chr(10).join([f"- {rule['rule']}" for rule in rules])}

Apply these learnings to provide better responses.
"""
```

## Integration Examples

### LangChain

```python
from langchain.agents import Agent
from langchain.memory import AgentBestiaryMemory

memory = AgentBestiaryMemory(
    agent_id="550e8400-e29b-41d4-a716-446655440000",
    api_key="your-api-key"
)

agent = Agent(
    llm=llm,
    memory=memory,
    tools=tools
)

# Memory automatically stores episodes and applies rules
result = agent.run("What's your pricing?")
```

### AutoGPT

```python
from autogpt.memory import AgentBestiaryProvider

memory = AgentBestiaryProvider(
    agent_id="550e8400-e29b-41d4-a716-446655440000",
    api_key="your-api-key"
)

autogpt = AutoGPT(memory=memory)
autogpt.run()
```

### Custom Python Agent

```python
import requests

class MyAgent:
    def __init__(self, agent_id, api_key):
        self.agent_id = agent_id
        self.api_key = api_key
        self.base_url = "https://agent-bestiary.world/api"
    
    def store_episode(self, episode):
        response = requests.post(
            f"{self.base_url}/agents/{self.agent_id}/episodes",
            headers={"Authorization": f"Bearer {self.api_key}"},
            json={"episode": episode}
        )
        return response.json()
    
    def get_rules(self):
        response = requests.get(
            f"{self.base_url}/agents/{self.agent_id}/rules",
            headers={"Authorization": f"Bearer {self.api_key}"}
        )
        return response.json()["rules"]
    
    def run(self, user_input):
        # Get learned rules
        rules = self.get_rules()
        
        # Build prompt with rules
        prompt = self._build_prompt(user_input, rules)
        
        # Generate response
        response = self._generate_response(prompt)
        
        # Store episode
        episode = f"User: {user_input}\nAgent: {response}"
        self.store_episode(episode)
        
        return response
```

## Best Practices

### When to Store Episodes

✅ **Do store**:
- Complete interactions (user input + agent response)
- Outcomes (success/failure, user satisfaction)
- Context (user preferences, constraints)

❌ **Don't store**:
- PII without user consent (use metadata filtering)
- Every single message (consolidate into episodes)
- Duplicate information

### When to Trigger Consolidation

- **Frequency**: After 20-50 new episodes
- **Timing**: During low-traffic periods (nightly)
- **Manual**: When you notice patterns in episodes

### Consolidation Best Practices

1. **Start with small batches** (20-30 episodes)
2. **Review extracted rules** (check quality)
3. **Adjust frequency** based on rule quality
4. **Use confidence scores** to filter rules

## Common Patterns

### Pattern 1: Store Episode After Each Interaction

```python
def handle_user_message(user_input):
    # Generate response
    response = agent.generate(user_input)
    
    # Store episode
    bestiary.store_episode(
        episode=f"User: {user_input}\nAgent: {response}",
        metadata={"timestamp": datetime.now().isoformat()}
    )
    
    return response
```

### Pattern 2: Nightly Consolidation

```python
import schedule

def consolidate_memory():
    bestiary.consolidate()
    print("Consolidation complete")

# Run every night at 2 AM
schedule.every().day.at("02:00").do(consolidate_memory)
```

### Pattern 3: Query Rules at Startup

```python
def initialize_agent():
    # Load learned rules
    rules = bestiary.get_rules()
    
    # Build system prompt with rules
    system_prompt = build_prompt_with_rules(rules)
    
    # Initialize agent
    return Agent(system_prompt=system_prompt)
```

## Troubleshooting

### Episodes Not Storing

**Problem**: POST request returns 401 Unauthorized  
**Solution**: Check your API key is correct and not expired

**Problem**: Episodes stored but not showing up  
**Solution**: Query with `GET /api/agents/{agent_id}/episodes` to verify

### Consolidation Taking Too Long

**Problem**: Consolidation job stuck in "processing"  
**Solution**: Check job status. Large batches (>100 episodes) take 5-10 minutes

**Problem**: Consolidation fails  
**Solution**: Check episode quality. Ensure episodes have enough context.

### Rules Not Appearing

**Problem**: Consolidation completes but no rules extracted  
**Solution**: Episodes might be too similar or lack patterns. Add more diverse episodes.

**Problem**: Rules are low quality  
**Solution**: Improve episode descriptions. Include outcomes and context.

## Next Steps

- **[API Reference](API.md)** - Complete endpoint documentation
- **[Integrations](INTEGRATIONS.md)** - Framework-specific guides
- **[Architecture](ARCHITECTURE.md)** - How consolidation works
- **[GDPR Guide](GDPR.md)** - Compliance and privacy

## Support

- **Documentation**: https://agent-bestiary.world/docs
- **Discord**: https://discord.gg/agent-bestiary
- **Email**: support@agent-bestiary.world

---

**You're ready!** Start storing episodes and building agents that truly learn from experience.
