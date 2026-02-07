# Fermi Project Rules & Context Management

**Last Updated:** 2026-02-04  
**Version:** 1.0

---

## Core Principles

1. **Modular Architecture** - Clean separation of concerns, loose coupling
2. **Avoid Monolithic Complexity** - Factor out components cleanly
3. **Documentation First** - All decisions captured in docs/ before implementation
4. **Context Preservation** - Never lose architectural decisions or discussions
5. **Git as Source of Truth** - All important artifacts version controlled
6. **🚨 DSL STABILITY IS CRITICAL** - Changes to FPL syntax are **HIGHLY SENSITIVE** and **ILL-ADVISED**

---

## 🚨 DSL STABILITY RULE - READ THIS FIRST

**FPL (Forecasting Programming Language) syntax changes are HIGHLY SENSITIVE and ILL-ADVISED.**

### Why DSL Stability Matters

Changing the DSL syntax has cascading effects across:
1. **Parser** (`src/parser.rs`, `src/lexer.rs`, `src/ast.rs`) - Core language implementation
2. **Tree-sitter Grammar** (`extensions/fermi/grammars/fpl/grammar.js`) - Syntax highlighting
3. **LSP Server** (`fermi-lsp/`) - Autocompletion, hover, diagnostics
4. **Templates** (`templates/*.fpl`) - All example files
5. **Tests** (59+ test files) - Language test suite
6. **Documentation** - All examples and guides
7. **User Code** - Every `.fpl` file users have written

### Before Changing FPL Syntax

**STOP and ask:**
1. Is this change absolutely necessary?
2. Can we solve this problem WITHOUT changing syntax?
3. Have we exhausted all alternatives?
4. What is the migration path for existing users?
5. Is this worth breaking everyone's code?

### If You MUST Change Syntax

Follow this process:

1. **Create ADR** - Document why change is necessary
2. **Design Migration** - How will old code be converted?
3. **Version the Language** - FPL v1 vs FPL v2
4. **Update Everything:**
   - [ ] Lexer (`src/lexer.rs`)
   - [ ] Parser (`src/parser.rs`)
   - [ ] AST (`src/ast.rs`)
   - [ ] Semantic analyzer (`src/semantic.rs`)
   - [ ] Tree-sitter grammar (`extensions/fermi/grammars/fpl/grammar.js`)
   - [ ] LSP completions (`fermi-lsp/src/main.rs`)
   - [ ] All templates (`templates/*.fpl`)
   - [ ] All tests (`src/*/tests/`)
   - [ ] Examples (`examples/*.fpl`)
   - [ ] Documentation
5. **Test Everything** - All 59+ tests must pass
6. **Communicate Change** - Announce in changelog, update docs

### Current FPL Syntax (v0.4.0)

**This is the stable syntax - DO NOT CHANGE without following the process above:**

```fpl
# Comment (also supports // and /* */)
question "What is your forecast question?"

driver name continuous {
    distribution: triangular(min, likely, max)
    unit: "description"
    rationale: "why this matters"
}

driver binary_driver binary {
    probability: 0.65p
}

evidence source_name {
    source: "citation"
    summary: "key findings"
    relevance: 0.9p
    date: 2026-01-15
}

agent agent_name {
    query: "what to research"
    schedule: every 1 week
}

model: mathematical_expression

simulate 10000 iterations
```

### Syntax Change History

- **2026-02-05**: Fixed grammar mismatch - standardized on `question`, driver blocks, `model:`, `simulate`
- **Before**: Had inconsistent syntax between parser, grammar, and templates
- **Lesson Learned**: This is why DSL stability matters!

---

## Documentation Structure

```
/home/ilabra/fermi/docs/
├── roadmap/              # High-level planning
│   ├── ROADMAP.md        # Master plan
│   ├── MODULE_ARCHITECTURE.md  # Module interaction design
│   └── SPRINT_PLAN.md    # Sprint-by-sprint breakdown
├── modules/              # Per-module deep dives
│   ├── 01_FPL_LSP.md
│   ├── 02_ZED_EXTENSIONS.md
│   ├── 03_AGENT_BESTIARY.md
│   └── ... (one file per module)
├── decisions/            # Architecture Decision Records (ADRs)
│   ├── 001_architecture_option_c.md
│   ├── 002_rust_backend_rebuild.md
│   └── ... (numbered sequentially)
├── discussions/          # Key conversation transcripts
│   └── conversation_YYYY-MM-DD_topic.md
├── sessions/             # Session summaries
│   └── SESSION_YYYY-MM-DD.md
├── TODO.md               # Living list of open questions
├── DECISIONS.md          # Quick reference of all ADRs
└── PROJECT_RULES.md      # This file
```

