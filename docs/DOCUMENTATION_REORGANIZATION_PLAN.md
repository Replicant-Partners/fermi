# Documentation Reorganization Plan

**Date**: 2026-02-06  
**Current State**: 57 markdown files in root, 52 in docs/  
**Goal**: Organized, maintainable documentation structure

---

## Current Problems

1. **Root Clutter**: 57 markdown files in project root
2. **Duplication**: Multiple files covering similar topics
3. **Naming Inconsistency**: Various naming patterns
4. **Outdated Content**: Session logs mixed with current docs
5. **Poor Discoverability**: Hard to find specific information

---

## Proposed Structure

```
fermi/
├── README.md                     # Main project README (keep)
├── QUICKSTART.md                 # Quick start guide (keep)
├── CHANGELOG.md                  # Version history (new)
├── Cargo.toml                    # Workspace config (keep)
│
└── docs/
    ├── README.md                 # Documentation index (new)
    │
    ├── guides/                   # User-facing guides
    │   ├── getting-started.md
    │   ├── fpl-language-guide.md
    │   ├── agent-guide.md
    │   ├── adm-guide.md
    │   └── deployment-guide.md
    │
    ├── architecture/             # System architecture
    │   ├── overview.md
    │   ├── fpl-engine.md
    │   ├── agent-system.md
    │   ├── adm-system.md
    │   ├── database-schema.md
    │   └── MEMORY_SCHEMA.sql
    │
    ├── api/                      # API documentation
    │   ├── rest-api.md
    │   ├── mcp-server.md
    │   ├── lsp-server.md
    │   └── agent-cards.md
    │
    ├── development/              # Developer docs
    │   ├── contributing.md
    │   ├── code-review-guide.md
    │   ├── testing-guide.md
    │   └── release-process.md
    │
    ├── decisions/                # ADRs (keep existing)
    │   ├── 000_TEMPLATE.md
    │   ├── 001_architecture_option_c.md
    │   └── ...
    │
    ├── sessions/                 # Session logs (keep existing)
    │   ├── SESSION_2026-02-04.md
    │   └── ...
    │
    ├── archive/                  # Historical docs (new)
    │   ├── old-sessions/
    │   ├── deprecated-features/
    │   └── migration-logs/
    │
    ├── reports/                  # Status reports (new)
    │   ├── STATE_OF_THE_PROJECT_2026_02_06.md
    │   ├── COMPREHENSIVE_SYSTEM_AUDIT_2026_02_06.md
    │   ├── CODE_REVIEW_2026_02_06.md
    │   └── SESSION_NOTES_2026_02_06.md
    │
    └── roadmap/                  # Planning docs (keep existing)
        ├── ROADMAP_ADM_IMPLEMENTATION.md
        └── MODULE_ARCHITECTURE.md
```

---

## File Categorization

### Keep in Root (5 files)
- `README.md` - Main project overview
- `QUICKSTART.md` - Quick start (consolidate from others)
- `CHANGELOG.md` - Version history (to be created)
- `LICENSE` - License file (if exists)
- `Cargo.toml` - Workspace configuration

### Move to docs/guides/ (User-facing)
- `GETTING_STARTED.md` → `docs/guides/getting-started.md`
- `README_ADM.md` → `docs/guides/adm-guide.md`
- `README_MCP.md` → `docs/guides/mcp-guide.md`
- `QUICK_REFERENCE.md` → `docs/guides/fpl-reference.md`
- `RUNNING_FORECASTS.md` → `docs/guides/forecasting-guide.md`
- `ZED_QUICK_TEST.md` → `docs/guides/zed-integration.md`

### Move to docs/architecture/ (System design)
- `FERMI_BROCA_ARCHITECTURE.md` → `docs/architecture/overview.md`
- `DOMAIN_MODEL.md` → `docs/architecture/domain-model.md`
- `DSL_GRAMMAR.md` → `docs/architecture/fpl-grammar.md`
- `CODEBASE_ANALYSIS.md` → `docs/architecture/codebase-structure.md`
- `COMPONENT_DEPENDENCIES.md` (already in docs) - keep

### Move to docs/api/ (API docs)
- `EXECUTE_COMMAND.md` → `docs/api/execute-command.md`
- `AGENT_TEMPLATES.md` → `docs/api/agent-cards.md`
- `MCP_SETUP.md` (already in docs) → `docs/api/mcp-server.md`

### Move to docs/development/ (Developer guides)
- `LEXER_README.md` → `docs/development/lexer-implementation.md`
- `PARSER_README.md` → `docs/development/parser-implementation.md`
- `EXECUTOR_README.md` → `docs/development/executor-implementation.md`
- `SEMANTIC_ANALYZER_README.md` → `docs/development/semantic-analyzer.md`
- `AUTOCOMPLETE_FEATURES.md` → `docs/development/lsp-features.md`

