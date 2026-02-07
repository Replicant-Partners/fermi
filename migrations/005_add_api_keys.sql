-- Migration: Add API keys table for programmatic access
-- Date: 2026-02-08
-- Description: Creates api_keys table for service-to-service and programmatic authentication

CREATE TABLE IF NOT EXISTS public.api_keys (
    key_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES public.users(id) ON DELETE CASCADE,  -- Reference existing PK

    -- Key security
    key_hash TEXT NOT NULL,  -- Argon2 hash of API key
    key_prefix TEXT NOT NULL UNIQUE,  -- First 12 chars for identification (e.g., "ferm_abc123...")

    -- Metadata
    name TEXT NOT NULL,  -- User-friendly name
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read'],  -- ['read', 'write', 'execute', 'admin']

    -- Usage tracking
    last_used_at TIMESTAMPTZ,
    request_count BIGINT NOT NULL DEFAULT 0,

    -- Expiration
    expires_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON public.api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON public.api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_prefix ON public.api_keys(key_prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON public.api_keys(is_active, expires_at) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_api_keys_expires_at ON public.api_keys(expires_at) WHERE expires_at IS NOT NULL;

-- Trigger for updated_at
DROP TRIGGER IF EXISTS update_api_keys_updated_at ON public.api_keys;
CREATE TRIGGER update_api_keys_updated_at BEFORE UPDATE ON public.api_keys
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Comments
COMMENT ON TABLE public.api_keys IS 'API keys for programmatic access to Fermi services';
COMMENT ON COLUMN public.api_keys.key_hash IS 'Argon2 hash - never store keys in plaintext';
COMMENT ON COLUMN public.api_keys.key_prefix IS 'First 12 characters of key for identification';
COMMENT ON COLUMN public.api_keys.scopes IS 'Permissions array: read, write, execute, admin';
