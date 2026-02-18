-- Migration 093: Add UNIQUE constraint on users.user_id
--
-- The users table has user_id as TEXT (nullable, added post-creation).
-- Migration 091 (swarm_participants) references users(user_id) as a FK,
-- which requires a UNIQUE constraint on the target column.
--
-- Previously only a partial unique index existed:
--   idx_users_user_id_unique (WHERE user_id IS NOT NULL AND auth_provider <> 'legacy')
--
-- This was insufficient for FK references. We add a proper UNIQUE constraint
-- so that any table can REFERENCES users(user_id).
--
-- Pre-conditions verified on prod 2026-02-18:
--   - No NULL user_id values exist
--   - No duplicate user_id values exist
--
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

-- Safety: ensure no nulls before constraining
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM users WHERE user_id IS NULL) THEN
        RAISE EXCEPTION 'Cannot add UNIQUE constraint: users.user_id has NULL values. Backfill them first.';
    END IF;
END $$;

-- Add the constraint (idempotent — skips if already exists)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
        WHERE conrelid = 'users'::regclass
          AND conname = 'users_user_id_unique'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_user_id_unique UNIQUE (user_id);
        RAISE NOTICE 'Added UNIQUE constraint users_user_id_unique on users(user_id)';
    ELSE
        RAISE NOTICE 'Constraint users_user_id_unique already exists — skipping';
    END IF;
END $$;
