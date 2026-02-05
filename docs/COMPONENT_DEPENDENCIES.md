# FPL Component Dependencies

**Date:** 2026-02-05  
**Purpose:** Document the dependency chain between FPL language components to prevent synchronization issues

## Overview

The FPL language system consists of multiple interconnected components. When one component changes, related components must be updated to maintain consistency.

## Component Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Language Definition                      │
├─────────────────────────────────────────────────────────────┤
│                                                               │
│  ┌──────────┐      ┌──────────┐      ┌──────────────┐      │
│  │  Lexer   │─────>│  Parser  │─────>│   Semantic   │      │
│  │          │      │          │      │   Analyzer   │      │
│  └──────────┘      └──────────┘      └──────────────┘      │
│       │                 │                     │              │
│       │                 │                     │              │
└───────┼─────────────────┼─────────────────────┼─────────────┘
        │                 │                     │
        v                 v                     v
┌───────────────────────────────────────────────────────────┐
│                    Editor Integration                      │
├───────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────┐    ┌────────────┐    ┌──────────────┐   │
│  │   Grammar   │    │    LSP     │    │  Extension   │   │
│  │(Tree-sitter)│    │            │    │   (Zed)      │   │
│  └─────────────┘    └────────────┘    └──────────────┘   │
│         │                  │                   │           │
│         └──────────────────┴───────────────────┘           │
│                     highlights.scm                         │
└───────────────────────────────────────────────────────────┘
```

## Dependency Chain

### 1. Keywords

When adding a new keyword to the language:

#### Required Updates:
1. **Lexer** (`src/lexer.rs`)
   - Add to `TokenType` enum
   - Add to keyword matching logic
   - Add to keyword table

2. **Parser** (`src/parser.rs`)
   - Add parsing logic for the keyword
   - Update relevant `parse_*` functions

3. **AST** (`src/ast.rs`)
   - Add new struct or enum if needed
   - Update existing structs with new fields

4. **LSP - Hover** (`fermi-lsp/src/hover/keywords.rs`)
   - Add keyword hover documentation
   - Include examples and syntax

5. **LSP - Completions** (`fermi-lsp/src/completions/keywords.rs`)
   - Add keyword to completion list
   - Add snippet template
   - Add detail and documentation

6. **Grammar** (`extensions/fermi/grammars/fpl/grammar.js`)
   - Add keyword to grammar rules
   - Update `queries/highlights.scm` for syntax highlighting

#### Example: Adding `base_rate` keyword

1. Lexer: Added `BaseRate` token type
2. Parser: Added `parse_base_rate()` function
3. AST: Added `BaseRate` struct to `QuestionStmt`
4. LSP Hover: Added base_rate keyword documentation
5. LSP Completions: Added base_rate snippet with properties
6. Grammar: Added base_rate highlighting as `@keyword.control`

### 2. Properties

When adding a new property to a statement block:

#### Required Updates:
1. **AST** (`src/ast.rs`)
   - Add field to relevant struct (e.g., `EvidenceStmt`, `DriverStmt`)
   - Make it `Option<T>` if not required

2. **Parser** (`src/parser.rs`)
   - Add field to variable declarations in `parse_*` function
   - Add match arm in field parsing loop
   - Add field to struct initialization

3. **Semantic Analyzer** (if validation needed) (`src/semantic.rs`)
   - Add validation rules for the property
   - Check type compatibility

4. **LSP - Hover** (`fermi-lsp/src/hover/properties.rs`)
   - Add property hover documentation
   - Document which blocks it's used in

5. **LSP - Completions** (`fermi-lsp/src/completions/mod.rs` or specific files)
   - Add property to relevant completion functions
   - Add snippet with placeholder

#### Example: Adding `strength` property

1. AST: Added `strength: Option<f64>` to `EvidenceStmt`
2. Parser: Added parsing logic in `parse_evidence()`
3. LSP Hover: Added strength property documentation
4. LSP Completions: Added strength to evidence completions

### 3. Distribution Functions

When adding a new distribution:

#### Required Updates:
1. **Lexer** (`src/lexer.rs`)
   - Add token type if keyword-based (e.g., `Exponential`)

2. **AST** (`src/ast.rs`)
   - Add variant to `Distribution` enum

3. **Parser** (`src/parser.rs`)
   - Add parsing logic in `parse_distribution()`

4. **Executor** (`src/executor.rs`)
   - Add sampling logic for the distribution

5. **LSP - Hover** (`fermi-lsp/src/hover/functions.rs`)
   - Add function documentation
   - Include parameter descriptions

6. **LSP - Completions** (`fermi-lsp/src/completions/functions.rs`)
   - Add to distribution completions
   - Include parameter snippets

### 4. Operators and Control Flow

When adding new operators or control structures:

#### Required Updates:
1. **Lexer** - Add token type
2. **Parser** - Add to expression parsing (precedence climbing)
3. **AST** - Add expression variant
4. **Executor** - Add evaluation logic
5. **LSP** - Add to hover and completions

## Common Synchronization Issues

### Issue 1: Parser supports feature, LSP doesn't
**Symptom:** No autocomplete or hover for valid syntax  
**Cause:** Parser/AST updated but LSP hover/completions not updated  
**Fix:** Add to `fermi-lsp/src/hover/` and `fermi-lsp/src/completions/`

### Issue 2: LSP suggests invalid syntax
**Symptom:** Autocomplete suggests code that parser rejects  
**Cause:** LSP completions out of sync with parser  
**Fix:** Update LSP to match current parser syntax

### Issue 3: Grammar highlighting doesn't work
**Symptom:** New keywords don't highlight in editor  
**Cause:** Grammar or highlights.scm not updated  
**Fix:** 
1. Update `extensions/fermi/grammars/fpl/grammar.js`
2. Rebuild grammar: `cd extensions/fermi && tree-sitter generate`
3. Update `queries/highlights.scm`
4. Clear Zed cache and reinstall extension

### Issue 4: Date parsing errors
**Symptom:** `Expected string but found Date(...)` errors  
**Cause:** Parser expecting string, lexer tokenizing as Date  
**Fix:** Update parser to accept both `Date` tokens and `String` tokens

## Validation Process

### Automated Validation

Run the validation script to check for synchronization issues:

```bash
./scripts/validate-components.sh
```

This checks:
- Keywords in lexer have hover documentation
- Major keywords have completions
- AST fields have property documentation
- Grammar files are present and synced
- Build artifacts exist
- Extension is installed

### Manual Validation Checklist

When adding a new feature:

- [ ] Lexer updated with new tokens
- [ ] Parser updated with parsing logic
- [ ] AST updated with new structs/fields
- [ ] Semantic analyzer updated (if needed)
- [ ] Executor updated (if needed)
- [ ] LSP hover documentation added
- [ ] LSP completions added
- [ ] Grammar updated (if syntax visible)
- [ ] Tests added/updated
- [ ] Examples updated
- [ ] Documentation updated

### Testing After Changes

1. **Build everything:**
   ```bash
   cargo build --release
   cd fermi-lsp && cargo build --release
   cd ..
   ```

2. **Run validation:**
   ```bash
   ./scripts/validate-components.sh
   ```

3. **Install extension:**
   ```bash
   bash scripts/install-extension.sh
   ```

4. **Test in Zed:**
   - Clear cache: `rm -rf ~/.cache/zed/*`
   - Restart Zed completely (not just reload extensions)
   - Open `.fpl` file
   - Test hover on new keywords/properties
   - Test autocomplete

5. **Run test file:**
   ```bash
   ./target/release/fermi examples/your_test.fpl
   ```

## File Locations Reference

### Core Language
- Lexer: `src/lexer.rs`
- Parser: `src/parser.rs`
- AST: `src/ast.rs`
- Semantic Analyzer: `src/semantic.rs`
- Executor: `src/executor.rs`

### LSP
- Main: `fermi-lsp/src/main.rs`
- Hover (keywords): `fermi-lsp/src/hover/keywords.rs`
- Hover (properties): `fermi-lsp/src/hover/properties.rs`
- Hover (functions): `fermi-lsp/src/hover/functions.rs`
- Completions (keywords): `fermi-lsp/src/completions/keywords.rs`
- Completions (properties): `fermi-lsp/src/completions/driver_properties.rs`
- Completions (functions): `fermi-lsp/src/completions/functions.rs`

### Zed Extension
- Extension manifest: `extensions/fermi/extension.toml`
- Grammar: `extensions/fermi/grammars/fpl/grammar.js`
- Syntax highlighting: `extensions/fermi/grammars/fpl/queries/highlights.scm`
- Language config: `extensions/fermi/languages/fpl/config.toml`

### Scripts
- Install extension: `scripts/install-extension.sh`
- Validate components: `scripts/validate-components.sh`
- Verify extension: `scripts/verify-extension.sh`

## Best Practices

1. **Always update LSP when updating parser** - Users expect hover and autocomplete for valid syntax

2. **Test with validation script** - Run `./scripts/validate-components.sh` before committing

3. **Clear Zed cache when debugging** - Stale cache is a common issue:
   ```bash
   rm -rf ~/.cache/zed/*
   rm -rf ~/.local/share/zed/extensions/installed/fermi
   ```

4. **Document new features** - Update this file when adding major features

5. **Use consistent naming** - Match token names, property names, and hover names

6. **Add examples** - Include examples in hover documentation and completions

## Recent Fixes

### 2026-02-05: base_rate and strength support
- Added `base_rate` keyword to lexer, parser, and AST
- Added `strength` property to `EvidenceStmt`
- Fixed date parsing to accept both `Date` tokens and strings
- Added hover and completion support for all new features
- Created validation script to catch future issues

### 2026-02-05: Syntax highlighting fix
- Root cause: Zed was using stale cached files
- Solution: Clear all Zed caches and reinstall extension
- Documented in `SYNTAX_HIGHLIGHTING_FIX.md`

## Troubleshooting

### Problem: "No hover information"
**Check:**
1. Is keyword in `fermi-lsp/src/hover/keywords.rs`?
2. Is property in `fermi-lsp/src/hover/properties.rs`?
3. Is LSP rebuilt? `cd fermi-lsp && cargo build --release`
4. Is extension reinstalled? `bash scripts/install-extension.sh`

### Problem: "Expected X but found Y" parse error
**Check:**
1. Is lexer tokenizing correctly?
2. Is parser handling the token type?
3. Does parser match both expected formats (e.g., Date and String)?

### Problem: Syntax highlighting doesn't work
**Solution:**
1. Clear Zed caches
2. Reinstall extension
3. Restart Zed completely
4. See `SYNTAX_HIGHLIGHTING_FIX.md`

---

**Maintainer:** Update this document when adding new language features or fixing dependency issues.
