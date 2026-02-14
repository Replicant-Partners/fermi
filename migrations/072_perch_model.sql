-- Migration 072: Perch model — walk_in_price column
-- NOTE: tx_type constraint removed in migration 076. No constraint update needed here.

-- Add walk_in_price to swarm_events
-- NULL = private (no walk-in door), 0 = free open, 2+ = paid walk-in
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS walk_in_price INTEGER;
