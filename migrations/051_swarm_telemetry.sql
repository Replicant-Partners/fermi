-- Migration 051: Swarm telemetry ingestion tables
-- Supports Onto4MAT ontology (arxiv 2203.12955) data properties as typed columns

-- Swarm sessions (a telemetry collection window)
CREATE TABLE IF NOT EXISTS swarm_sessions (
    session_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    agent_count INTEGER NOT NULL DEFAULT 0,
    formation_type TEXT,
    mission_type TEXT,
    environment JSONB DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active',
    started_at TIMESTAMPTZ DEFAULT NOW(),
    ended_at TIMESTAMPTZ,
    metadata JSONB DEFAULT '{}'
);

-- Individual telemetry samples (append-only, high-frequency)
CREATE TABLE IF NOT EXISTS swarm_telemetry (
    telemetry_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES swarm_sessions(session_id),
    agent_label TEXT NOT NULL,
    agent_type TEXT NOT NULL DEFAULT 'artificial',
    timestamp_ms BIGINT NOT NULL,
    x_location DOUBLE PRECISION NOT NULL,
    y_location DOUBLE PRECISION NOT NULL,
    z_location DOUBLE PRECISION DEFAULT 0.0,
    heading DOUBLE PRECISION,
    speed DOUBLE PRECISION,
    energy DOUBLE PRECISION,
    distance_to_goal DOUBLE PRECISION,
    team_alignment DOUBLE PRECISION,
    team_cohesion DOUBLE PRECISION,
    team_separation DOUBLE PRECISION,
    influence DOUBLE PRECISION,
    action TEXT,
    temperament TEXT,
    extra JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_telemetry_session ON swarm_telemetry(session_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_session_time ON swarm_telemetry(session_id, timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_telemetry_agent ON swarm_telemetry(session_id, agent_label);
CREATE INDEX IF NOT EXISTS idx_sessions_owner ON swarm_sessions(owner_id);
CREATE INDEX IF NOT EXISTS idx_sessions_status ON swarm_sessions(status);

-- Add new tx_types for swarm telemetry gas fees
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
        'swarm_session_create', 'swarm_telemetry_ingest'
    ));
