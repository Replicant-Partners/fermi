-- Pass 7 (Spec 25 §10) — migrate the legacy WC mutex to the group-tag model.
--
-- Registers the `wc_2026_winner` mutex group and tags all 48 WC
-- team-prior forecasts with it. Idempotent: re-running upserts the group
-- and only appends the tag to forecasts that don't already carry it.
--
-- Prereqs: migrations 155 + 156 applied (relationship_groups column,
-- forecast_relationship_groups table).
--
-- Run:  psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f scripts/world_cup/migrate_mutex_to_group.sql
--
-- The legacy forecast_relationships row (90e1eea8-…) is intentionally
-- left in place — the queue/apply paths prefer group_id when present, and
-- the legacy row stays as an audit trail until a later archival migration.

INSERT INTO public.forecast_relationship_groups (group_id, kind, parameters, description, owner_id)
VALUES (
  'wc_2026_winner', 'mutex',
  '{"tournament": "FIFA World Cup 2026", "constraint": "Exactly one team wins."}'::jsonb,
  'FIFA World Cup 2026 — winner mutex (48 teams). Migrated from legacy forecast_relationships 90e1eea8.',
  '2e644008-f5c7-47c5-854c-3801df9879cc'
)
ON CONFLICT (group_id) DO UPDATE
  SET kind = EXCLUDED.kind,
      parameters = EXCLUDED.parameters,
      description = EXCLUDED.description,
      updated_at = NOW();

UPDATE public.fermi_forecasts
   SET relationship_groups = array_append(relationship_groups, 'wc_2026_winner')
 WHERE question_text ILIKE '%2026 FIFA World Cup%'
   AND NOT (relationship_groups @> ARRAY['wc_2026_winner']);
