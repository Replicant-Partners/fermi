# Architectural Decisions

**Last Updated:** 2026-02-04

Quick reference index of all Architecture Decision Records (ADRs).

---

## Index (Chronological)

1. [ADR-001: Architecture Option C - Loose Coupling](decisions/001_architecture_option_c.md) - 2026-02-04 ✅
2. [ADR-002: Rust Backend Rebuild](decisions/002_rust_backend_rebuild.md) - 2026-02-04 ✅
3. [ADR-003: Hybrid Fermi Coaching Integration](decisions/003_hybrid_fermi_coaching.md) - 2026-02-04 ✅
4. [ADR-004: Adaptive Coaching Verbosity](decisions/004_adaptive_coaching_verbosity.md) - 2026-02-04 ✅
5. [ADR-005: Hybrid Execution Model with 100K Threshold](decisions/005_hybrid_execution_threshold.md) - 2026-02-04 ✅
6. [ADR-006: Tree-sitter Grammar Generation via rust-sitter](decisions/006_rust_sitter_grammar_generation.md) - 2026-02-04 ✅
7. [ADR-007: Comprehensive Sparkline Types](decisions/007_comprehensive_sparkline_types.md) - 2026-02-04 ✅
8. [ADR-008: Multi-Method Execute Command UX](decisions/008_multi_method_execute_command.md) - 2026-02-04 ✅
9. [ADR-009: Right Sidebar Results Panel](decisions/009_right_sidebar_results_panel.md) - 2026-02-04 ✅

---

## By Category

### Overall Architecture
- **ADR-001:** Architecture Option C - Clean separation (LSP, Backend, Extensions)
- **ADR-005:** Hybrid Execution Model - Local <100K iterations, backend ≥100K

### Backend
- **ADR-002:** Rebuild backend in Rust (from Node.js)

### Language Server (Module 1)
- **ADR-003:** Hybrid Fermi Coaching - Standard LSP diagnostics + custom extension
- **ADR-004:** Adaptive Coaching Verbosity - Aggressive → moderate → personalized
- **ADR-005:** Hybrid Execution Model with 100K threshold

### Zed Extensions (Module 2)
- **ADR-006:** Tree-sitter grammar via rust-sitter tool
- **ADR-007:** Comprehensive Sparklines - Distribution + historical + confidence
- **ADR-008:** Multi-Method Execute - Keyboard + palette + auto-execute
- **ADR-009:** Right Sidebar Results Panel

### Agents
- *No ADRs yet* (ACP integration details pending)

### Collaboration
- *No ADRs yet* (versioning strategy pending)

---

## By Status

### ✅ Accepted
- ADR-001: Architecture Option C
- ADR-002: Rust Backend Rebuild
- ADR-003: Hybrid Fermi Coaching Integration
- ADR-004: Adaptive Coaching Verbosity
- ADR-005: Hybrid Execution Model with 100K Threshold
- ADR-006: Tree-sitter Grammar via rust-sitter
- ADR-007: Comprehensive Sparkline Types
- ADR-008: Multi-Method Execute Command UX
- ADR-009: Right Sidebar Results Panel

### ⏳ Proposed
- *None yet*

### 🚫 Superseded
- *None yet*

### ❌ Rejected
- *None yet*

---

## By Impact

### Critical (Affects entire system)
- ADR-001: Architecture Option C
- ADR-005: Hybrid Execution Model

### High (Affects multiple modules)
- ADR-002: Rust Backend Rebuild
- ADR-003: Hybrid Fermi Coaching Integration
- ADR-006: Tree-sitter Grammar Generation

### Medium (Affects single module)
- ADR-004: Adaptive Coaching Verbosity
- ADR-007: Comprehensive Sparkline Types
- ADR-008: Multi-Method Execute Command
- ADR-009: Right Sidebar Results Panel

### Low (Implementation detail)
- *None yet*

---

## Recent Decisions (Last 7 Days)

- 2026-02-04: ADR-001 (Architecture Option C) - **Accepted**
- 2026-02-04: ADR-002 (Rust Backend Rebuild) - **Accepted**
- 2026-02-04: ADR-003 (Hybrid Fermi Coaching) - **Accepted**
- 2026-02-04: ADR-004 (Adaptive Coaching Verbosity) - **Accepted**
- 2026-02-04: ADR-005 (Hybrid Execution Model) - **Accepted**
- 2026-02-04: ADR-006 (Tree-sitter Grammar) - **Accepted**
- 2026-02-04: ADR-007 (Comprehensive Sparklines) - **Accepted**
- 2026-02-04: ADR-008 (Multi-Method Execute) - **Accepted**
- 2026-02-04: ADR-009 (Right Sidebar Results) - **Accepted**

---

## Pending Decisions

### High Priority (Sprint 1-2)
- [ ] Incremental parsing strategy (salsa vs rowan) [Module 1, Q1.1]
- [ ] Web framework choice (axum vs actix-web) [Module 5, Q5.1]
- [ ] Zed ACP integration architecture [Module 5, Q5.2]

### Medium Priority (Sprint 2-4)
- [ ] Charting library choice (Plotly vs plotters vs native) [Module 4, Q4.1]
- [ ] Agent callback mechanism (WebSocket vs polling) [Module 5, Q5.4]
- [ ] Yokai avatar system (pre-designed vs AI-generated) [Module 3, Q3.1]
- [ ] Manual review workflow design [Module 5, Q5.5]

### Low Priority (Sprint 5+)
- [ ] Settings access model (agent-only vs traditional UI) [Module 8, Q8.1]
- [ ] Forecast versioning (Git-like vs snapshots) [Module 7, Q7.5]
- [ ] Mobile platform (React Native vs Flutter) [Module 10, Q10.2]
- [ ] Mermaid rendering (native vs WebView) [Module 6, Q6.1]

---

## How to Create an ADR

When making a significant architectural decision:

1. **Copy the template** from `decisions/000_TEMPLATE.md`
2. **Number it sequentially** (next available number)
3. **Fill in all sections:**
   - Context (what's the issue?)
   - Decision (what did we decide?)
   - Consequences (trade-offs?)
   - Alternatives (what else was considered?)
4. **Save as** `decisions/XXX_descriptive_name.md`
5. **Update this index** (DECISIONS.md)
6. **Commit both files** together
7. **Reference in code** if relevant

### What Deserves an ADR?

**Create ADR for:**
- Framework/library choices
- Architectural patterns
- Data flow designs
- API contracts
- Security decisions
- Performance trade-offs
- User-facing behavior changes

**Don't create ADR for:**
- Variable naming
- Code formatting
- Minor refactoring
- Bug fixes
- Documentation updates

---

## ADR Template

See `decisions/000_TEMPLATE.md` for the standard template.

**Quick template:**
```markdown
# ADR-XXX: [Title]

**Date:** YYYY-MM-DD
**Status:** Proposed | Accepted | Superseded | Rejected
**Deciders:** [Who]

## Context
[What's the issue?]

## Decision
[What did we decide?]

## Consequences
[Trade-offs?]

## Alternatives Considered
[What else? Why not?]
```

---

## Related Documents

- [PROJECT_RULES.md](PROJECT_RULES.md) - How we work
- [ROADMAP.md](ROADMAP.md) - Project timeline
- [MODULE_ARCHITECTURE.md](roadmap/MODULE_ARCHITECTURE.md) - System design
- [TODO.md](TODO.md) - Pending work

---

**Next Review:** After each sprint (every 2 weeks)
