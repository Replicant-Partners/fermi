-- Migration 134: Add source column to notifications for surface-scoped filtering
--
-- Problem: Rabble (Flutter app) calls GET /api/notifications with no filter
-- and receives ABW platform notifications (admin grants, eval regressions,
-- low_balance, execution_failure) that are meaningless in the Rabble UI.
--
-- Fix: add a source column. The list endpoint accepts ?source= to filter.
-- Rabble calls /api/notifications?source=rabble (via query param added
-- server-side by the Rabble-specific notification path, or by default
-- behaviour change for the rabble endpoint variant).
--
-- Source values:
--   'abw'     — Agent Bestiary World platform notifications (default)
--   'rabble'  — Rabble creature/swarm/social notifications
--   'system'  — Platform-wide (visible in all surfaces)
--
-- Existing rows are backfilled as 'abw' since they were all created by
-- the ABW platform. Rabble-specific notification creation paths will
-- be updated to pass source='rabble'.

DO $$ BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'notifications' AND column_name = 'source'
    ) THEN
        ALTER TABLE notifications ADD COLUMN source TEXT NOT NULL DEFAULT 'abw';
    END IF;
END $$;

-- Backfill: all existing notifications are ABW surface
UPDATE notifications SET source = 'abw' WHERE source = 'abw';

-- Efficient index for source-filtered queries
CREATE INDEX IF NOT EXISTS idx_notifications_user_source
    ON notifications(user_id, source, read, created_at DESC);
