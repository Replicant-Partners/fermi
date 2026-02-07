# Documentation Partition Plan

## Problem

Current documentation mixes:
- **Agent Bestiary** (universal memory backend for any agent)
- **Fermi** (forecasting agents that use Agent Bestiary)

This creates confusion for:
- Agent Bestiary users (don't care about Fermi forecasting)
- Fermi users (need both, but unclear which is which)

## Solution

Create clear documentation boundaries in the monorepo.

## Proposed Structure

```
docs/
├── agent-bestiary/           # Agent Bestiary (standalone product)
│   ├── README.md             # "Agent Bestiary Documentation"
│   ├── FEATURES.md           # Core features (MOVED from AGENT_BESTIARY_FEATURES.md)
│   ├── ARCHITECTURE.md       # Memory architecture (MOVED from AGENT_BESTIARY_DESIGN.md)
│   ├── API.md                # REST API spec (MOVED from API_SPECIFICATION.md)
│   ├── MEMORY_SCHEMA.sql     # Database schema (MOVED)
│   ├── QUICK_START.md        # How to use Agent Bestiary
│   ├── INTEGRATIONS.md       # LangChain, AutoGPT, CrewAI integrations
│   ├── GDPR.md               # GDPR compliance guide
│   ├── DEPLOYMENT.md         # Self-hosting, Vercel deployment
│   └── go-to-market/         # GTM plan (MOVED)
│
├── fermi/                    # Fermi forecasting system
│   ├── README.md             # "Fermi Documentation" (MOVED from docs/README.md)
│   ├── QUICK_START.md        # Fermi quick start (MOVED)
│   ├── ROADMAP.md            # Fermi roadmap (MOVED)
│   ├── guides/               # Fermi-specific guides (MOVED)
│   ├── architecture/         # Fermi architecture (MOVED)
│   ├── api/                  # Fermi API docs (MOVED)
│   ├── sessions/             # Development sessions (MOVED)
│   └── decisions/            # ADRs (MOVED)
│
├── shared/                   # Shared between both
│   ├── ARCHITECTURE_ADM.md   # ADM conceptual overview (both use)
│   ├── MCP_SETUP.md          # MCP integration (both use)
│   └── VERCEL_DOMAIN_SETUP.md # Infrastructure (both use)
│
└── README.md                 # Top-level: explains the structure
```

## What Goes Where?

### Agent Bestiary Docs (docs/agent-bestiary/)

**Core product docs**:
- AGENT_BESTIARY_FEATURES.md → FEATURES.md
- AGENT_BESTIARY_DESIGN.md → ARCHITECTURE.md
- API_SPECIFICATION.md → API.md
- MEMORY_SCHEMA.sql → MEMORY_SCHEMA.sql
- go-to-market/ → go-to-market/

**New docs to create**:
- README.md (entry point for Agent Bestiary users)
- QUICK_START.md (integrate Agent Bestiary in 5 minutes)
- INTEGRATIONS.md (LangChain, AutoGPT, CrewAI examples)
- GDPR.md (detailed compliance guide)
- DEPLOYMENT.md (self-hosting, cloud deployment)

**Audience**: AI agent developers, framework maintainers, anyone building agents

**Should NOT mention**: Fermi forecasting agents, FPL, specific Fermi features

### Fermi Docs (docs/fermi/)

**Everything Fermi-specific**:
- README.md (current docs/README.md)
- QUICK_START.md (current docs/QUICK_START.md)
- ROADMAP.md, TODO.md
- guides/ (all Fermi guides)
- architecture/ (Fermi-specific architecture)
- api/ (Fermi API docs)
- sessions/, decisions/, reports/

**New content**:
- How Fermi uses Agent Bestiary
- Fermi-specific memory usage patterns
- Forecasting agent examples

**Audience**: Fermi users, probabilistic forecasters, researchers

**Should mention**: "Fermi uses Agent Bestiary for memory" but link to agent-bestiary docs

### Shared Docs (docs/shared/)

**Cross-cutting concerns**:
- ARCHITECTURE_ADM.md (conceptual foundation for both)
- MCP_SETUP.md (both can use MCP)
- VERCEL_DOMAIN_SETUP.md (infrastructure for both)

## Migration Steps

### Step 1: Create New Structure (5 min)
```bash
mkdir -p docs/agent-bestiary/go-to-market
mkdir -p docs/fermi
mkdir -p docs/shared
```

### Step 2: Move Agent Bestiary Docs (10 min)
```bash
# Core docs
mv docs/AGENT_BESTIARY_FEATURES.md docs/agent-bestiary/FEATURES.md
mv docs/AGENT_BESTIARY_DESIGN.md docs/agent-bestiary/ARCHITECTURE.md
mv docs/API_SPECIFICATION.md docs/agent-bestiary/API.md
mv docs/MEMORY_SCHEMA.sql docs/agent-bestiary/MEMORY_SCHEMA.sql

# GTM plan
mv docs/go-to-market/* docs/agent-bestiary/go-to-market/
rmdir docs/go-to-market
```

### Step 3: Move Fermi Docs (10 min)
```bash
# Main docs
mv docs/README.md docs/fermi/README.md
mv docs/QUICK_START.md docs/fermi/QUICK_START.md
mv docs/ROADMAP.md docs/fermi/ROADMAP.md
mv docs/TODO.md docs/fermi/TODO.md

# Directories
mv docs/guides docs/fermi/
mv docs/architecture docs/fermi/
mv docs/api docs/fermi/
mv docs/sessions docs/fermi/
mv docs/decisions docs/fermi/
mv docs/reports docs/fermi/
mv docs/roadmap docs/fermi/
mv docs/development docs/fermi/
```

### Step 4: Move Shared Docs (5 min)
```bash
mv docs/ARCHITECTURE_ADM.md docs/shared/
mv docs/MCP_SETUP.md docs/shared/
mv docs/VERCEL_DOMAIN_SETUP.md docs/shared/
```

### Step 5: Create New READMEs (20 min)
- docs/README.md (top-level overview)
- docs/agent-bestiary/README.md (Agent Bestiary entry point)
- docs/agent-bestiary/QUICK_START.md (integration guide)

### Step 6: Update Links (10 min)
- Update internal links in moved files
- Update main README.md to point to new structure

## Timeline

**Total time**: ~60 minutes

**Priority**: High (blocking Agent Bestiary launch)

**When**: Before continuing API implementation

## Benefits

1. **Clear product boundaries** - Agent Bestiary can be used standalone
2. **Better for users** - Fermi users only see Fermi docs, Agent Bestiary users only see theirs
3. **Easier to split repos later** - If we decide to separate Agent Bestiary into its own repo
4. **Better for marketing** - agent-bestiary.world can have its own docs site
5. **Cleaner navigation** - No mixing of concerns

## Risks

**Risk**: Broken links  
**Mitigation**: Update all internal links, test thoroughly

**Risk**: Duplicated content  
**Mitigation**: Use shared/ for truly shared concepts

**Risk**: Confusion during transition  
**Mitigation**: Clear top-level README explaining structure

## Next Steps

1. Execute migration (Steps 1-6 above)
2. Create Agent Bestiary README.md
3. Create Agent Bestiary QUICK_START.md
4. Update top-level README.md
5. Test all links
6. Commit: "docs: partition Agent Bestiary and Fermi documentation"

## Future: Separate Repos?

This structure makes it easy to split later:

**Option 1: Monorepo (current)**
```
fermi/
├── docs/agent-bestiary/  # Agent Bestiary docs
├── docs/fermi/           # Fermi docs
├── fermi-memory/         # Shared memory crates
└── ...
```

**Option 2: Separate repos**
```
agent-bestiary/           # New repo
├── docs/                 # From docs/agent-bestiary/
├── api/                  # REST API
├── memory/               # From fermi-memory/
└── ontology/             # From fermi-ontology/

fermi/                    # Original repo
├── docs/                 # From docs/fermi/
├── agents/               # Forecasting agents
└── ...                   # Uses agent-bestiary as dependency
```

For now: **Stay monorepo**, clean partition makes future split easy.
