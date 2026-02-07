# Agent Bestiary Documentation

**Agent Bestiary** is a universal Active Dreaming Memory (ADM) backend for AI agents. It consolidates episodic experiences into semantic knowledge, enabling agents that truly learn from experience.

## 🚀 Quick Links

- **[Quick Start](QUICK_START.md)** - Integrate in 5 minutes
- **[API Reference](API.md)** - Complete REST API documentation
- **[Features](FEATURES.md)** - Core capabilities and differentiators
- **[Architecture](ARCHITECTURE.md)** - Technical design and implementation
- **[Go-To-Market Plan](go-to-market/)** - Launch strategy (for maintainers)

## What is Agent Bestiary?

Agent Bestiary provides two-stage memory for AI agents, inspired by human memory consolidation:

1. **Episodic Memory**: Detailed storage of what happened (PostgreSQL + pgvector)
2. **Semantic Memory**: Extracted patterns and rules (via consolidation)

Unlike vector databases that only store and retrieve, Agent Bestiary **consolidates** experiences into higher-level understanding.

## Key Features

✅ **Real Learning** - Episodic → semantic consolidation, not just storage  
✅ **GDPR Compliant** - Per-agent git repositories enable complete data deletion  
✅ **GitHub as Source of Truth** - Transparent, auditable learning  
✅ **Framework Agnostic** - Works with LangChain, AutoGPT, CrewAI, custom agents  
✅ **Production Ready** - PostgreSQL + pgvector, Vercel serverless deployment  
✅ **Multi-Provider Embeddings** - Anthropic, OpenAI, Mistral, Qwen support  

## Example: Before and After

**Before Consolidation** (episodic memory only):
```
Episode 1: "User asked about pricing, I quoted $20/mo"
Episode 2: "User said that's too expensive"
Episode 3: "User asked about discounts"
```

**After Consolidation** (semantic rules extracted):
```
Rule: "Users often find $20/mo expensive. Suggest annual discount first."
Rule: "When discussing pricing, proactively mention discount options."
```

Now the agent can apply these learned patterns to future interactions.

## Architecture Overview

```
┌─────────────────────────────────────────────────────┐
│                  Your Agent                         │
│            (LangChain, AutoGPT, etc.)               │
└────────────────────┬────────────────────────────────┘
                     │ REST API
                     ↓
┌─────────────────────────────────────────────────────┐
│              Agent Bestiary API                     │
│         (Vercel Serverless Functions)               │
└───────┬──────────────────────────┬──────────────────┘
        │                          │
        ↓                          ↓
┌──────────────────┐      ┌───────────────────────┐
│  PostgreSQL      │      │  GitHub Repos         │
│  + pgvector      │      │  (Per-agent)          │
│                  │      │                       │
│ • Episodes       │      │ • Ontologies          │
│ • Embeddings     │      │ • Semantic Rules      │
│ • Metadata       │      │ • Git History         │
└──────────────────┘      └───────────────────────┘
         ↑                          ↑
         │                          │
         └──────────┬───────────────┘
                    │
         ┌──────────────────────┐
         │  Consolidation       │
         │  (LLM-based)         │
         └──────────────────────┘
```

## Use Cases

### 1. AI Agent Frameworks
Add memory to LangChain, AutoGPT, or CrewAI agents:
```python
from langchain import Agent
from agent_bestiary import Memory

agent = Agent(memory=Memory(agent_id="my-agent"))
agent.run("Help me plan a trip")
```

### 2. Customer Support Bots
Agents that learn from support interactions:
- Remember customer preferences
- Extract common issue patterns
- Improve responses over time

### 3. Personal AI Assistants
Long-term memory for personal agents:
- Learn user preferences
- Remember past conversations
- Build context over months/years

### 4. Research Agents
Agents that accumulate knowledge:
- Store research findings
- Extract patterns from papers
- Build knowledge graphs

## GDPR Compliance

