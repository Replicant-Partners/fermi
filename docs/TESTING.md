# Testing harness

Small lints and checks that protect against bug classes we've actually
hit in production. Each rule has a real incident behind it — they're
not speculative; they earned their place.

## Setup

Once per clone:

```bash
./scripts/install-git-hooks.sh
```

Installs `scripts/git-hooks/*` as symlinks under `.git/hooks/`. The
hooks update automatically as the tracked scripts evolve — no need
to re-run after edits.

## Lints that run on commit

### Migration lint — `scripts/lint-migrations.sh`

Fires on staged `migrations/*.sql` files. Rules:

- **Rule 1**: `BEGIN` / `COMMIT` are errors. PgBouncer manages
  transactions in transaction mode and explicit transactions
  silently fail.
- **Rule 2**: multi-statement DDL outside a `DO $$ BEGIN … END $$;`
  block is a warning (an error specifically for `DROP CONSTRAINT` +
  `ADD CONSTRAINT` pairs, which we've seen silently lose the ADD).
- **Rule 3**: `ALTER TABLE ADD COLUMN` without `IF NOT EXISTS` is a
  warning. Migrations run on every server boot, so non-idempotent
  DDL is a footgun.

**Background:** [`memory/MEMORY.md` → PgBouncer Pitfalls](../memory/MEMORY.md).
The constraint-mutation incident bit us when migration 030 had
`DROP CONSTRAINT … ADD CONSTRAINT …` as separate statements — the
DROP succeeded, the ADD silently disappeared, and every downstream
handler started failing.

### Schema-consistency lint — `scripts/lint-schema-consistency.py`

Fires on staged `*.rs` files. Rule:

- For every qualified column reference `<qualifier>.<column>` in a
  `sqlx::query[_as|_scalar|_with]?(...)` SQL string, the `<column>`
  must exist in some migration's `CREATE TABLE`, `ADD COLUMN`, or
  `RENAME COLUMN` declaration.

The rule is deliberately narrow (qualified refs only) to keep
false positives near zero. Unqualified references — aliases, function
results, postgres internals — aren't checked.

**Background:** two production 500s in May 2026.

  - `teams.mission` was SELECTed by `workspace/core.rs` but no
    migration ever added the column. Fix: migration 113 (then
    backstop migration 119 when 113 didn't take on prod).
  - `composition_versions.rejected_by` and `.rejection_note` were
    SELECTed by `list_composition_versions` but no migration added
    them. Fix: migration 120.

Both would have been caught at commit time by this lint.

**Allowlist:** `LEGACY_COLUMNS` in the script holds names that
exist in the live schema but predate the migrations tracked in this
repo. Currently: `password_hash`, `password_salt`. Add to it only
when you've confirmed the column is real and just unmigrated.

## Running manually

```bash
# All staged files (what the hook does)
./scripts/lint-migrations.sh
./scripts/lint-schema-consistency.py

# Whole tree (good before a big merge)
./scripts/lint-schema-consistency.py --all

# Specific files
./scripts/lint-migrations.sh migrations/123_foo.sql
./scripts/lint-schema-consistency.py src/handlers/foo.rs
```

## Adding a new lint

We add rules when we've hit the same class of bug twice and want
to stop hitting it a third time. The schema-consistency lint is the
prototype — keep new ones narrow:

1. Author a script under `scripts/` that takes file paths as args
   and exits non-zero on findings.
2. Wire it into `scripts/git-hooks/pre-commit`. The hook reads
   staged files and dispatches to the appropriate linter based on
   file extension.
3. Update this doc with the rule, the underlying incident, and any
   allowlist mechanism.

Lints should be opinionated about the specific failure mode, not
general code-quality scolds — they earn trust by catching real
bugs without crying wolf on shipping code.
