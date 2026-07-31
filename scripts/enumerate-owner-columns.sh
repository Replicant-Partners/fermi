#!/usr/bin/env bash
# Enumerate every (table, column) pair that stores a user reference,
# by scanning migrations. Used to design the rbac_orphans view and the
# NOT VALID FK migration.
set -euo pipefail

python3 - <<'PY'
import re
import pathlib
import collections

OWNER_COLS = {
    "owner_id", "user_id", "creator_id", "created_by", "added_by",
    "granted_by", "invited_by", "invitee_user_id", "blocker_user_id",
    "blocked_user_id", "reporter_user_id", "actor_user_id",
    "owner_user_id", "friend_owner_id", "from_owner_id",
    "ejected_user_id", "observer_id", "author",
}

# tables to skip (not user resources or intentionally not owner-typed)
SKIP_TABLES = {
    "users",              # the source of truth
    "siwe_nonces",        # session ephemera
    "wallets",            # owner_id can be user OR workspace (polymorphic); handled separately
    "credit_ledger",      # no owner_id column
    "waitlist",           # pre-user records
}

migrations = sorted(pathlib.Path("migrations").glob("*.sql"))

# (table, column) -> list of migrations where declared
found = collections.defaultdict(list)

# Rough SQL table parser: track current table via CREATE TABLE, capture columns
CREATE_RE = re.compile(
    r"CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+(?:public\.)?(\w+)\s*\(",
    re.IGNORECASE,
)
ALTER_RE = re.compile(
    r"ALTER\s+TABLE(?:\s+ONLY)?\s+(?:public\.)?(\w+)",
    re.IGNORECASE,
)

for mig in migrations:
    text = mig.read_text()
    current_table = None
    depth = 0
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("--"):
            continue

        m = CREATE_RE.search(line)
        if m:
            current_table = m.group(1).lower()
            depth = line.count("(") - line.count(")")
            continue

        if current_table:
            depth += line.count("(") - line.count(")")
            if depth <= 0:
                current_table = None

        alter_m = ALTER_RE.search(line)
        alter_table = alter_m.group(1).lower() if alter_m else None

        table_ctx = current_table or alter_table
        if not table_ctx or table_ctx in SKIP_TABLES:
            continue

        # Match column definition or ADD COLUMN
        col_m = re.search(
            r"(?:ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?)?(\w+)\s+(?:TEXT|UUID|VARCHAR)",
            stripped,
            re.IGNORECASE,
        )
        if not col_m:
            continue
        col = col_m.group(1).lower()
        if col in OWNER_COLS:
            has_fk = "references users" in stripped.lower() or "references public.users" in stripped.lower()
            found[(table_ctx, col)].append((mig.name, has_fk))

# Print report
print(f"{'TABLE':<40} {'COLUMN':<20} {'FK?':<6} {'FIRST_MIG':<40}")
print("-" * 110)
for (table, col), migs in sorted(found.items()):
    first = migs[0]
    has_fk = any(fk for (_, fk) in migs)
    print(f"{table:<40} {col:<20} {'yes' if has_fk else 'NO':<6} {first[0]:<40}")

# Also count
total = len(found)
fk_count = sum(1 for (_, _), migs in found.items() if any(fk for (_, fk) in migs))
print()
print(f"Total (table, column) owner references: {total}")
print(f"With FK to users:                       {fk_count}")
print(f"Without FK (drift risk):                {total - fk_count}")
PY
