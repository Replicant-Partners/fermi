-- Migration 220: bind a gate decision to the artifact it was about.
--
-- ## What this unblocks
--
-- `GET /api/episodes/:episode_id/trace` renders a belt of checkpoints, and every
-- rung but one reads `outcome: not_recorded` with the reason *"`gate_decisions`
-- carries no `episode_id`, so no row can be joined to this artifact."* This is
-- that column.
--
-- ## The column alone would have been useless, and that is worth recording
--
-- The obvious reading of the gap is "add the join key". Measured before writing
-- it: **every per-episode gate is `Retention::Counted`, and both `Recorded` gates
-- are not per-episode.** `coherence` fires on an AgentWide correction and
-- `admission` at publish; `grounding`, `input_binding`, `credit`, `rate_limit`,
-- `attachment` and `output_schema` all write no ledger row at all. So this column
-- would have been NULL on every row that would ever exist, while making the
-- trace's `not_recorded` look solved.
--
-- The blocker was retention, not the key. Migration 221 promotes `grounding`.
--
-- ## No foreign key, and the reason is the batch
--
-- `assertion_verifications.episode_id` is a real FK and this deliberately is not.
-- The difference is the writer: `gate_trust::spawn_gate_recorder` drains a queue
-- with a single `INSERT ... SELECT FROM UNNEST(...)`, so **one bad reference
-- rejects the whole batch.** An episode write that failed for its own reasons
-- would take every unrelated gate decision in that flush down with it — a gate's
-- audit trail lost because something else did not land, which is precisely the
-- coupling the ledger exists to avoid.
--
-- A decision is also enqueued *before* its episode row exists: the gate fires
-- mid-request and the recorder flushes on a timer. That is the same race that
-- made Loop 2's original raise fail silently for the life of the feature, when it
-- referenced an episode id whose row had not been written yet.
--
-- So the reference is unenforced and **checked instead**, by
-- `tests/gate_decision_lineage.rs`. The precedent is
-- `assertion_verifications.assertion_id`, which is not a foreign key either
-- because its target lives inside a JSONB array — different reason, same remedy:
-- an unresolvable reference is a finding rather than a rejected write.
--
-- ## Nullable, permanently
--
-- Not a defect to be backfilled. `credit` and `rate_limit` decide **whether to
-- run at all** — they fire before any artifact exists and there may never be one,
-- so NULL is the correct and final answer for them. A NOT NULL here would make
-- the pre-execution gates unrecordable.

ALTER TABLE public.gate_decisions
    ADD COLUMN IF NOT EXISTS episode_id UUID;

-- The read this exists for: "what did every gate decide about this artifact".
-- Partial, because the pre-execution gates are permanently NULL and indexing
-- them buys a selectivity the planner cannot use.
CREATE INDEX IF NOT EXISTS gate_decisions_episode_idx
    ON public.gate_decisions (episode_id)
    WHERE episode_id IS NOT NULL;

COMMENT ON COLUMN public.gate_decisions.episode_id IS
    'The artifact this decision was about. NULL for gates that fire before the '
    'artifact exists (credit, rate_limit) - correct and final for those, not a '
    'backfill target. Deliberately NOT a foreign key: the recorder inserts a '
    'whole batch in one statement, so one bad reference would reject every '
    'unrelated decision in the flush, and a decision is enqueued before its '
    'episode row is written. Checked by tests/gate_decision_lineage.rs instead.';
