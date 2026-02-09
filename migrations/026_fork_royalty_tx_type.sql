-- Migration 026: Add fork_royalty, fork_fee, publish_fee tx_types to credit ledger
-- Drop and re-add the CHECK constraint to include new transaction types.

BEGIN;

ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;

ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee', 'publish_fee'
    ));

COMMIT;
