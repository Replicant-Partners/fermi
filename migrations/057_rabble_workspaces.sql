-- Migration 057: Rabble workspace integration
-- Agent payout tracking, new tx_types, personal workspace, relationship type

-- Agent payout tracking table
CREATE TABLE IF NOT EXISTS agent_episode_payouts (
    payout_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    episode_id UUID NOT NULL,
    agent_id UUID NOT NULL,
    workspace_id UUID,
    amount INTEGER NOT NULL,
    contribution_tier TEXT DEFAULT 'equal',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_agent_payouts_episode ON agent_episode_payouts(episode_id);
CREATE INDEX IF NOT EXISTS idx_agent_payouts_agent ON agent_episode_payouts(agent_id);

-- Add new tx_types for agent royalties and collaboration payouts
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
        'platform_read'
    ));

-- Personal workspace tracking
ALTER TABLE users ADD COLUMN IF NOT EXISTS personal_workspace_id UUID;

-- Allow 'system' relationship for auto-hired agents in rabble workspaces
ALTER TABLE workspace_agents DROP CONSTRAINT IF EXISTS workspace_agents_relationship_check;
ALTER TABLE workspace_agents ADD CONSTRAINT workspace_agents_relationship_check
    CHECK (relationship IN ('hired', 'owned', 'created_here', 'system'));
