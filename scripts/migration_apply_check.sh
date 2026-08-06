#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Verify that migrations actually APPLY — offline, throwaway cluster
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# `run_migrations` in src/api_server.rs deliberately does not panic:
#
#     Err(e) => eprintln!("Migration {} warning: {}", file, e)
#
# That's the right call for a runner that re-executes every file on every
# boot (most "failures" are just "already applied"). But it means a
# genuinely broken migration fails SILENTLY — the server comes up, the
# column never exists, and you discover it when a query 500s in
# production.
#
# So: apply the named migrations to a disposable cluster, twice, and fail
# loudly on any error. Twice because the runner really does run every file
# on every boot — a migration that works once but errors on re-run turns
# every restart into a log full of noise that hides real problems.
#
# The fixture below carries only the columns the target migrations touch,
# at production's real types. Extend it as needed — it is not meant to be
# a full schema.
#
# Usage:   scripts/migration_apply_check.sh <migration.sql> [more.sql ...]
# Example: scripts/migration_apply_check.sh migrations/177_*.sql migrations/178_*.sql
# Exit:    0 = every migration applied twice with no error

set -euo pipefail

if [ $# -eq 0 ]; then
    echo "usage: $0 <migration.sql> [...]" >&2
    exit 1
fi

PGBIN="${PGBIN:-/usr/lib/postgresql/17/bin}"
[ -d "$PGBIN" ] && export PATH="$PGBIN:$PATH"

for tool in initdb pg_ctl psql; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: $tool not on PATH. Set PGBIN to your Postgres bin dir." >&2
        exit 1
    }
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

DATA_DIR="$(mktemp -d /tmp/migcheck-pg.XXXXXX)"
PORT="${MIGCHECK_PGPORT:-55433}"
SOCK_DIR="$DATA_DIR/sock"
mkdir -p "$SOCK_DIR"

cleanup() {
    pg_ctl -D "$DATA_DIR/data" stop -m immediate >/dev/null 2>&1 || true
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

initdb -D "$DATA_DIR/data" -U validator --auth=trust -E UTF8 \
    >"$DATA_DIR/initdb.log" 2>&1
pg_ctl -D "$DATA_DIR/data" \
    -o "-p $PORT -k $SOCK_DIR -c listen_addresses=''" \
    -l "$DATA_DIR/server.log" start >/dev/null

PSQL="psql -h $SOCK_DIR -p $PORT -U validator -v ON_ERROR_STOP=1 -q"
$PSQL -d postgres -c "CREATE DATABASE migcheck"

# ── Fixture: only what the target migrations reference ────────────────
$PSQL -d migcheck <<'SQL'
CREATE TABLE users (
    user_id      TEXT PRIMARY KEY,
    display_name TEXT,
    name         TEXT,
    email        TEXT
);

-- agents: mig-177 rewrites mcp_servers, mig-178 adds mcp_tools.
-- agent_id is UUID in production (see mig-015's FK).
CREATE TABLE agents (
    agent_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_name   TEXT NOT NULL,
    mcp_servers  JSONB,
    description  TEXT,
    author       TEXT NOT NULL DEFAULT 'Fermi Team',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed the exact shape mig-177 is meant to clean up: a legacy row whose
-- mcp_servers column holds card `mcp_tools` payloads (the pre-177 bug),
-- alongside a correct row and a NULL row. If 177 is over-eager it will
-- clobber the correct one; if it is under-eager the legacy one survives.
INSERT INTO agents (agent_name, mcp_servers) VALUES
    ('legacy_bug', '[{"name":"fpl_execute","description":"d","input_schema":{}}]'::jsonb),
    ('correct',    '[{"name":"ctx7","url":"https://example.test/mcp"}]'::jsonb),
    ('untouched',  NULL);

-- team_members: mig-179 adds `capabilities` and backfills owner/admin.
-- Seeded with one row per role so the backfill's discrimination is visible
-- in the state dump — a backfill that grants everyone, or nobody, is the
-- failure mode that matters and both look like "UPDATE n" otherwise.
CREATE TABLE team_members (
    team_id     UUID NOT NULL,
    member_type TEXT NOT NULL DEFAULT 'user',
    member_id   TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member',
    invited_by  TEXT,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, member_id)
);

INSERT INTO team_members (team_id, member_id, role) VALUES
    ('11111111-1111-1111-1111-111111111111', 'an_owner',  'owner'),
    ('11111111-1111-1111-1111-111111111111', 'an_admin',  'admin'),
    ('11111111-1111-1111-1111-111111111111', 'a_member',  'member'),
    ('11111111-1111-1111-1111-111111111111', 'a_viewer',  'viewer');
SQL

echo "▸ fixture ready (3 agents rows: legacy_bug / correct / untouched)"
echo

FAILED=0
for pass in 1 2; do
    echo "═══ pass $pass ═══"
    for m in "$@"; do
        printf '  %-52s ' "$(basename "$m")"
        if out=$($PSQL -d migcheck -f "$m" 2>&1); then
            echo "OK"
        else
            echo "FAILED"
            echo "$out" | sed 's/^/      /'
            FAILED=1
        fi
    done
done

# Fixture-specific diagnostic, NOT a check. Best-effort on purpose: it only
# makes sense for migrations that touch `agents`, and a hard failure here
# once made a migration that had applied perfectly look broken. Diagnostics
# must never be able to fail a validation run.
echo
if psql -h "$SOCK_DIR" -p "$PORT" -U validator -d migcheck -tAc \
    "SELECT 1 FROM information_schema.columns
      WHERE table_name='agents' AND column_name='mcp_tools'" 2>/dev/null | grep -q 1; then
    echo "▸ resulting state (agents fixture)"
    psql -h "$SOCK_DIR" -p "$PORT" -U validator -d migcheck -c \
        "SELECT agent_name,
                mcp_servers::text AS mcp_servers,
                mcp_tools::text   AS mcp_tools
           FROM agents ORDER BY agent_name"
fi

# Whatever the migration touched, show the columns it left behind on the
# fixture tables. Generic, so this stays useful as the fixture grows.
# mig-179's backfill is a deliberate TIGHTENING (owner/admin get 'resolve',
# member/viewer do not), so show per-role results rather than a row count.
if psql -h "$SOCK_DIR" -p "$PORT" -U validator -d migcheck -tAc \
    "SELECT 1 FROM information_schema.columns
      WHERE table_name='team_members' AND column_name='capabilities'" 2>/dev/null | grep -q 1; then
    echo "▸ resulting state (team_members fixture)"
    psql -h "$SOCK_DIR" -p "$PORT" -U validator -d migcheck -c \
        "SELECT member_id, role, capabilities::text
           FROM team_members ORDER BY role, member_id"
fi

echo "▸ fixture columns after migration"
psql -h "$SOCK_DIR" -p "$PORT" -U validator -d migcheck -c \
    "SELECT table_name, column_name, data_type
       FROM information_schema.columns
      WHERE table_schema='public'
        AND table_name IN ('agents','team_members','fermi_forecast_updates',
                           'fermi_portfolio_forecasts','object_shares')
        AND column_name IN ('mcp_tools','mcp_servers','capabilities',
                           'actor_user_id','added_by')
      ORDER BY table_name, column_name"

if [ "$FAILED" -ne 0 ]; then
    echo "✗ at least one migration failed — the runner would swallow this"
    exit 1
fi
echo "✓ all migrations applied twice, no errors"
