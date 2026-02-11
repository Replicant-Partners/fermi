-- Rabble funding modes and QR tokens for swarm events
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS funding_mode TEXT NOT NULL DEFAULT 'hosted';
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS invite_pool INT NOT NULL DEFAULT 0;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS invite_pool_remaining INT NOT NULL DEFAULT 0;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS suggested_contribution INT NOT NULL DEFAULT 1;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS total_contributions INT NOT NULL DEFAULT 0;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS qr_token TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_swarm_events_qr_token ON swarm_events(qr_token) WHERE qr_token IS NOT NULL;

-- Add rabble_chat tx_type to credit_ledger constraint
DO $$
BEGIN
    ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
    ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
        CHECK (tx_type IN (
            'deposit', 'withdrawal', 'execution_fee', 'gas_fee',
            'education_alloc', 'education_spend',
            'transfer_out', 'transfer_in',
            'grant', 'refund',
            'fork_royalty', 'fork_fee', 'publish_fee', 'eval_fee',
            'consolidation_fee',
            'marketplace_listing_fee', 'marketplace_match_purchase', 'marketplace_match_payout',
            'creature_mint', 'creature_flight', 'swarm_create', 'swarm_join',
            'collection_create',
            'rabble_chat'
        ));
EXCEPTION
    WHEN others THEN NULL;
END $$;
