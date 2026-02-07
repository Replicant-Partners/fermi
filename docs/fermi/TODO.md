# Fermi TODO

**Last Updated:** 2026-02-04  
**Current Sprint:** Sprint 1 (Module 1 + Module 2)

---

## Immediate (Current Sprint)

### Module 1: FPL LSP
- [ ] Research incremental parsing options (salsa vs rowan) [Module 1]
- [ ] Set up tower-lsp project structure [Module 1]
- [ ] Integrate existing lexer/parser into LSP [Module 1]
- [ ] Implement LSP diagnostics (errors, warnings) [Module 1]
- [ ] Add Fermi coaching as special diagnostic type [Module 1]
- [ ] Test LSP with mock client [Module 1]

### Module 2: Zed Extensions
- [ ] Learn Zed extension API [Module 2]
- [ ] Create tree-sitter grammar for FPL [Module 2]
- [ ] Build fermi-lsp extension skeleton [Module 2]
- [ ] Connect extension to LSP [Module 2]
- [ ] Add execute command (Cmd+R) [Module 2]
- [ ] Create basic results panel [Module 2]

### Documentation
- [ ] Create Module 1 detailed doc [Module 1]
- [ ] Create Module 2 detailed doc [Module 2]
- [ ] Create ADR for incremental parsing choice [Module 1]
- [ ] Create session summary for 2026-02-04 [Meta]

---

## Short Term (Next Sprint - Sprint 2)

### Module 1: FPL LSP (continued)
- [ ] Implement hover information (show driver details) [Module 1]
- [ ] Add autocompletion (driver names, functions) [Module 1]
- [ ] Implement code actions ("Add evidence", "Run forecast") [Module 1]

### Module 2: Zed Extensions (continued)
- [ ] Add inline sparklines (Tufte-style) [Module 2]
- [ ] Improve results panel (basic charts) [Module 2]
- [ ] Add status indicator during execution [Module 2]

---

## Medium Term (Phase 2 - Agent Bestiary)

### Module 5: Backend
- [ ] Research Zed's ACP (Agent Communication Protocol) [Module 5]
- [ ] Choose Rust web framework (axum vs actix-web) [Module 5]
- [ ] Design PostgreSQL schema [Module 5]
- [ ] Build agent registry (ACP-compatible) [Module 5]
- [ ] Implement agent coordinator (async execution) [Module 5]
- [ ] Add authentication system [Module 5]

### Module 3: Agent Bestiary
- [ ] Design yokai avatar system [Module 3]
- [ ] Create agent card UI components [Module 3]
- [ ] Implement handle/drag-drop system [Module 3]
- [ ] Build agent preview cards [Module 3]
- [ ] Add agent configuration UI [Module 3]

---

## Long Term (Future Phases)

### Module 4: Visualization
- [ ] Choose charting library (Plotly vs plotters) [Module 4]
- [ ] Implement histogram visualization [Module 4]
- [ ] Add confidence band charts [Module 4]
- [ ] Create tornado chart (sensitivity analysis) [Module 4]
- [ ] Build calibration plot [Module 4]

### Module 6: Mermaid Viewer
- [ ] Research mermaid rendering in Rust [Module 6]
- [ ] Design agent ontology evolution system [Module 6]
- [ ] Implement ER diagram viewer [Module 6]
- [ ] Add time-travel for ontology versions [Module 6]

### Module 7: Collaboration
- [ ] Design tournament lifecycle [Module 7]
- [ ] Implement leaderboard calculation [Module 7]
- [ ] Build resolution mechanism [Module 7]
- [ ] Add real-time sync (WebSocket) [Module 7]

### Module 9: Navigation
- [ ] Design forecast library UI [Module 9]
- [ ] Implement tag-based filtering [Module 9]
- [ ] Build command palette search [Module 9]
- [ ] Add semantic search capability [Module 9]

### Module 8: Settings
- [ ] Design settings schema [Module 8]
- [ ] Implement Fermi-assisted configuration [Module 8]
- [ ] Build traditional settings UI [Module 8]

