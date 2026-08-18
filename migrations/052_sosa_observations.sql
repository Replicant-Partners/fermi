-- Migration 052: Universal sensor observations (W3C SSN/SOSA)
-- Domain-agnostic telemetry ingestion: any sensor, any property, one table.
-- See: https://www.w3.org/TR/vocab-ssn/

-- Platforms host sensors (drone, weather station, greenhouse, vehicle, wearable)
CREATE TABLE IF NOT EXISTS sosa_platforms (
    platform_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    platform_type TEXT NOT NULL,
    description TEXT,
    location JSONB DEFAULT '{}',
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sensors belong to platforms and observe properties
CREATE TABLE IF NOT EXISTS sosa_sensors (
    sensor_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    platform_id UUID NOT NULL REFERENCES sosa_platforms(platform_id),
    name TEXT NOT NULL,
    observable_property TEXT NOT NULL,
    unit TEXT,
    description TEXT,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Observation sessions (collection windows, like swarm_sessions but universal)
CREATE TABLE IF NOT EXISTS observation_sessions (
    session_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    platform_id UUID NOT NULL REFERENCES sosa_platforms(platform_id),
    name TEXT NOT NULL,
    description TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    started_at TIMESTAMPTZ DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'
);

-- Individual observations (append-only, high-frequency)
-- Each row = one sosa:Observation
CREATE TABLE IF NOT EXISTS sosa_observations (
    observation_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES observation_sessions(session_id),
    sensor_id UUID REFERENCES sosa_sensors(sensor_id),
    platform_id UUID NOT NULL,
    observable_property TEXT NOT NULL,
    feature_of_interest TEXT,
    result_value DOUBLE PRECISION NOT NULL,
    result_unit TEXT,
    phenomenon_time BIGINT NOT NULL,
    result_time BIGINT,
    procedure TEXT,
    extra JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_sosa_obs_session ON sosa_observations(session_id);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_session_time ON sosa_observations(session_id, phenomenon_time);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_property ON sosa_observations(session_id, observable_property);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_sensor ON sosa_observations(sensor_id);
CREATE INDEX IF NOT EXISTS idx_sosa_obs_platform ON sosa_observations(platform_id);
CREATE INDEX IF NOT EXISTS idx_sosa_platforms_owner ON sosa_platforms(owner_id);
CREATE INDEX IF NOT EXISTS idx_sosa_sensors_platform ON sosa_sensors(platform_id);
CREATE INDEX IF NOT EXISTS idx_obs_sessions_owner ON observation_sessions(owner_id);
CREATE INDEX IF NOT EXISTS idx_obs_sessions_platform ON observation_sessions(platform_id);

-- Opt-in flag for Rabble creatures: owner must explicitly enable SOSA telemetry sharing
-- Defaults to false — respects AKP consent model (agent_interaction_policies roadmap)
--
-- Guarded 2026-08-18. Migration 078 copies this into
-- `creature_conditions.sosa_opt_in` and 080 drops it, so on an already-migrated
-- database this line re-added a column that was dropped again moments later —
-- every boot. Postgres holds a dropped column's slot forever against the hard
-- 1600-column ceiling, and together with 058 and 065 this burned five slots per
-- boot until `creatures` hit 1600 of 1600 (1,575 dropped, 25 live) and could
-- accept nothing further. See migration 058 for the full account.
--
-- Staged only while the destination is still absent.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'creature_conditions'
           AND column_name = 'sosa_opt_in'
    ) THEN
        ALTER TABLE public.creatures
            ADD COLUMN IF NOT EXISTS sosa_opt_in BOOLEAN NOT NULL DEFAULT false;
    END IF;
END $$;

-- Add new tx_types
--
-- Wrapped in a DO block 2026-08-18. The commit that did so claimed this pair
-- half-applied through PgBouncer — DROP committing, ADD failing, constraint
-- deleted — and offered that as the reason `credit_ledger_tx_type_check` was
-- absent. **That claim was wrong and had not been tested.**
--
-- `run_migrations` hands each file WHOLE to `sqlx::raw_sql`, which sends it as one
-- simple query, and Postgres wraps a multi-statement simple query in a single
-- implicit transaction. A failure anywhere rolls the whole file back; the pooler
-- never gets the chance to split it. Measured through the production pooler in
-- `tests/migration_atomicity.rs`, which exists because that belief was repeated
-- in a lint rule, in migration headers and in a paper without anyone checking it.
--
-- Why the DO block stays: `psql -f` runs each statement in its own transaction,
-- and that is how these files are validated by hand. The pair genuinely can
-- half-apply there. So the wrapping is still correct — for a smaller and true
-- reason.
--
-- Why this migration still fails: its list predates a dozen tx_types the code now
-- emits, so the ADD cannot succeed on an established database. It reports as
-- failing, honestly, and changes nothing. Migration 204 holds the authoritative
-- list.
--
-- And why `credit_ledger_tx_type_check` was missing: **still unknown.** The
-- replay path could not have deleted it, and it was never managed by
-- `ensure_critical_schema`. An admitted gap beats an invented mechanism.
DO $$
BEGIN
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee',
        'publish_fee', 'eval_fee',
        'consolidation_fee',
        'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
        'avatar_generate', 'embedding_import',
        'ontology_generation', 'prompt_generation', 'file_write',
        'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
        'collection_create', 'rabble_chat',
        'gbif_contribution', 'rabble_platform_fee',
        'akp_alignment', 'akp_transfer', 'akp_bootstrap', 'akp_diff',
        'swarm_session_create', 'swarm_telemetry_ingest',
        'observation_session_create', 'observation_ingest'
    ));
END $$;
