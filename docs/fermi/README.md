# Fermi Documentation

Welcome to the Fermi project documentation! This directory contains all technical documentation, guides, and architecture information for the Fermi probabilistic forecasting platform.

---

## 📚 Documentation Structure

### 🚀 [Guides](guides/) - Getting Started & User Documentation
Start here if you're new to Fermi or want to learn how to use specific features.

- **[Getting Started](guides/GETTING_STARTED.md)** - First steps with Fermi
- **[Quickstart](guides/quickstart.md)** - Fast introduction
- **[FPL Reference](guides/fpl-reference.md)** - Complete FPL language reference
- **[Forecasting Guide](guides/forecasting-guide.md)** - How to run forecasts
- **[ADM Guide](guides/adm-guide.md)** - Active Dreaming Memory system
- **[MCP Guide](guides/mcp-guide.md)** - Model Context Protocol integration
- **[Zed Integration](guides/zed-integration.md)** - Using Fermi with Zed IDE
- **[Deployment Guide](guides/deployment-guide.md)** - Deployment instructions

### 🏗️ [Architecture](architecture/) - System Design & Technical Architecture
Deep dive into how Fermi is built and why design decisions were made.

- **[System Overview](architecture/overview.md)** - High-level architecture
- **[Domain Model](architecture/domain-model.md)** - Core concepts and types
- **[FPL Grammar](architecture/fpl-grammar.md)** - Language grammar specification
- **[Codebase Structure](architecture/codebase-structure.md)** - Code organization
- **[Whitepaper](architecture/whitepaper.md)** - Agent-assisted architecture paper
- **[ADM Architecture](ARCHITECTURE_ADM.md)** - Active Dreaming Memory design
- **[Database Schema](MEMORY_SCHEMA.sql)** - Complete PostgreSQL schema

### 🔌 [API](api/) - API Documentation
Documentation for REST APIs, MCP server, LSP server, and agent interfaces.

- **[Execute Command](api/execute-command.md)** - Command execution API
- **[Agent Cards](api/agent-cards.md)** - Agent definition format
- **[MCP Server](MCP_SETUP.md)** - Model Context Protocol server

### 💻 [Development](development/) - Developer Guides
Information for contributors and developers working on Fermi.

- **[Lexer Implementation](development/lexer-implementation.md)** - Lexer internals
- **[Parser Implementation](development/parser-implementation.md)** - Parser design
- **[Executor Implementation](development/executor-implementation.md)** - Execution engine
- **[Semantic Analyzer](development/semantic-analyzer.md)** - Type checking
- **[LSP Features](development/lsp-features.md)** - Language server features

### 📋 [Decisions](decisions/) - Architecture Decision Records
ADRs documenting important technical decisions and their rationale.

- [000 - ADR Template](decisions/000_TEMPLATE.md)
- [001 - Architecture Option C](decisions/001_architecture_option_c.md)
- [002 - Rust Backend Rebuild](decisions/002_rust_backend_rebuild.md)
- [003 - Hybrid Fermi Coaching](decisions/003_hybrid_fermi_coaching.md)
- [More ADRs...](decisions/)

### 📊 [Reports](reports/) - Status Reports & Audits
Current project status, audits, and comprehensive analyses.

- **[State of the Project](reports/STATE_OF_THE_PROJECT_2026_02_06.md)** - Complete project status
- **[System Audit](COMPREHENSIVE_SYSTEM_AUDIT_2026_02_06.md)** - Full system audit
- **[Code Review](CODE_REVIEW_2026_02_06.md)** - Code quality analysis
- **[Session Notes](SESSION_NOTES_2026_02_06.md)** - Development session context
- **[Phase 0 Complete](SESSION_COMPLETE_ADM_PHASE_0.md)** - ADM Phase 0 summary
- **[Phase 1 Complete](SESSION_COMPLETE_ADM_PHASE_1.md)** - ADM Phase 1 summary
- **[Phase 2 Complete](SESSION_COMPLETE_ADM_PHASE_2.md)** - ADM Phase 2 summary
- **[Phase 3 Complete](SESSION_COMPLETE_ADM_PHASE_3.md)** - ADM Phase 3 summary
- **[Phase 4 Complete](SESSION_COMPLETE_ADM_PHASE_4.md)** - ADM Phase 4 summary
- **[Phases 2-4 Summary](SESSION_SUMMARY_ADM_PHASES_2_3_4.md)** - Combined summary

### 📅 [Roadmap](roadmap/) - Planning & Future Work
Development roadmap and feature planning.

- **[ADM Roadmap](ROADMAP_ADM_IMPLEMENTATION.md)** - Active Dreaming Memory roadmap
- **[Project Roadmap](ROADMAP.md)** - Overall project roadmap
- **[Module Architecture](roadmap/MODULE_ARCHITECTURE.md)** - Module planning

