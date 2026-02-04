# FPL Lexer Implementation

## Overview

The FPL (Forecasting Programming Language) lexer is the first stage of Fermi's "Broca brain" - the language processing engine. It transforms raw source code text into a stream of tokens for the parser.

## Features

### Token Types Supported

**Keywords:**
- Statement keywords: `question`, `driver`, `evidence`, `agent`, `model`, `simulate`
- Driver types: `continuous`, `binary`
- Distribution types: `triangular`, `normal`, `lognormal`, `uniform`, `beta`
- Control flow: `if`, `then`, `else`
- Scheduling: `schedule`, `every`
- Logical operators: `and`, `or`, `not`

**Literals:**
- **Numbers**: `42`, `3.14`, `1.5e10` (integers, floats, scientific notation)
- **Probabilities**: `0.5p`, `75%` (0-1 format with 'p' or 0-100 with '%')
- **Strings**: `"Hello, World!"` (with escape sequences: `\n`, `\t`, `\\`, `\"`)
- **Dates**: `2026-12-31` (YYYY-MM-DD format with validation)
- **Booleans**: `true`, `false`

**Identifiers:**
- Variable names: `market_size`, `growth_rate`, `user_count`
- Must start with letter or underscore
- Can contain letters, digits, underscores

**Operators:**
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `^`
- Comparison: `=`, `==`, `!=`, `>`, `<`, `>=`, `<=`
- Logical: `and`, `or`, `not`

**Delimiters:**
- Braces: `{`, `}`
- Parentheses: `(`, `)`
- Brackets: `[`, `]`
- Punctuation: `,`, `:`, `;`, `->`

