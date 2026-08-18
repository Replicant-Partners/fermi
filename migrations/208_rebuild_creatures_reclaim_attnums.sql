-- Migration 208: rebuild `creatures` to reclaim 1,575 dropped column slots.
--
-- ## Why the table cannot take another column
--
-- Postgres assigns every column a permanent `attnum`. Dropping a column marks
-- it `attisdropped` but never releases the number, and the hard ceiling of 1600
-- counts dropped and live alike. The only way to reclaim them is to build a new
-- table and move the rows: `VACUUM FULL` does not do it, and neither does a
-- type-change rewrite.
--
-- `creatures` reached **1600 of 1600 — 1,575 dropped, 25 live**, so
-- `ALTER TABLE creatures ADD COLUMN` now fails unconditionally with "tables can
-- have at most 1600 columns". It is the core table of the Rabble product.
--
-- The cause is fixed separately, in migrations 052, 058 and 065: those files
-- staged five columns that 080 dropped, and `run_migrations` replays every file
-- on every boot, so five slots were consumed per restart for roughly 315 boots.
-- **That fix must be deployed before this one runs**, or the reclaimed space
-- starts draining again immediately. It is, in the same release.
--
-- ## What this does
--
-- `CREATE TABLE ... (LIKE creatures ...)` copies only LIVE columns — dropped ones
-- are invisible to it — which is the whole mechanism. The rest is putting back
-- everything that hangs off the table:
--
--   26 inbound foreign keys, from 23 tables
--    1 outbound foreign key (owner_id -> users, NOT VALID)
--    5 secondary indexes plus the primary key
--    1 dependent view (rbac_orphans)
--    0 triggers, 0 row-level security policies
--
-- Inbound keys, indexes and the view are **captured from the catalogue at
-- runtime rather than transcribed here.** Twenty-six hand-copied constraint
-- definitions would be twenty-six chances to differ from what is actually
-- deployed, and a foreign key silently recreated with the wrong `ON DELETE`
-- action would be worse than the problem being fixed — it would look identical
-- and behave differently at the moment it mattered. `pg_get_constraintdef` and
-- `pg_get_indexdef` cannot drift from the database they are read out of.
--
-- The primary key is the one exception, stated literally, because
-- `INCLUDING INDEXES` would recreate every index under a generated name derived
-- from the temporary table (`creatures_rebuild_pkey` and friends) and renaming
-- them back by matching on column lists is guesswork. Indexes are therefore
-- excluded from `LIKE` and replayed from their own definitions, which carry their
-- real names.
--
-- ## Idempotence, and why it matters more than usual here
--
-- This file replays on every boot like every other. Rebuilding the product's
-- core table on every restart would be reckless, so the whole body is guarded on
-- the condition it exists to remove: **`creatures` has at least one dropped
-- column.** After a successful run that count is zero and this migration is a
-- no-op forever. The guard is the work being finished, not a flag someone has to
-- remember to set.
--
-- ## Atomicity
--
-- One DO block, which through PgBouncer is a single statement and therefore
-- genuinely atomic. That is not a style preference here: between dropping the
-- inbound foreign keys and putting them back, referential integrity does not
-- hold. If that window could be left open by a half-applied migration, the
-- database would be silently unconstrained. Either the whole swap lands or none
-- of it does.
--
-- Row count at time of writing: 131. The ACCESS EXCLUSIVE lock is held for
-- milliseconds.

DO $$
DECLARE
    dropped_count int;
    live_before   int;
    rows_before   bigint;
    rows_after    bigint;
    inbound_fks   text[] := '{}';
    index_defs    text[] := '{}';
    outbound_fk   text;
    view_def      text;
    stmt          text;
    r             record;
