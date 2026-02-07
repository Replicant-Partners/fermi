# Fermi: Agent Bestiary & FPL Engine

A two-service system for managing AI forecasting agents with a beautiful web interface and probabilistic forecasting language.

## Project Status: v0.6.0

**Agent Bestiary (Web):** ✅ Live at [agent-bestiary.world](https://agent-bestiary-production.up.railway.app)  
**MCP Server (Zed):** ✅ Complete  
**Core FPL Engine:** ✅ Complete  
**Active Dreaming Memory:** 🚧 Phase 1 In Progress

---

## 🦁 Agent Bestiary

A beautiful, modern web interface for cataloguing and managing AI forecasting agents.

**Live Site:** https://agent-bestiary-production.up.railway.app

### Features

- **Beautiful UI** - Gruvbox Dark theme with sleek, modern design
- **Agent Catalogue** - Browse all documented specimens
- **Detailed Views** - Complete agent information including:
  - Performance statistics (executions, accuracy, confidence)
  - Configuration (model, executor, temperature)
  - Knowledge graphs (ontology stats with visualization link)
  - Economic ledger (wallet, costs, budget)
  - MCP tools integration
  - Credentials registry
- **Avatar Generation** - Hasui Kawase-style portraits via Gemini AI
- **Avatar Caching** - Generated once, cached forever
- **Responsive Design** - Works on all screen sizes

### Tech Stack

- **Backend:** Rust + Axum
- **Database:** PostgreSQL (Neon)
- **AI:** Gemini 2.5 Flash Image for avatars
- **Deployment:** Railway
- **Templates:** Pure HTML/CSS/JS (no framework bloat)

---

## 🔌 MCP Server (Zed Integration)

Model Context Protocol server for accessing agents directly from Zed editor.

### Available Tools

- `list_agents` - List all forecasting agents
- `get_agent` - Get detailed agent information
- `execute_agent` - Run research queries
- `save_agent` - Save stats and commit to git

### Setup

1. Build the MCP server:
```bash
cargo build --bin agent-mcp-server
```

2. Add to `~/.config/zed/settings.json`:
```json
{
  "context_servers": {
    "fermi-agent-bestiary": {
      "command": "/home/your-username/fermi/target/debug/agent-mcp-server",
      "args": [],
      "env": {
        "ANTHROPIC_API_KEY": "your_key_here",
        "AGENTS_DIR": "/home/your-username/fermi/agents/curated"
      }
    }
  }
}
```

3. Use in Zed:
```
"List available agents"
"Execute market_research with query: What's the AI chip market outlook?"
```

---

## 📊 FPL Language (Forecasting Programming Language)

Domain-specific language for probabilistic forecasting with Monte Carlo simulation.

### Features

- **Probabilistic Distributions:** Triangular, Normal, Lognormal, Uniform, Beta
- **Monte Carlo Simulation:** 10K-10M iterations
- **Expression Evaluation:** Full arithmetic, functions, conditionals
- **Type System:** Static type checking with semantic analysis
- **CLI:** Interactive 4-stage execution flow

### Example

```fpl
forecast "AMD Q4 2024 Revenue" {
    # Define probabilistic drivers
    driver gpu_market triangular(20000, 32000, 50000)
    driver market_share normal(0.15, 0.05)
    driver avg_price triangular(800, 1200, 2000)
    
    # Agent-assisted research
    agent market_research {
        type: "research"
        query: "AMD datacenter GPU market share trends"
        executor: "llm"
        driver_refs: ["market_share"]
    }
    
    # Final estimate
    estimate gpu_market * market_share * avg_price
}
```

### Run FPL

```bash
cargo build --release
cargo run --release examples/amd_forecast.fpl
```

---

## 🧠 Active Dreaming Memory (ADM)

Biologically-inspired memory system for agents (in progress).

### Features

- **Episodic Memory** - Raw agent execution traces
- **Semantic Memory** - Verified, consolidated rules
- **Knowledge Graphs** - Entities, facts, relationships
- **Ontology Snapshots** - Version-controlled worldviews (Mermaid ER)
- **Vector Search** - pgvector-powered similarity matching
- **Bi-temporal Tracking** - Full history of what agents knew when
- **Race-safe Consolidation** - Distributed locking prevents conflicts

**Status:** Phase 0 complete, Phase 1 in progress  
**Documentation:** See [README_ADM.md](README_ADM.md) and [docs/ARCHITECTURE_ADM.md](docs/ARCHITECTURE_ADM.md)

---

## 🏗️ Architecture

### Two-Service System

```
┌─────────────────────────────────────────────────┐
│                                                 │
│  1. Agent Bestiary Web Service (Railway)       │
│     - Web UI for browsing agents                │
│     - REST API endpoints                        │
│     - Avatar generation & caching               │
│     - PostgreSQL database                       │
│     → https://agent-bestiary.world              │
│                                                 │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│                                                 │
│  2. MCP Server (Local/Zed)                      │
│     - Model Context Protocol server             │
│     - Direct access from Zed editor             │
│     - Loads agents from filesystem              │
│     - Execute research queries                  │
│     → Runs locally via Zed                      │
│                                                 │
└─────────────────────────────────────────────────┘
```

### Project Structure

```
fermi/
├── src/
│   ├── lib.rs                  # FPL core library
│   ├── main.rs                 # FPL CLI
│   ├── api_server.rs           # Web service (Railway)
│   └── bin/
│       ├── agent-mcp-server.rs # MCP server (Zed)
│       └── agent-web-ui.rs     # Alternative web UI
├── templates/
│   ├── index.html              # Agent catalogue
│   └── agent_detail.html       # Agent detail view
├── agents/curated/
│   ├── market_research/
│   │   └── agent_card.json
│   └── sentiment_analyzer/
│       └── agent_card.json
├── avatars_cache/              # Cached avatar images
├── agent-bestiary/
│   ├── memory/                 # ADM memory system
│   ├── ontology/               # Knowledge graphs
│   └── consolidate/            # Memory consolidation
├── scripts/
│   └── update_namecom_dns.sh   # DNS helper
├── Dockerfile                  # Railway deployment
└── docs/
    ├── SESSION_NOTES.md        # Latest session notes
    ├── ARCHITECTURE_ADM.md     # Memory system design
    └── ROADMAP_ADM_*.md        # Implementation roadmap
```

---

## 🚀 Quick Start

### 1. Run FPL Forecasts (Local)

```bash
# Build the project
cargo build --release

# Run an example forecast
cargo run --release examples/amd_forecast.fpl

# Run tests
cargo test
```

### 2. Use Agent Bestiary (Web)

Visit: **https://agent-bestiary-production.up.railway.app**

Or run locally:
```bash
# Set environment variables
export DATABASE_URL="your_postgresql_url"
export GEMINI_API_KEY="your_gemini_key"

# Run the server
cargo run --bin api-server

# Visit http://localhost:3000
```

### 3. Use MCP Server in Zed

1. Build: `cargo build --bin agent-mcp-server`
2. Configure Zed (see MCP Server section above)
3. Use in Zed assistant: "List available agents"

---

## 🎨 Agent Bestiary Design

### Color Palette (Gruvbox Dark)
- Background: `#1d2021` → `#282828` → `#3c3836`
- Text: `#ebdbb2` (primary), `#d5c4a1` (secondary)
- Accent: `#fabd2f` (yellow)
- Success: `#b8bb26` (green)
- Error: `#fb4934` (red)

### Design Principles
- **Left-justified** - Modern, efficient layout
- **Compressed** - Tight spacing, more content visible
- **Sleek borders** - Subtle 1px borders, no heavy shadows
- **Grid-based** - Responsive, efficient use of space
- **Typography** - Inter, SF fonts for clean readability

---

## 🗂️ Current Agents

### market_research
- **Type:** research
- **Model:** Claude 3 Haiku
- **Description:** Researches market trends, competitive dynamics, and market sizing for forecasts
- **Tags:** market, research, competitive-analysis
- **Stats:** 2 executions, $0.0004 cost

### sentiment_analyzer
- **Type:** sentiment
- **Model:** Claude 3 Haiku
- **Description:** Analyzes sentiment from social media, news, and forums to gauge market perception
- **Tags:** sentiment, social-media, public-opinion
- **Stats:** 1 execution, $0.0001 cost

---

## 🔧 Development

### Prerequisites
- Rust 2021+ (`rustc --version`)
- PostgreSQL client (for database features)
- Zed IDE (optional, for MCP integration)

### Environment Setup

1. Copy `.env.example` to `.env`
2. Add credentials:
```bash
DATABASE_URL=postgresql://...
ANTHROPIC_API_KEY=sk-ant-...
GEMINI_API_KEY=AIza...
```

3. Build:
```bash
cargo build --workspace
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Test FPL core
cargo test --package fermi

# Test memory system
cargo test --package agent-bestiary-memory

# Test specific module
cargo test semantic_analysis
```

---

## 🚢 Deployment

### Railway (Web Service)

The web service deploys automatically from the `main` branch:

```bash
# Deploy manually
railway up --detach

# Check logs
railway logs --tail 50

# Check status
railway status
```

**Configuration:**
- `Dockerfile` - Multi-stage Rust build
- Copies: src, templates, agents, creates avatars_cache
- Environment variables set in Railway dashboard

### MCP Server (Local)

Runs locally on your machine via Zed:
```bash
cargo build --bin agent-mcp-server
# Configure in ~/.config/zed/settings.json
```

---

## 📚 API Documentation

### REST API Endpoints

**Base URL:** `https://agent-bestiary-production.up.railway.app`

#### `GET /api/health`
Health check endpoint.

```json
{
  "status": "ok",
  "service": "Agent Bestiary",
  "description": "A naturalist's catalogue of dreaming agents",
  "version": "1.0.0"
}
```

#### `GET /api/agents`
List all agents.

```json
{
  "agents": [...],
  "total": 2
}
```

#### `GET /api/agents/:id/avatar`
Get agent avatar (cached).

```json
{
  "agent_id": "market_research",
  "image": {
    "mime_type": "image/png",
    "data": "base64_encoded_image_data"
  }
}
```

#### `GET /agent/:id`
View agent detail page (HTML).

---

## 🎯 Roadmap

### Short Term
- [ ] Add more curated agents
- [ ] Implement ontology graph visualization
- [ ] Add agent wallet management UI
- [ ] Create agent creation/editing interface

### Medium Term
- [ ] Agent execution dashboard
- [ ] Real-time performance monitoring
- [ ] Agent tournament system
- [ ] Search and filtering in catalogue

### Long Term
- [ ] Inter-agent knowledge sharing (AKP protocol)
- [ ] Multi-agent collaboration workflows
- [ ] Agent marketplace
- [ ] Advanced analytics and forecasting

See [docs/ROADMAP_ADM_IMPLEMENTATION.md](docs/ROADMAP_ADM_IMPLEMENTATION.md) for memory system roadmap.

---

## 📖 Documentation

### Getting Started
- [Session Notes](docs/SESSION_NOTES.md) - Latest development session
- [ADM Overview](README_ADM.md) - Memory system quick reference
- [Architecture](docs/ARCHITECTURE_ADM.md) - Complete system design

### Development
- [Lexer](LEXER_README.md) - FPL tokenization
- [Parser](PARSER_README.md) - AST construction
- [Semantic Analyzer](SEMANTIC_ANALYZER_README.md) - Type checking
- [Executor](EXECUTOR_README.md) - Monte Carlo simulation

### Deployment
- [DNS Setup](scripts/update_namecom_dns.sh) - name.com DNS helper
- [Railway Config](Dockerfile) - Docker deployment configuration

---

## 🤝 Contributing

See [docs/PROJECT_RULES.md](docs/PROJECT_RULES.md) for:
- Git workflow and commit conventions
- ADR (Architecture Decision Record) process
- Documentation standards
- Testing requirements

---

## 🔗 Links

- **Live Site:** https://agent-bestiary-production.up.railway.app
- **Custom Domain:** https://agent-bestiary.world
- **GitHub:** https://github.com/Replicant-Partners/fermi
- **Railway:** agent-bestiary (production environment)
- **Issues:** https://github.com/Replicant-Partners/fermi/issues

---

## 📄 License

[License TBD]

---

**Built by Replicant Partners**  
**Version:** 0.6.0  
**Status:** Active Development  
**Latest:** Sleek modern Agent Bestiary with MCP integration 🦁✨

*Make your agents dreams come true.*
