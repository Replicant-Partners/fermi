-- Migration: Add users table for authentication
-- Date: 2026-02-08
-- Description: Creates users table to cache Zitadel identities and support multi-provider auth

-- Create users table
CREATE TABLE IF NOT EXISTS public.users (
    user_id TEXT PRIMARY KEY,  -- Zitadel user ID or Ethereum address (not UUID)
    email TEXT UNIQUE NOT NULL,
    display_name TEXT,
    avatar_url TEXT,
    role TEXT NOT NULL DEFAULT 'developer' CHECK (role IN ('admin', 'developer', 'viewer')),
    zitadel_org_id TEXT,  -- Multi-tenancy support
    auth_provider TEXT CHECK (auth_provider IN ('email', 'github', 'google', 'ethereum')),

    -- GitHub OAuth fields
    github_username TEXT,
    github_id TEXT,

    -- Google OAuth fields
    google_id TEXT,

    -- Web3 / Ethereum fields
    ethereum_address TEXT,
    ens_name TEXT,

    -- Timestamps
    last_login_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient lookups
CREATE INDEX idx_users_email ON public.users(email);
CREATE INDEX idx_users_org_id ON public.users(zitadel_org_id) WHERE zitadel_org_id IS NOT NULL;
CREATE INDEX idx_users_auth_provider ON public.users(auth_provider);
CREATE INDEX idx_users_github_username ON public.users(github_username) WHERE github_username IS NOT NULL;
CREATE INDEX idx_users_google_id ON public.users(google_id) WHERE google_id IS NOT NULL;
CREATE INDEX idx_users_ethereum_address ON public.users(ethereum_address) WHERE ethereum_address IS NOT NULL;

-- Unique constraint on ethereum_address (one address = one account)
CREATE UNIQUE INDEX idx_users_ethereum_address_unique ON public.users(ethereum_address)
WHERE ethereum_address IS NOT NULL;

-- Function to automatically update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Trigger to auto-update updated_at
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON public.users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Comments for documentation
COMMENT ON TABLE public.users IS 'User authentication identities - cached from Zitadel and Web3';
COMMENT ON COLUMN public.users.user_id IS 'Primary identifier - Zitadel user ID or Ethereum address';
COMMENT ON COLUMN public.users.auth_provider IS 'How user authenticated: email, github, google, or ethereum';
COMMENT ON COLUMN public.users.ethereum_address IS 'Checksummed Ethereum address for Web3 users (EIP-55)';
COMMENT ON COLUMN public.users.ens_name IS 'ENS domain name if resolved (e.g., vitalik.eth)';
