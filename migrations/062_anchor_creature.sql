-- Migration 062: Anchor creature system
-- A rabble is anchored to the creature that seeded it.
-- If the creature has a GPS device, the rabble moves with it.
-- If the anchor creature leaves, the organizer is warned and can transfer.

-- Add anchor columns to swarm_events
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS anchor_creature_id UUID;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS anchor_transferred_at TIMESTAMPTZ;

-- Index for quick lookup of which rabble a creature anchors
CREATE INDEX IF NOT EXISTS idx_swarm_anchor_creature
    ON swarm_events(anchor_creature_id) WHERE anchor_creature_id IS NOT NULL;
