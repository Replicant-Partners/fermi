-- Migration 081: Normalize visibility value contacts_only → contacts
--
-- Flutter sends/expects "contacts", DB constraint had "contacts_only".
-- Rename to match the client convention.

UPDATE creature_conditions SET visibility = 'contacts' WHERE visibility = 'contacts_only';

DO $$ BEGIN
    ALTER TABLE creature_conditions DROP CONSTRAINT IF EXISTS creature_conditions_visibility_check;
    ALTER TABLE creature_conditions ADD CONSTRAINT creature_conditions_visibility_check
        CHECK (visibility IN ('public', 'contacts', 'private'));
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'visibility constraint update skipped: %', SQLERRM;
END $$;
