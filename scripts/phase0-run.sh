#!/usr/bin/env bash
#
# Phase 0 — Rabble UX Unblockers
#
# Verifies production database readiness for the four-pillar UX rollout.
# Optionally re-runs migrations if checks fail.
#
# Usage:
#   ./scripts/phase0-run.sh                    # verify only
#   ./scripts/phase0-run.sh --fix              # verify + re-run failed migrations
#   ./scripts/phase0-run.sh --db <url>         # explicit database URL
#   ./scripts/phase0-run.sh --fix --db <url>   # both
#
# Requires: psql
#
set +e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

FIX_MODE=false
DB_URL="${DATABASE_URL:-}"
PASS_COUNT=0
FAIL_COUNT=0
WARN_COUNT=0

usage() {
    echo "Usage: $0 [--fix] [--db <database_url>]"
    echo ""
    echo "  --fix   Re-run migrations for any failed checks"
    echo "  --db    Database URL (defaults to \$DATABASE_URL env var)"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --fix)
            FIX_MODE=true
            shift
            ;;
        --db)
            DB_URL="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

if [[ -z "$DB_URL" ]]; then
    echo -e "${RED}ERROR: No database URL.${NC}"
    echo "Set DATABASE_URL or pass --db <url>"
    exit 1
fi

# Mask password in output
DB_DISPLAY=$(echo "$DB_URL" | sed -E 's/(:[^:@]+)@/:*****@/')
echo ""
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Phase 0 — Production Readiness Verification${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  Database: ${CYAN}${DB_DISPLAY}${NC}"
echo -e "  Fix mode: ${FIX_MODE}"
echo ""

# ─── Helper: run a SQL check and report pass/fail ──────────────────

check() {
    local label="$1"
    local sql="$2"
    local fix_migration="${3:-}"

    local result
    result=$(psql "$DB_URL" -tAX -c "$sql" 2>/dev/null || echo "__ERROR__")

    if [[ "$result" == "__ERROR__" ]]; then
        echo -e "  ${RED}❌ FAIL${NC}: ${label} — query error"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        if [[ -n "$fix_migration" && "$FIX_MODE" == "true" ]]; then
            attempt_fix "$fix_migration"
        fi
        return 1
    elif [[ "$result" == "t" || "$result" == "1" ]]; then
        echo -e "  ${GREEN}✅ PASS${NC}: ${label}"
        PASS_COUNT=$((PASS_COUNT + 1))
        return 0
    else
        echo -e "  ${RED}❌ FAIL${NC}: ${label}"
        FAIL_COUNT=$((FAIL_COUNT + 1))
        if [[ -n "$fix_migration" && "$FIX_MODE" == "true" ]]; then
            attempt_fix "$fix_migration"
        fi
        return 1
    fi
}

check_warn() {
    local label="$1"
    local sql="$2"

    local result
    result=$(psql "$DB_URL" -tAX -c "$sql" 2>/dev/null || echo "__ERROR__")

    if [[ "$result" == "__ERROR__" || "$result" != "t" && "$result" != "1" ]]; then
        echo -e "  ${YELLOW}⚠️  WARN${NC}: ${label}"
        WARN_COUNT=$((WARN_COUNT + 1))
        return 1
    else
        echo -e "  ${GREEN}✅ PASS${NC}: ${label}"
        PASS_COUNT=$((PASS_COUNT + 1))
        return 0
    fi
}

attempt_fix() {
    local migration="$1"
    local migration_path="${PROJECT_ROOT}/${migration}"

    if [[ ! -f "$migration_path" ]]; then
        echo -e "    ${YELLOW}→ Migration file not found: ${migration}${NC}"
        return 1
    fi

    echo -e "    ${CYAN}→ Running ${migration}...${NC}"
    if psql "$DB_URL" -f "$migration_path" >/dev/null 2>&1; then
        echo -e "    ${GREEN}→ Migration applied successfully${NC}"
        return 0
    else
        echo -e "    ${RED}→ Migration failed — check manually${NC}"
        return 1
    fi
}

# ─── 0.2: PostGIS ─────────────────────────────────────────────────

echo -e "${BOLD}── 0.2 PostGIS Extension ──────────────────────────────────────${NC}"

POSTGIS_OK=0
check "PostGIS extension installed" \
    "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'postgis');" || POSTGIS_OK=1

if [[ $POSTGIS_OK -ne 0 && "$FIX_MODE" == "true" ]]; then
    echo -e "    ${CYAN}→ Attempting: CREATE EXTENSION IF NOT EXISTS postgis;${NC}"
    if psql "$DB_URL" -c "CREATE EXTENSION IF NOT EXISTS postgis;" 2>/dev/null; then
        echo -e "    ${GREEN}→ PostGIS installed${NC}"
    else
        echo -e "    ${RED}→ Failed — may need superuser. Enable via Neon dashboard → Extensions.${NC}"
    fi
fi

if [[ $POSTGIS_OK -eq 0 ]]; then
    POSTGIS_VERSION=$(psql "$DB_URL" -tAX -c "SELECT PostGIS_Version();" 2>/dev/null || echo "N/A")
    if [[ "$POSTGIS_VERSION" != "N/A" ]]; then
        echo -e "    Version: ${CYAN}${POSTGIS_VERSION}${NC}"
    fi
fi

echo ""

# ─── 0.1: Migration 090 — Social Layer Tables ─────────────────────

echo -e "${BOLD}── 0.1 Migration 090: Social Layer ────────────────────────────${NC}"

check "creature_friendships table" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='creature_friendships');" \
    "migrations/090_social_layer.sql" || true

