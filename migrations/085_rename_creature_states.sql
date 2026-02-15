-- Rename creature states: perch_solo → perched, perch_rabble → hosting/in_rabble
-- Part of perch/rabble separation: perch = location only, host = rabble creation
DO $$ BEGIN
  -- Drop old CHECK constraints
  ALTER TABLE creature_state DROP CONSTRAINT IF EXISTS creature_state_state_check;
  ALTER TABLE creature_versions DROP CONSTRAINT IF EXISTS creature_versions_state_check;

  -- creature_state (mutable pointer)
  UPDATE creature_state SET state = 'perched' WHERE state = 'perch_solo';

  UPDATE creature_state SET state = 'hosting' WHERE state = 'perch_rabble'
    AND creature_id IN (
      SELECT anchor_creature_id FROM swarm_events
      WHERE anchor_creature_id IS NOT NULL AND creature_count > 0
    );

  UPDATE creature_state SET state = 'in_rabble' WHERE state = 'perch_rabble';

  -- creature_versions (append-only log)
  UPDATE creature_versions SET state = 'perched' WHERE state = 'perch_solo';

  UPDATE creature_versions SET state = 'hosting' WHERE state = 'perch_rabble'
    AND creature_id IN (
      SELECT anchor_creature_id FROM swarm_events WHERE anchor_creature_id IS NOT NULL
    );

  UPDATE creature_versions SET state = 'in_rabble' WHERE state = 'perch_rabble';

  -- Backfill: creatures with rabble_id whose swarm anchor != themselves
  UPDATE creature_state SET state = 'in_rabble'
  WHERE state = 'perched' AND rabble_id IS NOT NULL
    AND creature_id NOT IN (
      SELECT anchor_creature_id FROM swarm_events
      WHERE anchor_creature_id IS NOT NULL
      AND swarm_id = creature_state.rabble_id
    );

  -- Backfill: anchor creatures of active swarms still marked perched
  UPDATE creature_state SET state = 'hosting'
  WHERE state = 'perched'
    AND creature_id IN (
      SELECT anchor_creature_id FROM swarm_events
      WHERE anchor_creature_id IS NOT NULL AND creature_count > 0
        AND swarm_id = creature_state.rabble_id
    );

  -- Add new CHECK constraints with updated state names
  ALTER TABLE creature_state ADD CONSTRAINT creature_state_state_check
    CHECK (state IN ('perched', 'hosting', 'in_rabble', 'fly'));
  ALTER TABLE creature_versions ADD CONSTRAINT creature_versions_state_check
    CHECK (state IN ('perched', 'hosting', 'in_rabble', 'fly'));
END $$;
