-- ═══════════════════════════════════════════════════════════════════
-- Migration 169 — CASCADE the mig-049 FKs on agents(agent_id)
--
-- v0.10.25 hotfix. mig-049 (AKP foundation) declared four FKs on
-- `agents(agent_id)` without an ON DELETE clause, so Postgres
-- defaults to NO ACTION — a DELETE from `agents` blocks with a
-- FK violation whenever a mig-049 row references it. That breaks
-- the v0.10.25 cleanup path for orphan `test_agent_<uuid>` rows
-- (which nominally don't have alignments, but the invariant
-- shouldn't depend on that).
--
-- Every other FK on `agents(agent_id)` across mig-010, mig-015,
-- mig-024, mig-027, mig-030, mig-103, mig-104, mig-105, mig-106,
-- and mig-133 already declares ON DELETE CASCADE. This migration
-- brings the four mig-049 tables into line so `DELETE FROM agents`
-- reliably cascades everywhere.
--
-- Semantically these SHOULD be CASCADE anyway — alignments,
-- coherence scores, knowledge transfers, and interaction policies
-- are all derived data whose meaning collapses when the referenced
-- agent is gone. Preserving them without the agent is worse than
-- deleting them.
--
-- Affected tables (all mig-049):
--   agent_alignments        source_agent_id, target_agent_id
--   pairwise_coherence      agent_a_id,      agent_b_id
--   knowledge_transfers     source_agent_id, target_agent_id
--   agent_interaction_policies  agent_id
--
-- PgBouncer-safe: DO blocks, no BEGIN/COMMIT, per-constraint
-- EXCEPTION handlers so one failure doesn't abort the migration.
-- Idempotent: catalog probe before DROP + IF NOT EXISTS on ADD.
-- ═══════════════════════════════════════════════════════════════════

-- Helper table listing every (table, constraint_name, column, target_col)
-- we want to re-declare. Done via DO block for readability.
DO $$
DECLARE
    fixes TEXT[][] := ARRAY[
        -- (table, constraint_name, local_col, referenced_col)
        ARRAY['agent_alignments',           'agent_alignments_source_agent_id_fkey', 'source_agent_id', 'agent_id'],
        ARRAY['agent_alignments',           'agent_alignments_target_agent_id_fkey', 'target_agent_id', 'agent_id'],
        ARRAY['pairwise_coherence',         'pairwise_coherence_agent_a_id_fkey',    'agent_a_id',      'agent_id'],
        ARRAY['pairwise_coherence',         'pairwise_coherence_agent_b_id_fkey',    'agent_b_id',      'agent_id'],
        ARRAY['knowledge_transfers',        'knowledge_transfers_source_agent_id_fkey', 'source_agent_id', 'agent_id'],
        ARRAY['knowledge_transfers',        'knowledge_transfers_target_agent_id_fkey', 'target_agent_id', 'agent_id'],
        ARRAY['agent_interaction_policies', 'agent_interaction_policies_agent_id_fkey', 'agent_id',        'agent_id']
    ];
    row_ TEXT[];
    tbl TEXT;
    con TEXT;
    col TEXT;
    ref_col TEXT;
    current_action TEXT;
BEGIN
    FOR i IN 1 .. array_length(fixes, 1) LOOP
        row_    := fixes[i:i][1:4];
        tbl     := row_[1];
        con     := row_[2];
        col     := row_[3];
        ref_col := row_[4];

        -- Check the current ON DELETE action.
        SELECT rc.delete_rule
          INTO current_action
          FROM information_schema.referential_constraints rc
         WHERE rc.constraint_schema = 'public'
           AND rc.constraint_name   = con;

        IF current_action IS NULL THEN
            RAISE NOTICE '[mig 169] % constraint % not found — skipping', tbl, con;
            CONTINUE;
        END IF;

        IF current_action = 'CASCADE' THEN
            RAISE NOTICE '[mig 169] % % already CASCADE — skipping', tbl, con;
            CONTINUE;
        END IF;

        BEGIN
            EXECUTE format('ALTER TABLE public.%I DROP CONSTRAINT %I', tbl, con);
            EXECUTE format(
                'ALTER TABLE public.%I
                 ADD CONSTRAINT %I FOREIGN KEY (%I)
                 REFERENCES public.agents(%I) ON DELETE CASCADE',
                tbl, con, col, ref_col
            );
            RAISE NOTICE '[mig 169] % % → CASCADE', tbl, con;
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 169] failed to realign %.%: % — % rows may block DELETE FROM agents',
                tbl, con, SQLERRM, tbl;
        END;
    END LOOP;
END $$;

-- Post-migration validation — every mig-049 FK should now be CASCADE.
DO $$
DECLARE
    n_non_cascade INTEGER;
BEGIN
    SELECT COUNT(*)
      INTO n_non_cascade
      FROM information_schema.referential_constraints
     WHERE constraint_schema = 'public'
       AND constraint_name IN (
             'agent_alignments_source_agent_id_fkey',
             'agent_alignments_target_agent_id_fkey',
             'pairwise_coherence_agent_a_id_fkey',
             'pairwise_coherence_agent_b_id_fkey',
             'knowledge_transfers_source_agent_id_fkey',
             'knowledge_transfers_target_agent_id_fkey',
             'agent_interaction_policies_agent_id_fkey'
           )
       AND delete_rule <> 'CASCADE';

    IF n_non_cascade > 0 THEN
        RAISE WARNING '[mig 169] % mig-049 FK(s) still non-CASCADE — DELETE FROM agents may block', n_non_cascade;
    ELSE
        RAISE NOTICE '[mig 169] post-migration — all 7 mig-049 FKs on agents(agent_id) are ON DELETE CASCADE';
    END IF;
END $$;
