#!/bin/bash
set -e

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Fermi Extension Installation ===${NC}\n"

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
EXTENSION_DIR="$PROJECT_ROOT/extensions/fermi"
ZED_EXTENSIONS_DIR="$HOME/.local/share/zed/extensions/installed"

# Version tracking (use timestamp + git commit)
VERSION=$(date +%Y%m%d-%H%M%S)
if command -v git &> /dev/null && [ -d "$PROJECT_ROOT/.git" ]; then
    GIT_HASH=$(git -C "$PROJECT_ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")
    VERSION="$VERSION-$GIT_HASH"
fi

echo -e "${YELLOW}Step 1: Building tree-sitter grammar...${NC}"
cd "$EXTENSION_DIR/grammars/fpl"
./node_modules/.bin/tree-sitter generate
./node_modules/.bin/tree-sitter build --wasm
cp tree-sitter-fpl.wasm "$EXTENSION_DIR/grammars/fpl.wasm"
echo -e "${GREEN}✓ Grammar built: $(ls -lh "$EXTENSION_DIR/grammars/fpl.wasm" | awk '{print $5}')${NC}\n"

echo -e "${YELLOW}Step 2: Building extension WASM...${NC}"
cd "$EXTENSION_DIR"
cargo build --target wasm32-wasip1 --release --quiet
cp target/wasm32-wasip1/release/fermi_extension.wasm extension.wasm
echo -e "${GREEN}✓ Extension built: $(ls -lh "$EXTENSION_DIR/extension.wasm" | awk '{print $5}')${NC}\n"

echo -e "${YELLOW}Step 3: Building LSP server...${NC}"
cd "$PROJECT_ROOT/fermi-lsp"
cargo build --release --quiet
echo -e "${GREEN}✓ LSP server built${NC}\n"

echo -e "${YELLOW}Step 4: Syncing highlights to languages directory...${NC}"
# Zed looks for highlights.scm in languages/fpl/ not grammars/fpl/queries/
mkdir -p "$EXTENSION_DIR/languages/fpl"
cp "$EXTENSION_DIR/grammars/fpl/queries/highlights.scm" "$EXTENSION_DIR/languages/fpl/highlights.scm"
echo -e "${GREEN}✓ Highlights synced${NC}\n"

echo -e "${YELLOW}Step 5: Writing version info...${NC}"
cat > "$EXTENSION_DIR/.version" <<EOF
version=$VERSION
built=$(date -Iseconds)
grammar_size=$(stat -c%s "$EXTENSION_DIR/grammars/fpl.wasm")
extension_size=$(stat -c%s "$EXTENSION_DIR/extension.wasm")
lsp_size=$(stat -c%s "$PROJECT_ROOT/fermi-lsp/target/release/fermi-lsp")
EOF
echo -e "${GREEN}✓ Version: $VERSION${NC}\n"

echo -e "${YELLOW}Step 6: Installing to Zed...${NC}"
mkdir -p "$ZED_EXTENSIONS_DIR"

# Remove old symlink/directory if exists
if [ -e "$ZED_EXTENSIONS_DIR/fermi" ] || [ -L "$ZED_EXTENSIONS_DIR/fermi" ]; then
    rm -rf "$ZED_EXTENSIONS_DIR/fermi"
    echo -e "${GREEN}✓ Removed old installation${NC}"
fi

# Create symlink
ln -s "$EXTENSION_DIR" "$ZED_EXTENSIONS_DIR/fermi"
echo -e "${GREEN}✓ Symlinked extension${NC}\n"

echo -e "${GREEN}=== Installation Complete ===${NC}\n"
echo -e "Extension directory: ${YELLOW}$EXTENSION_DIR${NC}"
echo -e "Zed extensions: ${YELLOW}$ZED_EXTENSIONS_DIR${NC}"
echo -e "Version: ${YELLOW}$VERSION${NC}\n"

echo -e "${YELLOW}Next steps:${NC}"
echo -e "1. Restart Zed or run: ${YELLOW}Cmd/Ctrl+Shift+P → 'zed: reload extensions'${NC}"
echo -e "2. Open a .fpl file to test"
echo -e "3. Run ${YELLOW}./scripts/verify-extension.sh${NC} to check installation\n"
