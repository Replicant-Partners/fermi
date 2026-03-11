#!/usr/bin/env bash
# Migration linter for PgBouncer safety
# Catches multi-statement SQL that will silently fail through PgBouncer in transaction mode.
#
# Rules:
#   1. Multiple statements (;) outside a DO block → ERROR
#   2. BEGIN/COMMIT → ERROR (PgBouncer manages transactions)
#   3. Bare ALTER TABLE DROP + ADD constraint without DO block → ERROR
#
# Usage: ./scripts/lint-migrations.sh [file ...]
#   No args = lint all staged migration files
#   With args = lint specified files

set -uo pipefail

RED='\033[0;31m'
YELLOW='\033[0;33m'
GREEN='\033[0;32m'
NC='\033[0m'

errors=0
warnings=0

lint_file() {
    local file="$1"
    local basename
    basename=$(basename "$file")

    # Skip non-SQL
    [[ "$file" == *.sql ]] || return 0

    local content
    content=$(cat "$file")

    # Rule 1: BEGIN/COMMIT (explicit transactions)
    if echo "$content" | grep -qiE '^\s*(BEGIN|COMMIT)\s*;'; then
        echo -e "${RED}ERROR${NC} [$basename]: Contains BEGIN/COMMIT — PgBouncer manages transactions"
        ((errors++))
    fi

    # Rule 2: Count top-level statements (outside DO blocks)
    # Strip DO blocks, then count semicolons
    local stripped
    stripped=$(echo "$content" | sed '/^--/d' | perl -0777 -pe 's/DO\s+\$\$.*?\$\$\s*;//gs' 2>/dev/null || echo "$content" | sed '/^--/d')
    local stmt_count
    stmt_count=$(echo "$stripped" | grep -c ';\s*$' 2>/dev/null || echo 0)

    if [ "$stmt_count" -gt 1 ]; then
        # Check if it's a DROP+ADD constraint pattern
        if echo "$stripped" | grep -qi 'DROP CONSTRAINT' && echo "$stripped" | grep -qi 'ADD CONSTRAINT'; then
            echo -e "${RED}ERROR${NC} [$basename]: DROP+ADD CONSTRAINT outside DO block — will silently fail through PgBouncer"
            echo -e "       Wrap in: DO \$\$ BEGIN ... END \$\$;"
            ((errors++))
        else
            echo -e "${YELLOW}WARN${NC}  [$basename]: ${stmt_count} statements outside DO block — may fail through PgBouncer"
            echo -e "       Consider wrapping in: DO \$\$ BEGIN ... END \$\$;"
            ((warnings++))
        fi
    fi

    # Rule 3: Warn on any ALTER TABLE without IF EXISTS/IF NOT EXISTS
    if echo "$content" | sed '/^--/d' | grep -qiE 'ALTER TABLE.*ADD COLUMN[^I]*$' 2>/dev/null; then
        if ! echo "$content" | sed '/^--/d' | grep -qi 'IF NOT EXISTS'; then
            echo -e "${YELLOW}WARN${NC}  [$basename]: ALTER TABLE ADD COLUMN without IF NOT EXISTS — not idempotent"
            ((warnings++))
        fi
    fi
}

# Determine files to lint
if [ $# -gt 0 ]; then
    files=("$@")
else
    # Lint staged migration files
    mapfile -t files < <(git diff --cached --name-only --diff-filter=ACM 2>/dev/null | grep 'migrations/.*\.sql$' || true)
    if [ ${#files[@]} -eq 0 ]; then
        exit 0  # No staged migrations
    fi
fi

echo "Linting ${#files[@]} migration(s)..."
echo ""

for file in "${files[@]}"; do
    if [ -f "$file" ]; then
        lint_file "$file"
    fi
done

echo ""
if [ $errors -gt 0 ]; then
    echo -e "${RED}${errors} error(s)${NC}, ${warnings} warning(s)"
    echo "Fix errors before committing. See: memory/MEMORY.md → PgBouncer Pitfalls"
    exit 1
elif [ $warnings -gt 0 ]; then
    echo -e "${GREEN}0 errors${NC}, ${YELLOW}${warnings} warning(s)${NC}"
    exit 0
else
    echo -e "${GREEN}All migrations OK${NC}"
    exit 0
fi
