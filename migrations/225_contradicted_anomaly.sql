-- The agent's account of its own sourcing, contradicted by evidence.
--
-- ─── Why a sixth kind ─────────────────────────────────────────────────
--
-- `grounding` (migration 200) is the agent populating a field no tool of its
-- could supply: fabrication. This is the mirror image and it had no name — the
-- agent leaving a field EMPTY that its own declared tool can fill, and grading
-- the block `tool_no_match`, which reads as "the tool answered and had nothing".
--
-- Measured, on two production runs carrying the identical grade:
--
--   genome_profiler   called `ncbi_genome_search`, 210 bytes back. Lucanus
--                     cervus has no sequenced genome. Correct behaviour.
--   football_analyst  graded `advanced_metrics` `tool_no_match`. xG lives in
--                     `fixtures/statistics.expected_goals`. It called
--                     standings, teams/statistics, players, injuries and
--                     players/topscorers. It never asked.
--
-- `src/grounding_trust.rs` is explicit that the grade cannot tell them apart —
-- "Content present ~ tool returned data" — because it is inferred from the
-- field being empty. The distinction needs evidence from outside the document,
-- and `/api/episodes/:id/probe` is where that evidence now comes from.
--
-- The field contract predicted this exact failure and could not test it:
--
--   > trusting an agent's self-report about its own tool's capabilities is the
--   > identical error to trusting its self-report about a genome size.
--
-- ─── Why it is an anomaly and not a verdict ───────────────────────────
--
-- Because Loop 2 already exists and this is its input. `anomaly_events` feeds
-- the HITL queue, which feeds `InterventionEncoder`, the `CoherenceGate` and
-- `TwoWriteMemory`, whose synthetic episode enters Loop 1 and becomes a
-- semantic rule the agent retrieves on its next run.
--
-- That path has validation, a coherence gate, second-reviewer consensus for
-- agent-wide scope, and an immutable audit trail. A surface that wrote a
-- correction directly would have none of them — and the moment verification
-- output becomes training input, the thing protecting the agent's world model
-- from a bad verdict is the only thing standing between a misclick and a rule.
--
-- ─── Not backfilled ───────────────────────────────────────────────────
--
-- Nothing here scans history. A contradiction requires somebody to have run the
-- tool and read the answer; inferring one from stored grades would be inventing
-- the evidence this kind exists to carry.

DO $$
BEGIN
    ALTER TABLE public.anomaly_events
        DROP CONSTRAINT IF EXISTS anomaly_events_kind_check;
    ALTER TABLE public.anomaly_events
        ADD CONSTRAINT anomaly_events_kind_check
        CHECK (kind IN ('drift', 'rolling_conflict', 'rupture', 'safety',
                        'grounding', 'contradicted'));
END $$;

COMMENT ON COLUMN public.anomaly_events.kind IS
  'drift | rolling_conflict | rupture | safety | grounding | contradicted. '
  '`grounding` (mig-200) = the agent populated an output field no tool of its '
  'could supply. `contradicted` (mig-225) = the mirror: the agent left a '
  'contracted field empty and a tool run shows its own declared tool can '
  'supply it. Raised only with evidence from an actual tool call, never '
  'inferred from a grade.';
