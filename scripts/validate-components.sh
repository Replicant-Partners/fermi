#!/bin/bash
# Component Dependency Validation Script
# Validates consistency between lexer, parser, grammar, and LSP
#
# This script catches dependency chain breaks when:
# - New keywords are added to the lexer/parser but not the LSP
# - New properties are added to AST but not hover/completion
# - Grammar is updated but LSP isn't synced

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

echo "╔════════════════════════════════════════════════════════╗"
echo "║  FPL Component Dependency Validation                  ║"
echo "╚════════════════════════════════════════════════════════╝"
echo ""

ERRORS=0
WARNINGS=0

# Colors
RED='\033[0;31m'
YELLOW='\033[1;33m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

error() {
    echo -e "${RED}✗ ERROR:${NC} $1"
    ERRORS=$((ERRORS + 1))
}

warn() {
    echo -e "${YELLOW}⚠ WARNING:${NC} $1"
    WARNINGS=$((WARNINGS + 1))
}

success() {
    echo -e "${GREEN}✓${NC} $1"
}

info() {
    echo "  $1"
}

echo "1. Extracting keywords from lexer..."
echo "──────────────────────────────────────────────────────"

# Extract keywords from lexer TokenType enum
LEXER_KEYWORDS=$(grep -A 200 "pub enum TokenType" src/lexer.rs | \
    grep "^    [A-Z]" | \
    grep -v "//" | \
    grep -v "String\|Number\|Probability\|Date\|Boolean\|Identifier\|Plus\|Minus\|Star\|EOF\|Comment\|Whitespace\|Newline\|Arrow\|Semicolon\|Colon\|Comma\|LBrace\|RBrace\|LParen\|RParen\|LBracket\|RBracket\|Equals\|Greater\|Less\|Slash\|Percent\|Caret\|DoubleEquals\|NotEquals\|GreaterEqual\|LessEqual\|And\|Or\|Not" | \
    awk '{print tolower($1)}' | \
    tr -d ',' | \
    sort)

info "Found $(echo "$LEXER_KEYWORDS" | wc -l) keywords in lexer"

echo ""
echo "2. Checking LSP hover coverage..."
echo "──────────────────────────────────────────────────────"

# Check each lexer keyword has hover documentation
MISSING_HOVER=()
for keyword in $LEXER_KEYWORDS; do
    if ! grep -q "\"$keyword\"" fermi-lsp/src/hover/keywords.rs 2>/dev/null; then
        MISSING_HOVER+=("$keyword")
    fi
done

