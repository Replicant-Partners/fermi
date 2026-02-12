-- AKP (Agent Knowledge Protocol) Foundation
-- Cross-agent ontology alignment, knowledge transfer, and collaboration coherence

-- Agent alignment scores (ontology similarity between agent pairs)
CREATE TABLE IF NOT EXISTS agent_alignments (
    alignment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    target_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    alignment_score FLOAT NOT NULL DEFAULT 0.0,
    shared_entity_count INTEGER NOT NULL DEFAULT 0,
    divergent_entity_count INTEGER NOT NULL DEFAULT 0,
    shared_entities JSONB NOT NULL DEFAULT '[]',
    divergent_entities JSONB NOT NULL DEFAULT '[]',
    last_computed_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(source_agent_id, target_agent_id)
);

-- Pairwise coherence history (from workspace interactions)
CREATE TABLE IF NOT EXISTS pairwise_coherence (
    coherence_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_a_id UUID NOT NULL REFERENCES agents(agent_id),
    agent_b_id UUID NOT NULL REFERENCES agents(agent_id),
    workspace_id UUID,
    global_score FLOAT NOT NULL,
    principle_scores JSONB NOT NULL DEFAULT '{}',
    episode_count INTEGER NOT NULL DEFAULT 1,
    computed_at TIMESTAMPTZ DEFAULT NOW()
);

-- Knowledge transfer log (append-only)
CREATE TABLE IF NOT EXISTS knowledge_transfers (
    transfer_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    target_agent_id UUID NOT NULL REFERENCES agents(agent_id),
    transfer_type TEXT NOT NULL,
    item_count INTEGER NOT NULL DEFAULT 0,
    accepted_count INTEGER NOT NULL DEFAULT 0,
    rejected_count INTEGER NOT NULL DEFAULT 0,
    conflict_count INTEGER NOT NULL DEFAULT 0,
    details JSONB NOT NULL DEFAULT '{}',
    transferred_at TIMESTAMPTZ DEFAULT NOW()
);

-- Agent interaction policies (socialization rules)
CREATE TABLE IF NOT EXISTS agent_interaction_policies (
    policy_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id UUID NOT NULL REFERENCES agents(agent_id),
    policy_type TEXT NOT NULL,
    target_agent_id UUID,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(agent_id, policy_type, target_agent_id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_alignments_source ON agent_alignments(source_agent_id);
CREATE INDEX IF NOT EXISTS idx_alignments_target ON agent_alignments(target_agent_id);
CREATE INDEX IF NOT EXISTS idx_pairwise_agents ON pairwise_coherence(agent_a_id, agent_b_id);
CREATE INDEX IF NOT EXISTS idx_pairwise_workspace ON pairwise_coherence(workspace_id);
CREATE INDEX IF NOT EXISTS idx_transfers_source ON knowledge_transfers(source_agent_id);
CREATE INDEX IF NOT EXISTS idx_transfers_target ON knowledge_transfers(target_agent_id);
CREATE INDEX IF NOT EXISTS idx_policies_agent ON agent_interaction_policies(agent_id);

-- Add AKP tx_types to credit_ledger constraint
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
            'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
            'collection_create', 'rabble_chat',
            'gbif_contribution', 'rabble_platform_fee',
            'akp_alignment', 'akp_transfer', 'akp_bootstrap', 'akp_diff'
        ));
EXCEPTION
    WHEN others THEN NULL;
END $$;
