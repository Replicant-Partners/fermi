-- 224: a2a_push_configs — webhook delivery records for A2A push notifications.
--
-- Push configs may be registered BEFORE the task episode row exists
-- (the caller sets a webhook in the initial SendMessageRequest, before
-- the task ID is assigned). task_id is therefore NOT a foreign key.
--
-- Design: docs/DESIGN_a2a_provider.md §9 Phase 4.

CREATE TABLE IF NOT EXISTS a2a_push_configs (
    config_id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    -- A2A task id — the episode_id assigned by the platform.
    task_id           UUID        NOT NULL,
    -- Agent slug this config is scoped to.
    agent_slug        TEXT        NOT NULL,
    -- ABW user id of the external caller who registered this webhook.
    caller_user_id    TEXT        NOT NULL,
    -- Webhook URL. The platform POSTs a StreamResponse payload here.
    webhook_url       TEXT        NOT NULL,
    -- Optional HTTP auth scheme ("Bearer", "Basic", etc.).
    auth_scheme       TEXT,
    -- Optional credentials (token for Bearer, base64 for Basic).
    -- Stored in plaintext for Phase 4; encrypt at rest in Phase 5.
    auth_credentials  TEXT,
    -- Caller-provided token for HMAC or bearer verification on their side.
    token             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Populated on first successful delivery.
    delivered_at      TIMESTAMPTZ,
    delivery_attempts INT         NOT NULL DEFAULT 0,
    -- Last error message if delivery failed.
    last_error        TEXT
);

CREATE INDEX IF NOT EXISTS a2a_push_configs_task_idx
    ON a2a_push_configs (task_id);

CREATE INDEX IF NOT EXISTS a2a_push_configs_caller_idx
    ON a2a_push_configs (caller_user_id);
