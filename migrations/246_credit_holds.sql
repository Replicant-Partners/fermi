-- 246: credit holds (SPEC 36 R1.3 / LLD G04, #48; owner decision 1(b)).
--
-- Reserve before work, settle after. Append-only: a hold is closed by
-- inserting its closure, never by updating it, so no new UPDATE exists on
-- these tables (directive rule 1). Available balance =
-- wallets.balance - sum(open holds). `unpaid` records any part of a settled
-- charge the wallet could not cover; it is repaid at the next hold
-- (credit_debt_repayments) and a wallet that cannot repay cannot open one.

CREATE TABLE IF NOT EXISTS public.credit_holds (
    hold_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id   UUID NOT NULL REFERENCES public.wallets(wallet_id) ON DELETE RESTRICT,
    amount      INTEGER NOT NULL CHECK (amount > 0),
    purpose     TEXT NOT NULL,
    episode_id  UUID UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_credit_holds_wallet ON public.credit_holds(wallet_id);

CREATE TABLE IF NOT EXISTS public.credit_hold_closures (
    hold_id    UUID PRIMARY KEY REFERENCES public.credit_holds(hold_id) ON DELETE RESTRICT,
    kind       TEXT NOT NULL CHECK (kind IN ('settle', 'release')),
    charged    INTEGER NOT NULL DEFAULT 0 CHECK (charged >= 0),
    unpaid     INTEGER NOT NULL DEFAULT 0 CHECK (unpaid >= 0),
    reason     TEXT,
    closed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (kind = 'settle' OR (charged = 0 AND unpaid = 0))
);

CREATE TABLE IF NOT EXISTS public.credit_debt_repayments (
    repayment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id    UUID NOT NULL REFERENCES public.wallets(wallet_id) ON DELETE RESTRICT,
    amount       INTEGER NOT NULL CHECK (amount > 0),
    repaid_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_credit_debt_repayments_wallet
    ON public.credit_debt_repayments(wallet_id);
