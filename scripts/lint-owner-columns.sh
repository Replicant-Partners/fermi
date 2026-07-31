#!/usr/bin/env bash
# lint-owner-columns.sh — v0.10.4 substrate CI enforcement.
#
# Fails CI if any migration introduces a user-reference column
# (owner_id / user_id / creator_id / …) on a resource table without a
# FOREIGN KEY REFERENCES users(user_id) constraint.
#
# Complements migration 162's NOT VALID substrate. That migration
# retrofits the invariant across existing tables; this script blocks
# regressions from new migrations.
#
# Exit codes:
#   0 — clean; no un-FK'd owner columns in tracked migrations.
#   1 — one or more offenders; message names each.
#
# Extending: to allowlist a specific (table, column) — e.g. an
# intentionally-audit-only column that must survive a user delete —
# add it to `ALLOWED` below with a one-line justification.

set -euo pipefail

python3 - "$@" <<'PY'
import re
import pathlib
import sys

OWNER_COL_PATTERN = re.compile(
    r"^\s*(?:ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?)?"
    r"(owner_id|user_id|creator_id|created_by|added_by|granted_by|"
    r"invited_by|invitee_user_id|blocker_user_id|blocked_user_id|"
    r"reporter_user_id|actor_user_id|owner_user_id|friend_owner_id|"
    r"from_owner_id|ejected_user_id|observer_id)"
    r"\s+(TEXT|UUID|VARCHAR)",
    re.IGNORECASE,
)

# (table, column) pairs deliberately without FK to users.user_id.
# Add here with a comment explaining why. Rule of thumb: only AUDIT
# columns that MUST survive a user deletion belong here.
ALLOWED = {
    # audit trail — the actor may be deleted; the event still matters
    ("activity_events", "actor_user_id"): "audit — historical fact",
    ("reports", "reporter_user_id"): "audit — historical fact",
    ("rabble_ejections", "ejected_user_id"): "audit — moderation record",
    ("user_blocks", "blocker_user_id"): "audit — moderation record",
    ("user_blocks", "blocked_user_id"): "audit — moderation record",
    ("creature_blocks", "blocker_user_id"): "audit — moderation record",
    ("creature_blocks", "blocked_user_id"): "audit — moderation record",
    ("object_shares", "granted_by"): "audit — grant history",
    ("team_members", "invited_by"): "audit — invite history",
    ("workspace_agents", "added_by"): "audit — provenance",
    # semi-audit invite intake; healed by v0.10.3 email-fallback
    ("forecast_invites", "invitee_user_id"): "back-fill on sign-in; nullable",
    # secrets access log — history that survives user deletion
    ("secret_access_log", "user_id"): "audit — access log",
    # agents.author — legacy free-form label, not an ownership ref
    ("agents", "author"): "legacy free-form label, not an owner ref",
    # api_keys uses users(id) FK not users(user_id) — already FK'd
    ("api_keys", "user_id"): "FK to users(id), not users(user_id)",
    # wallets is polymorphic — owner may be user OR workspace
    ("wallets", "owner_id"): "polymorphic (user | workspace)",
    # notifications — cascades to user; will be added in v0.10.5
    ("notifications", "user_id"): "cascade on delete; migration pending",
    # memory tables — legacy 007 mig, not first-class ownership
    ("entities", "user_id"): "ADM knowledge graph; agent-scoped",
    ("facts", "user_id"): "ADM knowledge graph; agent-scoped",
    ("episodes", "user_id"): "ADM episodic memory; agent-scoped",
    ("semantic_rules", "user_id"): "ADM rules; agent-scoped",
    ("embedding_provenance", "user_id"): "provenance metadata",
}

MIGRATIONS_DIR = pathlib.Path("migrations")

# Substrate boundary: migration 162 retrofits FK NOT VALID across every
# pre-existing owner column (see RELEASE_NOTES_v0.10.4.md). Migrations
# from this number onward must declare FK inline; older ones are
# grandfathered because mig 162 covers them.
SUBSTRATE_BOUNDARY = 162

NUM_RE = re.compile(r"^(\d+)_")

CREATE_RE = re.compile(
    r"CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+(?:public\.)?(\w+)\s*\(",
    re.IGNORECASE,
)
ALTER_RE = re.compile(
    r"ALTER\s+TABLE(?:\s+ONLY)?\s+(?:public\.)?(\w+)",
    re.IGNORECASE,
)

offenders = []

for mig in sorted(MIGRATIONS_DIR.glob("*.sql")):
    num_m = NUM_RE.match(mig.name)
    if not num_m or int(num_m.group(1)) < SUBSTRATE_BOUNDARY:
        continue  # grandfathered by mig 162
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
                # exit table body on the ); line
                pass

        alter_m = ALTER_RE.search(line)
        alter_table = alter_m.group(1).lower() if alter_m else None
        table_ctx = current_table or alter_table
        if not table_ctx or table_ctx == "users":
            continue

        col_m = OWNER_COL_PATTERN.search(stripped)
        if not col_m:
            # Also close the CREATE TABLE block context when we hit ')'
            if current_table and depth <= 0:
                current_table = None
            continue

        col = col_m.group(1).lower()
        has_fk = (
            "references users(user_id)" in stripped.lower()
            or "references public.users(user_id)" in stripped.lower()
        )
        if not has_fk and (table_ctx, col) not in ALLOWED:
            offenders.append((str(mig), table_ctx, col, stripped.strip()))

        if current_table and depth <= 0:
            current_table = None

if offenders:
    print("❌ lint-owner-columns: un-FK'd owner columns in migrations")
    print()
    print("The following (table, column) pairs declare a user reference")
    print("without a FOREIGN KEY to users(user_id). This is exactly the")
    print("class of drift v0.10.4 substrate was written to prevent.")
    print()
    print("Either add:")
    print("    REFERENCES users(user_id) ON DELETE <policy>")
    print("or, if this is a legitimate audit column that must outlive")
    print("its user, add it to ALLOWED in scripts/lint-owner-columns.sh")
    print("with a one-line justification.")
    print()
    for path, table, col, line in offenders:
        print(f"  {path}  {table}.{col}")
        print(f"    → {line}")
        print()
    sys.exit(1)

print("✓ lint-owner-columns: all user-reference columns FK'd or allowlisted")
PY
