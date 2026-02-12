-- Migration 058: Creature presence states (active, sleeping, parked)

ALTER TABLE creatures ADD COLUMN IF NOT EXISTS presence TEXT NOT NULL DEFAULT 'active';
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS presence_changed_at TIMESTAMPTZ DEFAULT NOW();
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS parked_at_workspace UUID;

CREATE INDEX IF NOT EXISTS idx_creatures_presence ON creatures(presence);