---

## Context Management Tips

### Between Sessions

**At Start of Session:**
1. Read `TODO.md` - What's pending?
2. Read relevant module docs (e.g., `modules/01_FPL_LSP.md`)
3. Check `DECISIONS.md` - What's already decided?

**During Session:**
- Update module docs with new information
- Create ADR for any architectural decision
- Add to `TODO.md` if questions arise

**At End of Session:**
1. Create `sessions/SESSION_YYYY-MM-DD.md` summary
2. Update `TODO.md` with next steps
3. Commit all changes to Git
4. Update `DECISIONS.md` if new ADRs added

### Avoiding Context Loss

**All Important Decisions:**
- Must be captured in `docs/` (not just chat)
- Use Markdown cross-references to link related docs
- Git commit messages reference documentation

**Example Workflow:**
```bash
# Work on Module 1 (FPL LSP)
vim docs/modules/01_FPL_LSP.md

# Make architectural decision
vim docs/decisions/003_incremental_parsing_strategy.md

# Update master decision log
vim docs/DECISIONS.md

# Commit with context
git add docs/
git commit -m "Decided on salsa for incremental parsing - see decisions/003"
```

---

## Module Discussion Rules

Each module has its own documentation file in `docs/modules/`.

### Module Document Structure

```markdown
# Module N: [Name]

## Overview
- Purpose
- Dependencies
- Status

## Open Questions
- Question 1?
- Question 2?

## Decisions Made
- [Date] Decision 1 (see ADR-XXX)
- [Date] Decision 2

## Implementation Notes
- Technical details
- Code locations
- Testing strategy

## Next Steps
- [ ] Task 1
- [ ] Task 2
```

---

## Architecture Decision Records (ADRs)

When making significant architectural decisions:

### ADR Template

```markdown
# ADR-XXX: [Title]

**Date:** YYYY-MM-DD  
**Status:** Proposed | Accepted | Superseded  
**Deciders:** [Who was involved]

## Context
What's the issue we're facing?

## Decision
What did we decide?

## Consequences
What are the trade-offs?

## Alternatives Considered
What else did we consider and why not?
```

### ADR Numbering
- Start at 001
- Increment sequentially
- Never reuse numbers
- Update `DECISIONS.md` index when adding ADR

---

## Git Workflow

