-- Migration 064: Creature animation layers for parametric wing animation
-- Stores segmented body/wing images for client-side Chen et al. wing flapping.
-- Railway FS is ephemeral — layers MUST be in DB (same pattern as creature_images).

CREATE TABLE IF NOT EXISTS creature_animation_layers (
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    layer_name  TEXT NOT NULL,  -- 'body', 'left_wing', 'right_wing'
    image_bytes BYTEA NOT NULL,
    mime_type   TEXT NOT NULL DEFAULT 'image/png',
    file_size   INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (creature_id, layer_name)
);

CREATE INDEX IF NOT EXISTS idx_animation_layers_creature
    ON creature_animation_layers(creature_id);

-- Track animation readiness on the creature itself
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS animation_status TEXT;
-- NULL = not animated, 'processing', 'ready', 'failed'

-- Add creature_animate tx_type to credit ledger constraint
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
        'attraction_reward',
        'creature_animate'
    ));
