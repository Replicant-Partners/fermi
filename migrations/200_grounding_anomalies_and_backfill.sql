-- Migration 200: make an ungrounded field a reportable anomaly, and tag the
-- profiles that already shipped with them.
--
-- ## Part 1 — `anomaly_events.kind` gains 'grounding'
--
-- `src/grounding_trust.rs` detects output fields an agent could not possibly
-- have sourced: `genome_profiler` is asked for genome size, karyotype,
-- divergence date and IUCN status, and has two GBIF tools that return
-- taxonomy. Detection without a place to record it is the
-- `tracing`-with-no-subscriber failure repeated, so the finding needs a row.
--
-- `anomaly_events` (migration 105) is the right home: append-only, an
-- `episode_id` FK, a `requires_review` HITL flag, and it is already read by
-- the observatory (`handlers/observatory.rs:159`), the pending-review queue
-- (`:219`) and the `anomaly_triager` agent. Nothing new needs building.
--
-- What did need changing is the CHECK. `kind` was constrained to
-- ('drift','rolling_conflict','rupture','safety'), so a grounding violation
-- had no value it could be filed under and the insert would have been
-- rejected. Note what that means: the observability system was closed to
-- exactly the class of defect nobody had thought of yet.
--
-- `DROP CONSTRAINT IF EXISTS` then `ADD CONSTRAINT`, inside a `DO` block.
-- The block is not stylistic: `scripts/lint-migrations.sh` rejects a bare
-- DROP+ADD pair because through PgBouncer's transaction mode the second
-- statement can be lost, leaving the table with **no** kind constraint at
-- all — a widening that silently becomes a removal. The lint caught this
-- file on its first draft.
--
-- ## Part 2 — tag, do not delete
--
-- 13 rows in `creature_conditions.genome_profile` carry values like
-- `estimated_size_mb: "200-400"` and `iucn_status: "Not Evaluated (presumed
-- Least Concern)"` for species nobody has sequenced or assessed. 56 episodes
-- produced them.
--
-- They are **not** overwritten here. Two reasons:
--
--   1. The read path (`handlers/creatures/agent_modules.rs`) now runs
--      `grounding_trust::enforce` on every cached profile before returning
--      it, so the fabricated values already stop reaching users without a
--      data migration and without re-running the agent at 2 credits a call.
--      Destroying data to fix a display problem that is already fixed would
--      be gratuitous.
--   2. When NCBI / TimeTree / IUCN tools do get wired up, these values are
--      the only record of what the model guessed. Comparing a guess against
--      a later measurement is a free calibration signal, and deleting them
--      throws it away. "Tag for reprocessing" was the requirement; this is
--      that.
--
-- So each row gains `_grounding_review` (why it is flagged, and what would
-- clear it) plus the two `_provenance` keys the contract defines, and keeps
-- everything else. The `NOT ... ? '_grounding_review'` guard makes it
-- idempotent, which matters because `run_migrations` re-runs every file on
-- every boot.
--
-- Deliberately NOT touched: `phylogeny.sister_taxa`, which
-- `gbif_taxonomy_tree` genuinely returns, and the whole `taxonomy` block. A
-- backfill that over-reaches destroys real data and teaches everyone to
-- distrust the next one.

-- ─── Part 1 ───────────────────────────────────────────────────────────

DO $$
BEGIN
    ALTER TABLE public.anomaly_events
        DROP CONSTRAINT IF EXISTS anomaly_events_kind_check;
    ALTER TABLE public.anomaly_events
        ADD CONSTRAINT anomaly_events_kind_check
        CHECK (kind IN ('drift', 'rolling_conflict', 'rupture', 'safety', 'grounding'));
END $$;

COMMENT ON COLUMN public.anomaly_events.kind IS
  'drift | rolling_conflict | rupture | safety | grounding. `grounding` (mig-200) = the agent populated an output field no tool of its could supply; see src/grounding_trust.rs.';

-- ─── Part 2 ───────────────────────────────────────────────────────────

UPDATE public.creature_conditions
   SET genome_profile = genome_profile || jsonb_build_object(
           'genome_provenance',       'unavailable_no_tool_source',
           'conservation_provenance', 'unavailable_no_tool_source',
           '_grounding_review', jsonb_build_object(
               'flagged_at', to_jsonb(now()),
               'contract',   'src/grounding_trust.rs',
               'reason',     'Written before migration 200. genome.*, '
                             'phylogeny.superorder, phylogeny.divergence_mya, '
                             'phylogeny.defining_traits and conservation.* have no '
                             'tool source in this agent; the values present are '
                             'model output, not lookups.',
               'clears_when', 'Reprocessed after NCBI Assembly / TimeTree / IUCN '
                              'Red List tools are integrated. Expect most to '
                              'resolve to null even then — sparse coverage for '
                              'non-model insects is the correct answer, not a bug.'
           ))
 WHERE genome_profile IS NOT NULL
   AND genome_profile ? 'genome'
   AND NOT genome_profile ? '_grounding_review';
