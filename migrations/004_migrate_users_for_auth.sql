-- Migration: Extend existing users table for multi-provider authentication
-- This preserves existing data while adding support for Zitadel OIDC, GitHub, Google, and SIWE

BEGIN;

-- Add new columns for multi-provider auth (keeping existing password_hash for backward compatibility)
ALTER TABLE public.users
    ADD COLUMN IF NOT EXISTS user_id TEXT,  -- Zitadel user ID (will become primary key)
    ADD COLUMN IF NOT EXISTS zitadel_org_id TEXT,  -- Multi-tenancy support
    ADD COLUMN IF NOT EXISTS auth_provider TEXT CHECK (auth_provider IN ('email', 'github', 'google', 'ethereum', 'legacy')),
    ADD COLUMN IF NOT EXISTS role TEXT DEFAULT 'developer' CHECK (role IN ('admin', 'developer', 'viewer')),
    ADD COLUMN IF NOT EXISTS display_name TEXT,
    ADD COLUMN IF NOT EXISTS github_username TEXT,
    ADD COLUMN IF NOT EXISTS github_id TEXT,
    ADD COLUMN IF NOT EXISTS google_id TEXT,
    ADD COLUMN IF NOT EXISTS ethereum_address TEXT,
    ADD COLUMN IF NOT EXISTS ens_name TEXT;

-- For existing users, mark as 'legacy' auth provider
UPDATE public.users
SET auth_provider = 'legacy',
    user_id = id::text,
    display_name = name
WHERE auth_provider IS NULL;

-- Create indexes for new columns
CREATE INDEX IF NOT EXISTS idx_users_user_id ON public.users(user_id) WHERE user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_org_id ON public.users(zitadel_org_id) WHERE zitadel_org_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_auth_provider ON public.users(auth_provider);
CREATE INDEX IF NOT EXISTS idx_users_github_username ON public.users(github_username) WHERE github_username IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_google_id ON public.users(google_id) WHERE google_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_ethereum_address ON public.users(ethereum_address) WHERE ethereum_address IS NOT NULL;

-- Unique constraint on ethereum address (for SIWE)
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_ethereum_address_unique
ON public.users(ethereum_address)
WHERE ethereum_address IS NOT NULL;

-- Unique constraint on user_id (for new Zitadel users)
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_user_id_unique
ON public.users(user_id)
WHERE user_id IS NOT NULL AND auth_provider != 'legacy';

-- Update trigger already exists, reusing it

-- Comment on new columns
COMMENT ON COLUMN public.users.user_id IS 'Zitadel user ID or legacy UUID as text';
COMMENT ON COLUMN public.users.auth_provider IS 'Authentication provider: email, github, google, ethereum, or legacy';
COMMENT ON COLUMN public.users.ethereum_address IS 'Checksummed Ethereum address for SIWE';
COMMENT ON COLUMN public.users.ens_name IS 'ENS domain name if resolved (e.g., vitalik.eth)';

COMMIT;
