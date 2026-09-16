-- Migration 238: `carbon_emission_factors` — the ledger that makes an
-- emission factor falsifiable.
--
-- ## Why this table exists
--
-- `carbon_accountant` had six `Sourced` fields and, until this migration, six
-- entries in `CROSS_CHECK_EXEMPTIONS`. The reason was honest and is worth
-- restating: the platform holds no copy of ecoinvent, Agribalyse or the DEFRA
-- factors, three of which are licensed rather than merely absent. There was no
-- second copy of anything one JOIN away, the way `genome_profiler.taxonomy`
-- had a GBIF-verified row sitting on the creature row.
--
-- (The counts moved after this migration was written, and the direction is the
-- point: the second-publisher fields added three more `Sourced` paths, so the
-- agent now declares nine. Two of the nine carry a live cross-check — this
-- one, and the independence check on `corroborating_dataset` — and seven
-- remain exempt, each naming its own route out. `src/grounding_trust.rs` is
-- authoritative for the current tally; this header is authoritative only for
-- why the table exists.)
--
-- This builds the second copy out of the agent's own work. Every factor the
-- agent retrieves is appended here with the key that makes it comparable —
-- (material, geography, reference_year, dataset) — so that the SECOND time any
-- run resolves the same key, the two values can be compared. No external
-- corpus, no licence, no egress. The check that results is declared as
-- `inventory.items[].factor_kg_co2e_per_kg`'s `cross_check_sql`, and the
-- exemption it replaces is deleted in the same commit.
--
-- ## Append-only, deliberately
--
-- There is NO unique constraint on the key, and that is the entire point. A
-- cache would keep one row per key and overwrite; this keeps every resolution,
-- because two independent answers to the same question is precisely the
-- evidence a cross-check needs. Deduplicating would destroy the signal in
-- order to save bytes on a table that grows by a handful of rows per product.
--
-- ## Why `dataset` is part of the comparison key
--
-- ecoinvent and Agribalyse legitimately publish different factors for the same
-- material, geography and year: different system models, different allocation
-- rules. Comparing across datasets would fire on entirely correct behaviour,
-- and a check that fires on correct output gets switched off — the
-- switching-off looking like cleanup. See `LeakRule::Quantity`'s note about
-- GBIF in src/grounding_trust.rs. Two readings of the SAME published figure
-- must agree; two datasets need not.
--
-- ## Why `retrieval` exists when only one value is ever written
--
-- The handler inserts `'search'` rows and nothing else. The column is here so
-- that a later change which serves a factor from this table back into a
-- statement cannot silently pollute the evidence: a value this ledger supplied
-- is not an independent confirmation of itself, and a cross-check that
-- compares a number against a copy of itself is the `xgd` trap — three numbers
-- we made consistent are evidence of nothing. The check predicates on
-- `retrieval = 'search'` so that the guard is in the query rather than in
-- somebody's memory.

-- ## Why the whole thing is one DO block
--
-- Five statements — the table, two indexes and two COMMENTs — and through
-- PgBouncer a multi-statement migration can commit the first and lose the
-- rest. `scripts/lint-migrations.sh` warns about exactly this, and the
-- constraint migrations in this family already wrap for the same reason
-- (tests/constraint_trust.rs). The failure would be quiet and awkward rather
-- than loud: the table would exist without its indexes, so the self-join in
-- the cross-check would still return the right answer while scanning the whole
-- ledger, and the COMMENTs — which are where a DBA reading `\d
-- carbon_emission_factors` learns that the duplicates are deliberate — would
-- simply not be there. Everything below is `IF NOT EXISTS`, so it stays as
-- replay-safe as the rest of the directory.

DO $$ BEGIN

    CREATE TABLE IF NOT EXISTS carbon_emission_factors (
        id                    BIGSERIAL PRIMARY KEY,

        -- The comparison key. Normalised in Rust (lowercased, whitespace
        -- collapsed) so that "Dried hibiscus  calyces" and "dried hibiscus
        -- calyces" are one key rather than two that never meet.
        material_key          TEXT        NOT NULL,
        geography             TEXT        NOT NULL,
        reference_year        INTEGER     NOT NULL,
        dataset_key           TEXT        NOT NULL,

        -- What was retrieved, as retrieved.
        value_kg_co2e_per_kg  NUMERIC     NOT NULL CHECK (value_kg_co2e_per_kg >= 0),
        material              TEXT        NOT NULL,
        dataset               TEXT,
        lca_basis             TEXT,
        source_url            TEXT,
        source_title          TEXT,

        -- How it got here. `search` is the only value the handler writes; see the
        -- header for why the column exists anyway.
        retrieval             TEXT        NOT NULL DEFAULT 'search'
                                          CHECK (retrieval IN ('search', 'platform_cache')),

        -- Provenance of the resolution itself, so a disagreement can be traced to
        -- two runs rather than merely reported as a number.
        agent_name            TEXT        NOT NULL,
        workspace_id          UUID,
        action_id             UUID,
        item_id               TEXT,
        resolved_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
    );

    -- The cross-check self-joins on the four-part key, and the cache read looks up
    -- the three-part key. One index serves both, because the three-part prefix is
    -- leftmost.
    CREATE INDEX IF NOT EXISTS carbon_emission_factors_key_idx
        ON carbon_emission_factors (material_key, geography, reference_year, dataset_key);

    CREATE INDEX IF NOT EXISTS carbon_emission_factors_resolved_idx
        ON carbon_emission_factors (resolved_at DESC);

    COMMENT ON TABLE carbon_emission_factors IS
        'Append-only ledger of every emission factor carbon_accountant has retrieved. '
        'NOT a cache with one row per key: duplicates are the evidence. Two rows sharing '
        '(material_key, geography, reference_year, dataset_key) are two independent readings '
        'of one published figure and must agree; the cross_check_sql on '
        'inventory.items[].factor_kg_co2e_per_kg counts the pairs that do not. '
        'Rows with retrieval <> ''search'' are excluded from that comparison, because a value '
        'this table supplied is not independent confirmation of itself.';

    COMMENT ON COLUMN carbon_emission_factors.dataset_key IS
        'Part of the comparison key on purpose. ecoinvent and Agribalyse legitimately differ '
        'for the same material/geography/year, so comparing across datasets would fire on '
        'correct behaviour — and a check that fires on correct output gets switched off.';

END $$;
