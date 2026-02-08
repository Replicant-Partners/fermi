-- Migration 012: Credit ledger and wallets
-- Foundation for AKP economics: credit-based transactions with gas fees.

BEGIN;

-- User and workspace wallets
CREATE TABLE IF NOT EXISTS public.wallets (
    wallet_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_type TEXT NOT NULL CHECK (owner_type IN ('user', 'workspace')),
    owner_id TEXT NOT NULL UNIQUE,
    balance INTEGER NOT NULL DEFAULT 0,
    total_deposited INTEGER NOT NULL DEFAULT 0,
    total_spent INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wallets_owner ON public.wallets(owner_type, owner_id);

-- Append-only credit ledger (every mutation is a row)
CREATE TABLE IF NOT EXISTS public.credit_ledger (
    tx_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id UUID NOT NULL REFERENCES public.wallets(wallet_id),
    amount INTEGER NOT NULL,           -- positive = credit, negative = debit
    balance_after INTEGER NOT NULL,
    tx_type TEXT NOT NULL CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund'
    )),
    description TEXT,
    related_id TEXT,                    -- episode_id, agent_id, job_id, etc.
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_credit_ledger_wallet
    ON public.credit_ledger(wallet_id, created_at DESC);

-- Auto-update updated_at on wallets
DROP TRIGGER IF EXISTS update_wallets_updated_at ON public.wallets;
CREATE TRIGGER update_wallets_updated_at BEFORE UPDATE ON public.wallets
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

COMMIT;
