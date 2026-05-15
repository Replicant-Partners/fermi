#!/usr/bin/env python3
"""Schema consistency linter for fermi.

Catches the pattern that bit us twice:
  - Rust code SELECTs/INSERTs/UPDATEs a column.
  - No migration ever added that column.
  - Production 500s with "column X does not exist".

The rule is narrow on purpose to keep false positives low:

  Flag identifiers of the form  <qualifier>.<column>  appearing in a
  sqlx::query* SQL string where <column> does not exist in any
  migration's CREATE TABLE / ADD COLUMN / RENAME COLUMN declaration.

We deliberately ignore unqualified column references (too noisy — every
alias, function call, or computed-column name shows up). Both bugs we
hit (cv.rejected_by, t.mission) used qualified refs, so this rule
catches the actual failure mode without crying wolf.

Usage:
    ./scripts/lint-schema-consistency.py             # all staged .rs files
    ./scripts/lint-schema-consistency.py src/foo.rs  # specific files
    ./scripts/lint-schema-consistency.py --all       # all tracked .rs files

Exit codes:
    0 — clean (or only warnings)
    1 — at least one ERROR
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

# ─── ANSI ─────────────────────────────────────────────────────────────
RED = "\033[0;31m"
YELLOW = "\033[0;33m"
GREEN = "\033[0;32m"
DIM = "\033[2m"
NC = "\033[0m"

REPO_ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS_DIR = REPO_ROOT / "migrations"

# Columns that exist in production schema but aren't added by any
# migration in this repo (legacy schema from before migrations were
# tracked). Keep tight — only add real columns the linter can't see.
LEGACY_COLUMNS: set[str] = {
    "password_hash",
    "password_salt",
}

# Postgres internal columns / catalog references that look like column
# references but aren't user-table columns.
PG_INTERNALS: set[str] = {
    "atttypmod", "attrelid", "attname", "attisdropped",
    "relname", "relkind", "nspname", "indrelid",
    "table_name", "table_schema", "column_name",
    "data_type", "is_nullable", "column_default",
    "constraint_name", "constraint_type",
}

# Common SELECT-expression aliases that aren't real columns but look
# like qualified refs (`SUM(x) AS my_count` → my_count gets aliased,
# but never qualified — so this set isn't strictly needed for the
# qualified-ref rule. Kept as a safety net for variant patterns.)
COMPUTED_ALIASES: set[str] = {
    "my_count", "my_workspace_count", "last_my_spawn_at",
    "n_resolved", "unconsolidated", "agent_count", "last_coherence_at",
    "total_credits", "reads",
}


def _collect_columns_from_migrations() -> set[str]:
    """Walk migrations/*.sql and return the union of all column names ever declared.

    Patterns recognised:
      - ALTER TABLE ... ADD COLUMN [IF NOT EXISTS] <name> <type>
      - CREATE TABLE ... ( <name> <type>, ... )
      - ALTER TABLE ... RENAME COLUMN <old> TO <new>  (we keep both)
    """
    cols: set[str] = set()

    if not MIGRATIONS_DIR.is_dir():
        return cols

    add_col_re = re.compile(
        r"ADD\s+COLUMN\s+(?:IF\s+NOT\s+EXISTS\s+)?([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    )
    rename_col_re = re.compile(
        r"RENAME\s+COLUMN\s+([a-z_][a-z0-9_]*)\s+TO\s+([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    )
    # CREATE TABLE body — balanced-paren extraction is hard via pure regex
    # because CREATE TABLE bodies can contain DEFAULT (...) with nested
    # parens. We grab a generous slice and parse column-definition lines
    # heuristically.
    create_table_re = re.compile(
        r"CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:[a-z0-9_]+\.)?[a-z0-9_]+\s*\(",
        re.IGNORECASE,
    )
    # First-token-on-a-comma-separated-line pattern inside CREATE TABLE.
    col_def_re = re.compile(r"^\s*\"?([a-z_][a-z0-9_]*)\"?\s+", re.MULTILINE)

    # Table-level constraint keywords that start a line but aren't columns.
    constraint_starts = {
        "primary", "foreign", "unique", "check", "constraint",
        "exclude", "like", "include", "partition",
    }

    for sql_file in sorted(MIGRATIONS_DIR.glob("*.sql")):
        text = sql_file.read_text(encoding="utf-8", errors="ignore")
        text_no_comments = re.sub(r"--[^\n]*", "", text)

        for m in add_col_re.finditer(text_no_comments):
            cols.add(m.group(1).lower())

        for m in rename_col_re.finditer(text_no_comments):
            cols.add(m.group(1).lower())
            cols.add(m.group(2).lower())

        # For each CREATE TABLE, walk forward with paren-balancing to find
        # the matching ).
        for ct in create_table_re.finditer(text_no_comments):
            i = ct.end()
            depth = 1
            while i < len(text_no_comments) and depth > 0:
                c = text_no_comments[i]
                if c == "(":
                    depth += 1
                elif c == ")":
                    depth -= 1
                i += 1
            body = text_no_comments[ct.end() : i - 1]
            for cd in col_def_re.finditer(body):
                name = cd.group(1).lower()
                if name in constraint_starts:
                    continue
                cols.add(name)

    return cols


# ─── Rust SQL extraction ──────────────────────────────────────────────
# sqlx::query, query_as, query_scalar with raw r#"…"# strings (DOTALL).
SQLX_RAW_RE = re.compile(
    r"""sqlx::query(?:_as|_scalar|_with)?
        (?:::<[^>]+>)?
        \s*\(\s*
        r\#*\"
        ([\s\S]*?)
        \"\#*
    """,
    re.VERBOSE,
)
# Also bare "…" strings (single-line — adequate for shorter SQL).
SQLX_PLAIN_RE = re.compile(
    r"""sqlx::query(?:_as|_scalar|_with)?
        (?:::<[^>]+>)?
        \s*\(\s*
        \"([^\"\\\n]*(?:\\.[^\"\\\n]*)*)\"
    """,
    re.VERBOSE,
)


def _extract_sql_blocks(rs_text: str) -> list[str]:
    blocks: list[str] = []
    for m in SQLX_RAW_RE.finditer(rs_text):
        blocks.append(m.group(1))
    for m in SQLX_PLAIN_RE.finditer(rs_text):
        blocks.append(m.group(1))
    return blocks


# qualified column references: <alias>.<column> where alias is 1+ chars,
# column is at least 2 chars (single-letter columns are vanishingly rare
# and create more noise than signal). We capture column for checking.
QUALIFIED_REF_RE = re.compile(
    r"\b([a-z_][a-z0-9_]*)\s*\.\s*([a-z_][a-z0-9_]{1,})\b",
    re.IGNORECASE,
)

# Patterns to skip even if they look like qualified refs:
#   - schema.table  (e.g. public.teams, public.agents)
#   - alias.* (the wildcard form, e.g. a.*)
SKIP_QUALIFIERS = {
    "public", "information_schema", "pg_catalog",
}


def _scan_rs(rs_path: Path, migration_cols: set[str]) -> list[tuple[str, str, str]]:
    """Return (qualifier, column, snippet) for suspect qualified refs."""
    text = rs_path.read_text(encoding="utf-8", errors="ignore")
    findings: list[tuple[str, str, str]] = []
    seen: set[tuple[str, str]] = set()

    known = migration_cols | LEGACY_COLUMNS | PG_INTERNALS | COMPUTED_ALIASES

    for sql in _extract_sql_blocks(text):
        for m in QUALIFIED_REF_RE.finditer(sql):
            qualifier = m.group(1).lower()
            column = m.group(2).lower()
            if qualifier in SKIP_QUALIFIERS:
                continue
            # The "column" half of x.y is what we check.
            if column in known:
                continue
            if (qualifier, column) in seen:
                continue
            # Pull a short snippet centered on the match.
            start = max(0, m.start() - 40)
            end = min(len(sql), m.end() + 40)
            snippet = sql[start:end].replace("\n", " ").strip()
            findings.append((qualifier, column, snippet))
            seen.add((qualifier, column))

    return findings


# ─── File selection ───────────────────────────────────────────────────
def _staged_rs_files() -> list[Path]:
    try:
        out = subprocess.check_output(
            ["git", "diff", "--cached", "--name-only", "--diff-filter=ACM"],
            cwd=REPO_ROOT,
        ).decode("utf-8", errors="ignore")
    except Exception:
        return []
    return [REPO_ROOT / line for line in out.splitlines() if line.endswith(".rs")]


def _all_tracked_rs_files() -> list[Path]:
    try:
        out = subprocess.check_output(
            ["git", "ls-files", "--", "*.rs"],
            cwd=REPO_ROOT,
        ).decode("utf-8", errors="ignore")
    except Exception:
        return []
    return [REPO_ROOT / line for line in out.splitlines()]


# ─── Main ─────────────────────────────────────────────────────────────
def main() -> int:
    args = sys.argv[1:]
    if "--all" in args:
        files = _all_tracked_rs_files()
        args.remove("--all")
    elif args:
        files = [Path(a) for a in args if a.endswith(".rs")]
    else:
        files = _staged_rs_files()

    files = [f for f in files if f.is_file()]
    if not files:
        return 0

    migration_cols = _collect_columns_from_migrations()
    if not migration_cols:
        print(f"{YELLOW}WARN{NC}  Schema-consistency linter: no migrations found, skipping")
        return 0

    print(
        f"Schema-consistency lint: scanning {len(files)} Rust file(s) against "
        f"{len(migration_cols)} known columns (qualified refs only)…"
    )
    print()

    total_findings = 0
    for f in files:
        rel = f.relative_to(REPO_ROOT) if f.is_absolute() else f
        s = str(rel)
        if "/target/" in s or s.endswith(".pb.rs"):
            continue
        findings = _scan_rs(f, migration_cols)
        if findings:
            print(f"{RED}suspect column references in {rel}:{NC}")
            for qualifier, column, snippet in findings:
                print(
                    f"  {YELLOW}{qualifier}.{column}{NC}   {DIM}{snippet}{NC}"
                )
            print()
            total_findings += len(findings)

    if total_findings == 0:
        print(f"{GREEN}All qualified SQL column references resolve to a migration.{NC}")
        return 0

    print(f"{RED}{total_findings} suspect column reference(s).{NC}")
    print("Each is a <qualifier>.<column> ref whose <column> is not declared by")
    print("any migration. Likely fix: add a migration that introduces the column.")
    print("If the column exists for legacy reasons, add it to LEGACY_COLUMNS in")
    print("scripts/lint-schema-consistency.py.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