### Module 10: Mobile
- [ ] Research mobile frameworks (React Native vs Flutter) [Module 10]
- [ ] Design mobile-specific UI [Module 10]
- [ ] Prototype view-only mode [Module 10]

---

## Blocked (Waiting On)

- None currently

---

## Questions Needing Answers

### Module 1 Questions
- [ ] Salsa vs rowan for incremental parsing? (Need to prototype both)
- [ ] Should Fermi coaching be part of diagnostics or separate LSP extension?
- [ ] Execution local vs remote - when to use backend?
- [ ] How verbose should Fermi coaching be?

### Module 2 Questions
- [ ] Can Zed's inlay hints support sparklines (or need custom decorations)?
- [ ] Best placement for results panel (bottom, right, floating)?
- [ ] Auto-execute on save or explicit command only?

### Module 3 Questions
- [ ] Pre-designed yokai avatars or AI-generated?
- [ ] How many agents in initial bestiary (5, 10, 20)?
- [ ] Agent configuration in-panel or in-code?

### Module 5 Questions
- [ ] How does Zed's ACP registry work exactly?
- [ ] Agent callback mechanism (WebSocket, polling, webhook)?
- [ ] Database schema - what tables needed?
- [ ] Real-time sync strategy (WebSocket everywhere or mixed)?

### Module 4 Questions
- [ ] Charting library choice impacts bundle size?
- [ ] Native Rust (plotters) vs WebView (Plotly)?
- [ ] What charts are most important (priority order)?

---

## Research Needed

### High Priority
- [ ] **Zed Extension API documentation** - What's possible?
- [ ] **tower-lsp examples** - How to build LSP servers
- [ ] **Incremental parsing** - salsa vs rowan performance comparison
- [ ] **Zed's ACP protocol** - How does agent registry work?

### Medium Priority
- [ ] **tree-sitter grammar creation** - Tutorial/examples
- [ ] **WebSocket in Rust** - Best practices for real-time
- [ ] **PostgreSQL schema design** - Forecasting-specific patterns
- [ ] **Mermaid rendering** - Rust libraries available?

### Low Priority
- [ ] **Mobile frameworks** - React Native vs Flutter comparison
- [ ] **Agent ontology representation** - ER diagram best practices
- [ ] **Calibration scoring** - Brier score implementation details

---

## Technical Debt

- None yet (greenfield project)

---

## Ideas / Future Enhancements

- [ ] Voice input for mobile (dictate forecasts)
- [ ] AI-generated agent avatars (custom per agent)
- [ ] Forecast templates (pre-built common scenarios)
- [ ] Export forecasts (PDF, PNG, data)
- [ ] Import historical data (CSV → evidence)
- [ ] Forecast comparison view (A vs B side-by-side)
- [ ] Time-travel debugging (replay forecast execution)
- [ ] Agent marketplace (community-contributed agents)
- [ ] Forecast versioning with visual diff
- [ ] Collaborative editing (Google Docs style)

---

## Completed (Archive)

### Phase 0: FPL Core Engine ✅
- [x] Lexer implementation (900 lines, 13 tests)
- [x] Parser implementation (850 lines, 8 tests)
- [x] Semantic Analyzer (1,020 lines, 12 tests)
- [x] Execution Engine (1,330 lines, 26 tests)
- [x] Documentation (15,000+ lines)

### Documentation Setup ✅
- [x] Create docs/ directory structure
- [x] Write PROJECT_RULES.md
- [x] Write MODULE_ARCHITECTURE.md
- [x] Write ROADMAP.md
- [x] Create TODO.md (this file)
- [x] Set up Git workflow

---

## Notes

- Use `[Module N]` tags to track which module each task belongs to
- Move completed items to "Completed" section at bottom
- Review and update this file at end of each session
- Add new questions to "Questions Needing Answers" as they arise
- Blocked items need clear explanation of blocker

**Next Review:** 2026-02-11 (end of Sprint 1)
