-- Migration 120: composition_versions.rejected_by + rejection_note
--
-- Migration 113 created the composition_versions table without the
-- rejection-side columns. The Rust code added rejection support later
-- (list_composition_versions SELECTs rejected_by + rejection_note,
-- the loop_health handler filters WHERE rejected_by IS NULL, the
-- reject_composition_version_handler writes both) but no migration
-- ever added the columns to the table. Result: every read path that
-- touches composition_versions returns 500 with
--   column "rejected_by" does not exist
-- This breaks the workspace overlay's Composition panel and the
-- dashboard's Loop 4 tile.
--
-- DO-block pattern (same as migration 119) so PgBouncer treats it as
-- one indivisible statement and can't silently drop part of it.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'public.composition_versions'::regclass
          AND attname = 'rejected_by'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE public.composition_versions ADD COLUMN rejected_by TEXT;
        RAISE NOTICE '[migration 120] added composition_versions.rejected_by';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM pg_attribute
        WHERE attrelid = 'public.composition_versions'::regclass
          AND attname = 'rejection_note'
          AND NOT attisdropped
    ) THEN
        ALTER TABLE public.composition_versions ADD COLUMN rejection_note TEXT;
        RAISE NOTICE '[migration 120] added composition_versions.rejection_note';
    END IF;
END $$;
