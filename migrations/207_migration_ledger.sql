-- Migration 207: record what `run_migrations` did.
--
-- ## Why this exists
--
-- `run_migrations` replays every registered file on every boot, logs failures
-- with `eprintln!`, and continues. The continue is correct and is not changing:
-- most failures here are benign replays of already-applied DDL, and a migration
-- able to take the service down would be a worse problem than the one being
-- fixed.
--
-- What was missing is that nothing was written down.
-- `credit_ledger_tx_type_check` is the cost. Seventeen migrations declared it,
-- each dropped the constraint, each failed to re-add it because rows already
-- violated the new list, and each left one line in a boot log. Three of them
-- exist for no purpose other than that repair. So the repair was performed,
-- believed and repeated for the life of the project, while the net effect of
-- every attempt was to DELETE the thing being repaired.
--
-- A failure that is only ever printed is a failure nobody can be asked about.
--
-- ## The one place the schema is declared twice, and why
--
-- `api_server::ensure_migration_ledger` creates this same table inline, before
-- the migration loop runs. That is not redundancy for its own sake: a migration
-- that records migrations cannot record itself, so on a database where this file
-- has never run there would be nowhere to write the result of running it.
--
-- The inline copy is therefore the bootstrap, and this file is the DECLARATION.
-- Having it here is what lets `scripts/lint-schema-consistency.py` see these
-- columns at all — the lint scans qualified column references in Rust against
-- columns some migration introduces, and it correctly rejected the first version
-- of this work for referencing a table no migration declared. Both copies are
-- `IF NOT EXISTS`, so whichever runs first wins and the second is a no-op.
--
-- If the two ever disagree, this file is authoritative and the inline copy is the
-- bug.
--
-- ## Upsert, not append
--
-- One row per migration, updated in place, rather than one row per boot. The
-- files replay on every start, so an append-only log would grow without bound
-- and answer no question the counters do not. What the counters answer and a
-- print cannot:
--
--   * has this migration EVER succeeded          -> first_succeeded_at
--   * is it failing RIGHT NOW                    -> consecutive_failures
--   * has the file changed since it applied      -> content_sha256
--
-- The second is the check that seventeen migrations went without. The third
-- catches a migration edited after deploy, which is otherwise invisible because
-- replay makes it look identical to one that never changed.
--
-- `first_succeeded_at` is also the field the verification work has been missing.
-- `liveness_trust` asks whether a write path ever ran, and answers by comparing
-- rows in a sink against the opportunities the writer had. With no record of when
-- a migration landed, "opportunity" can only mean all time — so every newly
-- deployed writer reports as broken, because history is always full of chances it
-- could not have taken. That is currently a documented exemption. With a landing
-- time, the window can start at the deploy.

-- One DO block: PgBouncer runs in transaction-pooling mode, where top-level
-- statements get separate implicit transactions.
DO $$
BEGIN
    CREATE TABLE IF NOT EXISTS public.schema_migrations (
        -- Path as registered in `run_migrations`, e.g. `migrations/204_...sql`.
        filename              TEXT PRIMARY KEY,
        -- SHA-256 of the file contents as executed. Empty string when the file
        -- could not be read at all.
        content_sha256        TEXT NOT NULL,
        attempts              INTEGER NOT NULL DEFAULT 0,
        successes             INTEGER NOT NULL DEFAULT 0,
        failures              INTEGER NOT NULL DEFAULT 0,
        -- Reset to 0 by any success. Non-zero means it is failing now, which is
        -- a different and more urgent fact than having failed at some point.
        consecutive_failures  INTEGER NOT NULL DEFAULT 0,
        -- NULL means this migration has NEVER applied. Whatever it declares does
        -- not exist in the database, however many times it has run.
        first_succeeded_at    TIMESTAMPTZ,
        last_attempt_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
        -- 'ok' | 'failed' | 'unreadable'. The third is its own status because a
        -- registered file missing from the image is a different fault from SQL
        -- that ran and errored: the deploy is not carrying what the code
        -- believes it is carrying.
        last_status           TEXT NOT NULL,
        last_error            TEXT,
        last_duration_ms      INTEGER
    );

    COMMENT ON TABLE public.schema_migrations IS
        'What run_migrations did, per registered file. Upserted on every boot; '
        'NOT append-only, because migrations replay and the counters answer '
        'everything a per-boot log would. first_succeeded_at IS NULL means the '
        'migration has never once applied - the shape in which '
        'credit_ledger_tx_type_check was declared seventeen times and existed '
        'never. Also created inline by api_server::ensure_migration_ledger, '
        'which is the bootstrap; THIS FILE is the declaration and is '
        'authoritative if they disagree.';

    -- The queue view: what is broken, worst first.
    CREATE INDEX IF NOT EXISTS idx_schema_migrations_failing
        ON public.schema_migrations(consecutive_failures DESC, filename)
        WHERE consecutive_failures > 0;

    -- The stronger question: attempted, and never once applied.
    CREATE INDEX IF NOT EXISTS idx_schema_migrations_never_applied
        ON public.schema_migrations(filename)
        WHERE first_succeeded_at IS NULL;

    ALTER TABLE public.schema_migrations
        DROP CONSTRAINT IF EXISTS schema_migrations_last_status_check;
    ALTER TABLE public.schema_migrations
        ADD CONSTRAINT schema_migrations_last_status_check
        CHECK (last_status IN ('ok', 'failed', 'unreadable'));
END $$;
