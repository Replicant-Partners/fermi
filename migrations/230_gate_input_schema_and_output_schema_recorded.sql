-- Migration 230: two things at once, documented together because they belong together.
--
-- ## 1. Widen gate_decisions and gate_decision_reviews for `input_schema`
--
-- Gate::InputSchema (discriminant 8) was added in the Phase C A2A work with
-- `id: "input_schema"`. `gate_trust::GATE_IDS` gained the token. The two DB
-- CHECK constraints (gate_decisions_gate_check, gate_decision_reviews_gate_check)
-- were not widened at the time — `InputSchema` is `Retention::Counted`, so it
-- writes no ledger row. The token was still latent in GATE_IDS, and
-- `seam_vocabulary_contract` pins GATE_IDS against the constraint. This widen
-- removes that latent failure before it becomes a live one.
--
-- ## 2. Promote Gate::OutputSchema to Retention::Recorded
--
-- `Gate::OutputSchema` was promoted from `Retention::Counted` to
-- `Retention::Recorded` in `gate_trust.rs` alongside this migration. The
-- gate_decisions constraint already accepts `output_schema` (migration 217);
-- gate_decision_reviews already accepts it (migration 219). No new DDL is
-- needed for the promotion itself — this file is the argument, which is the
-- same pattern migration 221 used for grounding.
--
-- What changes with the promotion: every delegation hop that calls
-- `envelope::build` now writes a ledger row for Gate::OutputSchema. The fidelity
-- score — `approved / (approved + refused)` per agent — becomes queryable from
-- `gate_decisions WHERE gate = 'output_schema' AND subject = agent_name`.
-- Before this, that query returned 0 rows for every agent, and any surface that
-- read it would have silently reported perfect fidelity by arithmetic default.
--
-- ## Why one migration for two changes
--
-- They are causally linked. Widening for input_schema is required by the seam
-- vocabulary contract. Widening is a safe moment to also document the OutputSchema
-- promotion — both are changes to what the gate ledger records and both need to be
-- found together by whoever audits the gate decision schema next.

DO $$ BEGIN
    ALTER TABLE public.gate_decisions
        DROP CONSTRAINT IF EXISTS gate_decisions_gate_check;

    ALTER TABLE public.gate_decisions
        ADD CONSTRAINT gate_decisions_gate_check
        CHECK (gate IN (
            'coherence', 'grounding', 'input_binding',
            'admission', 'credit', 'rate_limit',
            'attachment', 'output_schema', 'input_schema'
        ));

    COMMENT ON COLUMN public.gate_decisions.gate IS
        'Which gate decided. Closed vocabulary owned by gate_trust::GATE_IDS '
        'and registered in src/seam_vocabulary.rs. '
        'Widened for output_schema by migration 217, '
        'for input_schema by migration 230.';
END $$;

DO $$ BEGIN
    ALTER TABLE public.gate_decision_reviews
        DROP CONSTRAINT IF EXISTS gate_decision_reviews_gate_check;

    ALTER TABLE public.gate_decision_reviews
        ADD CONSTRAINT gate_decision_reviews_gate_check
        CHECK (gate IN (
            'coherence', 'grounding', 'input_binding',
            'admission', 'credit', 'rate_limit',
            'attachment', 'output_schema', 'input_schema'
        ));

    COMMENT ON COLUMN public.gate_decision_reviews.gate IS
        'Denormalised from gate_decisions.gate. '
        'Widened for output_schema by migration 219, '
        'for input_schema by migration 230.';
END $$;
