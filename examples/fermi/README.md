# Fermi Zed Extension

Zed editor extension for the Fermi Forecasting Programming Language (FPL).

## Features

- **Syntax Highlighting**: Tree-sitter based syntax highlighting for `.fpl` files
  - Keywords: `question`, `driver`, `evidence`, `agent`, `model`, `simulate`
  - Driver types: `continuous`, `binary`, `discrete`
  - Properties and functions with semantic highlighting
  
- **LSP Support**: Language server providing:
  - Real-time diagnostics and error checking
  - Intelligent autocomplete for keywords, types, and properties
  - Hover documentation for distributions and functions
  - Snippet expansion for common patterns

- **Slash Commands**:
  - `/run-forecast`: Execute the current FPL forecast

## Installation

### Quick Install

From the project root:

```bash
./scripts/install-extension.sh
```

This will:
1. Build the tree-sitter grammar
2. Build the extension WASM module
3. Build the LSP server
4. Create version tracking
5. Install to Zed via symlink

### Verify Installation

```bash
./scripts/verify-extension.sh
```

Checks that all components are properly installed and up-to-date.

### Manual Install

If you need to install manually:

```bash
# Build grammar
cd extensions/fermi/grammars/fpl
npm install
./node_modules/.bin/tree-sitter generate
./node_modules/.bin/tree-sitter build --wasm
cp tree-sitter-fpl.wasm ../../grammars/fpl.wasm

# Build extension
cd ../..
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/fermi_extension.wasm extension.wasm

# Build LSP
cd ../../fermi-lsp
cargo build --release

# Link to Zed
ln -sf $(pwd)/../extensions/fermi ~/.config/zed/extensions/fermi
```

## After Installation

1. **Restart Zed** or reload extensions:
   - Open command palette: `Cmd/Ctrl+Shift+P`
   - Run: `zed: reload extensions`

2. **Test the extension**:
   - Open or create a `.fpl` file
   - Type `question` - should show autocomplete
   - Type `driver test continuous` - `continuous` should be highlighted
   - Inside a driver block, type `distribution:` - should autocomplete

## Development

### Directory Structure

```
extensions/fermi/
├── extension.toml           # Extension manifest
├── extension.wasm          # Extension WASM binary
├── Cargo.toml              # Rust build config
├── src/                    # Extension Rust source
│   └── lib.rs
├── grammars/
│   ├── fpl.wasm           # Compiled tree-sitter grammar
│   └── fpl/
│       ├── grammar.js     # Tree-sitter grammar definition
│       ├── queries/
│       │   └── highlights.scm  # Syntax highlighting queries
│       └── ...
└── languages/
    └── fpl/
        └── config.toml    # Language configuration
```

### Rebuilding After Changes

**Grammar changes** (grammar.js or highlights.scm):
```bash
cd extensions/fermi/grammars/fpl
./node_modules/.bin/tree-sitter generate
./node_modules/.bin/tree-sitter build --wasm
cp tree-sitter-fpl.wasm ../../grammars/fpl.wasm
```

**Extension changes** (src/lib.rs, extension.toml):
```bash
cd extensions/fermi
cargo build --target wasm32-wasip1 --release
cp target/wasm32-wasip1/release/fermi_extension.wasm extension.wasm
```

**LSP changes** (fermi-lsp/src/main.rs):
```bash
cd fermi-lsp
cargo build --release
```

Then reload Zed extensions.

### Using the Scripts

The installation scripts provide version tracking:

```bash
# Full rebuild and install
./scripts/install-extension.sh

# Verify current installation
./scripts/verify-extension.sh
```

Version info is stored in `extensions/fermi/.version` with timestamps and build sizes.

## Troubleshooting

### Syntax highlighting not working

1. Verify grammar is built:
   ```bash
   ls -lh extensions/fermi/grammars/fpl.wasm
   ```
   Should be ~20-30KB

2. Check highlights file exists:
   ```bash
   cat extensions/fermi/grammars/fpl/queries/highlights.scm
   ```

3. Rebuild grammar and reload Zed

### Autocomplete not working

1. Check LSP server is running:
   - Look for `fermi-lsp` process: `ps aux | grep fermi-lsp`
   
2. Check Zed's language server logs:
   - Open command palette: `zed: open log`
   - Look for fermi-lsp errors

3. Rebuild LSP server:
   ```bash
   cd fermi-lsp && cargo build --release
   ```

### Extension not loading

1. Run verification:
   ```bash
   ./scripts/verify-extension.sh
   ```

2. Check symlink:
   ```bash
   ls -la ~/.config/zed/extensions/fermi
   ```
   Should point to project directory

3. Reinstall:
   ```bash
   ./scripts/install-extension.sh
   ```

4. Check Zed extension logs for errors

## Version Information

Run `./scripts/verify-extension.sh` to see:
- Build timestamp
- Git commit hash
- File sizes
- Feature checklist

The `.version` file tracks each build for debugging.
