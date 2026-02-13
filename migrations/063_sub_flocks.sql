-- Migration 063: Sub-flocks and attraction tracking
-- Sub-flocks allow users to bring a group of creatures to a rabble as a cohesive unit.
-- Attraction tracking rewards creatures that draw others to join a rabble.

-- Sub-flock table: a named group of creatures within a rabble
CREATE TABLE IF NOT EXISTS swarm_sub_flocks (
    sub_flock_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    species_filter TEXT,
    formation_algorithm_id UUID REFERENCES swarm_algorithms(algorithm_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sub_flocks_swarm ON swarm_sub_flocks(swarm_id);

-- Link creature flights to sub-flocks
ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS sub_flock_id UUID REFERENCES swarm_sub_flocks(sub_flock_id);

-- Attraction tracking: which creature attracted the joiner
ALTER TABLE creature_flights ADD COLUMN IF NOT EXISTS attracted_by_creature_id UUID;

-- Attraction score on creatures: accumulated credits earned by attracting others
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS attraction_score INT NOT NULL DEFAULT 0;

-- Update tx_type constraint to include attraction_reward
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
        'observation_session_create', 'observation_ingest',
        'execution_royalty', 'agent_collaboration_payout',
        'creature_art',
        'platform_read',
        'agent_collect_out', 'agent_collect_in',
        'agent_allocate_dream', 'agent_allocate_education', 'agent_allocate_coherence',
        'formation_activate',
        'attraction_reward'
    ));