### 📝 [Sessions](sessions/) - Development Session Logs
Detailed logs of development sessions for historical reference.

- [Session 2026-02-04](sessions/SESSION_2026-02-04.md)
- [Session 2026-02-05](sessions/SESSION_2026-02-05.md)
- [More sessions...](sessions/)

### 📦 [Archive](archive/) - Historical Documentation
Older documentation for completed features and historical context.

- Completed feature summaries
- Migration logs
- Deprecated documentation

---

## 🔍 Quick Navigation

### By Role

**👤 New User?**
1. Start with [Getting Started](guides/GETTING_STARTED.md)
2. Read [Quickstart](guides/quickstart.md)
3. Try the [Forecasting Guide](guides/forecasting-guide.md)

**🔧 Developer?**
1. Review [System Overview](architecture/overview.md)
2. Check [Codebase Structure](architecture/codebase-structure.md)
3. Read relevant [Development Guides](development/)

**🏢 Decision Maker?**
1. Read [State of the Project](reports/STATE_OF_THE_PROJECT_2026_02_06.md)
2. Review [System Audit](COMPREHENSIVE_SYSTEM_AUDIT_2026_02_06.md)
3. Check [ADM Architecture](ARCHITECTURE_ADM.md)

**🤖 AI/Agent Developer?**
1. Start with [ADM Guide](guides/adm-guide.md)
2. Review [ADM Architecture](ARCHITECTURE_ADM.md)
3. Check [Agent Cards](api/agent-cards.md)

### By Topic

**📊 Forecasting:**
- [FPL Reference](guides/fpl-reference.md)
- [Forecasting Guide](guides/forecasting-guide.md)
- [FPL Grammar](architecture/fpl-grammar.md)

**🧠 Memory System:**
- [ADM Guide](guides/adm-guide.md)
- [ADM Architecture](ARCHITECTURE_ADM.md)
- [Database Schema](MEMORY_SCHEMA.sql)
- [Phase Completion Docs](reports/)

**🤖 Agents:**
- [Agent Cards](api/agent-cards.md)
- [ADM Guide](guides/adm-guide.md)
- [Agent Bestiary Design](AGENT_BESTIARY_DESIGN.md)

**🔌 Integration:**
- [MCP Guide](guides/mcp-guide.md)
- [Zed Integration](guides/zed-integration.md)
- [LSP Features](development/lsp-features.md)

---

## 📖 Key Documents

### Essential Reading
1. **[README.md](../README.md)** - Main project README
2. **[State of the Project](reports/STATE_OF_THE_PROJECT_2026_02_06.md)** - Current status
3. **[STATUS.md](../STATUS.md)** - Quick status overview
4. **[System Audit](COMPREHENSIVE_SYSTEM_AUDIT_2026_02_06.md)** - Complete audit

### Architecture
- [System Overview](architecture/overview.md)
- [ADM Architecture](ARCHITECTURE_ADM.md)
- [Database Schema](MEMORY_SCHEMA.sql)

### Implementation
- [ADM Phases 0-4](reports/) - Complete implementation journey
- [Code Review](CODE_REVIEW_2026_02_06.md)
- [Session Notes](SESSION_NOTES_2026_02_06.md)

---

## 🎯 Current Focus

**Active Development**: Phase 5 - LLM Integration  
**Completed**: Phases 0-4 of Active Dreaming Memory  
**Next Up**: Ontology Snapshots (Phase 6)

See [Roadmap](roadmap/ROADMAP_ADM_IMPLEMENTATION.md) for details.

---

## 📊 Documentation Stats

- **Total Docs**: 108+ files
- **Guides**: 8 user-facing docs
- **Architecture**: 7 design docs
- **API Docs**: 3 specifications
- **Development**: 5 implementation guides
- **ADRs**: 11 decision records
- **Session Logs**: 19 detailed logs
- **Reports**: 8 status/audit reports
- **Archived**: 30+ historical docs

---

## 🤝 Contributing to Documentation

Found an issue or want to improve the docs?

1. Check [Contributing Guide](development/contributing.md) (if exists)
2. Follow the documentation structure above
3. Keep docs concise and actionable
4. Add links for cross-references
5. Update this index when adding new docs

---

## 📞 Getting Help

- **Issues**: Check existing documentation first
- **Questions**: Review [Quick Start](QUICK_START.md)
- **Bugs**: See project issue tracker
- **Ideas**: Create an ADR in [decisions/](decisions/)

---

## 📜 License

See [LICENSE](../LICENSE) file in project root (if exists).

---

**Last Updated**: 2026-02-06  
**Documentation Version**: 1.0  
**Project Version**: 0.5.0

*This documentation is maintained alongside the Fermi project. For the latest updates, see the project repository.*
