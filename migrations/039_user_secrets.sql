-- Migration 039: User secrets + audit log
-- Encrypted credential storage for agent integrations (Instagram, Bluesky, Stripe, etc.)

CREATE TABLE IF NOT EXISTS user_secrets (
    secret_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    secret_name TEXT NOT NULL,
    encrypted_value BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    scope TEXT NOT NULL DEFAULT '*',
    label TEXT,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, secret_name)
);

CREATE INDEX IF NOT EXISTS idx_user_secrets_user ON user_secrets(user_id);

-- Append-only audit log for secret access
CREATE TABLE IF NOT EXISTS secret_access_log (
    log_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id TEXT NOT NULL,
    secret_name TEXT NOT NULL,
    agent_name TEXT NOT NULL,
    workspace_id UUID,
    action TEXT NOT NULL CHECK (action IN ('read', 'used', 'created', 'updated', 'deleted')),
    tool_name TEXT,
    ip_address TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_secret_access_log_user ON secret_access_log(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_secret_access_log_agent ON secret_access_log(agent_name, created_at DESC);
