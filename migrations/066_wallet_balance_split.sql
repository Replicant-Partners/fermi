-- Migration 066: Wallet balance split — granted (non-transferable) vs purchased (transferable)
-- Grants (signup bonus, admin grants) cannot be transferred to other users.
-- Purchased credits (Stripe) and earned credits (revenue, royalties) can be transferred.
-- Spend priority: granted first, then purchased.

-- Add split balance columns
ALTER TABLE wallets ADD COLUMN IF NOT EXISTS granted_balance INTEGER NOT NULL DEFAULT 0;
ALTER TABLE wallets ADD COLUMN IF NOT EXISTS purchased_balance INTEGER NOT NULL DEFAULT 0;

-- Backfill: move existing balance to granted (conservative — can't distinguish historical purchases)
UPDATE wallets SET granted_balance = balance WHERE granted_balance = 0 AND balance > 0;

-- CHECK constraint: balance must always equal sum of parts
ALTER TABLE wallets DROP CONSTRAINT IF EXISTS wallet_balance_split_check;
ALTER TABLE wallets ADD CONSTRAINT wallet_balance_split_check
  CHECK (balance = granted_balance + purchased_balance);

-- Composite index for the visible-flights query (covers WHERE ended_at IS NULL + visibility + owner_id)
CREATE INDEX IF NOT EXISTS idx_creature_flights_active_visible
  ON creature_flights (owner_id, visibility)
  WHERE ended_at IS NULL;
