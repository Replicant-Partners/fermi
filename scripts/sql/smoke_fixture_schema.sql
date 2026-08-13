-- ═══════════════════════════════════════════════════════════════════
-- Minimal fixture schema for scripts/smoke_economics.sh
--
-- WHAT THIS IS: just enough of the real schema for migration 189 and
-- the economics queries to run. Column names and types mirror the
-- production migrations they are drawn from:
--
--   users         → migrations/004_add_users_table.sql
--   agents        → migrations/010_add_adm_tables_and_dreaming.sql
--   episodes      → migrations/010_add_adm_tables_and_dreaming.sql
--   wallets       → migrations/012_credit_ledger.sql
--   credit_ledger → migrations/012_credit_ledger.sql
--
-- WHAT THIS IS NOT: proof of schema parity with production. Replaying
-- all 189 migrations here would be slow and fragile, so this asserts
-- that the SQL is *correct against the shapes it expects*. Whether prod
-- actually has those shapes is the boot-time trust contract's job
-- (src/schema_trust.rs, /api/admin/schema-health).
--
-- Only the columns the queries touch are included, plus the NOT NULLs
-- needed to insert a row.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.users (
    user_id       TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    display_name  TEXT,
    role          TEXT NOT NULL DEFAULT 'developer'
                  CHECK (role IN ('admin', 'developer', 'viewer')),
    auth_provider TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.agents (
    agent_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_name TEXT NOT NULL UNIQUE,
    tier       TEXT NOT NULL DEFAULT 'curated',
    user_id    TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- `context` is the JSONB the economics queries read
-- funding_principal / provider / model_used out of (SPEC_28).
CREATE TABLE IF NOT EXISTS public.episodes (
    episode_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    query       TEXT NOT NULL DEFAULT '',
    context     JSONB NOT NULL DEFAULT '{}'::jsonb,
    tokens_used INTEGER,
    cost_usd    DECIMAL(10, 6),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.wallets (
    wallet_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_type TEXT NOT NULL CHECK (owner_type IN ('user', 'workspace')),
    owner_id   TEXT NOT NULL UNIQUE,
    balance    INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.credit_ledger (
    tx_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    wallet_id     UUID NOT NULL REFERENCES public.wallets(wallet_id),
    amount        INTEGER NOT NULL,   -- negative = debit, positive = credit
    balance_after INTEGER NOT NULL DEFAULT 0,
    tx_type       TEXT NOT NULL,
    description   TEXT,
    related_id    TEXT,               -- episode_id / agent_id as text
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
