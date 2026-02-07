# Getting Started with Fermi FPL Lexer

## Prerequisites

You need Rust installed. If you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

## Quick Start

### 1. Build the Project

```bash
cd /home/ilabra/fermi
cargo build --release
```

### 2. Run the REPL (Interactive Mode)

```bash
cargo run
```

You'll see:
```
╔═══════════════════════════════════════════╗
║   Fermi - Forecasting Language v0.1.0   ║
║   Agent Fermi's Broca Brain              ║
╚═══════════════════════════════════════════╝

Welcome to the Fermi REPL!
Type FPL code or 'help' for help, 'exit' to quit.

fermi> 
```

### 3. Try Some Code

**Simple example:**
```
fermi> driver market_size continuous

✓ Tokenized 3 token(s):
  • Driver 'driver'
  • Identifier("market_size") 'market_size'
  • Continuous 'continuous'
```

**Probability example:**
```
fermi> 0.75p 50%

✓ Tokenized 2 token(s):
  • Probability(0.75) '0.75p'
  • Probability(0.5) '50%'
```

**Multi-line example (press Enter twice to execute):**
```
fermi> driver market_size continuous {
     >     distribution: triangular(500, 1200, 2500)
     > }
     > 
✓ Tokenized 12 token(s):
  • Driver 'driver'
  • Identifier("market_size") 'market_size'
  • Continuous 'continuous'
  • LBrace '{'
  • Identifier("distribution") 'distribution'
  • Colon ':'
  • Triangular 'triangular'
  ...
```

### 4. Process a Complete File

```bash
cargo run examples/amd_forecast.fpl
```

You'll see a complete analysis:
```
📄 Processing file: examples/amd_forecast.fpl

✓ Lexical analysis successful!

Token Summary:
  Statements: 12
  Literals: 45
  Identifiers: 28
  Distributions: 4
  Operators: 8
  Other: 42

Tokens:
  1. Question 'question' at 5:1
  2. String("Will AMD reach $200 by 2026-12-31?") '"Will AMD reach $200 by 2026-12-31?"' at 5:10
  3. Driver 'driver' at 8:1
  ...
```

## What's Included

### Source Files

```
fermi/
├── src/
│   ├── lib.rs           # Library entry point
│   ├── main.rs          # CLI/REPL implementation
│   └── lexer.rs         # Lexer implementation (main component)
├── examples/
│   └── amd_forecast.fpl # Complete forecast example
├── Cargo.toml           # Rust project configuration
├── LEXER_README.md      # Detailed lexer documentation
├── GETTING_STARTED.md   # This file
└── FERMI_BROCA_ARCHITECTURE.md  # System architecture diagrams
```

### Features Implemented

✅ **Complete Lexer** (src/lexer.rs)
- All FPL keywords (question, driver, evidence, agent, model, simulate)
- Number literals (42, 3.14, 1.5e10)
- Probability literals (0.5p, 75%)
- Date literals (2026-12-31)
- String literals with escapes
- All operators and delimiters
- Comments
- Comprehensive error handling
- Position tracking (line, column)

✅ **Interactive REPL** (src/main.rs)
- Line-by-line or multi-line input
- Pretty-printed output with colors
- Built-in help system
- Error reporting

✅ **File Processing** (src/main.rs)
- Read .fpl files
- Token summary statistics
- Full token listing

✅ **Test Suite** (src/lexer.rs)
- 10+ comprehensive test cases
- All token types covered
- Error conditions tested
- Complete forecast example test

## Example FPL Programs

### Minimal Example

```fpl
question "Will X happen?"
driver factor continuous {
    distribution: triangular(1, 5, 10)
}
simulate 1000 iterations
```

### Complete Example

See `examples/amd_forecast.fpl` for a full-featured forecast with:
- Question definition
- Multiple drivers (continuous and binary)
- Evidence items
- Research agents with scheduling
- Model expression
- Simulation command

### Token-by-Token Walkthrough

**Input:**
```fpl
driver market_size continuous
```

**Lexer processing:**
1. Reads 'd' → starts identifier scan
2. Reads "driver" → recognizes keyword → Token(Driver)
3. Reads ' ' → whitespace, skip
4. Reads 'm' → starts identifier scan
5. Reads "market_size" → identifier → Token(Identifier("market_size"))
6. Reads ' ' → whitespace, skip
7. Reads 'c' → starts identifier scan
8. Reads "continuous" → recognizes keyword → Token(Continuous)
9. Reaches end → Token(EOF)

