#!/usr/bin/env bash
#
# Verify that a database rebuilt from migrations produces the `users` shape
# the 2026-08-06 production audit recorded.
#
# Migration 181's comments assert three things about production that no
# migration creates: `users.id`, `users.password_hash` / `password_salt`
# NOT NULL without default, and a CHECK on `auth_provider` that admits
# 'legacy'. 181 declares all three so a rebuild is faithful. This checks the
# declaration actually produced that shape, rather than trusting the comment.
#
# Also re-applies 181 twice, because `run_migrations` re-executes every file
# on every boot.

set -uo pipefail

CONTAINER="fermi-users-shape-$$"
cleanup() { docker rm -f "$CONTAINER" >/dev/null 2>&1; }
trap cleanup EXIT

cd "$(git rev-parse --show-toplevel)" || exit 1

echo "Starting postgres …"
docker run -d --name "$CONTAINER" \
    -e POSTGRES_PASSWORD=postgres -e POSTGRES_DB=fermi_test \
    pgvector/pgvector:pg16 >/dev/null || exit 1
for _ in $(seq 1 90); do
    docker exec "$CONTAINER" pg_isready -U postgres -q >/dev/null 2>&1 && break
    sleep 1
done

PSQL=(docker exec -i -e PGPASSWORD=postgres "$CONTAINER" psql -U postgres -d fermi_test)
"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "uuid-ossp";' >/dev/null 2>&1
"${PSQL[@]}" -q -c 'CREATE EXTENSION IF NOT EXISTS "vector";'    >/dev/null 2>&1

order=$(grep -oE '"migrations/[^"]+\.sql"' src/api_server.rs | tr -d '"')
while IFS= read -r f; do
    [ -n "$f" ] && [ -f "$f" ] && "${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f - < "$f" >/dev/null 2>&1
done <<< "$order"

fail=0
check() {
    local label="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        printf '  ok    %-46s %s\n' "$label" "$actual"
    else
        printf '  FAIL  %-46s got %-22s want %s\n' "$label" "$actual" "$expected"
        fail=1
    fi
}

q() { "${PSQL[@]}" -tAq -c "$1" 2>/dev/null | tr -d '[:space:]'; }

echo
echo "users shape after a rebuild from migrations:"

check "id exists" "uuid" \
    "$(q "SELECT data_type FROM information_schema.columns WHERE table_name='users' AND column_name='id'")"
check "id NOT NULL" "NO" \
    "$(q "SELECT is_nullable FROM information_schema.columns WHERE table_name='users' AND column_name='id'")"

for col in password_hash password_salt; do
    check "$col exists" "text" \
        "$(q "SELECT data_type FROM information_schema.columns WHERE table_name='users' AND column_name='$col'")"
    check "$col NOT NULL" "NO" \
        "$(q "SELECT is_nullable FROM information_schema.columns WHERE table_name='users' AND column_name='$col'")"
    # Production audit: "NOT NULL without default".
    check "$col has no default" "" \
        "$(q "SELECT COALESCE(column_default,'') FROM information_schema.columns WHERE table_name='users' AND column_name='$col'")"
done

check "auth_provider CHECK admits legacy" "t" \
    "$(q "SELECT EXISTS (SELECT 1 FROM pg_constraint WHERE conname='users_auth_provider_check' AND pg_get_constraintdef(oid) LIKE '%legacy%')")"
check "auth_provider CHECK is validated" "t" \
    "$(q "SELECT convalidated FROM pg_constraint WHERE conname='users_auth_provider_check'")"
check "abw-system principal present" "1" \
    "$(q "SELECT count(*) FROM users WHERE user_id='abw-system'")"
check "abw-system auth_provider" "legacy" \
    "$(q "SELECT auth_provider FROM users WHERE user_id='abw-system'")"

# Re-runnable: run_migrations re-executes every file on every boot.
echo
if err=$("${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f - < migrations/181_integrity_reconciliation.sql 2>&1); then
    echo "  ok    mig 181 re-applies cleanly (2nd run)"
else
    echo "  FAIL  mig 181 fails on re-run:"
    echo "$err" | grep -oE '(ERROR|FATAL):.*' | head -2 | sed 's/^/          /'
    fail=1
fi
if err=$("${PSQL[@]}" -v ON_ERROR_STOP=1 -q -f - < migrations/181_integrity_reconciliation.sql 2>&1); then
    echo "  ok    mig 181 re-applies cleanly (3rd run)"
else
    echo "  FAIL  mig 181 fails on 3rd run"
    fail=1
fi
check "abw-system still single row" "1" \
    "$(q "SELECT count(*) FROM users WHERE user_id='abw-system'")"

echo
if [ "$fail" -eq 0 ]; then
    echo "PASS — rebuilt schema matches the production shape 181 documents"
else
    echo "FAIL — rebuilt schema diverges from what 181 claims"
fi
exit $fail
