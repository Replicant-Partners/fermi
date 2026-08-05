-- Migration 171: Agent credential store + abw-system principal (P0)
--
-- See docs/specs/AGENT_CREDENTIAL_MODEL.md.
--
-- Replaces the "system-tier -> ANTHROPIC_API_KEY env var" path with a
-- managed, encrypted, per-(principal, provider, scope) credential store.
-- Distinct from user_secrets (which stays for tool/integration secrets
-- like Instagram/Bluesky/Stripe) — this table is exclusively LLM/embedding
-- provider credentials that fund agent execution.
--
--   UNIQUE(principal_id, provider, scope) lets a principal hold, per
--   provider: one '*' default + one key per agent (scope = agent_name).
--   That is the per-agent funding-isolation primitive.
--
-- abw-system is the owning principal for platform-service agents
-- (ontologist, dream_narrator, cohere_and_coordinate, fermi, xaman_ek).
-- It is a non-login users row; its keys are the platform "system keys".
--
-- Single DO block => one statement => PgBouncer-safe + idempotent.

DO $$
BEGIN
    -- Encrypted provider-credential store.
    CREATE TABLE IF NOT EXISTS agent_credentials (
        credential_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        principal_id    TEXT NOT NULL,          -- users.user_id of the owning principal
        provider        TEXT NOT NULL,          -- 'openai' | 'anthropic' | 'mistral' | ...
        scope           TEXT NOT NULL DEFAULT '*', -- '*' (principal default) | '<agent_name>'
        encrypted_value BYTEA NOT NULL,         -- AES-256-GCM ciphertext (SecretEncryptor)
        nonce           BYTEA NOT NULL,
        label           TEXT,
        created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        UNIQUE (principal_id, provider, scope)
    );

    CREATE INDEX IF NOT EXISTS idx_agent_credentials_principal
        ON agent_credentials (principal_id);

    CREATE INDEX IF NOT EXISTS idx_agent_credentials_lookup
        ON agent_credentials (principal_id, provider, scope);

    -- Seed the abw-system principal (non-login). Idempotent.
    -- users NOT-NULL-without-default columns (verified against prod):
    -- email, password_hash, password_salt. auth_provider left NULL to
    -- avoid CHECK-constraint drift between mig-004 and mig-004b.
    IF NOT EXISTS (SELECT 1 FROM users WHERE user_id = 'abw-system') THEN
        INSERT INTO users (user_id, email, password_hash, password_salt, role, display_name)
        VALUES ('abw-system', 'system@abw.local', '', '', 'admin', 'ABW System');
    END IF;
END $$;
