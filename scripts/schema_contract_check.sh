#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────
# Is the schema trust contract SATISFIABLE? — offline, throwaway cluster
# ─────────────────────────────────────────────────────────────────────
#
# WHY THIS EXISTS
#
# src/schema_trust.rs declares a contract: every table, matview, column and
# function the Rust code assumes exists. At boot, api-server probes the live
# DB against it and (under SCHEMA_STRICT=1) refuses to serve on drift.
#
# That mechanism is only worth anything if the contract is SATISFIABLE. From
# v0.11.0 to v0.11.8 it was not: `fermi_leaderboard` is a MATERIALIZED VIEW
# but was declared as a table and probed via `information_schema.tables`,
# which omits matviews entirely. The contract therefore reported permanent
# drift, SCHEMA_STRICT=1 was un-enablable, and nobody noticed — because the
# verdict was only ever written to stderr, and the module was `#[path]`-
# included into the binary so `cargo test` could not see it.
#
# This script closes that loop. It builds a real schema from the migrations
# in a disposable cluster, then asserts the contract holds against it, via
# tests/schema_trust_contract.rs.
#
# WHAT A FAILURE MEANS
#
#   * missing table/matview/column  → either genuine migration drift, or the
#                                     contract over-declares.
#   * missing function              → likely declared only in
#                                     `ensure_critical_schema` (api_server.rs)
#                                     and not in any migration. That is the
#                                     "two competing schema definitions"
#                                     problem; see Phase 2.3 of
#                                     docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md
#   * relation kind drift           → an object changed kind (e.g. matview
#                                     became a table).
#
# Usage:  scripts/schema_contract_check.sh [--skip-tests]
# Exit:   0 = every migration applied AND the contract is satisfied

set -euo pipefail

SKIP_TESTS=0
[ "${1:-}" = "--skip-tests" ] && SKIP_TESTS=1

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

DATA_DIR="$(mktemp -d /tmp/contractcheck-pg.XXXXXX)"
PORT="${CONTRACTCHECK_PGPORT:-55434}"
SOCK_DIR="$DATA_DIR/sock"
DB=contractcheck
USER=validator
mkdir -p "$SOCK_DIR"

