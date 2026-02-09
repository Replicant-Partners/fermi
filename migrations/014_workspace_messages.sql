-- Migration 014: Workspace messages (chat)
-- Persistent message store for workspace communication between people and agents.

BEGIN;

CREATE TABLE IF NOT EXISTS workspace_messages (
    message_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES teams(id) ON DELETE CASCADE,
    sender_type   TEXT NOT NULL CHECK (sender_type IN ('user', 'agent', 'system')),
    sender_id     TEXT NOT NULL,
    sender_name   TEXT,
    content       TEXT NOT NULL,
    message_type  TEXT NOT NULL DEFAULT 'chat'
                      CHECK (message_type IN ('chat', 'execution_result', 'coherence_update', 'system_event')),
    metadata      JSONB DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ws_messages_workspace ON workspace_messages(workspace_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ws_messages_sender ON workspace_messages(sender_id);

COMMIT;