Agent Bestiary is **GDPR-compliant by design**:

| GDPR Right | How Agent Bestiary Supports It |
|------------|-------------------------------|
| Right to Access | Grant user read access to their agent's git repository |
| Right to Erasure | Delete agent's git repository = complete data deletion |
| Right to Portability | User can clone git repository in standard format |
| Right to Rectification | User can submit pull requests to correct data |
| Data Minimization | Per-agent isolation prevents cross-contamination |
| Consent Management | Agent creation requires opt-in, deletion = consent withdrawal |

See [GDPR Guide](GDPR.md) for detailed compliance information.

## Getting Started

### 1. Create an Agent
```bash
curl -X POST https://agent-bestiary.world/api/agents \
  -H "Content-Type: application/json" \
  -d '{
    "agent_name": "my-assistant",
    "agent_type": "personal-assistant"
  }'
```

### 2. Store Episodes
```bash
curl -X POST https://agent-bestiary.world/api/agents/{agent_id}/episodes \
  -H "Content-Type: application/json" \
  -d '{
    "episode": "User asked about pricing and found $20/mo too expensive"
  }'
```

### 3. Trigger Consolidation
```bash
curl -X POST https://agent-bestiary.world/api/agents/{agent_id}/consolidate
```

### 4. Query Semantic Rules
```bash
curl https://agent-bestiary.world/api/agents/{agent_id}/rules
```

See [Quick Start](QUICK_START.md) for detailed integration guide.

## Integrations

- **[LangChain](INTEGRATIONS.md#langchain)** - Official memory integration
- **[AutoGPT](INTEGRATIONS.md#autogpt)** - Memory provider plugin
- **[CrewAI](INTEGRATIONS.md#crewai)** - Multi-agent memory support
- **[Custom Agents](INTEGRATIONS.md#custom)** - REST API for any framework

## Documentation

### Getting Started
- [Quick Start](QUICK_START.md) - 5-minute integration guide
- [Integrations](INTEGRATIONS.md) - Framework-specific guides
- [API Reference](API.md) - Complete endpoint documentation

### Core Concepts
- [Features](FEATURES.md) - Core capabilities and benefits
- [Architecture](ARCHITECTURE.md) - Technical design
- [GDPR Compliance](GDPR.md) - Privacy and compliance guide

### Deployment
- [Self-Hosting](DEPLOYMENT.md#self-hosting) - Docker, local setup
- [Vercel Deployment](DEPLOYMENT.md#vercel) - Serverless deployment
- [Production Guide](DEPLOYMENT.md#production) - Scaling and monitoring

### Advanced
- [Consolidation Prompts](ARCHITECTURE.md#consolidation) - Customizing consolidation
- [Multi-Agent Memory](ARCHITECTURE.md#multi-agent) - Shared memory spaces
- [Knowledge Graphs](ARCHITECTURE.md#ontology) - Ontology visualization

## Pricing

**Free Tier**:
- 1 agent
- 100 episodes/month
- Community support

**Pro ($20/month)**:
- Unlimited agents
- Unlimited episodes
- Priority support
- Advanced features

**Enterprise (Custom)**:
- Self-hosted option
- SSO/SAML
- SLA guarantees
- Dedicated support

See https://agent-bestiary.world/pricing for details.

## Community

- **Discord**: [Join our community](https://discord.gg/agent-bestiary)
- **GitHub**: [Report issues](https://github.com/agent-bestiary/agent-bestiary/issues)
- **Twitter**: [@AgentBestiary](https://twitter.com/AgentBestiary)
- **Email**: support@agent-bestiary.world

## Support

- **Documentation**: https://agent-bestiary.world/docs
- **API Status**: https://status.agent-bestiary.world
- **Email Support**: support@agent-bestiary.world

## Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[Your license here]

---

**Next Steps**: Start with the [Quick Start Guide](QUICK_START.md) to integrate Agent Bestiary in 5 minutes.
