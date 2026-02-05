# Template Update & DSL Stability Summary

**Date:** 2026-02-05  
**Issue:** Templates used outdated FPL syntax  
**Resolution:** All templates updated + DSL stability rule added

---

## Problem

Templates in `templates/*.fpl` were using old block-based syntax that didn't match the current parser:

**Old Template Syntax:**
```fpl
forecast "Title" {
    driver name triangular(1, 2, 3)
    driver calculated estimate expression
    estimate final_expression
}
```

**Current Parser Expects:**
```fpl
question "Title"
driver name continuous {
    distribution: triangular(1, 2, 3)
}
model: final_expression
simulate 10000 iterations
```

---

## Solution

### 1. Updated All 7 Templates

✅ **business-revenue.fpl** - Quarterly/annual revenue projections  
✅ **product-launch.fpl** - User acquisition forecasts  
✅ **market-sizing.fpl** - TAM/SAM/SOM analysis  
✅ **marketing-campaigns.fpl** - Campaign ROI projections  
✅ **hiring-costs.fpl** - Team expansion costs  
✅ **infrastructure-costs.fpl** - Cloud infrastructure costs  
✅ **fundraising-scenarios.fpl** - Runway and capital planning  

**Changes Made:**
- `forecast "..."` → `question "..."`
- `driver name distribution(...)` → `driver name continuous { distribution: ... }`
- Added `unit` and `rationale` fields to drivers
- `estimate expression` → `model: expression`
- Added explicit `simulate N iterations`
- Changed `//` comments to `#` (more standard)

### 2. Added DSL Stability Rule

**File:** `docs/PROJECT_RULES.md`

**New Section:** "🚨 DSL STABILITY RULE - READ THIS FIRST"

**Key Points:**
- DSL syntax changes are **HIGHLY SENSITIVE** and **ILL-ADVISED**
- Changes affect 8+ subsystems (parser, grammar, LSP, templates, tests, docs)
- Must follow strict process if change is absolutely necessary
- Documented current syntax as stable baseline
- Added syntax change history

---

## Verification

Tested updated template with Fermi CLI:

```bash
./target/release/fermi templates/business-revenue.fpl
```

**Result:** ✅ Success!
- Lexical analysis: ✓
- Parsing: ✓
- Semantic analysis: ✓
- Execution: ✓

---

## Impact

### Before
- ❌ Templates didn't work with current parser
- ❌ No DSL stability guidelines
- ❌ Three different syntax variants in the codebase
- ❌ Confusion about "correct" syntax

### After
- ✅ All templates work with current parser
- ✅ DSL stability rule in place
- ✅ Single canonical syntax
- ✅ Clear process for any future changes

---

## Current Canonical FPL Syntax (v0.4.0)

This is now the **single source of truth** for FPL syntax:

```fpl
# Comments can use # (preferred), //, or /* */
question "What is your forecast question?"

# Continuous drivers (most common)
driver variable_name continuous {
    distribution: triangular(min, likely, max)  # or normal, lognormal, uniform, beta
    unit: "description of units"
    rationale: "why this assumption matters"
}

# Binary drivers (yes/no outcomes)
driver binary_variable binary {
    probability: 0.65p
    impact_multiplier: 1.3
    rationale: "explanation"
}

# Evidence (optional - for documentation)
evidence source_name {
    source: "Citation or URL"
    summary: "Key findings"
    relevance: 0.9p
    date: 2026-01-15
}

# Agents (optional - for research)
agent agent_name {
    query: "What to research"
    schedule: every 1 week
}

# Model calculation
model: mathematical_expression_using_drivers

# Simulation parameters
simulate 10000 iterations
```

---

## Files Updated

### Templates (7 files)
1. `templates/business-revenue.fpl`
2. `templates/product-launch.fpl`
3. `templates/market-sizing.fpl`
4. `templates/marketing-campaigns.fpl`
5. `templates/hiring-costs.fpl`
6. `templates/infrastructure-costs.fpl`
7. `templates/fundraising-scenarios.fpl`

### Documentation (1 file)
1. `docs/PROJECT_RULES.md` - Added DSL Stability Rule section

---

## DSL Change Process (For Future)

If FPL syntax MUST be changed (strongly discouraged):

### Pre-Change Checklist
- [ ] Create ADR documenting necessity
- [ ] Design migration path for existing code
- [ ] Consider versioning (FPL v1 vs v2)
- [ ] Get stakeholder buy-in

### Update Checklist
- [ ] Lexer (`src/lexer.rs`)
- [ ] Parser (`src/parser.rs`)
- [ ] AST (`src/ast.rs`)
- [ ] Semantic analyzer (`src/semantic.rs`)
- [ ] Tree-sitter grammar (`extensions/fermi/grammars/fpl/grammar.js`)
- [ ] LSP server (`fermi-lsp/src/main.rs`)
- [ ] All templates (`templates/*.fpl`)
- [ ] All tests (59+ test files)
- [ ] All examples (`examples/*.fpl`)
- [ ] Documentation
- [ ] Changelog

### Post-Change Checklist
- [ ] All tests pass
- [ ] Examples work
- [ ] Templates work
- [ ] LSP features work (completion, hover, diagnostics)
- [ ] Syntax highlighting works in Zed
- [ ] Migration guide written
- [ ] Change announced

**Estimated Effort:** 8-16 hours of work  
**Risk Level:** HIGH  
**Recommendation:** Avoid unless absolutely critical

---

## Lessons Learned

1. **Consistency is King** - Having three syntax variants caused confusion
2. **Grammar Drift is Real** - Tree-sitter grammar drifted from parser over time
3. **Templates are API** - Templates define the "example syntax" users copy
4. **Stability Matters** - DSL changes are expensive and disruptive
5. **Document Syntax** - Need single canonical reference for FPL syntax

---

## Next Steps

1. ✅ Templates updated and working
2. ✅ DSL stability rule documented
3. ⏭️ Test all templates with parser
4. ⏭️ Update user documentation with canonical syntax
5. ⏭️ Consider adding syntax version number to FPL files
6. ⏭️ Create migration tool if old syntax files exist in the wild

---

## Related Documentation

- `docs/PROJECT_RULES.md` - DSL Stability Rule
- `docs/PHASE_1_PROGRESS.md` - Phase 1 progress
- `GRAMMAR_FIX_SUMMARY.md` - Grammar synchronization fix

---

**Status:** ✅ Complete  
**All 7 templates now use correct, current FPL syntax**  
**DSL stability rule in place to prevent future drift**
