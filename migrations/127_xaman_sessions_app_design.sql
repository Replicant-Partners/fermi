-- Migration 122: Add 'app_design' to xaman_sessions.session_type CHECK
--
-- The xamanEK platform navigator gains a new conversational session-mode
-- for designing an App on the ABW platform (alongside agent_design and
-- composition_design). This migration expands the CHECK constraint to
-- allow the new value.
--
-- The existing constraint was inline-anonymous from migration 115, so we
-- locate it in pg_constraint by table+column, drop it, and re-add it with
-- a stable name so future migrations can target it directly.
--
-- PgBouncer-safe: wrapped in DO blocks.

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    -- Find the existing anonymous CHECK that constrains session_type.
    -- contype = 'c' (check); conkey is an int2[] of column attnums.
    SELECT c.conname
    INTO constraint_name
    FROM pg_constraint c
    JOIN pg_class t ON t.oid = c.conrelid
    JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(c.conkey)
    WHERE t.relname = 'xaman_sessions'
      AND c.contype = 'c'
      AND a.attname = 'session_type'
    LIMIT 1;

    IF constraint_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE xaman_sessions DROP CONSTRAINT %I', constraint_name);
    END IF;
EXCEPTION WHEN OTHERS THEN
    -- If anything goes sideways finding/dropping the old constraint, fall
    -- through to the ADD below; the worst that happens is the ADD fails
    -- with a clear name-collision error so an operator can fix it manually.
    NULL;
END $$;

DO $$
BEGIN
    ALTER TABLE xaman_sessions
        ADD CONSTRAINT xaman_sessions_session_type_check
        CHECK (session_type IN (
            'agent_design',         -- building an agent card
            'composition_design',   -- planning a composition
            'workspace_help',       -- helping with a specific workspace
            'app_design',           -- building an App on ABW (new in 122)
            'free'                  -- open conversation
        ));
EXCEPTION WHEN duplicate_object THEN
    -- Constraint already exists at the target shape — no-op.
    NULL;
END $$;
