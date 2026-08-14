-- Migration 091: Swarm participants table
-- Tracks user participation in rabble (swarm) events
-- This enables proper social features: who's in which rabble, join/leave tracking

CREATE TABLE IF NOT EXISTS swarm_participants (
    participant_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    creature_id UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,

    -- Participation metadata
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    left_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'left', 'kicked')),

    -- Optional: user's role in the swarm
    role TEXT DEFAULT 'member' CHECK (role IN ('host', 'cohost', 'member')),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for efficient lookups
CREATE INDEX IF NOT EXISTS idx_swarm_participants_swarm ON swarm_participants(swarm_id);
CREATE INDEX IF NOT EXISTS idx_swarm_participants_user ON swarm_participants(user_id);
CREATE INDEX IF NOT EXISTS idx_swarm_participants_active ON swarm_participants(swarm_id, user_id) WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_swarm_participants_creature ON swarm_participants(creature_id) WHERE creature_id IS NOT NULL;

-- Unique constraint: one active participation per user per swarm
CREATE UNIQUE INDEX IF NOT EXISTS idx_swarm_participants_unique_active
    ON swarm_participants(swarm_id, user_id)
    WHERE status = 'active';

-- Trigger to auto-update updated_at
CREATE TRIGGER update_swarm_participants_updated_at
    BEFORE UPDATE ON swarm_participants
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Comments for documentation
COMMENT ON TABLE swarm_participants IS 'Tracks user participation in rabble (swarm) events';
COMMENT ON COLUMN swarm_participants.swarm_id IS 'The rabble event this user is participating in';
COMMENT ON COLUMN swarm_participants.user_id IS 'The user participating in the rabble';
COMMENT ON COLUMN swarm_participants.creature_id IS 'The creature the user brought to the rabble (optional)';
COMMENT ON COLUMN swarm_participants.status IS 'Participation status: active, left, or kicked';
COMMENT ON COLUMN swarm_participants.role IS 'User role in the swarm: host, cohost, or member';

-- Backfill existing participants from creature_flights
-- Users who have flown creatures in swarms are considered participants
INSERT INTO swarm_participants (swarm_id, user_id, creature_id, joined_at, status)
SELECT DISTINCT
    cf.swarm_id,
    cf.owner_id,
    cf.creature_id,
    MIN(cf.started_at) OVER (PARTITION BY cf.swarm_id, cf.owner_id),
    'active'
FROM creature_flights cf
WHERE cf.swarm_id IS NOT NULL
  AND cf.ended_at IS NULL
  AND NOT EXISTS (
      SELECT 1 FROM swarm_participants sp
      WHERE sp.swarm_id = cf.swarm_id AND sp.user_id = cf.owner_id
  )
-- Inference, not a constraint name.
--
-- This said `ON CONFLICT ON CONSTRAINT idx_swarm_participants_unique_active`,
-- which cannot work: that name belongs to a partial UNIQUE INDEX (created
-- above with `WHERE status = 'active'`), and `ON CONFLICT ON CONSTRAINT`
-- accepts only a table constraint. A partial unique index cannot be promoted
-- to one either — Postgres has no partial UNIQUE constraint. So this file has
-- never applied anywhere, in CI or in production, and the backfill it performs
-- has never run.
--
-- The index is still the right uniqueness rule; it just has to be inferred,
-- which requires restating its predicate so Postgres can match it.
ON CONFLICT (swarm_id, user_id) WHERE status = 'active' DO NOTHING;
