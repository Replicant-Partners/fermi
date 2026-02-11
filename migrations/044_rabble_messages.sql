-- Rabble chat messages: creature-attributed messaging for event gatherings
CREATE TABLE IF NOT EXISTS rabble_messages (
    message_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    sender_id TEXT NOT NULL,
    creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,
    creature_name TEXT,
    species_name TEXT,
    species_group TEXT,
    content TEXT NOT NULL,
    message_type TEXT NOT NULL DEFAULT 'chat',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rabble_messages_swarm ON rabble_messages(swarm_id, created_at);
CREATE INDEX IF NOT EXISTS idx_rabble_messages_sender ON rabble_messages(sender_id);
