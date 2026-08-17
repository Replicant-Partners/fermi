-- Migration 204: restore `credit_ledger_tx_type_check`, which has not existed
-- since before migration 032.
--
-- ## Seventeen declarations, zero constraints
--
-- `credit_ledger_tx_type_check` is declared by migrations 027, 030, 032, 035,
-- 042, 045, 049, 050, 051, 052, 057, 059, 061, 063, 064, 075 and 099. Three of
-- them exist for no purpose other than to fix it. `pg_constraint` has never
-- held it.
--
-- The mechanism, because it will recur:
--
--   1. Each early migration ran `DROP CONSTRAINT IF EXISTS` and `ADD
--      CONSTRAINT` as two TOP-LEVEL statements.
--   2. Through PgBouncer in transaction-pooling mode those are two separate
--      implicit transactions. There is no rollback between them.
--   3. The `ADD` failed, because rows already on the ledger violated the new
--      list. The `DROP` had already committed.
--   4. `run_migrations` logs a failed migration with `eprintln!` and continues.
--
-- So the net effect of each attempted repair was to DELETE the constraint, and
-- the only trace was a line in a boot log. Migration 075 finally wrapped the
-- pair in a DO block, which is correct and atomic — and by then the code had
-- invented 22 transaction types absent from 075's list, so its `ADD` could
-- never succeed. It has been a no-op ever since.
--
-- Nothing noticed because nothing asked. `schema_trust::SCHEMA_COLUMNS` would
-- have caught a missing column at boot; constraints had no equivalent. They do
-- now: `SCHEMA_CONSTRAINTS` plus `tests/constraint_trust.rs`, whose live tier
-- was red until this migration.
--
-- ## Why this list
--
-- The union of migration 075's 41 declared types and the 43 types actually
-- present on the ledger — 64 in total, of which 23 were previously
-- unacceptable (4,975 rows) and 21 are declared but never used.
--
-- Be clear about what that buys. A constraint whose list is derived from the
-- table cannot reject anything already there, so it makes no claim about the
-- 4,975 existing rows. What it does is close the hole going forward: `tx_type`
-- is a bare `&str` at every call site in `fermi-auth/src/credits.rs`, there is
-- no enum and no closed set in Rust, and so this CHECK is the only thing
-- between a typo and a silently mis-categorised row on the money table. A
-- misspelled type is currently accepted, charged, and invisible to every
-- report that groups by type: the money moves, it is just filed under a
-- category nobody queries.
--
-- The 21 unused-but-declared types are RETAINED rather than pruned. They name
-- planned or dormant features (`marketplace_*`, `akp_*`, `education_*`,
-- `withdrawal`), and dropping a type because no row happens to use it today
-- would make the next deploy of that feature fail at the ledger. Pruning is a
-- product decision, not a schema one.
--
-- The proper fix is a Rust enum so the compiler holds the set and this CHECK
-- becomes a restatement rather than the only copy. That is a refactor across
-- the economy and is deliberately not attempted here — see the follow-up
-- issue. This migration exists so that the ledger is guarded in the meantime.
--
-- ## Atomic
--
-- One DO block, so DROP and ADD succeed or fail together. If the ADD fails,
-- the DROP is rolled back and the previous constraint survives — which is the
-- property whose absence caused all of this.
DO $$
BEGIN
    ALTER TABLE public.credit_ledger
        DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;

    ALTER TABLE public.credit_ledger
        ADD CONSTRAINT credit_ledger_tx_type_check
        CHECK (tx_type IN (
            'agent_royalty_in', 'akp_alignment', 'akp_bootstrap', 'akp_diff',
            'akp_transfer', 'avatar_generate', 'collection_create', 'composition_dream',
            'consolidation_fee', 'creature_animate', 'creature_flight', 'creature_mint',
            'deposit', 'dream_topup', 'education_alloc', 'education_spend',
            'embedding_import', 'enemy_sensor_check', 'enemy_sensor_enable', 'eval_fee',
            'execution_fee', 'expedition', 'file_write', 'flight_plan',
            'fly', 'forage_scout', 'fork_fee', 'fork_royalty',
            'fpl_execute', 'gas_fee', 'gbif_contribution', 'genome_profiler_check',
            'genome_profiler_enable', 'grant', 'host_rabble', 'marketplace_listing_fee',
            'marketplace_match_payout', 'marketplace_match_purchase', 'observation_ingest', 'observation_session_create',
            'ontology_generation', 'perch', 'platform_read', 'polymarket_search',
            'polymarket_snapshot', 'prey_locator_enable', 'prey_locator_scan', 'prey_locator_stalk',
            'prey_locator_strategy', 'prompt_generation', 'publish_fee', 'publish_forecast',
            'rabble_chat', 'rabble_platform_fee', 'reconciliation', 'refund',
            'swarm_create', 'swarm_join', 'tether', 'transfer_in',
            'transfer_out', 'walk_in_fee', 'walk_in_revenue', 'withdrawal'
        ));

    COMMENT ON CONSTRAINT credit_ledger_tx_type_check ON public.credit_ledger IS
        'Closed set of ledger transaction types. THE ONLY closed set: '
        '`credit_charge` and friends in fermi-auth/src/credits.rs take '
        '`tx_type: &str`, so a misspelling is otherwise accepted and filed '
        'under a category no report queries. Declared in '
        '`schema_trust::SCHEMA_CONSTRAINTS` and verified live by '
        '`tests/constraint_trust.rs` — it was absent from before migration 032 '
        'until 204, because seventeen migrations declared it and none checked. '
        'Adding a tx_type means editing this list; the ADD is inside a DO block '
        'so a violating row fails the whole migration instead of silently '
        'leaving the table unconstrained.';
END $$;
