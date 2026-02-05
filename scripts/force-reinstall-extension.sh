#!/bin/bash
# Force reinstall extension - clears ALL caches like the syntax highlighting fix
# Based on SYNTAX_HIGHLIGHTING_FIX.md solution that actually worked

set -e

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${RED}=== Force Reinstall Extension (Clear ALL Caches) ===${NC}\n"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo -e "${YELLOW}Step 1: Clearing ALL Zed caches...${NC}"
echo "This is necessary because 'reload extensions' doesn't clear caches."
echo ""

# Remove all cached extension files (from SYNTAX_HIGHLIGHTING_FIX.md)
if [ -d ~/.local/share/zed/extensions/installed/fermi ]; then
    rm -rf ~/.local/share/zed/extensions/installed/fermi
    echo -e "${GREEN}✓ Removed ~/.local/share/zed/extensions/installed/fermi${NC}"
fi

if [ -d ~/.config/zed/extensions/fermi ]; then
    rm -rf ~/.config/zed/extensions/fermi
    echo -e "${GREEN}✓ Removed ~/.config/zed/extensions/fermi${NC}"
fi

if [ -d ~/.cache/zed ]; then
    rm -rf ~/.cache/zed/*
    echo -e "${GREEN}✓ Cleared ~/.cache/zed/*${NC}"
fi

echo ""
echo -e "${YELLOW}Step 2: Reinstalling extension cleanly...${NC}"
bash "$PROJECT_ROOT/scripts/install-extension.sh"

echo ""
echo -e "${RED}=== IMPORTANT: Next Steps ===${NC}"
echo -e "${YELLOW}1. CLOSE Zed completely (don't just reload extensions)${NC}"
echo -e "${YELLOW}2. Reopen Zed${NC}"
echo -e "${YELLOW}3. Open a .fpl file${NC}"
echo -e "${YELLOW}4. Test hover on keywords like 'base_rate', 'strength'${NC}"
echo ""
echo -e "${RED}NOTE: 'Reload extensions' does NOT work - you must fully restart Zed!${NC}"
echo ""
