-- clear_rabble_data.sql
-- Clears all rabble/swarm data while preserving creatures, users, wallets, friendships.
-- Run with: psql $DATABASE_URL -f scripts/clear_rabble_data.sql

BEGIN;

-- Clear rabble messages
DELETE FROM rabble_messages;

-- Clear flight data (all flights tied to rabbles or not)
DELETE FROM creature_flights;

-- Clear swarm participants
DELETE FROM swarm_participants;

-- Clear object shares for rabbles
DELETE FROM object_shares WHERE object_type = 'rabble';

-- Tables that may not exist in all environments
DO $$
BEGIN
    EXECUTE 'DELETE FROM rabble_follows';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

DO $$
BEGIN
    EXECUTE 'DELETE FROM swarm_sub_flocks';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

DO $$
BEGIN
    EXECUTE 'DELETE FROM swarm_activations';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

DO $$
BEGIN
    EXECUTE 'DELETE FROM swarm_telemetry';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

DO $$
BEGIN
    EXECUTE 'DELETE FROM swarm_sessions';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

DO $$
BEGIN
    EXECUTE 'DELETE FROM creature_co_presence';
EXCEPTION WHEN undefined_table THEN NULL;
END $$;

-- Clear swarm events (the rabbles themselves)
DELETE FROM swarm_events;

-- Reset all creature states to clean (no rabble, no flight)
UPDATE creature_state SET state = 'idle', rabble_id = NULL, updated_at = NOW();

-- Clear activity events related to rabbles
DELETE FROM activity_events WHERE rabble_id IS NOT NULL;

COMMIT;

-- Verify cleanup
SELECT 'flights' AS tbl, COUNT(*) AS cnt FROM creature_flights
UNION ALL SELECT 'swarms', COUNT(*) FROM swarm_events
UNION ALL SELECT 'messages', COUNT(*) FROM rabble_messages
UNION ALL SELECT 'creatures', COUNT(*) FROM creatures
UNION ALL SELECT 'creature_state_active', COUNT(*) FROM creature_state WHERE state IS NOT NULL;
