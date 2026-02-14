-- Migration 076: Remove tx_type CHECK constraint permanently.
--
-- WHY: The CHECK constraint on credit_ledger.tx_type has caused repeated
-- production 402 errors. Through PgBouncer (transaction mode), multi-statement
-- migrations that DROP+ADD the constraint silently lose the ADD, leaving an
-- old constraint that rejects new tx_types (perch, fly, tether, etc.).
--
-- This has broken perch, fly, and other operations across multiple deploys.
-- The constraint provides no real safety — tx_type values are controlled by
-- application code (charge_gas calls), not user input. Removing it eliminates
-- an entire class of deploy failures.
--
-- tx_type validation is enforced at the application layer in charge_gas().
DO $$
BEGIN
    ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
END $$;