### Move to docs/archive/ (Historical/completed)
- `PARSER_COMPLETE.md`
- `EXECUTOR_COMPLETE.md`
- `SEMANTIC_COMPLETE.md`
- `GRAMMAR_FIX_SUMMARY.md`
- `LSP_REFACTORING_COMPLETE.md`
- `SESSION_2026-02-04_EXTENSION_COMPLETE.md`
- `SESSION_2026-02-05_FINAL_SUMMARY.md`
- `BEFORE_AND_AFTER.md`
- `AUTOCOMPLETE_IMPROVEMENTS.md`
- `CODE_ACTIONS.md`
- `SYNTAX_HIGHLIGHTING_DEBUG.md`
- `SYNTAX_HIGHLIGHTING_FIX.md`
- `TEMPLATE_UPDATE_SUMMARY.md`
- `WEB_UI_STATUS.md`
- `WEB_UI_SUCCESS.md`
- `ZED_MCP_TESTING.md`
- `READY_TO_TEST_IN_ZED.md`
- `MARKDOWN_RENDERER.md`
- `DISCRETE_DRIVERS.md`
- `NATURAL_LANGUAGE_DRIVERS.md`
- `DISPLAY_PANEL_DESIGN.md`
- `REPORT_SYSTEM_DESIGN.md`
- `RESTORE_v0.5.0.md`
- `IMPLEMENTATION_STATUS.md`
- `CURRENT_STATUS.md` (superseded by STATUS.md)

### Move to docs/reports/ (Current reports)
- `STATE_OF_THE_PROJECT_2026_02_06.md` (already created)
- `COMPREHENSIVE_SYSTEM_AUDIT_2026_02_06.md` (in docs)
- `CODE_REVIEW_2026_02_06.md` (in docs)
- `SESSION_NOTES_2026_02_06.md` (in docs)
- `SESSION_COMPLETE_ADM_PHASE_*.md` (in docs)
- `SESSION_SUMMARY_ADM_PHASES_2_3_4.md` (in docs)

### Consolidate/Remove
- `STATUS.md` → Update and keep in root
- `QUICKSTART.md` → Consolidate with GETTING_STARTED
- `SESSION_STATE_VERCEL_DEPLOYMENT.md` → Archive or remove
- `SESSION_SUMMARY_MCP_SETUP.md` → Archive
- `DEPLOYMENT.md` → Move to docs/guides/deployment-guide.md
- `fpl-agent-assisted-architecture-whitepaper.md` → docs/architecture/

---

## Implementation Steps

### Phase 1: Create Structure (Done ✅)
```bash
mkdir -p docs/{guides,architecture,api,development,archive,reports}
```

### Phase 2: Move Files
```bash
# Move user guides
git mv GETTING_STARTED.md docs/guides/getting-started.md
git mv README_ADM.md docs/guides/adm-guide.md
git mv README_MCP.md docs/guides/mcp-guide.md
# ... (continue for all files)

# Move historical docs
mkdir docs/archive/old-sessions
git mv SESSION_2026-02-04_EXTENSION_COMPLETE.md docs/archive/
# ... (continue for all archived files)
```

### Phase 3: Create Index
- Create docs/README.md with navigation
- Link to all major sections
- Add search tips

### Phase 4: Update Links
- Search for internal links in all docs
- Update paths to new locations
- Test all links

### Phase 5: Clean Root
- Only keep essential files in root
- Update main README to point to docs/

---

## New Files to Create

### docs/README.md (Documentation Index)
- Overview of documentation structure
- Links to all major sections
- Quick navigation guide

### docs/guides/getting-started.md (Consolidated)
- Combine GETTING_STARTED.md and QUICKSTART.md
- Single entry point for new users

### docs/guides/fpl-reference.md
- Comprehensive FPL language reference
- Examples and best practices

### docs/development/contributing.md
- How to contribute
- Code style guide
- Testing requirements

### CHANGELOG.md (Root)
- Version history
- Release notes
- Breaking changes

---

## Benefits

1. **Discoverability**: Clear structure makes finding docs easy
2. **Maintainability**: Related docs grouped together
3. **Professionalism**: Clean, organized project
4. **Onboarding**: New developers can navigate easily
5. **Focus**: Root directory is clean and purposeful

---

## Timeline

- **Phase 1**: Structure creation (5 min) ✅
- **Phase 2**: Move files (30 min)
- **Phase 3**: Create index (20 min)
- **Phase 4**: Update links (30 min)
- **Phase 5**: Final cleanup (15 min)

**Total Estimated Time**: ~2 hours

---

## Post-Reorganization Checklist

- [ ] All 57 root files categorized
- [ ] Files moved to appropriate directories
- [ ] docs/README.md created with navigation
- [ ] Internal links updated
- [ ] Main README.md updated
- [ ] STATUS.md updated
- [ ] Git commit with clear message
- [ ] Verify no broken links

---

**Status**: Plan created, ready for execution  
**Next**: Execute Phase 2 (move files)
