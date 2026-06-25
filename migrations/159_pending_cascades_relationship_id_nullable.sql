-- ─────────────────────────────────────────────────────────────────────
-- 159 — pending_cascades.relationship_id NULLABLE (Spec 25 group model)
-- ─────────────────────────────────────────────────────────────────────
--
-- mig 153 created pending_cascades with relationship_id (UUID FK to the
-- legacy forecast_relationships) NOT NULL. mig 156 added the group_id
-- column for the group-tag model but never relaxed that NOT NULL.
--
-- Result: every group-model cascade INSERT (queue_pending_cascade and the
-- requeue endpoint, which set group_id and leave relationship_id NULL)
-- failed with "null value in column relationship_id violates not-null
-- constraint" — so NO cascades could ever be queued under the new model.
--
-- A cascade now references EITHER group_id (preferred) OR relationship_id
-- (legacy). Both being null is still nonsensical but is enforced at the
-- application layer, not here, to avoid a CHECK that an old legacy-only
-- row might trip.

ALTER TABLE public.pending_cascades
    ALTER COLUMN relationship_id DROP NOT NULL;