BEGIN
    SELECT count(*) FILTER (WHERE attisdropped),
           count(*) FILTER (WHERE NOT attisdropped)
      INTO dropped_count, live_before
      FROM pg_attribute
     WHERE attrelid = 'public.creatures'::regclass AND attnum > 0;

    IF dropped_count = 0 THEN
        RAISE NOTICE 'creatures has no dropped columns; rebuild already done';
        RETURN;
    END IF;

    SELECT count(*) INTO rows_before FROM public.creatures;
    RAISE NOTICE 'rebuilding creatures: % live columns, % dropped, % rows',
        live_before, dropped_count, rows_before;

    -- ── 1. Capture everything that will be destroyed with the table ──────
    --
    -- Read from the catalogue, in the same transaction that will drop it, so
    -- what goes back is exactly what was there.

    FOR r IN
        SELECT conrelid::regclass AS tbl, conname, pg_get_constraintdef(oid) AS def
          FROM pg_constraint
         WHERE confrelid = 'public.creatures'::regclass AND contype = 'f'
    LOOP
        inbound_fks := inbound_fks || format(
            'ALTER TABLE %s ADD CONSTRAINT %I %s', r.tbl, r.conname, r.def);
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', r.tbl, r.conname);
    END LOOP;
    RAISE NOTICE '  captured and dropped % inbound foreign key(s)', array_length(inbound_fks, 1);

    -- Secondary indexes only. The primary key arrives with the new table.
    FOR r IN
        SELECT indexdef FROM pg_indexes
         WHERE schemaname = 'public' AND tablename = 'creatures'
           AND indexname <> 'creatures_pkey'
    LOOP
        index_defs := index_defs || r.indexdef;
    END LOOP;

    SELECT pg_get_constraintdef(oid) INTO outbound_fk
      FROM pg_constraint
     WHERE conrelid = 'public.creatures'::regclass AND contype = 'f'
       AND conname = 'creatures_owner_id_fk';

    -- The view must go before the table can be dropped, and come back after.
    SELECT pg_get_viewdef('public.rbac_orphans'::regclass, true) INTO view_def;
    IF view_def IS NOT NULL THEN
        DROP VIEW public.rbac_orphans;
    END IF;

    -- ── 2. New table, live columns only ──────────────────────────────────
    --
    -- INCLUDING INDEXES is deliberately absent; see the header. DEFAULTS,
    -- COMMENTS and STORAGE are carried, and column types plus NOT NULL come
    -- with LIKE unconditionally.
    CREATE TABLE public.creatures_rebuild (
        LIKE public.creatures
        INCLUDING DEFAULTS
        INCLUDING CONSTRAINTS
        INCLUDING COMMENTS
        INCLUDING STORAGE
    );

    INSERT INTO public.creatures_rebuild SELECT * FROM public.creatures;
    SELECT count(*) INTO rows_after FROM public.creatures_rebuild;

    IF rows_after <> rows_before THEN
        RAISE EXCEPTION 'creatures rebuild copied % of % rows; aborting',
            rows_after, rows_before;
    END IF;

    -- ── 3. Swap ──────────────────────────────────────────────────────────
    DROP TABLE public.creatures;
    ALTER TABLE public.creatures_rebuild RENAME TO creatures;

    -- ── 4. Put everything back ───────────────────────────────────────────
    ALTER TABLE public.creatures
        ADD CONSTRAINT creatures_pkey PRIMARY KEY (creature_id);

    IF outbound_fk IS NOT NULL THEN
        -- Replayed verbatim, NOT VALID included. Validating it here would fail
        -- on any creature whose owner row has since gone, and those orphans are
        -- exactly what `rbac_orphans` exists to report rather than to reject.
        EXECUTE format('ALTER TABLE public.creatures ADD CONSTRAINT %I %s',
                       'creatures_owner_id_fk', outbound_fk);
    END IF;

    FOREACH stmt IN ARRAY index_defs LOOP
        EXECUTE stmt;
    END LOOP;

    FOREACH stmt IN ARRAY inbound_fks LOOP
        EXECUTE stmt;
    END LOOP;

    IF view_def IS NOT NULL THEN
        EXECUTE format('CREATE VIEW public.rbac_orphans AS %s', view_def);
    END IF;

    -- ── 5. Prove it worked, in the same transaction that did it ──────────
    SELECT count(*) FILTER (WHERE attisdropped)
      INTO dropped_count
      FROM pg_attribute
     WHERE attrelid = 'public.creatures'::regclass AND attnum > 0;
    IF dropped_count <> 0 THEN
        RAISE EXCEPTION 'creatures still reports % dropped columns after rebuild',
            dropped_count;
    END IF;

    IF (SELECT count(*) FROM pg_constraint
         WHERE confrelid = 'public.creatures'::regclass AND contype = 'f')
       <> array_length(inbound_fks, 1) THEN
        RAISE EXCEPTION 'inbound foreign keys not fully restored: % of %',
            (SELECT count(*) FROM pg_constraint
              WHERE confrelid = 'public.creatures'::regclass AND contype = 'f'),
            array_length(inbound_fks, 1);
    END IF;

    RAISE NOTICE '  done: % rows, 0 dropped columns, % inbound keys restored',
        rows_after, array_length(inbound_fks, 1);
END $$;