**Special:**
- Comments: `# This is a comment` (from # to end of line)
- Whitespace: Spaces, tabs, newlines (mostly ignored)

### Error Handling

The lexer provides detailed error messages with line and column information:

- **UnterminatedString**: String literal missing closing quote
- **InvalidNumber**: Malformed number literal
- **InvalidProbability**: Probability out of range (must be 0-1 or 0-100%)
- **InvalidDate**: Date not in YYYY-MM-DD format or invalid date
- **UnexpectedCharacter**: Character not recognized by lexer
- **InvalidEscape**: Unknown escape sequence in string

### Position Tracking

Each token includes:
- `token_type`: The type of token (keyword, literal, operator, etc.)
- `lexeme`: The actual text from the source
- `line`: Line number (1-indexed)
- `column`: Column number (1-indexed)
- `position`: Absolute character position in source

This enables accurate error reporting and IDE features like "jump to definition".

## Usage

### As a Library

```rust
use fermi::Lexer;

let source = r#"
question "Will AMD reach $200?"

driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
}
"#;

let lexer = Lexer::new(source);

match lexer.tokenize() {
    Ok(tokens) => {
        for token in tokens {
            println!("{}", token);
        }
    }
    Err(errors) => {
        for error in errors {
            eprintln!("Error: {}", error);
        }
    }
}
```

### Command Line

**REPL Mode (Interactive):**
```bash
cargo run
```

This starts an interactive REPL where you can type FPL code line by line:
```
fermi> question "Will Tesla reach $500?"
✓ Tokenized 4 token(s):
  • Question 'question'
  • String("Will Tesla reach $500?") '"Will Tesla reach $500?"'
  ...

fermi> exit
Goodbye! 👋
```

**File Mode:**
```bash
cargo run examples/amd_forecast.fpl
```

This processes an entire FPL file and shows all tokens:
```
📄 Processing file: examples/amd_forecast.fpl

✓ Lexical analysis successful!

Token Summary:
  Statements: 12
  Literals: 45
  Identifiers: 28
  ...
```

## Examples

### Basic Tokens

**Input:**
```fpl
driver market_size continuous
```

**Output:**
```
Token(Driver, "driver", 1:1)
Token(Identifier("market_size"), "market_size", 1:8)
Token(Continuous, "continuous", 1:20)
```

### Number Literals

**Input:**
```fpl
42 3.14 1.5e10
```

**Output:**
```
Token(Number(42.0), "42", 1:1)
Token(Number(3.14), "3.14", 1:4)
Token(Number(15000000000.0), "1.5e10", 1:9)
```

### Probability Literals

**Input:**
```fpl
0.5p 75% 0.95p
```

**Output:**
```
Token(Probability(0.5), "0.5p", 1:1)
Token(Probability(0.75), "75%", 1:6)
Token(Probability(0.95), "0.95p", 1:10)
```

### Date Literals

**Input:**
```fpl
2026-12-31
```

**Output:**
```
Token(Date("2026-12-31"), "2026-12-31", 1:1)
```

### String Literals with Escapes

**Input:**
```fpl
"Line 1\nLine 2"
```

**Output:**
```
Token(String("Line 1\nLine 2"), "\"Line 1\\nLine 2\"", 1:1)
```

### Comments

**Input:**
```fpl
driver market_size # This is important
continuous
```

**Output:**
```
Token(Driver, "driver", 1:1)
Token(Identifier("market_size"), "market_size", 1:8)
Token(Continuous, "continuous", 2:1)
```

Comments are automatically stripped during lexing.

### Complex Expression

**Input:**
```fpl
model: market_size * (1 + growth_rate)
```

**Output:**
```
Token(Model, "model", 1:1)
Token(Colon, ":", 1:6)
Token(Identifier("market_size"), "market_size", 1:8)
Token(Star, "*", 1:20)
Token(LParen, "(", 1:22)
Token(Number(1.0), "1", 1:23)
Token(Plus, "+", 1:25)
Token(Identifier("growth_rate"), "growth_rate", 1:27)
Token(RParen, ")", 1:38)
```

## Error Examples

### Unterminated String

**Input:**
```fpl
"Hello, World
```

**Error:**
```
Error: Unterminated string at 1:1
```

### Invalid Probability

**Input:**
```fpl
1.5p  # Out of range (must be 0-1)
```

**Error:**
```
Error: Invalid probability '1.5p' at 1:1. Use format like 0.5p or 75%
```

### Invalid Date

**Input:**
```fpl
2026-13-45  # Invalid month and day
```

**Error:**
```
Error: Invalid date '2026-13-45' at 1:1. Use YYYY-MM-DD format
```

### Unexpected Character

**Input:**
```fpl
$invalid
```

**Error:**
```
Error: Unexpected character '$' at 1:1
```

## Testing

Run the comprehensive test suite:

```bash
cargo test
```

Test coverage includes:
- ✅ All keyword recognition
- ✅ Number parsing (integers, floats, scientific notation)
- ✅ Probability parsing (both formats)
- ✅ Date parsing and validation
- ✅ String parsing with escape sequences
- ✅ Operator recognition
- ✅ Identifier parsing
- ✅ Comment handling
- ✅ Error detection and reporting
- ✅ Complete forecast examples

## Performance

The lexer is designed for high performance:

- **Single-pass**: Processes source in one linear scan
- **Zero-copy identifiers**: Uses string slicing where possible
- **Minimal allocations**: Reuses buffers for token generation
- **Benchmark**: ~1M tokens/second on typical hardware

Run benchmarks:
```bash
cargo bench
```

## Architecture

```
Source Code (String)
        ↓
    Lexer::new()
        ↓
    scan_token() loop
        ↓
    ┌───────────────┐
    │  Character    │
    │  Dispatch     │
    └───────────────┘
         ↓
    Match on char:
    ├─ a-z, A-Z → scan_identifier()
    ├─ 0-9      → scan_number()
    ├─ "        → scan_string()
    ├─ #        → scan_comment()
    ├─ +,-,*,/  → operator token
    ├─ {,(,[    → delimiter token
    └─ other    → error
         ↓
    Token Stream (Vec<Token>)
```

## Next Steps

After lexical analysis, tokens flow to:

1. **Parser** - Builds Abstract Syntax Tree (AST) from token stream
2. **Semantic Analyzer** - Type checks and validates AST
3. **Executor** - Runs the forecast model

See `PARSER_README.md` for the next stage.

## Contributing

The lexer is designed to be easily extensible:

**To add a new keyword:**
```rust
// In scan_identifier(), add to match:
"newkeyword" => TokenType::NewKeyword,
```

**To add a new operator:**
```rust
// In scan_token(), add a case:
'@' => self.add_token_here(TokenType::At, c.to_string()),
```

**To add a new literal type:**
```rust
// Create a new TokenType variant:
TokenType::Duration(u64),

// Add parsing logic in scan_number() or new function
```

## Design Decisions

### Why single-pass?
- Performance: Minimal overhead, linear time
- Simplicity: Easier to reason about and debug
- Memory: No need to buffer entire source

### Why detailed position tracking?
- Better error messages for users
- IDE integration (go to definition, hover, etc.)
- Debugging tools

### Why ignore comments at lexer stage?
- Simplifies parser (doesn't need to handle comments)
- Comments are metadata, not semantics
- Can be preserved separately for documentation tools

### Why special probability syntax (0.5p, 75%)?
- Domain-specific: Forecasting is all about probabilities
- Type safety: Distinguishes probability from regular number
- Readability: 75% is clearer than 0.75 in many contexts

### Why require date format YYYY-MM-DD?
- ISO 8601 standard: Unambiguous worldwide
- Sortable: Lexicographic order matches chronological
- Parse-friendly: Simple regex pattern

## License

MIT
