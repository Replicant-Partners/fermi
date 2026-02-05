#!/bin/bash
# Update and Reload Extension - One-command workflow
# Builds everything, clears caches, reinstalls extension, and prompts to restart Zed
#
# Usage: ./scripts/update-and-reload.sh

set -e

# Color output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo -e "${BLUE}╔════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║  Fermi: Update and Reload Extension              ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════╝${NC}"
echo ""

# Step 1: Build core fermi
echo -e "${YELLOW}[1/5] Building fermi...${NC}"
cd "$PROJECT_ROOT"
cargo build --release --quiet
echo -e "${GREEN}✓ Fermi built${NC}\n"

# Step 2: Build fermi-lsp
echo -e "${YELLOW}[2/5] Building fermi-lsp...${NC}"
cd "$PROJECT_ROOT/fermi-lsp"
cargo build --release --quiet
echo -e "${GREEN}✓ LSP built${NC}\n"

# Step 3: Clear ALL Zed caches (critical - from SYNTAX_HIGHLIGHTING_FIX.md)
echo -e "${YELLOW}[3/5] Clearing ALL Zed caches...${NC}"
echo -e "  ${BLUE}This is necessary because 'reload extensions' doesn't clear caches${NC}"

CLEARED=0
if [ -d ~/.local/share/zed/extensions/installed/fermi ]; then
    rm -rf ~/.local/share/zed/extensions/installed/fermi
    echo -e "${GREEN}  ✓ Removed ~/.local/share/zed/extensions/installed/fermi${NC}"
    CLEARED=1
fi

if [ -d ~/.config/zed/extensions/fermi ]; then
    rm -rf ~/.config/zed/extensions/fermi
    echo -e "${GREEN}  ✓ Removed ~/.config/zed/extensions/fermi${NC}"
    CLEARED=1
fi

if [ -d ~/.cache/zed ]; then
    rm -rf ~/.cache/zed/*
    echo -e "${GREEN}  ✓ Cleared ~/.cache/zed/*${NC}"
    CLEARED=1
fi

if [ $CLEARED -eq 0 ]; then
    echo -e "${GREEN}  ✓ No cached files found${NC}"
fi
echo ""

# Step 4: Reinstall extension
echo -e "${YELLOW}[4/5] Reinstalling extension...${NC}"
cd "$PROJECT_ROOT"
bash "$SCRIPT_DIR/install-extension.sh" 2>&1 | grep -E "(✓|Error|Version:)" || true
echo ""

# Step 5: Check if Zed is running
echo -e "${YELLOW}[5/5] Checking Zed status...${NC}"
if pgrep -x "zed" > /dev/null; then
    echo -e "${RED}⚠ Zed is currently running${NC}"
    echo ""
    echo -e "${YELLOW}You need to restart Zed for changes to take effect:${NC}"
    echo -e "  1. Close Zed completely (Cmd/Ctrl+Q)"
    echo -e "  2. Reopen Zed"
    echo -e "  3. Open a .fpl file"
    echo ""
    read -p "$(echo -e ${YELLOW}Do you want to kill Zed now? [y/N]: ${NC})" -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        pkill -x zed
        echo -e "${GREEN}✓ Zed closed. Please reopen it manually.${NC}"
    else
        echo -e "${YELLOW}Please close and reopen Zed manually.${NC}"
    fi
else
    echo -e "${GREEN}✓ Zed is not running${NC}"
    echo -e "${YELLOW}You can now start Zed and open a .fpl file${NC}"
fi

echo ""
echo -e "${BLUE}════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✓ Update complete!${NC}"
echo ""
echo -e "${YELLOW}Test hover by:${NC}"
echo -e "  - Hovering over 'base_rate', 'strength', 'date'"
echo -e "  - Typing 'base' and checking autocomplete"
echo -e "  - Inside evidence blocks, typing 'str' for 'strength'"
echo ""
echo -e "${BLUE}Version: $(cat $PROJECT_ROOT/extensions/fermi/.version | grep version | cut -d= -f2)${NC}"
echo -e "${BLUE}════════════════════════════════════════════════════${NC}"