### Commit Frequency
- **After each meaningful decision** (don't batch)
- **End of each work session**
- **Before context switch** (changing modules)

### Commit Message Format
```
[module] Brief description

Longer explanation if needed.

References: docs/decisions/XXX, docs/modules/YY
```

### Branching Strategy
- `main` - Stable, working code
- `develop` - Integration branch
- `feature/module-N-feature-name` - Per-feature branches
- `docs/topic` - Documentation-only changes

---

## Module Development Rules

### Before Starting a Module
1. ✅ Create module doc in `docs/modules/XX_NAME.md`
2. ✅ List all open questions
3. ✅ Get answers to questions (discuss with team)
4. ✅ Create ADRs for key decisions
5. ✅ Only then start coding

### During Development
- Update module doc with implementation notes
- Create ADRs for any architectural choices
- Keep TODO.md updated with blockers

### When Completing a Module
1. ✅ Mark all questions as answered
2. ✅ Update ROADMAP.md status
3. ✅ Write tests (documented in module doc)
4. ✅ Create session summary
5. ✅ Update next module's dependencies

---

## Session Summary Format

Each session gets a summary file: `sessions/SESSION_YYYY-MM-DD.md`

### Template
```markdown
# Session: YYYY-MM-DD

## What We Worked On
- Module X: Feature Y
- Discussion of Z

## Decisions Made
- Decision 1 (ADR-XXX)
- Decision 2 (ADR-YYY)

## Progress
- ✅ Completed Task A
- ⏳ In Progress Task B
- ❌ Blocked Task C (reason)

## Next Session Goals
- [ ] Goal 1
- [ ] Goal 2

## Open Questions
- Question 1?
- Question 2?

## Files Changed
- path/to/file.md
- path/to/code.rs
```

---

## TODO.md Management

Keep `docs/TODO.md` as a living document:

### Format
```markdown
# Fermi TODO

**Last Updated:** YYYY-MM-DD

## Immediate (Current Sprint)
- [ ] Task 1 [Module 1]
- [ ] Task 2 [Module 2]

## Short Term (Next Sprint)
- [ ] Task 3 [Module 3]

## Long Term (Future)
- [ ] Task 4 [Module 7]

## Blocked (Waiting On)
- [ ] Task 5 - Blocked by: reason

## Questions Needing Answers
- [ ] Question 1 about Module X
- [ ] Question 2 about Module Y
```

---

## DECISIONS.md Quick Reference

Keep `docs/DECISIONS.md` as an index of all ADRs:

### Format
```markdown
# Architectural Decisions

Quick reference to all Architecture Decision Records (ADRs).

## Index

- [ADR-001: Architecture Option C](decisions/001_architecture_option_c.md) - 2026-02-04
- [ADR-002: Rust Backend Rebuild](decisions/002_rust_backend_rebuild.md) - 2026-02-04
- [ADR-003: Incremental Parsing Strategy](decisions/003_incremental_parsing_strategy.md) - 2026-02-04

## By Category

### Architecture
- ADR-001: Architecture Option C

### Backend
- ADR-002: Rust Backend Rebuild

### Language Server
- ADR-003: Incremental Parsing Strategy
```

---

## Code Documentation Rules

### Rust Code Comments
```rust
/// Public API - full documentation
/// 
/// # Examples
/// ```
/// let result = function(param);
/// ```
pub fn function() {}

// Internal implementation notes
fn helper() {}
```

### Markdown Documentation
- Use **bold** for emphasis
- Use `code` for technical terms
- Use > blockquotes for important notes
- Use tables for comparisons
- Use mermaid for diagrams

---

## Communication Conventions

### When Asking Questions
- Be specific about which module
- Reference existing docs
- Explain context if needed

### When Answering Questions
- Update relevant module doc
- Create ADR if decision made
- Link to related docs

### When Disagreeing
- Reference specific concerns
- Propose alternatives
- Document trade-offs
- Let data/testing decide when possible

---

## Review Checklist

Before committing significant work:

- [ ] Module doc updated?
- [ ] ADRs created for decisions?
- [ ] TODO.md updated?
- [ ] Session summary written?
- [ ] Tests documented?
- [ ] Code comments added?
- [ ] ROADMAP.md status updated?
- [ ] Git commit message references docs?

---

## Versioning

### Documentation Versions
- Update version number when major structure changes
- Note date of last update in each doc
- Keep old versions in Git history (don't delete)

### Code Versions
Follow semantic versioning (MAJOR.MINOR.PATCH):
- Currently: v0.4.0 (core engine complete)
- Next: v0.5.0 (agent system)
- Future: v1.0.0 (full MMOG features)

---

## Emergency Context Recovery

**If you lose context mid-session:**

1. Read latest `sessions/SESSION_YYYY-MM-DD.md`
2. Check `git log --oneline -10` for recent commits
3. Review modified files: `git diff HEAD~5 --stat`
4. Read relevant module docs
5. Check `TODO.md` for what was in progress

**If starting fresh after long break:**

1. Read `ROADMAP.md` - Where are we in the project?
2. Read `DECISIONS.md` - What's been decided?
3. Read `TODO.md` - What's pending?
4. Read latest session summary
5. Pick up where last session left off

---

## Module Priority Stack

Work on modules in this order:

1. **Module 1: FPL LSP** (foundation)
2. **Module 2: Zed Extensions** (UI integration)
3. **Module 5: Backend** (agent registry)
4. **Module 3: Agent Bestiary** (agent UI)
5. **Module 4: Visualization** (charts)
6. **Module 6: Mermaid Viewer** (ER diagrams)
7. **Module 9: Navigation** (discovery)
8. **Module 7: Collaboration** (tournaments)
9. **Module 8: Settings** (configuration)
10. **Module 10: Mobile** (future)

**Current Focus:** Module 1 (FPL LSP)

---

## Success Metrics

### Documentation Quality
- Can someone new understand the project from docs/ alone?
- Are all decisions documented?
- Are there no orphaned decisions (undocumented choices)?

### Code Quality
- Does code match the documented architecture?
- Are module boundaries clean?
- Is coupling loose?

### Context Preservation
- Can we resume after weeks away?
- Are discussions captured?
- Is rationale preserved?

---

## Anti-Patterns to Avoid

❌ **Don't:**
- Make architectural decisions without ADRs
- Commit code without updating docs
- Skip session summaries
- Let TODO.md get stale
- Make cross-module dependencies tight
- Build monolithic components

✅ **Do:**
- Document first, code second
- Keep modules independent
- Write session summaries
- Update TODO.md actively
- Create ADRs for decisions
- Commit docs and code together

---

## Getting Help

**If you're stuck:**
1. Check module doc for open questions
2. Review related ADRs
3. Read session summaries for context
4. Ask specific questions referencing docs

**If you find an issue with this system:**
1. Propose change in discussion
2. Create ADR if needed
3. Update PROJECT_RULES.md
4. Announce change in session summary

---

**Remember:** Documentation is not overhead - it's the foundation of maintainable, modular architecture. Invest time here to save time later.
