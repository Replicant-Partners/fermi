-- Migration 223: which loop produced a semantic rule.
--
-- ## Why a closure that works is still not demonstrable
--
-- `FEEDBACK_LOOPS.md` states the nesting as a live claim: "Loop 3 -> Loop 1
-- (coordination observations become semantic rules in member memory)". The sink
-- is real and populated -- `semantic_rules` holds 263 rows.
--
-- But its columns are `source_episode_cluster`, `episode_count`,
-- `verification_status`, `verification_method`. Every one of them describes
-- consolidation: a rule extracted from a cluster of episodes, which is Loop 1.
-- There is no field that says a rule arrived any other way.
--
-- So a coordination-derived rule is indistinguishable from an ordinary
-- consolidation rule, and the Loop 3 -> Loop 1 hop is **unfalsifiable even when
-- it fires**. It can be asserted and it cannot be shown, which is the defect
-- class the verification ladder exists to name: the easier question ("are there
-- rules?") passes while the harder one ("did coordination produce any?") cannot
-- be asked at all.
--
-- This is also why it matters more than it looks. Loop 3's own stages currently
-- read `plans: 0`, `intentions: 0`, `brief: 0` with `settled: 25` -- coherence is
-- measured and no observation reaches a member. If that is fixed, this column is
-- the only way anyone will be able to tell.
--
-- ## Open vocabulary, deliberately
--
-- A CHECK constraint here would have to be widened by whoever adds the next
-- producer, and migration 219 is the record of what happens when one of a pair
-- of constraints is widened and the other is not: the decision was recordable
-- and its review was not. An unrecognised origin should reach a screen and
-- render as unknown -- which is what the trust surfaces do with every
-- unrecognised token -- rather than be rejected at the write.
--
-- Expected values, none enforced:
--   consolidation   the extraction sweep over an episode cluster (Loop 1)
--   coordination    a coordination observation written back to a member (Loop 3)
--   calibration     a rule motivated by measured error (Loop 5.A -> 5.B)
--   human           a person wrote it
--
-- ## Nullable, and NULL means "before this column existed"
--
-- The 263 existing rows are not backfilled. Attributing them would mean
-- guessing, and a guessed provenance is worse than an absent one: it would make
-- the Loop 3 claim look answered. NULL reads as "unattributed, predates the
-- column", which is the honest state and is distinguishable from every real
-- value.

ALTER TABLE public.semantic_rules
    ADD COLUMN IF NOT EXISTS origin TEXT;

CREATE INDEX IF NOT EXISTS semantic_rules_origin_idx
    ON public.semantic_rules (origin)
    WHERE origin IS NOT NULL;

COMMENT ON COLUMN public.semantic_rules.origin IS
    'Which loop produced this rule: consolidation (Loop 1), coordination '
    '(Loop 3), calibration (Loop 5), or human. Deliberately unconstrained - an '
    'unrecognised value should render as unknown on a surface rather than be '
    'rejected at the write, and a CHECK here would need widening by whoever adds '
    'the next producer (see migration 219 for the cost of widening one '
    'constraint and not its pair). NULL means the row predates the column and is '
    'NOT backfilled: a guessed provenance would make the Loop 3 -> Loop 1 claim '
    'look answered when it is not.';
