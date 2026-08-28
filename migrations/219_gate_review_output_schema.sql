-- Migration 219: widen `gate_decision_reviews.gate` to the eighth gate.
--
-- ## The drift this repairs, and the note that predicted it
--
-- `gate_trust::GATE_IDS` gained `output_schema`. Migration 214's CHECK on
-- `gate_decisions.gate` was widened to match. Migration 216's CHECK on
-- `gate_decision_reviews.gate` was not.
--
-- 216's own registry entry in `src/seam_vocabulary.rs` says what that costs, and
-- it was written before it happened:
--
--     Two CHECKs over one vocabulary is exactly the drift this registry is for:
--     widening `gate_trust::GATES` and migration 214's constraint while leaving
--     216's alone makes the new gate's decisions recordable and its reviews
--     unwritable, which is the worse half — the decision is logged, the reviewer
--     is told nothing, and the gate reads unreviewed forever.
--
-- That is precisely the state production was in when this was written:
-- `gate_decisions.gate` accepting 8 values, `gate_decision_reviews.gate`
-- accepting 7, and `tests/seam_vocabulary_contract.rs` reporting
-- `Rust declares ["output_schema"], which the database will REFUSE`.
--
-- ## Why it was latent rather than breaking
--
-- `output_schema` is `Retention::Counted`, so it writes no ledger row and there
-- is nothing to review yet. The failure was waiting for the promotion, at which
-- point a reviewer would have pressed a button, seen a 500, and had no way to
-- know the cause was a constraint two migrations back.
--
-- Worth stating because it is the argument for registering a vocabulary at all:
-- the seam contract found this with no traffic, no promotion and no reviewer —
-- by comparing two declarations that nothing else compares.
--
-- ## Why not derive the list from `GATE_IDS`
--
-- Because Postgres cannot read a Rust const. That is the whole reason
-- `seam_vocabulary` exists: the two sides each hold an opinion, neither can see
-- the other, and the only defence is a test that reads both. This file is the
-- Postgres half being brought back into line; the Rust half is unchanged.

DO $$ BEGIN
    ALTER TABLE public.gate_decision_reviews
        DROP CONSTRAINT IF EXISTS gate_decision_reviews_gate_check;
    ALTER TABLE public.gate_decision_reviews
        ADD CONSTRAINT gate_decision_reviews_gate_check
        CHECK (gate IN ('coherence', 'grounding', 'input_binding',
                        'admission', 'credit', 'rate_limit',
                        'attachment', 'output_schema'));
END $$;

COMMENT ON COLUMN public.gate_decision_reviews.gate IS
    'Which gate the reviewed decision belongs to. Denormalised from '
    'gate_decisions.gate so the per-gate standing is one index scan. Owned by '
    'gate_trust::GATE_IDS and registered in seam_vocabulary; widened to eight '
    'by migration 219 after `output_schema` was added to the Rust side and only '
    'migration 214''s constraint followed.';
