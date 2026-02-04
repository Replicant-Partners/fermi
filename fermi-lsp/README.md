# Fermi Language Server (LSP)

Language Server Protocol implementation for the Fermi Forecasting Programming Language (FPL).

## Features

- ✅ **Diagnostics** - Real-time syntax and semantic error detection
- 🚧 **Hover Info** - Show distribution details on hover
- 🚧 **Autocompletion** - Driver names, functions, keywords
- 🚧 **Go to Definition** - Navigate to driver definitions
- 🚧 **Code Actions** - Quick fixes and refactorings

## Architecture

```
┌─────────────────────────────────────────┐
│          Fermi Language Server          │
├─────────────────────────────────────────┤
│                                         │
│  Tower-LSP (JSON-RPC protocol)         │
│         ↓                               │
│  Lexer → Parser → Semantic Analyzer    │
│         ↓                               │
│  Rowan (lossless syntax tree)          │
│         ↓                               │
│  LSP Features:                          │
│  - Diagnostics (errors, warnings)      │
│  - Hover (show types, values)          │
│  - Completion (suggest names)          │
│  - Actions (quick fixes)               │
│                                         │
└─────────────────────────────────────────┘
```

## Installation

### From Source
```bash
cd fermi-lsp
cargo build --release
cargo install --path .
```

### Binary Location
```
target/release/fermi-lsp
```

## Usage

### Standalone
```bash
fermi-lsp
```

The server communicates via stdio using JSON-RPC.

### With Zed
See the `zed-fermi-lsp` extension for integration instructions.

### With VS Code
See the `vscode-fermi` extension for integration instructions.

## Configuration

The LSP server can be configured via initialization options:

```json
{
  "fermi": {
    "diagnostics": {
      "enabled": true,
      "debounce_ms": 500
    },
    "coaching": {
      "enabled": true,
      "verbosity": "adaptive"
    }
  }
}
```

## Development

### Running Tests
```bash
cargo test
```

### Debugging
Set `RUST_LOG=debug` to see detailed logs:

```bash
RUST_LOG=debug fermi-lsp
```

### Testing with a Client
```bash
# Terminal 1: Start LSP
RUST_LOG=debug fermi-lsp

# Terminal 2: Send LSP requests
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | fermi-lsp
```

## LSP Capabilities

### Currently Implemented
- ✅ `textDocument/didOpen`
- ✅ `textDocument/didChange`
- ✅ `textDocument/didSave`
- ✅ `textDocument/didClose`
- ✅ `textDocument/publishDiagnostics`

### Planned
- 🚧 `textDocument/hover`
- 🚧 `textDocument/completion`
- 🚧 `textDocument/definition`
- 🚧 `textDocument/codeAction`
- 🚧 `textDocument/formatting`
- 🚧 `textDocument/inlayHint` (for sparklines)

## Error Codes

| Code | Description |
|------|-------------|
| E001 | Lexical error (unexpected character) |
| E002 | Syntax error (parse failure) |
| E003 | Semantic error (type mismatch, undefined variable) |
| W001 | Warning (unused driver) |
| I001 | Coaching suggestion (Fermi coaching) |

## Architecture Decisions

- **ADR-001:** Architecture Option C (standalone LSP)
- **ADR-003:** Hybrid Fermi Coaching (diagnostics + custom extension)
- **ADR-004:** Adaptive Coaching Verbosity
- **ADR-010:** Rowan for lossless syntax trees

## Dependencies

- `tower-lsp` - LSP framework
- `rowan` - Lossless syntax tree
- `tokio` - Async runtime
- `fermi` - Core FPL library

## License

[TBD]

## Contributing

See [CONTRIBUTING.md](../CONTRIBUTING.md) for development guidelines.
