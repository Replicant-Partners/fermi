-- Migration 026: Add fork_royalty, fork_fee, publish_fee tx_types to credit ledger
-- Superseded by migration 032 which has the complete constraint.
-- Just drop any existing constraint; 032 will re-add with all types.
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
