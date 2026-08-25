-- ═══════════════════════════════════════════════════════════════════════
-- 215 — `committed_before_measured` compares against the measurement
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT WAS WRONG
--
-- Migration 141 gave `process_spacetime` the column that carries Loop 5.B's
-- entire claim — "a physical measurement is scored against what the model
-- projected, the one signal an agent cannot talk its way out of" — and defined
-- it as:
--
--     committed_before_measured BOOLEAN GENERATED ALWAYS AS (
--         committed_at IS NOT NULL AND committed_at < resolved_at
--     ) STORED
--
-- `resolved_at` defaults to `NOW()` and is the moment the platform got round to
-- *scoring*. `resolve_against_projection` can only score a projection whose
-- commit row it just read, so the commit always pre-dates the scoring pass.
--
-- **The column is therefore true by construction, for every row, always.** It
-- is a tautology sitting in the field named for the invariant, and it would have
-- reported `committed_before_measured = true` for a prediction anchored today
-- and scored against a sensor reading taken six weeks ago.
--
-- The table already carries the value the invariant needs: `measured_at`, bound
-- from the real observation's own `phenomenon_time`. It was never used.
--
-- WHY IT MATTERS MORE THAN A WRONG BOOLEAN
--
-- It is the difference between verification and transcription, which is the
-- distinction the whole SimOps benchmark exists to make. With the old
-- predicate, anchoring the 61 projections already on file — every one of them
-- predating the anchor hook — and resolving them against the 7,576 real
-- readings already on file would have produced 61 rows all claiming the
-- prediction came first. The loop would have read as turning, in the report, on
-- the strength of scoring answers that were already known.
--
-- `commit_to_resolve_hours` still records the commit-to-scoring latency, so
-- nothing is lost by the change; that number was simply never the invariant.
--
-- WHY THE GUARD
--
-- A generated column cannot be altered in place, so this is a DROP + ADD. An
-- unguarded pair runs on **every boot**, and Postgres holds a dropped column's
-- slot forever against the hard 1600-column ceiling — that is migration 058's
-- disaster, where `creatures` reached 1600 of 1600 (1,575 dropped, 25 live) and
-- could accept nothing further. So the rewrite fires only while the *old*
-- expression is still in place, read from `pg_attrdef` rather than assumed.
--
-- One statement, in a DO block, per `scripts/lint-migrations.sh` rule 2 and the
-- `psql -f` half-apply hazard it guards.
--
-- MEASURED, NOT BELIEVED
--
-- Applied against the production schema inside a transaction that was rolled
-- back, before being registered. Results, verbatim:
--
--   before          ((committed_at IS NOT NULL) AND (committed_at < resolved_at))
--   after           ((committed_at IS NOT NULL) AND (committed_at < measured_at))
--   commit -60m, measured -30m  ->  true    (anchored first: verification)
--   commit -30m, measured -60m  ->  false   (transcription)
--   second application          ->  skipped, expression unchanged
--   dropped column slots        ->  1, not 2
--
-- The last two lines are the guard working. The second-to-last is why the guard
-- exists: without it this file would burn a slot on every boot.
--
-- `tests/projection_anchor_contract.rs` holds the invariant from the other side,
-- and is RED against production until this deploys — not because a migration is
-- pending, but because the column is a tautology right now.

DO $$
DECLARE
    expr TEXT;
BEGIN
    IF to_regclass('public.process_spacetime') IS NULL THEN
        RAISE NOTICE '215: process_spacetime absent (migration 141 pending) — nothing to do.';
        RETURN;
    END IF;

    SELECT pg_get_expr(d.adbin, d.adrelid)
      INTO expr
      FROM pg_attrdef d
      JOIN pg_attribute a ON a.attrelid = d.adrelid AND a.attnum = d.adnum
     WHERE d.adrelid = 'public.process_spacetime'::regclass
       AND a.attname  = 'committed_before_measured';

    IF expr IS NOT NULL AND expr LIKE '%measured_at%' THEN
        RAISE NOTICE '215: already compares against measured_at — skipping the rewrite.';
        RETURN;
    END IF;

    IF expr IS NOT NULL THEN
        RAISE NOTICE '215: replacing tautological expression %', expr;
        ALTER TABLE public.process_spacetime
            DROP COLUMN committed_before_measured;
    END IF;

    ALTER TABLE public.process_spacetime
        ADD COLUMN committed_before_measured BOOLEAN GENERATED ALWAYS AS (
            committed_at IS NOT NULL AND committed_at < measured_at
        ) STORED;

    COMMENT ON COLUMN public.process_spacetime.committed_before_measured IS
        'Was the prediction anchored before the world was measured? Compares '
        'committed_at against measured_at (the observation''s own phenomenon '
        'time), NOT against resolved_at. Migration 141 compared against '
        'resolved_at, which is NOW() at scoring time and always later than the '
        'commit the scorer just read — so the column was true for every row and '
        'proved nothing. This is the only column that distinguishes '
        'verification from transcription; a false here means the answer was '
        'already knowable when the prediction was filed.';
END $$;
