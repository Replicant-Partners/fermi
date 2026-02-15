-- Drop FK constraint on creature_state.rabble_id
-- The background tokio::spawn tasks that write creature_state may run before
-- the swarm_events INSERT is visible via PgBouncer's transaction-mode pooling.
-- The swarm always exists by the time the data is read — FK was just safety.
ALTER TABLE creature_state DROP CONSTRAINT IF EXISTS creature_state_rabble_id_fkey;
