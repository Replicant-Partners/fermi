-- Migration 046: Add visibility column to swarm_events for private/shared rabbles.
-- Values: 'public' (anyone joins), 'shared' (QR/link only), 'private' (invite only).
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS visibility TEXT NOT NULL DEFAULT 'public';
