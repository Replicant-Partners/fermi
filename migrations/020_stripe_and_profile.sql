-- Migration 020: Stripe fields + profile fields

ALTER TABLE public.users
  ADD COLUMN IF NOT EXISTS stripe_customer_id TEXT,
  ADD COLUMN IF NOT EXISTS bio TEXT;

ALTER TABLE public.credit_ledger
  ADD COLUMN IF NOT EXISTS stripe_session_id TEXT;

CREATE INDEX IF NOT EXISTS idx_users_stripe
  ON users(stripe_customer_id)
  WHERE stripe_customer_id IS NOT NULL;
