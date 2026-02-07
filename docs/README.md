# Documentation

This repository contains two products:

## 🧠 [Agent Bestiary](agent-bestiary/) - Universal Memory Backend for AI Agents

Active Dreaming Memory backend that consolidates episodic experiences into semantic knowledge. Works with any agent framework (LangChain, AutoGPT, CrewAI, custom).

**Start here if you want to**:
- Add memory to your AI agents
- Integrate with LangChain/AutoGPT/CrewAI
- Build GDPR-compliant agent systems
- Use Agent Bestiary as a service

**Key docs**:
- [Features](agent-bestiary/FEATURES.md)
- [Quick Start](agent-bestiary/QUICK_START.md)
- [API Reference](agent-bestiary/API.md)
- [Architecture](agent-bestiary/ARCHITECTURE.md)

## 🎯 [Fermi](fermi/) - Probabilistic Forecasting Agents

AI agents that make probabilistic forecasts using the Fermi Programming Language (FPL). Built on top of Agent Bestiary for memory and learning.

**Start here if you want to**:
- Build forecasting agents
- Use FPL (Fermi Programming Language)
- Deploy Fermi forecasting systems
- Contribute to Fermi development

**Key docs**:
- [Quick Start](fermi/QUICK_START.md)
- [User Guides](fermi/guides/)
- [Architecture](fermi/architecture/)
- [Roadmap](fermi/ROADMAP.md)

## 📚 [Shared Concepts](shared/)

Documentation relevant to both products:
- [Active Dreaming Memory (ADM) Architecture](shared/ARCHITECTURE_ADM.md)
- [Model Context Protocol (MCP) Setup](shared/MCP_SETUP.md)
- [Vercel Domain Setup](shared/VERCEL_DOMAIN_SETUP.md)

## Quick Navigation

| I want to... | Go to... |
|--------------|----------|
| Add memory to my agents | [Agent Bestiary Quick Start](agent-bestiary/QUICK_START.md) |
| Integrate with LangChain | [Agent Bestiary Integrations](agent-bestiary/INTEGRATIONS.md) |
| Understand GDPR compliance | [Agent Bestiary GDPR Guide](agent-bestiary/GDPR.md) |
| Build forecasting agents | [Fermi Quick Start](fermi/QUICK_START.md) |
| Learn FPL | [FPL Reference](fermi/guides/fpl-reference.md) |
| Deploy to production | [Fermi Deployment](fermi/guides/deployment-guide.md) |
| Understand ADM concepts | [ADM Architecture](shared/ARCHITECTURE_ADM.md) |

## Relationship Between Products

```
┌─────────────────────────────────────────────┐
│          Fermi Forecasting Agents           │
│  (Probabilistic forecasting with FPL)       │
└─────────────────┬───────────────────────────┘
                  │ uses
                  ↓
┌─────────────────────────────────────────────┐
│           Agent Bestiary                    │
│  (Universal memory backend for any agent)   │
└─────────────────────────────────────────────┘
```

**Fermi** is a specific application built on **Agent Bestiary**. You can use Agent Bestiary without Fermi for any type of AI agent.

## Website & Domains

- **Agent Bestiary**: https://agent-bestiary.world
- **Fermi**: https://fermi.systems

## Contributing

- Agent Bestiary contributions: See [Agent Bestiary docs](agent-bestiary/)
- Fermi contributions: See [Fermi development guide](fermi/development/)

## License

[Your license here]
