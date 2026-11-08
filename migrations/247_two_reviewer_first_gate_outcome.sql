-- 247: persist the first reviewer's coherence-gate outcome (LLD G10).
--
-- The consensus handler re-ran the gate with an empty WorldModel, so every
-- agent-wide correction was recorded `undetermined` whatever the first review
-- found, and the first Γ was lost. The outcome is now written with the request
-- (same INSERT, directive rule 10) and the second reviewer replays it.
--
-- Nullable: a request created before this column has no outcome to replay, and
-- the handler refuses to confirm it rather than inventing one.

ALTER TABLE public.two_reviewer_requests
    ADD COLUMN IF NOT EXISTS first_gate_outcome JSONB;

COMMENT ON COLUMN public.two_reviewer_requests.first_gate_outcome IS
    'GateOutcome from the first review (agent-bestiary/coherence-gate GateOutcome JSON). Replayed by the consensus handler, never recomputed (G10).';
