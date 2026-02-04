# Fermi Extension for Zed

Adds support for the Fermi Forecasting Programming Language (FPL) to Zed editor.

## Features

- ✅ **Syntax Highlighting** - Color-coded FPL syntax
- ✅ **LSP Integration** - Real-time diagnostics via fermi-lsp
- ✅ **Auto-indentation** - Smart indentation for FPL code
- ✅ **Bracket Matching** - Automatic bracket pairing
- 🚧 **Sparklines** - Inline distribution visualizations (coming soon)
- 🚧 **Execute Commands** - Run forecasts from editor (coming soon)
- 🚧 **Results Panel** - View forecast results (coming soon)

## Installation

### From Zed Extension Gallery (Future)
1. Open Zed
2. Go to Extensions (Cmd+Shift+X)
3. Search for "Fermi"
4. Click Install

### Manual Installation (Current)
1. Clone the Fermi repository:
   ```bash
   git clone https://github.com/Replicant-Partners/fermi.git
   cd fermi
   ```

2. Build the LSP server:
   ```bash
   cd fermi-lsp
   cargo build --release
   ```

3. Link the extension:
   ```bash
   ln -s $(pwd)/extensions/fermi ~/.config/zed/extensions/fermi
   ```

4. Configure LSP in Zed settings:
   ```json
   {
     "lsp": {
       "fermi-lsp": {
         "binary": {
           "path": "/path/to/fermi/target/release/fermi-lsp"
         }
       }
     }
   }
   ```

5. Restart Zed

## Usage

### Create a Forecast

Create a new file with `.fpl` extension:

```fpl
forecast "AMD Q4 2024 Revenue" {
    // Market drivers
    driver gpu_market triangular(20000, 32000, 50000)
    driver market_share normal(0.15, 0.05)
    driver avg_price triangular(800, 1200, 2000)
    
    // Calculate revenue
    estimate gpu_market * market_share * avg_price
}
```

### Execute Forecast (Coming Soon)

Press `Cmd+Enter` to execute the forecast and see results.

## Configuration

Configure the Fermi extension in your Zed settings:

```json
{
  "fermi": {
    "lsp": {
      "enabled": true,
      "diagnostics": true
    },
    "sparklines": {
      "enabled": true,
      "width": 7
    },
    "coaching": {
      "enabled": true,
      "verbosity": "adaptive"
    }
  }
}
```

## Language Server Features

The extension connects to fermi-lsp which provides:

- **Diagnostics** - Real-time error detection
  - Lexical errors (unexpected characters)
  - Syntax errors (malformed statements)
  - Semantic errors (undefined variables, type mismatches)

- **Hover Info** (Coming Soon)
  - Distribution details
  - Variable values
  - Type information

- **Autocompletion** (Coming Soon)
  - Driver names
  - Function names
  - Keywords

- **Code Actions** (Coming Soon)
  - Quick fixes
  - Refactorings

## Architecture

```
┌─────────────────────────────────────────┐
│           Zed Editor                    │
├─────────────────────────────────────────┤
│                                         │
│  Extension (fermi)                     │
│  ├── Syntax Highlighting (tree-sitter) │
│  ├── Language Config                   │
│  └── LSP Client                        │
│         ↓                               │
│  fermi-lsp (tower-lsp)                 │
│  ├── Lexer                             │
│  ├── Parser                            │
│  ├── Semantic Analyzer                 │
│  └── Diagnostics                       │
│                                         │
└─────────────────────────────────────────┘
```

## Development

### Building

```bash
# Build LSP server
cd fermi-lsp
cargo build --release

# Generate tree-sitter parser
cd tree-sitter-fpl
npm install
npm run build
```

### Testing

```bash
# Test LSP
cd fermi-lsp
cargo test

# Test grammar
cd tree-sitter-fpl
npm test
```

### Debugging

Enable LSP logging in Zed:

```json
{
  "lsp": {
    "fermi-lsp": {
      "settings": {
        "RUST_LOG": "debug"
      }
    }
  }
}
```

View logs: `View → Debug → Language Server Logs`

## Architecture Decisions

This extension aligns with:
- **ADR-001:** Architecture Option C (Standalone LSP)
- **ADR-003:** Hybrid Fermi Coaching Integration
- **ADR-006:** Tree-sitter Grammar Generation
- **ADR-008:** Multi-Method Execute Command UX
- **ADR-009:** Right Sidebar Results Panel
- **ADR-010:** Rowan for Lossless Syntax Trees

## Roadmap

### Phase 1: Core Experience ✅
- [x] Syntax highlighting
- [x] LSP integration
- [x] Basic diagnostics

### Phase 2: Enhanced Editing (Current)
- [ ] Hover information
- [ ] Autocompletion
- [ ] Go to definition
- [ ] Code actions

### Phase 3: Execution
- [ ] Execute command (Cmd+Enter)
- [ ] Results panel
- [ ] Sparkline inlay hints

### Phase 4: Agent Integration
- [ ] Agent bestiary panel
- [ ] Agent coordination
- [ ] Manual review UI

## Support

- **Issues:** https://github.com/Replicant-Partners/fermi/issues
- **Documentation:** See docs/ directory
- **Contact:** Replicant Partners team

## License

[TBD]
