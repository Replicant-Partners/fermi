-- Add missing tx_types that were introduced after the last constraint update
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee', 'publish_fee',
        'eval_fee', 'consolidation_fee',
        'marketplace_listing_fee',
        'marketplace_match_purchase',
        'marketplace_match_payout',
        'avatar_generate',
        'embedding_import',
        'ontology_generation',
        'prompt_generation',
        'file_write'
    ));
