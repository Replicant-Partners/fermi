-- Migration: Add SIWE nonces table for replay protection
-- Date: 2026-02-08
-- Description: Creates siwe_nonces table to prevent replay attacks in Sign-In with Ethereum

CREATE TABLE IF NOT EXISTS public.siwe_nonces (
    nonce TEXT PRIMARY KEY,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL
);

-- Index for efficient cleanup of expired nonces
CREATE INDEX idx_siwe_nonces_expires_at ON public.siwe_nonces(expires_at);

-- Comments
COMMENT ON TABLE public.siwe_nonces IS 'Nonces for SIWE replay protection - expired nonces cleaned up automatically';
COMMENT ON COLUMN public.siwe_nonces.expires_at IS 'Nonce validity period (typically 5 minutes from creation)';

-- Scheduled cleanup function (to be called by cron job or periodic task)
CREATE OR REPLACE FUNCTION cleanup_expired_siwe_nonces()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    DELETE FROM public.siwe_nonces WHERE expires_at < NOW();
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

COMMENT ON FUNCTION cleanup_expired_siwe_nonces() IS 'Cleanup expired SIWE nonces - run periodically (e.g., every hour)';
