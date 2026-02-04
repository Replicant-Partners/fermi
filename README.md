# Fermi Forecasting Programming Language

A domain-specific language for probabilistic forecasting with Monte Carlo simulation, designed for use in a Zed IDE environment.

## Project Status: v0.4.0

**Core FPL Engine:** ✅ Complete  
**Zed IDE Integration:** 📋 Planned (Sprint 1)  
**Backend API:** 🚧 In Progress

## Features

- **FPL Language:** Domain-specific language for expressing probabilistic forecasts
- **Distribution Engine:** Triangular, Normal, Lognormal, Uniform, Beta distributions
- **Monte Carlo Executor:** Fast simulation (10K-10M iterations)
- **Expression Evaluator:** Full arithmetic, functions, conditionals
- **Type System:** Static type checking with semantic analysis
- **CLI:** Interactive 4-stage execution flow

## Quick Start

```bash
# Build the project
cargo build --release

# Run an example forecast
cargo run --release examples/amd_forecast.fpl

# Run tests
cargo test
```

## Architecture

Fermi follows a modular architecture with 10 core modules:

1. **FPL Language Server** - LSP implementation for editor integration
2. **Zed Extensions** - Custom Zed IDE panels and features
3. **Agent Bestiary** - Multi-agent research coordination
4. **Visualization** - Tufte-style sparklines and charts
5. **Backend** - Rust-based API (Vercel deployment)
6. **Mermaid Viewer** - Agent ontology visualization
7. **Collaboration** - Real-time multi-user forecasting
8. **Settings** - Agent-assisted configuration
9. **Navigation** - Forecast discovery and organization
10. **Mobile** - iOS/Android clients (future)

See [docs/roadmap/MODULE_ARCHITECTURE.md](docs/roadmap/MODULE_ARCHITECTURE.md) for details.

## Backend API

The Fermi backend is deployed on Vercel and provides:

- **POST /api/execute** - Execute forecasts (≥100K iterations)
- **GET /api/health** - Health check endpoint
- **Future:** Agent coordination, collaboration, tournaments

### Local Development

```bash
# Install Vercel CLI
npm i -g vercel

# Run locally
vercel dev

# Deploy to production
vercel --prod
```

## Documentation

- [Project Rules](docs/PROJECT_RULES.md) - Development workflow and conventions
- [Roadmap](docs/ROADMAP.md) - Implementation plan and phases
- [Architecture Decisions](docs/DECISIONS.md) - ADR index
- [Module Architecture](docs/roadmap/MODULE_ARCHITECTURE.md) - Detailed system design

### Implementation Guides
- [Lexer](LEXER_README.md) - Tokenization
- [Parser](PARSER_README.md) - AST construction
- [Semantic Analyzer](SEMANTIC_ANALYZER_README.md) - Type checking
- [Executor](EXECUTOR_README.md) - Monte Carlo simulation

## Example: FPL Forecast

```fpl
forecast "AMD Q4 2024 Revenue" {
    driver gpu_market triangular(20000, 32000, 50000)
    driver market_share normal(0.15, 0.05)
    driver avg_price triangular(800, 1200, 2000)
    
    estimate gpu_market * market_share * avg_price
}
```

## Contributing

See [docs/PROJECT_RULES.md](docs/PROJECT_RULES.md) for:
- Git workflow and commit conventions
- ADR (Architecture Decision Record) process
- Documentation standards
- Testing requirements

## Roadmap

**Phase 0:** ✅ FPL Core (Complete)  
**Phase 1:** 📋 LSP + Zed Extensions (Weeks 6-8)  
**Phase 2:** 📋 Agent Bestiary (Weeks 9-11)  
**Phase 3:** 📋 Visualization (Weeks 12-13)  
**Phase 4:** 📋 Collaboration (Weeks 14-16)  
**Phase 5:** 📋 Tournaments (Weeks 17-19)  
**Phase 6:** 📋 Polish (Weeks 20-21)

See [docs/ROADMAP.md](docs/ROADMAP.md) for detailed milestones.

## Technology Stack

**Language:** Rust 2021  
**Editor:** Zed IDE  
**Backend:** Vercel (Rust serverless functions)  
**Database:** PostgreSQL (planned)  
**Testing:** 59 comprehensive tests (all passing)

## License

[License TBD]

## Links

- **Repository:** https://github.com/Replicant-Partners/fermi
- **Documentation:** [docs/](docs/)
- **Issues:** https://github.com/Replicant-Partners/fermi/issues

---

**Built by Replicant Partners** | **Status:** Active Development | **Version:** 0.4.0
