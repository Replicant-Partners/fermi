# ADR-006: Tree-sitter Grammar Generation via rust-sitter

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** Module 2 Q2.1

## Context

Zed requires a tree-sitter grammar for syntax highlighting, code folding, and structural navigation. We have an existing Rust parser (lexer + recursive descent) for FPL that handles the language semantics.

**The Problem:** We need to create a tree-sitter grammar for Zed integration. We have three options:

1. **Generate from existing Rust parser** - Automated conversion using tools
2. **Hand-write grammar.js** - Manual tree-sitter grammar creation
3. **Start minimal and iterate** - Ship basic grammar, improve over time

**Key Constraints:**
- Don't want to maintain two parsers (semantic parser in LSP + tree-sitter for highlighting)
- FPL syntax is relatively simple (compared to C++ or Rust)
- Tree-sitter grammars can be tricky to write correctly (especially for error recovery)
- We already have a working parser with good error messages

**Tool Discovery:** User found [rust-sitter](https://github.com/hydro-project/rust-sitter), a tool that generates tree-sitter grammars from Rust parser code using procedural macros.

## Decision

We will **generate the tree-sitter grammar from our existing Rust parser using rust-sitter**.

**Approach:**

1. Annotate our existing parser with rust-sitter macros
2. Generate tree-sitter grammar automatically
3. Use generated grammar in Zed extension
4. Keep semantic parser separate (for LSP diagnostics, type checking, execution)

**Architecture:**

```
┌─────────────────────────────────────────────┐
│         FPL Language Tooling                │
├─────────────────────────────────────────────┤
│                                             │
│  Rust Parser (fermi-parser crate)          │
│  ├── Lexer (tokenization)                  │
│  ├── Parser (AST construction)             │
│  └── rust-sitter annotations               │
│           ↓                                 │
│  [rust-sitter codegen]                     │
│           ↓                                 │
│  grammar.js (tree-sitter grammar)          │
│           ↓                                 │
│  [tree-sitter generate]                    │
│           ↓                                 │
│  tree-sitter-fpl.wasm (for Zed)           │
│                                             │
└─────────────────────────────────────────────┘
```

**Example Implementation:**

```rust
// In fermi-parser/src/parser.rs
use rust_sitter::Grammar;

#[derive(Grammar)]
#[grammar(
    name = "fpl",
    extras = [" ", "\t", "\n", "\r"],
)]
pub struct FPLGrammar;

// Annotate existing parser rules
#[rust_sitter::rule(Forecast)]
fn parse_forecast(&mut self) -> ParseResult<Forecast> {
    // Existing parsing logic
    let title = self.parse_string()?;
    let drivers = self.parse_drivers()?;
    let estimate = self.parse_estimate()?;
    
    Ok(Forecast { title, drivers, estimate })
}

#[rust_sitter::rule(Driver)]
fn parse_driver(&mut self) -> ParseResult<Driver> {
    // Existing parsing logic
    self.expect_keyword("driver")?;
    let name = self.parse_identifier()?;
    let dist = self.parse_distribution()?;
    
    Ok(Driver { name, dist })
}

#[rust_sitter::rule(Distribution)]
fn parse_distribution(&mut self) -> ParseResult<Distribution> {
    let dist_type = self.parse_identifier()?;
    match dist_type.as_str() {
        "triangular" => self.parse_triangular(),
        "normal" => self.parse_normal(),
        "uniform" => self.parse_uniform(),
        _ => Err(ParseError::UnknownDistribution(dist_type)),
    }
}
```

**Build Process:**

```bash
# Generate grammar.js from Rust parser
cargo run --bin rust-sitter-gen

# Generate tree-sitter parser
tree-sitter generate

# Build WASM for Zed
tree-sitter build-wasm

# Copy to Zed extension
cp tree-sitter-fpl.wasm ../zed-fermi-extension/grammars/
```

## Consequences

### Positive

1. **Single Source of Truth:** Parser logic lives in one place (Rust code), grammar is derived
2. **Consistency Guaranteed:** Syntax highlighting always matches semantic parser
3. **Easier Maintenance:** Update parser once, regenerate grammar automatically
4. **Leverage Existing Work:** Don't rewrite parser from scratch in grammar.js DSL
5. **Better Error Recovery:** rust-sitter handles error recovery patterns automatically
6. **Type Safety:** Parser changes are caught at compile time before grammar generation

### Negative

1. **Dependency on rust-sitter:** If tool breaks or becomes unmaintained, we're stuck
2. **Build Complexity:** Additional build step (generate grammar → tree-sitter build → WASM)
3. **Learning Curve:** Team needs to understand rust-sitter annotation syntax
4. **Generated Code Quality:** Grammar.js might not be as optimized as hand-written
5. **Debugging Difficulty:** When grammar has issues, need to debug Rust code + generated grammar

### Neutral

1. **Two Parsers Still Exist:** Tree-sitter for highlighting, Rust parser for semantics (but shared source)
2. **Grammar Regeneration:** Need to regenerate on parser changes (add to CI/CD)

## Alternatives Considered

### B. Hand-write grammar.js
**Pros:** Full control, can optimize for tree-sitter, no external dependencies  
**Cons:** Maintain two parsers, easy to drift from semantic parser, steep learning curve  
**Rejected Because:** High maintenance burden, risk of inconsistency between highlighting and semantics

### C. Start Minimal and Iterate
**Pros:** Ship fast, learn what users need, avoid premature optimization  
**Cons:** Users get inconsistent highlighting, frustrating experience, technical debt  
**Rejected Because:** We already have a working parser - no reason to ship worse experience

## Implementation Notes

### Phase 1: Setup rust-sitter (Week 1)
1. Add rust-sitter dependency to fermi-parser
2. Annotate core parser rules (Forecast, Driver, Estimate)
3. Generate initial grammar.js
4. Verify generated grammar works with tree-sitter CLI
5. Build WASM and test in simple HTML page

### Phase 2: Zed Integration (Week 2)
1. Create zed-fermi-lsp extension structure
2. Copy tree-sitter-fpl.wasm to extension
3. Configure language in extension.toml
4. Test syntax highlighting in Zed
5. Add queries for highlighting (highlights.scm), indentation (indents.scm)

### Phase 3: Advanced Features (Week 3)
1. Add textobjects.scm for structural navigation (go to next driver, etc.)
2. Add injections.scm if we support embedded languages
3. Optimize grammar for performance
4. Add CI job to regenerate grammar on parser changes

### Testing Strategy

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_grammar_matches_parser() {
        let input = r#"
            forecast "Q4 Revenue" {
                driver revenue triangular(100, 200, 500)
                driver costs normal(150, 30)
                estimate revenue - costs
            }
        "#;
        
        // Parse with semantic parser
        let ast = parse_fpl(input).unwrap();
        
        // Parse with tree-sitter
        let tree = tree_sitter_parse(input).unwrap();
        
        // Compare structure
        assert_eq!(ast.drivers.len(), count_nodes(&tree, "driver"));
        assert_eq!(ast.estimate.is_some(), has_node(&tree, "estimate"));
    }
}
```

### CI/CD Integration

```yaml
# .github/workflows/grammar-gen.yml
name: Generate Tree-sitter Grammar