cleanup() {
    pg_ctl -D "$DATA_DIR/data" stop -m immediate >/dev/null 2>&1 || true
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

initdb -D "$DATA_DIR/data" -U "$USER" --auth=trust -E UTF8 \
    >"$DATA_DIR/initdb.log" 2>&1

# Prefer a Unix socket — it keeps the whole check off the network, which
# matters in sandboxed/CI environments that firewall loopback. Some
# sandboxes forbid creating Unix sockets outright, so fall back to
# 127.0.0.1. Either way nothing leaves the machine.
if pg_ctl -D "$DATA_DIR/data" \
        -o "-p $PORT -k $SOCK_DIR -c listen_addresses=''" \
        -l "$DATA_DIR/server.log" start >/dev/null 2>&1; then
    PGHOST_ARG="$SOCK_DIR"
    echo "▸ throwaway cluster up (unix socket $SOCK_DIR, port $PORT)"
elif pg_ctl -D "$DATA_DIR/data" \
        -o "-p $PORT -c listen_addresses=127.0.0.1 -c unix_socket_directories=''" \
        -l "$DATA_DIR/server.log" start >/dev/null 2>&1; then
    PGHOST_ARG="127.0.0.1"
    echo "▸ throwaway cluster up (loopback 127.0.0.1, port $PORT)"
    echo "  (unix socket unavailable — sandbox restriction)"
else
    echo "✗ could not start the throwaway cluster:" >&2
    sed 's/^/    /' "$DATA_DIR/server.log" >&2
    exit 1
fi

PSQL="psql -h $PGHOST_ARG -p $PORT -U $USER -q"
$PSQL -v ON_ERROR_STOP=1 -d postgres -c "CREATE DATABASE $DB"

# pgvector is required by mig-010 onward. Migration 010 wraps the CREATE
# EXTENSION in an exception handler, so without it the vector(1024) columns
# — and the tables carrying them — silently never get created, which would
# show up as a wall of "missing table" drift rather than "no pgvector".
# Fail fast and clearly instead.
if ! $PSQL -v ON_ERROR_STOP=1 -d "$DB" -c 'CREATE EXTENSION IF NOT EXISTS vector' >/dev/null 2>&1; then
    echo "✗ pgvector not available in this Postgres install." >&2
    echo "  Install postgresql-17-pgvector; without it the memory tables never" >&2
    echo "  get created and the contract check reports meaningless drift." >&2
    exit 1
fi
echo "▸ pgvector present"

# ── Apply every migration, in the runner's exact order ─────────────
#
# ORDER IS LOAD-BEARING. Production applies a hardcoded list in
# src/api_server.rs::run_migrations, and that list is deliberately NOT in
# lexicographic order — e.g. 006_add_user_id_to_agents.sql must run after
# the migration that creates `agents`. A sorted glob produces a cascade of
# ~26 spurious failures.
#
# `.github/workflows/ci.yml` uses `ls migrations/*.sql | sort`, so CI has
# never built the schema production actually runs. Parsing the real list
# here is what makes this check faithful.
MIGRATION_LIST=$(grep -oE '"migrations/[^"]+\.sql"' src/api_server.rs | tr -d '"')
LIST_COUNT=$(echo "$MIGRATION_LIST" | wc -l)
echo "▸ applying $LIST_COUNT migrations in run_migrations() order"

# Drift between the hardcoded list and the directory is its own defect:
# migrations/126 sat on disk, absent from the list, for weeks — applied by
# CI's glob but never by production.
ON_DISK=$(ls migrations/*.sql | grep -v rollback | sort)
ORPHANED=$(comm -23 <(echo "$ON_DISK") <(echo "$MIGRATION_LIST" | sort) || true)
if [ -n "$ORPHANED" ]; then
    echo "  ⚠ on disk but NOT in run_migrations() — production will never apply these:"
    echo "$ORPHANED" | sed 's/^/      /'
fi
GHOSTS=$(comm -13 <(echo "$ON_DISK") <(echo "$MIGRATION_LIST" | sort) || true)
if [ -n "$GHOSTS" ]; then
    echo "  ⚠ in run_migrations() but NOT on disk:"
    echo "$GHOSTS" | sed 's/^/      /'
fi

APPLY_FAILED=0
FAILED_FILES=()
while IFS= read -r f; do
    [ -z "$f" ] && continue
    if [ ! -f "$f" ]; then
        printf '  %-56s MISSING FROM DISK\n' "$(basename "$f")"
        FAILED_FILES+=("$f")
        APPLY_FAILED=1
        continue
    fi
    if ! out=$($PSQL -v ON_ERROR_STOP=1 -d "$DB" -f "$f" 2>&1); then
        FAILED_FILES+=("$f")
        APPLY_FAILED=1
        printf '  %-56s FAILED\n' "$(basename "$f")"
        echo "$out" | grep -E 'ERROR|FATAL' | head -2 | sed 's/^/      /'
    fi
done <<< "$MIGRATION_LIST"

if [ "$APPLY_FAILED" -eq 0 ]; then
    echo "  all $LIST_COUNT migrations applied cleanly"
else
    echo "  ${#FAILED_FILES[@]} migration(s) failed — production's runner swallows these"
fi

# ── Inventory, for context on any drift reported below ────────────────
echo
echo "▸ resulting schema"
psql -h "$PGHOST_ARG" -p "$PORT" -U "$USER" -d "$DB" -t -A -F' ' -c "
  SELECT c.relkind::text || ': ' || count(*)::text
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
   WHERE n.nspname = 'public' AND c.relkind IN ('r','p','m','v')
   GROUP BY c.relkind ORDER BY c.relkind" | sed 's/^/  /'

if [ "$SKIP_TESTS" -eq 1 ]; then
    echo
    echo "▸ --skip-tests given; not running the contract assertion"
    [ "$APPLY_FAILED" -ne 0 ] && exit 1
    exit 0
fi

# ── Assert the contract against that schema ───────────────────────────
#
# sqlx accepts both a Unix socket directory and a hostname through the
# `host` query parameter.
export DATABASE_URL="postgresql:///$DB?user=$USER&host=$PGHOST_ARG&port=$PORT"

echo
echo "▸ asserting schema trust contract"
if cargo test --test schema_trust_contract -- --nocapture --test-threads=1; then
    CONTRACT_OK=1
else
    CONTRACT_OK=0
fi

echo "═══════════════════════════════════════════════════════════"

if [ "$APPLY_FAILED" -ne 0 ]; then
    echo "✗ ${#FAILED_FILES[@]} migration(s) do not apply to an empty database."
    printf '    %s\n' "${FAILED_FILES[@]}"
    echo
    echo "  This means the migration set CANNOT REBUILD the schema from scratch."
    echo "  Production's schema is therefore a historical artifact, not a"
    echo "  reproducible artifact — some objects exist only because a migration"
    echo "  was edited in place after it had already run."
    echo
    echo "  Consequence for the contract check below: it is INCONCLUSIVE, not"
    echo "  failed. Any 'missing' item may simply be downstream of a migration"
    echo "  that aborted above (ON_ERROR_STOP halts a file at its first error,"
    echo "  leaving later statements in that file unapplied)."
    echo
    echo "  Rebuildability is a prerequisite for the Phase 3 migration diff"
    echo "  gate — see docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md"
    exit 1
fi

if [ "$CONTRACT_OK" -ne 1 ]; then
    echo "✗ Migrations all applied, but the schema trust contract is NOT satisfied."
    echo "  Because the schema built cleanly, this is a REAL result: either the"
    echo "  contract over-declares, or a migration genuinely fails to create"
    echo "  what the code expects. SCHEMA_STRICT=1 would refuse to boot here."
    exit 1
fi

echo "✓ Migrations rebuild the schema from empty, and the contract is satisfied."
echo "  SCHEMA_STRICT=1 is safe to enable against this schema."
