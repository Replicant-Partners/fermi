-- Migration 184: Spec 22 §1c embedding-provenance invariant, boot-safe
--
-- SUPERSEDES migration 136, which is intentionally NOT wired into
-- run_migrations. 136 was correct in intent and unusable in this runner:
--
--   * It uses bare `ALTER TABLE … ADD CONSTRAINT`. PostgreSQL has no
--     `ADD CONSTRAINT IF NOT EXISTS`, and run_migrations re-executes every
--     file on EVERY boot — so 136 would succeed once and then fail on every
--     subsequent boot, forever, with the error swallowed by the runner.
--
--   * It is 12 top-level statements. run_migrations sends each file through
--     `sqlx::raw_sql` as one batch, and PgBouncer in transaction mode has
--     repeatedly eaten multi-statement batches here (the v0.10.27 class of
--     bug). Single DO block is the house rule for exactly this reason.
--
-- That is why 136 sat on disk unwired since it was written: not because the
-- invariant was wrong, but because nobody could safely turn it on.
--
-- WHY IT IS SAFE TO ENABLE NOW
--
-- These are CHECK constraints; adding one to a table with violating rows
-- fails. Nobody knew the violation count, so nobody dared. The 2026-08-06
-- integrity audit measured it across all five constrained tables:
--
--     episodes 0 · semantic_rules 0 · entities 0 · communities 0 ·
--     shopping_profiles 0
--
-- Reconciled to zero, therefore enforceable. That ordering — measure, then
-- enforce — is the rule; see docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md.
--
-- NOT VALID + VALIDATE keeps the lock window short: ADD … NOT VALID is an
-- O(1) metadata change, VALIDATE scans without blocking writes.

DO $$
DECLARE
    r            record;
    v_added      integer := 0;
    v_violations bigint;
BEGIN
    FOR r IN
        SELECT * FROM (VALUES
            ('episodes',          'episodes_embedding_has_provenance',          'embedding'),
            ('semantic_rules',    'semantic_rules_embedding_has_provenance',    'embedding'),
            ('entities',          'entities_embedding_has_provenance',          'embedding'),
            ('communities',       'communities_embedding_has_provenance',       'embedding'),
            ('shopping_profiles', 'shopping_profiles_embedding_has_provenance', 'composite_embedding')
        ) AS t(tbl, con, vec_col)
    LOOP
        CONTINUE WHEN to_regclass('public.' || r.tbl) IS NULL;

        -- Idempotency: pg_constraint is the only reliable "already applied"
        -- signal available, since there is no migration ledger yet.
        CONTINUE WHEN EXISTS (
            SELECT 1 FROM pg_constraint c
             WHERE c.conrelid = ('public.' || r.tbl)::regclass
               AND c.conname  = r.con
        );

        -- Re-measure rather than trusting the audit snapshot. A constraint
        -- that fails here would abort the whole boot migration pass, so
        -- refuse loudly instead and leave the invariant off.
        EXECUTE format(
            'SELECT count(*) FROM public.%I WHERE %I IS NOT NULL AND (
                 embedding_model_id IS NULL OR embedding_model_version IS NULL
                 OR embedding_dim IS NULL)',
            r.tbl, r.vec_col
        ) INTO v_violations;

        IF v_violations > 0 THEN
            RAISE WARNING
                '[mig 184] % row(s) in % lack embedding provenance — constraint % NOT added. Backfill with scripts/backfill_embedding_provenance.rs first.',
                v_violations, r.tbl, r.con;
            CONTINUE;
        END IF;

        EXECUTE format(
            'ALTER TABLE public.%I ADD CONSTRAINT %I CHECK (
                 %I IS NULL OR (
                     embedding_model_id      IS NOT NULL
                 AND embedding_model_version IS NOT NULL
                 AND embedding_dim           IS NOT NULL
                 )
             ) NOT VALID', r.tbl, r.con, r.vec_col
        );
        EXECUTE format('ALTER TABLE public.%I VALIDATE CONSTRAINT %I', r.tbl, r.con);

        v_added := v_added + 1;
        RAISE NOTICE '[mig 184] enforced provenance invariant on %', r.tbl;
    END LOOP;

    -- Sidecar: a stored vector's length must equal its declared dim.
    IF to_regclass('public.embedding_provenance') IS NOT NULL
       AND NOT EXISTS (
           SELECT 1 FROM pg_constraint
            WHERE conrelid = 'public.embedding_provenance'::regclass
              AND conname  = 'embedding_provenance_dim_matches'
       )
    THEN
        SELECT count(*) INTO v_violations
          FROM public.embedding_provenance
         WHERE embedding IS NOT NULL AND vector_dims(embedding) <> dim;

        IF v_violations > 0 THEN
            RAISE WARNING '[mig 184] % embedding_provenance row(s) have dim mismatch — constraint NOT added', v_violations;
        ELSE
            ALTER TABLE public.embedding_provenance
                ADD CONSTRAINT embedding_provenance_dim_matches
                CHECK (embedding IS NULL OR vector_dims(embedding) = dim) NOT VALID;
            ALTER TABLE public.embedding_provenance
                VALIDATE CONSTRAINT embedding_provenance_dim_matches;
            v_added := v_added + 1;
            RAISE NOTICE '[mig 184] enforced dim-match invariant on embedding_provenance';
        END IF;
    END IF;

    IF v_added > 0 THEN
        RAISE NOTICE '[mig 184] added % constraint(s)', v_added;
    END IF;
END $$;
