# Fermi Quick Start Guide

Get up and running with Fermi in Zed editor in 5 minutes.

## Prerequisites

- **Rust:** Install from https://rustup.rs
- **Node.js:** Install from https://nodejs.org (v18+)
- **Zed Editor:** Install from https://zed.dev

## Installation

### 1. Clone Repository
```bash
git clone https://github.com/Replicant-Partners/fermi.git
cd fermi
```

### 2. Run Installation Script
```bash
./install-zed-extension.sh
```

This script will:
- Build the tree-sitter parser
- Compile the LSP server
- Link the Zed extension
- Configure Zed settings

### 3. Restart Zed
```bash
killall zed && zed
```

## Quick Test

### 1. Create a Test File
```bash
zed examples/test.fpl
```

### 2. Start Typing
```fpl
forecast "My First Forecast" {
    driver revenue triangular(100, 200, 500)
    estimate revenue
}
```

### 3. Verify Features

**Syntax Highlighting:** Keywords, numbers, strings should be color-coded

**Diagnostics:** Try making an error:
```fpl
driver x unknown_distribution(1, 2, 3)
```
You should see a red squiggly line with error message.

**Auto-indentation:** Press Enter after `{` and it should auto-indent

**Bracket Matching:** Type `(` and it should auto-close with `)`

## Features

### Syntax Highlighting
- ✅ Keywords (forecast, driver, estimate)
- ✅ Distributions (triangular, normal, etc.)
- ✅ Operators (+, -, *, /, ^)
- ✅ Numbers and strings
- ✅ Comments (// and /* */)

### Real-time Diagnostics
- ✅ Lexical errors (unexpected characters)
- ✅ Syntax errors (malformed statements)
- ✅ Semantic errors (undefined variables, type mismatches)

### Smart Editing
- ✅ Auto-indentation
- ✅ Bracket matching and auto-closing
- ✅ Comment toggling (Cmd+/)

## Troubleshooting

### Extension Not Working

**Check extension is linked:**
```bash
ls -la ~/.config/zed/extensions/fermi
```

**Check LSP binary exists:**
```bash
ls -la fermi-lsp/target/release/fermi-lsp
```

**View LSP logs:**
In Zed: `View → Debug → Language Server Logs`

### Syntax Highlighting Missing

**Rebuild tree-sitter parser:**
```bash
cd tree-sitter-fpl
npm run build
```

**Check grammar compiled:**
```bash
ls -la src/parser.c
```

### No Diagnostics

**Enable debug logging:**
Edit `~/.config/zed/settings.json`:
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

**Restart Zed and check logs**

### Common Errors

**"command not found: cargo"**
- Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

**"command not found: npm"**
- Install Node.js from https://nodejs.org

**"tree-sitter: command not found"**
- Run `npm install -g tree-sitter-cli`

## Examples

### Basic Forecast
```fpl
forecast "Revenue Estimate" {
    driver sales triangular(1000, 2000, 5000)
    driver margin normal(0.3, 0.05)
    estimate sales * margin
}
```

### Multiple Drivers
```fpl
forecast "Market Sizing" {
    driver tam triangular(10000000, 50000000, 100000000)
    driver penetration beta(2, 5, 0, 0.5)
    driver revenue_per_customer normal(100, 20)
    
    estimate tam * penetration * revenue_per_customer
}
```

### Complex Expression
```fpl
forecast "Profitability" {
    driver revenue triangular(100000, 200000, 500000)
    driver cogs_pct normal(0.4, 0.05)
    driver opex triangular(30000, 50000, 80000)
    
    estimate (revenue * (1 - cogs_pct)) - opex
}
```

## Next Steps

### Execute Forecasts (Coming Soon)
```fpl
// Press Cmd+Enter to execute
forecast "Q4 Revenue" {
    driver x triangular(100, 200, 300)
    estimate x
}
// Results will appear in right sidebar
```

### View Documentation
- [FPL Language Reference](DSL_GRAMMAR.md)
- [Extension README](extensions/fermi/README.md)
- [LSP Documentation](fermi-lsp/README.md)
- [Architecture Decisions](docs/DECISIONS.md)

### Join Development
- Report issues: https://github.com/Replicant-Partners/fermi/issues
- Read roadmap: [docs/ROADMAP.md](docs/ROADMAP.md)
- Check TODO: [docs/TODO.md](docs/TODO.md)

## Keyboard Shortcuts

| Action | Shortcut |
|--------|----------|
| Toggle comment | Cmd+/ |
| Format document | Cmd+Shift+F (coming soon) |
| Execute forecast | Cmd+Enter (coming soon) |
| Go to definition | Cmd+Click (coming soon) |

## Configuration

Edit `~/.config/zed/settings.json`:

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
  },
  "languages": {
    "FPL": {
      "tab_size": 4,
      "hard_tabs": false,
      "format_on_save": false
    }
  }
}
```

## Support

- **Documentation:** See `docs/` directory
- **Issues:** https://github.com/Replicant-Partners/fermi/issues
- **Discord:** [Coming Soon]
- **Email:** team@replicantpartners.com

---

**Happy Forecasting! 🎯📊**