if [ ${#MISSING_HOVER[@]} -eq 0 ]; then
    success "All keywords have hover documentation"
else
    for keyword in "${MISSING_HOVER[@]}"; do
        error "Keyword '$keyword' missing from LSP hover (fermi-lsp/src/hover/keywords.rs)"
    done
fi

echo ""
echo "3. Checking LSP completion coverage..."
echo "──────────────────────────────────────────────────────"

# Check major keywords have completions
MAJOR_KEYWORDS="question driver model simulate evidence agent base_rate"
MISSING_COMPLETIONS=()
for keyword in $MAJOR_KEYWORDS; do
    if ! grep -q "\"$keyword\"" fermi-lsp/src/completions/keywords.rs 2>/dev/null; then
        MISSING_COMPLETIONS+=("$keyword")
    fi
done

if [ ${#MISSING_COMPLETIONS[@]} -eq 0 ]; then
    success "All major keywords have completions"
else
    for keyword in "${MISSING_COMPLETIONS[@]}"; do
        error "Keyword '$keyword' missing from LSP completions (fermi-lsp/src/completions/keywords.rs)"
    done
fi

echo ""
echo "4. Checking AST struct fields vs LSP properties..."
echo "──────────────────────────────────────────────────────"

# Extract EvidenceStmt fields from AST
EVIDENCE_FIELDS=$(grep -A 10 "pub struct EvidenceStmt" src/ast.rs | \
    grep "pub " | \
    awk '{print $2}' | \
    tr -d ':' | \
    grep -v "^$")

info "EvidenceStmt fields: $(echo $EVIDENCE_FIELDS | tr '\n' ' ')"

# Check if each field has hover documentation
MISSING_EVIDENCE_HOVER=()
for field in $EVIDENCE_FIELDS; do
    if ! grep -q "\"$field\"" fermi-lsp/src/hover/properties.rs 2>/dev/null; then
        MISSING_EVIDENCE_HOVER+=("$field")
    fi
done

if [ ${#MISSING_EVIDENCE_HOVER[@]} -eq 0 ]; then
    success "All EvidenceStmt fields have hover documentation"
else
    for field in "${MISSING_EVIDENCE_HOVER[@]}"; do
        warn "Evidence field '$field' missing from hover properties (may be internal)"
    done
fi

# Check DriverStmt fields
DRIVER_FIELDS=$(grep -A 20 "pub struct DriverStmt" src/ast.rs | \
    grep "pub " | \
    awk '{print $2}' | \
    tr -d ':' | \
    grep -v "^$")

info "DriverStmt fields: $(echo $DRIVER_FIELDS | tr '\n' ' ')"

echo ""
echo "5. Checking grammar sync with extension..."
echo "──────────────────────────────────────────────────────"

# Check if grammar files exist in extension
if [ -f "extensions/fermi/grammars/fpl/queries/highlights.scm" ]; then
    success "Grammar highlights.scm exists in extension"

    # Check for base_rate in highlights
    if grep -q "base_rate" extensions/fermi/grammars/fpl/queries/highlights.scm; then
        success "base_rate found in grammar highlights"
    else
        warn "base_rate not found in grammar highlights - may need grammar update"
    fi
else
    error "Grammar highlights.scm missing from extension"
fi

echo ""
echo "6. Checking build artifacts..."
echo "──────────────────────────────────────────────────────"

# Check if binaries are up to date
if [ -f "target/release/fermi" ]; then
    success "fermi binary exists"
else
    warn "fermi binary not found - run: cargo build --release"
fi

if [ -f "fermi-lsp/target/release/fermi-lsp" ]; then
    success "fermi-lsp binary exists"
else
    warn "fermi-lsp binary not found - run: cd fermi-lsp && cargo build --release"
fi

echo ""
echo "7. Checking extension installation..."
echo "──────────────────────────────────────────────────────"

if [ -d "$HOME/.config/zed/extensions/fermi" ]; then
    success "Extension installed in Zed"

    EXT_VERSION=$(cat "$HOME/.config/zed/extensions/fermi/.version" 2>/dev/null || echo "unknown")
    info "Extension version: $EXT_VERSION"
else
    error "Extension not installed - run: bash scripts/install-extension.sh"
fi

echo ""
echo "═══════════════════════════════════════════════════════"
echo "Summary:"
echo "───────────────────────────────────────────────────────"

if [ $ERRORS -eq 0 ] && [ $WARNINGS -eq 0 ]; then
    echo -e "${GREEN}✓ All validation checks passed!${NC}"
    exit 0
elif [ $ERRORS -eq 0 ]; then
    echo -e "${YELLOW}⚠ $WARNINGS warning(s) found${NC}"
    exit 0
else
    echo -e "${RED}✗ $ERRORS error(s) found${NC}"
    if [ $WARNINGS -gt 0 ]; then
        echo -e "${YELLOW}⚠ $WARNINGS warning(s) found${NC}"
    fi
    echo ""
    echo "Fix errors by:"
    echo "1. Adding missing hover documentation to fermi-lsp/src/hover/keywords.rs"
    echo "2. Adding missing completions to fermi-lsp/src/completions/keywords.rs"
    echo "3. Adding missing property hovers to fermi-lsp/src/hover/properties.rs"
    echo "4. Rebuilding: cargo build --release && cd fermi-lsp && cargo build --release"
    echo "5. Reinstalling extension: bash scripts/install-extension.sh"
    exit 1
fi