check "creature_invites table" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='creature_invites');" \
    "migrations/090_social_layer.sql" || true

check "activity_events table" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='activity_events');" \
    "migrations/090_social_layer.sql" || true

check "rabble_co_presence table" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='rabble_co_presence');" \
    "migrations/090_social_layer.sql" || true

check "swarm_participants table (migration 091)" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='swarm_participants');" \
    "migrations/091_swarm_participants.sql" || true

echo ""
echo -e "${BOLD}── 0.1 Social Layer: Columns ───────────────────────────────────${NC}"

check "users.social_visibility column" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='users' AND column_name='social_visibility');" \
    "migrations/090_social_layer.sql" || true

echo ""
echo -e "${BOLD}── 0.1 Social Layer: SQL Functions ─────────────────────────────${NC}"

REQUIRED_FUNCTIONS=(
    "canonical_creature_pair"
    "get_creature_friends"
    "get_creatures_met_in_rabble"
    "get_pending_creature_invites"
    "get_pending_friendship_requests"
)

for fn in "${REQUIRED_FUNCTIONS[@]}"; do
    check "function: ${fn}()" \
        "SELECT EXISTS (SELECT 1 FROM information_schema.routines WHERE routine_schema='public' AND routine_name='${fn}');" \
        "migrations/090_social_layer.sql" || true
done

echo ""

# ─── 0.3: Notification Columns ────────────────────────────────────

echo -e "${BOLD}── 0.3 Notification Columns ────────────────────────────────────${NC}"

check "notifications.type column exists" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='notifications' AND column_name='type');" || true

check "notifications.message column exists" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='notifications' AND column_name='message');" || true

# Check for stale columns (should NOT exist)
STALE_NOTIF_TYPE=$(psql "$DB_URL" -tAX -c \
    "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='notifications' AND column_name='notification_type');" \
    2>/dev/null || echo "f")

if [[ "$STALE_NOTIF_TYPE" == "t" ]]; then
    echo -e "  ${RED}❌ FAIL${NC}: stale notifications.notification_type column exists"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    if [[ "$FIX_MODE" == "true" ]]; then
        attempt_fix "migrations/092_fix_social_layer.sql"
    fi
else
    echo -e "  ${GREEN}✅ PASS${NC}: no stale notification_type column"
    PASS_COUNT=$((PASS_COUNT + 1))
fi

