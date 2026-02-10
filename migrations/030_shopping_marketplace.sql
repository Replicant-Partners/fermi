-- Migration 030: Shopping profiles + Embedding Marketplace
-- Three tables: shopping_profiles, marketplace_listings, marketplace_transactions

BEGIN;

-- Shopping preference profiles (consumer side)
CREATE TABLE IF NOT EXISTS public.shopping_profiles (
    profile_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             TEXT NOT NULL,
    agent_id            UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    profile_name        TEXT NOT NULL DEFAULT 'default',
    composite_embedding vector(1024),
    embedding_version   INTEGER NOT NULL DEFAULT 1,
    episode_count       INTEGER NOT NULL DEFAULT 0,
    category_tags       TEXT[] DEFAULT '{}',
    price_sensitivity   DOUBLE PRECISION,
    quality_bias        DOUBLE PRECISION,
    brand_affinities    JSONB DEFAULT '{}',
    metadata            JSONB DEFAULT '{}',
    is_listed           BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, agent_id, profile_name)
);

CREATE INDEX IF NOT EXISTS idx_shopping_profiles_user ON public.shopping_profiles(user_id);
CREATE INDEX IF NOT EXISTS idx_shopping_profiles_listed ON public.shopping_profiles(is_listed) WHERE is_listed;

-- Marketplace listings (consumer lists profile for advertiser queries)
CREATE TABLE IF NOT EXISTS public.marketplace_listings (
    listing_id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    profile_id              UUID NOT NULL REFERENCES public.shopping_profiles(profile_id) ON DELETE CASCADE,
    seller_id               TEXT NOT NULL,
    price_credits           INTEGER NOT NULL DEFAULT 2,
    max_queries_per_buyer   INTEGER DEFAULT NULL,
    total_queries           INTEGER NOT NULL DEFAULT 0,
    total_earned            INTEGER NOT NULL DEFAULT 0,
    status                  TEXT NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active', 'paused', 'delisted')),
    category_tags           TEXT[] DEFAULT '{}',
    description             TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_marketplace_listings_status
    ON public.marketplace_listings(status) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_marketplace_listings_seller
    ON public.marketplace_listings(seller_id);
CREATE INDEX IF NOT EXISTS idx_marketplace_listings_tags
    ON public.marketplace_listings USING gin(category_tags);

-- Marketplace transactions (record of match queries)
CREATE TABLE IF NOT EXISTS public.marketplace_transactions (
    tx_id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    listing_id              UUID NOT NULL REFERENCES public.marketplace_listings(listing_id),
    buyer_id                TEXT NOT NULL,
    seller_id               TEXT NOT NULL,
    similarity_score        DOUBLE PRECISION NOT NULL,
    product_embedding_hash  TEXT,
    credits_charged         INTEGER NOT NULL,
    credits_to_seller       INTEGER NOT NULL,
    platform_fee            INTEGER NOT NULL DEFAULT 0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_marketplace_tx_buyer
    ON public.marketplace_transactions(buyer_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_marketplace_tx_seller
    ON public.marketplace_transactions(seller_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_marketplace_tx_listing
    ON public.marketplace_transactions(listing_id);

-- Extend credit_ledger tx_type constraint with marketplace types
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee', 'publish_fee',
        'eval_fee',
        'marketplace_listing_fee',
        'marketplace_match_purchase',
        'marketplace_match_payout'
    ));

COMMIT;
