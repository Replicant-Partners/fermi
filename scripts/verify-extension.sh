#!/bin/bash

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== Fermi Extension Verification ===${NC}\n"

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
EXTENSION_DIR="$PROJECT_ROOT/extensions/fermi"
ZED_EXTENSIONS_DIR="$HOME/.config/zed/extensions"

ERRORS=0

# Check 1: Symlink exists
echo -e "${YELLOW}Checking Zed extension symlink...${NC}"
if [ -L "$ZED_EXTENSIONS_DIR/fermi" ]; then
    TARGET=$(readlink "$ZED_EXTENSIONS_DIR/fermi")
    if [ "$TARGET" = "$EXTENSION_DIR" ]; then
        echo -e "${GREEN}✓ Symlink correct: $ZED_EXTENSIONS_DIR/fermi → $EXTENSION_DIR${NC}\n"
    else
        echo -e "${RED}✗ Symlink points to wrong location: $TARGET${NC}\n"
        ERRORS=$((ERRORS + 1))
    fi
else
    echo -e "${RED}✗ Symlink not found at $ZED_EXTENSIONS_DIR/fermi${NC}\n"
    ERRORS=$((ERRORS + 1))
fi

# Check 2: Required files exist
echo -e "${YELLOW}Checking required files...${NC}"
FILES=(
    "$EXTENSION_DIR/extension.toml"
    "$EXTENSION_DIR/extension.wasm"
    "$EXTENSION_DIR/grammars/fpl.wasm"
    "$PROJECT_ROOT/fermi-lsp/target/release/fermi-lsp"
)

for FILE in "${FILES[@]}"; do
    if [ -f "$FILE" ]; then
        SIZE=$(stat -c%s "$FILE")
        SIZE_KB=$((SIZE / 1024))
        echo -e "${GREEN}✓ $(basename "$FILE"): ${SIZE_KB}KB${NC}"
    else
        echo -e "${RED}✗ Missing: $FILE${NC}"
        ERRORS=$((ERRORS + 1))
    fi
done
echo ""

# Check 3: Grammar queries
echo -e "${YELLOW}Checking tree-sitter queries...${NC}"
if [ -f "$EXTENSION_DIR/grammars/fpl/queries/highlights.scm" ]; then
    LINES=$(wc -l < "$EXTENSION_DIR/grammars/fpl/queries/highlights.scm")
    echo -e "${GREEN}✓ Syntax highlighting queries: $LINES lines${NC}\n"
else
    echo -e "${RED}✗ Missing: highlights.scm${NC}\n"
    ERRORS=$((ERRORS + 1))
fi

# Check 4: Version info
echo -e "${YELLOW}Checking version info...${NC}"
if [ -f "$EXTENSION_DIR/.version" ]; then
    echo -e "${GREEN}✓ Version file exists${NC}"
    cat "$EXTENSION_DIR/.version" | while read line; do
        echo -e "  ${YELLOW}$line${NC}"
    done
    echo ""
else
    echo -e "${YELLOW}⚠ No version file (run install-extension.sh)${NC}\n"
fi

# Check 5: Extension config
echo -e "${YELLOW}Checking extension.toml...${NC}"
if grep -q "continuous" "$EXTENSION_DIR/grammars/fpl/grammar.js" 2>/dev/null; then
    echo -e "${GREEN}✓ Grammar includes 'continuous' keyword${NC}"
else
    echo -e "${RED}✗ Grammar missing 'continuous' keyword${NC}"
    ERRORS=$((ERRORS + 1))
fi

if grep -q "continuous" "$EXTENSION_DIR/grammars/fpl/queries/highlights.scm" 2>/dev/null; then
    echo -e "${GREEN}✓ Syntax highlighting includes 'continuous'${NC}"
else
    echo -e "${RED}✗ Syntax highlighting missing 'continuous'${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# Check 6: LSP capabilities
echo -e "${YELLOW}Checking LSP completions...${NC}"
if grep -q "continuous" "$PROJECT_ROOT/fermi-lsp/src/main.rs"; then
    echo -e "${GREEN}✓ LSP includes 'continuous' completion${NC}"
else
    echo -e "${RED}✗ LSP missing 'continuous' completion${NC}"
    ERRORS=$((ERRORS + 1))
fi

if grep -q "CompletionItemKind::PROPERTY" "$PROJECT_ROOT/fermi-lsp/src/main.rs"; then
    echo -e "${GREEN}✓ LSP includes property completions${NC}"
else
    echo -e "${RED}✗ LSP missing property completions${NC}"
    ERRORS=$((ERRORS + 1))
fi
echo ""

# Summary
echo -e "${YELLOW}=== Summary ===${NC}\n"
if [ $ERRORS -eq 0 ]; then
    echo -e "${GREEN}✓ All checks passed!${NC}\n"
    echo -e "${YELLOW}To apply changes:${NC}"
    echo -e "1. Restart Zed or reload extensions: ${YELLOW}Cmd/Ctrl+Shift+P → 'zed: reload extensions'${NC}"
    echo -e "2. Open a .fpl file"
    echo -e "3. Test autocomplete: type 'driver test_name con' and press Tab"
    echo -e "4. Test syntax highlighting: 'continuous' should be highlighted\n"
    exit 0
else
    echo -e "${RED}✗ $ERRORS check(s) failed${NC}\n"
    echo -e "${YELLOW}Run: ./scripts/install-extension.sh${NC}\n"
    exit 1
fi