# Check for stale body column on notifications specifically
STALE_BODY=$(psql "$DB_URL" -tAX -c \
    "SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name='notifications' AND column_name='body'
    );" 2>/dev/null || echo "f")

if [[ "$STALE_BODY" == "t" ]]; then
    echo -e "  ${YELLOW}⚠️  WARN${NC}: notifications.body column exists (might be stale)"
    WARN_COUNT=$((WARN_COUNT + 1))
    if [[ "$FIX_MODE" == "true" ]]; then
        attempt_fix "migrations/092_fix_social_layer.sql"
    fi
else
    echo -e "  ${GREEN}✅ PASS${NC}: no stale body column on notifications"
    PASS_COUNT=$((PASS_COUNT + 1))
fi

echo ""

# ─── 0.5: Dashboard Spatial Functions (Migration 089) ─────────────

echo -e "${BOLD}── 0.5 Dashboard Spatial Functions ─────────────────────────────${NC}"

check "get_my_rabbles_with_status() function" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.routines WHERE routine_schema='public' AND routine_name='get_my_rabbles_with_status');" \
    "migrations/089_dashboard_spatial_queries.sql" || true

echo ""

# ─── Row Counts (sanity) ──────────────────────────────────────────

echo -e "${BOLD}── Row Counts (sanity check) ───────────────────────────────────${NC}"

TABLES=(users creatures swarm_events notifications creature_friendships creature_invites activity_events rabble_co_presence swarm_participants)

for tbl in "${TABLES[@]}"; do
    COUNT=$(psql "$DB_URL" -tAX -c "SELECT COUNT(*) FROM ${tbl};" 2>/dev/null || echo "—")
    printf "  %-25s %s\n" "${tbl}" "${COUNT}"
done

echo ""

# ─── Recent Notifications ─────────────────────────────────────────

echo -e "${BOLD}── Recent Notifications (verify data shape) ────────────────────${NC}"

psql "$DB_URL" -c \
    "SELECT id, LEFT(user_id, 12) || '...' AS user_id, type, LEFT(title, 40) AS title, read, created_at
     FROM notifications ORDER BY created_at DESC LIMIT 5;" \
    2>/dev/null || echo "  (no notifications table or empty)"

echo ""

# ─── Summary ──────────────────────────────────────────────────────

TOTAL=$((PASS_COUNT + FAIL_COUNT + WARN_COUNT))

echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BOLD}  Results${NC}"
echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "  ${GREEN}✅ Passed:  ${PASS_COUNT}${NC}"
echo -e "  ${RED}❌ Failed:  ${FAIL_COUNT}${NC}"
echo -e "  ${YELLOW}⚠️  Warnings: ${WARN_COUNT}${NC}"
echo -e "  Total checks: ${TOTAL}"
echo ""

if [[ $FAIL_COUNT -eq 0 ]]; then
    echo -e "  ${GREEN}${BOLD}All critical checks passed. Ready for Phase 1.${NC}"
    echo ""
    echo "  Remaining Phase 0 manual tasks:"
    echo "    0.3  Deploy latest code to production (notification fixes already in code)"
    echo "    0.4  Fix Flutter creature picker (owner_id param) — separate repo"
    echo ""
else
    echo -e "  ${RED}${BOLD}${FAIL_COUNT} check(s) failed.${NC}"
    echo ""
    if [[ "$FIX_MODE" == "false" ]]; then
        echo "  To auto-fix, re-run with --fix:"
        echo "    ./scripts/phase0-run.sh --fix"
        echo ""
    fi
    echo "  Migrations run automatically on server startup (api_server.rs)."
    echo "  A fresh deploy should resolve most failures."
    echo ""
    echo "  If PostGIS is missing on Neon:"
    echo "    → Neon Dashboard → Project → Extensions → Enable PostGIS"
    echo ""
fi

echo -e "${BOLD}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Exit with failure code if any critical checks failed
if [[ $FAIL_COUNT -gt 0 ]]; then
    exit 1
fi
exit 0
