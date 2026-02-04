# ADR-010: Use Rowan for Lossless Syntax Trees

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** Module 1 Q1.1, ADR-006 (rust-sitter)

## Context

We need an incremental parsing strategy for the FPL Language Server that provides:
- Sub-100ms re-parse latency for real-time coaching
- Error recovery (invalid syntax shouldn't crash)
- Preservation of all source text (comments, whitespace) for tooling
- Integration with tree-sitter for Zed syntax highlighting

**Options Evaluated:**

### A. Salsa - Incremental Computation Framework
**What it is:** Generic framework for on-demand, incrementalized computation used in rust-analyzer and rustc.

**Pros:**
- Battle-tested in rust-analyzer and rustc query system
- Early cutoff optimization (unchanged results aren't recomputed)
- Durability system (mark stdlib queries as more durable than user code)
- Sophisticated caching and memoization

**Cons:**
- Steeper learning curve (requires understanding query system architecture)
- More complex to integrate (need to model everything as queries)
- Overkill for FPL's relatively simple grammar
- Harder to debug when queries aren't invalidating correctly

### B. Rowan - Lossless Syntax Tree
**What it is:** Library for lossless syntax trees with green/red tree architecture, used in rust-analyzer.

**Pros:**
- Lossless (preserves all source text including whitespace, comments)
- Error recovery built-in (can build tree for invalid syntax)
- Immutable green tree (cheap to change, good for incremental updates)
- Red tree layered on top (models exact source structure with offsets, parents)
- Simpler API than salsa (just build trees, not query systems)
- Perfect for IDEs (designed for real-time editing scenarios)

**Cons:**
- Less sophisticated than salsa (no automatic dependency tracking)
- Need to manually implement incremental logic
- Slightly larger memory footprint (stores both green and red trees)

### C. Custom Incremental Parser
**Pros:** Full control, exactly what we need  
**Cons:** High effort, reinventing the wheel, likely slower than battle-tested libraries  
**Rejected Because:** Not worth the engineering effort

## Decision

We will use **Rowan for lossless syntax trees** with our existing recursive descent parser.

**Architecture:**

```
┌─────────────────────────────────────────────────┐
│          FPL Language Server                    │
├─────────────────────────────────────────────────┤
│                                                 │
│  Existing Lexer (tokens) ──→ Parser (AST)     │
│                                  ↓              │
│  Rowan GreenNodeBuilder  ←──────┘              │
│  (lossless, immutable)                          │
│       ↓                                         │
│  Rowan SyntaxNode                               │
│  (red tree: offsets, parents, queries)          │
│       ↓                                         │
│  LSP Features:                                  │
│  - Diagnostics (use AST for semantic errors)   │
│  - Hover info (query syntax tree)              │
│  - Go to definition (traverse tree)            │
│  - Code actions (tree transformations)         │
│                                                 │
└─────────────────────────────────────────────────┘
```

**Incremental Strategy:**
1. On file edit, re-parse only changed regions
2. Rowan's green tree is immutable → cheap to share unchanged nodes
3. Red tree is rebuilt on-demand for queries (lazy evaluation)
4. Keep previous parse tree, diff to find minimal changes

## Consequences

### Positive

1. **Lossless Preservation:** All source text preserved → perfect for refactoring tools, formatters
2. **Error Tolerance:** Can build tree even for invalid syntax → LSP works while user is typing
3. **IDE-Optimized:** Designed for real-time editing, proven in rust-analyzer
4. **Simpler Than Salsa:** Just build trees, no query system mental model needed
5. **Tree-sitter Compatibility:** Both produce syntax trees → easy to keep in sync
6. **Fast Enough:** rust-analyzer proves Rowan is fast enough for large Rust files

### Negative

1. **Manual Incremental Logic:** Need to implement our own diffing and re-parsing strategy
2. **Memory Overhead:** Stores green + red trees (but FPL files are typically small)
3. **Not Automatic:** Unlike salsa, doesn't automatically track dependencies
4. **Learning Curve:** Need to understand green/red tree architecture

### Neutral

1. **Salsa Still Option:** If Rowan proves insufficient, can switch to salsa later
2. **Hybrid Approach Possible:** Could use salsa for semantic analysis, Rowan for syntax

## Implementation Notes

### Phase 1: Integrate Rowan (Week 1)

```rust
// fermi-lsp/src/syntax.rs
use rowan::{GreenNode, GreenNodeBuilder, Language};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyntaxKind {
    // Tokens
    Ident,
    Number,
    String,
    
    // Keywords
    Forecast,
    Driver,
    Estimate,
    
    // Nodes
    ForecastStmt,
    DriverStmt,
    EstimateStmt,
    Expression,
    
    // Special
    Error,
    Whitespace,
    Comment,
}

#[derive(Debug, Clone, Copy)]
pub struct FplLanguage;

impl Language for FplLanguage {
    type Kind = SyntaxKind;
    
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        // Map raw syntax kind to our enum
        unsafe { std::mem::transmute(raw.0) }
    }
    
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        rowan::SyntaxKind(kind as u16)
    }
}

pub type SyntaxNode = rowan::SyntaxNode<FplLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<FplLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<FplLanguage>;
```

### Phase 2: Parser Integration

```rust
// Modify existing parser to build green tree
pub struct Parser<'a> {
    tokens: &'a [Token],
    current: usize,
    builder: GreenNodeBuilder<'static>,  // Add this
}

impl<'a> Parser<'a> {
    fn parse_forecast(&mut self) -> ParseResult<()> {
        self.builder.start_node(SyntaxKind::ForecastStmt.into());
        
        // Existing parsing logic
        self.expect_keyword("forecast")?;
        let title = self.parse_string()?;
        
        // ... more parsing
        
        self.builder.finish_node();
        Ok(())
    }
    
    fn consume_token(&mut self) {
        let token = &self.tokens[self.current];
        self.builder.token(
            SyntaxKind::from_token(&token.type_).into(),
            token.lexeme.as_str()
        );
        self.current += 1;
    }
}
```

### Phase 3: Incremental Updates

```rust
pub struct DocumentState {
    text: String,
    green: GreenNode,
    version: i32,
}

impl DocumentState {
    pub fn update(&mut self, changes: Vec<TextDocumentContentChangeEvent>) {
        for change in changes {
            // Apply change to text
            apply_change(&mut self.text, &change);
            
            // Re-parse changed region
            // TODO: Implement smart diffing to minimize re-parsing
            let new_green = parse_to_green_tree(&self.text);
            self.green = new_green;
            self.version += 1;
        }
    }
}
```

## References

- **Rowan GitHub:** https://github.com/rust-analyzer/rowan
- **Lossless Syntax Trees:** https://dev.to/cad97/lossless-syntax-trees-280c
- **Rowan Examples:** https://github.com/rust-analyzer/rowan/blob/master/examples/s_expressions.rs
- **Salsa Framework:** https://github.com/salsa-rs/salsa
- **Salsa Algorithm Explained:** https://medium.com/@eliah.lakhin/salsa-algorithm-explained-c5d6df1dd291
- **rust-analyzer Durability:** https://rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html
- Module 1 Q1.1: Incremental Parsing Strategy

## Benchmarking Plan

Before committing long-term, benchmark Rowan with realistic FPL files:

```rust
#[bench]
fn bench_incremental_edit(b: &mut Bencher) {
    let source = include_str!("../test_files/large_forecast.fpl");
    let mut state = DocumentState::new(source);
    
    b.iter(|| {
        // Simulate typing a character
        state.update(vec![insert_char_at(100, 'x')]);
    });
    
    // Should be <10ms for typical edit
}
```

**Target:** <10ms for small edits, <100ms for full re-parse of 1000-line file

## Future Considerations

- **If Rowan insufficient:** Switch to salsa with query-based architecture
- **If salsa needed for semantics:** Use hybrid (Rowan for syntax, salsa for types)
- **Optimization:** Cache parsed subtrees that haven't changed
- **Advanced:** Implement error recovery heuristics (skip to next statement on error)

## Success Metrics

- **Parse Latency:** <10ms for typical single-character edit
- **Full Re-parse:** <100ms for 1000-line FPL file
- **Memory:** <10MB for typical workspace (10 files)
- **Error Recovery:** Can provide diagnostics even with syntax errors
- **Lossless:** Round-trip (parse → format → parse) preserves all text