on:
  push:
    paths:
      - 'fermi-parser/src/**'

jobs:
  generate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Install rust-sitter
        run: cargo install rust-sitter-cli
      - name: Generate grammar
        run: cargo run --bin rust-sitter-gen
      - name: Build tree-sitter
        run: |
          npm install -g tree-sitter-cli
          tree-sitter generate
          tree-sitter build-wasm
      - name: Commit generated files
        run: |
          git add grammar.js tree-sitter-fpl.wasm
          git commit -m "chore: regenerate tree-sitter grammar"
          git push
```

## References

- rust-sitter: https://github.com/hydro-project/rust-sitter
- tree-sitter documentation: https://tree-sitter.github.io/tree-sitter/
- Zed language extension guide: https://zed.dev/docs/extensions/languages
- Module 2 Q2.1: Tree-sitter Grammar Creation

## Open Questions

1. **rust-sitter Stability:** How stable is the tool? Last commit, issue tracker activity?
2. **WASM Size:** What's the size of generated WASM? Affects extension bundle size.
3. **Error Recovery:** Does rust-sitter generate good error recovery rules? Need to test with malformed FPL.
4. **Performance:** Is generated grammar as fast as hand-written? Benchmark on large files.
5. **Zed-Specific Features:** Does generated grammar work well with Zed's tree-sitter integration?

## Success Metrics

- **Generation Time:** <5 seconds to generate grammar from parser
- **Grammar Size:** <100KB for grammar.js, <500KB for WASM
- **Parse Speed:** <10ms for typical FPL file (100 lines) in Zed
- **Consistency:** 100% match between tree-sitter structure and semantic AST structure
- **Developer Experience:** Adding new syntax requires only updating Rust parser (no manual grammar.js edits)