**Output:**
```
[
    Token { type: Driver, lexeme: "driver", line: 1, column: 1 },
    Token { type: Identifier("market_size"), lexeme: "market_size", line: 1, column: 8 },
    Token { type: Continuous, lexeme: "continuous", line: 1, column: 20 },
    Token { type: EOF, lexeme: "", line: 1, column: 30 }
]
```

## Testing

### Run all tests
```bash
cargo test
```

Expected output:
```
running 13 tests
test lexer::tests::test_keywords ... ok
test lexer::tests::test_numbers ... ok
test lexer::tests::test_probability ... ok
test lexer::tests::test_date ... ok
test lexer::tests::test_string ... ok
test lexer::tests::test_operators ... ok
test lexer::tests::test_identifiers ... ok
test lexer::tests::test_comment ... ok
test lexer::tests::test_error_unterminated_string ... ok
test lexer::tests::test_error_invalid_probability ... ok
test lexer::tests::test_complete_forecast ... ok

test result: ok. 13 passed; 0 failed
```

### Run specific test
```bash
cargo test test_probability
```

### Run with output
```bash
cargo test -- --nocapture
```

## Common Issues

### "cargo: not found"

Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### Build errors

Make sure you're in the right directory:
```bash
cd /home/ilabra/fermi
cargo clean
cargo build
```

### REPL not responding

Press Enter twice after multi-line input to execute.

## Next Steps

After the lexer is working, we'll build:

1. **Parser** - Converts token stream to Abstract Syntax Tree (AST)
2. **Semantic Analyzer** - Type checking and validation
3. **Executor** - Runs the forecast model with Monte Carlo simulation
4. **Coaching Engine** - Intelligent guidance and suggestions
5. **LSP Server** - IDE integration for VS Code, etc.

Each component builds on the lexer's token stream.

## Development Workflow

### Add a new token type

1. Add to `TokenType` enum in `src/lexer.rs`:
```rust
pub enum TokenType {
    // ... existing types
    NewType,
}
```

2. Add keyword recognition in `scan_identifier()`:
```rust
"newtype" => TokenType::NewType,
```

3. Add test:
```rust
#[test]
fn test_new_type() {
    let source = "newtype";
    let lexer = Lexer::new(source);
    let tokens = lexer.tokenize().unwrap();
    assert!(matches!(tokens[0].token_type, TokenType::NewType));
}
```

4. Run test:
```bash
cargo test test_new_type
```

### Debug lexer behavior

Add debug prints in `src/lexer.rs`:
```rust
fn scan_token(&mut self) {
    let c = self.advance();
    println!("DEBUG: char='{}' at {}:{}", c, self.line, self.column);
    // ... rest of function
}
```

Then run:
```bash
cargo run -- --debug
```

## Performance Tips

The lexer is already optimized for performance, but if you're processing very large files:

- Use `--release` builds: `cargo build --release`
- Profile with: `cargo flamegraph` (requires flamegraph tool)
- Benchmark with: `cargo bench` (once benchmarks are added)

## Architecture Recap

```
┌─────────────────────────────────────────────────────┐
│                  FPL Source Code                     │
│   "question \"Will AMD reach $200?\""               │
└──────────────────┬──────────────────────────────────┘
                   │
                   ↓
┌─────────────────────────────────────────────────────┐
│              Lexer (src/lexer.rs)                    │
│                                                       │
│  • Character-by-character scanning                   │
│  • Pattern matching for keywords, operators          │
│  • Number/probability/date parsing                   │
│  • String handling with escapes                      │
│  • Error detection and reporting                     │
└──────────────────┬──────────────────────────────────┘
                   │
                   ↓
┌─────────────────────────────────────────────────────┐
│                   Token Stream                       │
│  [                                                   │
│    Token(Question, "question", 1:1),                │
│    Token(String("..."), "\"...\"", 1:10),          │
│    Token(EOF, "", 1:42)                             │
│  ]                                                   │
└──────────────────┬──────────────────────────────────┘
                   │
                   ↓
              (Next: Parser)
```

## Help & Support

- Read the full documentation: `LEXER_README.md`
- View architecture diagrams: `FERMI_BROCA_ARCHITECTURE.md`
- See the DSL grammar: `DSL_GRAMMAR.md`
- Example code: `examples/amd_forecast.fpl`

## Summary

You now have a fully functional FPL lexer that can:
- ✅ Tokenize complete FPL programs
- ✅ Handle all language constructs
- ✅ Provide detailed error messages
- ✅ Work interactively (REPL) or batch (files)
- ✅ Track source positions for debugging

The lexer is the foundation for the rest of Fermi's "Broca brain". Next, we'll build the parser to create Abstract Syntax Trees from these tokens.

Happy forecasting! 🚀
