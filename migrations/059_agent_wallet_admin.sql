-- Migration 059: Agent Wallet Admin
-- Adds auto-collect percentage and new tx_types for collect/allocate flows

-- Auto-collect: percentage of royalty payouts auto-forwarded to agent owner (0 = manual only)
ALTER TABLE agents ADD COLUMN IF NOT EXISTS auto_collect_pct INTEGER NOT NULL DEFAULT 0;

-- Extend tx_type constraint with agent wallet admin types
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
        'agent_allocate_dream', 'agent_allocate_education', 'agent_allocate_coherence'
    ));
