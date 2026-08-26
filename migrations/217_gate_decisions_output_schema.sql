-- 217 — widen `gate_decisions_gate_check` for `output_schema`.
--
-- ## What this gate is
--
-- `Gate::OutputSchema` counts documents that contradict the schema their own
-- producer declared, decided in `agent_backend::envelope::build` at every
-- delegation hop.
--
-- Before it existed, `build` computed exactly that verdict and reported it to
-- nobody. The verdict went into the delegation result JSON under
-- `envelope.validation.status` and no consumer read it — which sat awkwardly
-- against `gate_trust`'s own premise, that a refusal nobody counted is the
-- state that module exists to make impossible.
--
-- ## Why widen a CHECK for a gate that writes no rows
--
-- `Gate::OutputSchema` is `Retention::Counted`, so `gate_trust::flush` never
-- inserts a row for it and this constraint is never exercised today.
--
-- The token is added anyway, because `gate_trust::GATE_IDS` is *registered* in
-- `src/seam_vocabulary.rs` as the vocabulary of this column. A Rust constant
-- listing a token Postgres rejects is a latent version of the failure 214 was
-- written about: the day someone promotes the retention to `Recorded`, every
-- decision by this gate becomes unwritable inside a batch insert whose error is
-- swallowed by design, and the loss is counted as a write failure and nothing
-- else. Widening now costs one statement and removes the trap. Leaving it
-- narrow saves nothing and stores a swallowed insert for a future author.
--
-- The alternative — omit the token from `GATE_IDS` — is worse: it would make
-- the constant stop describing the gate set, and
-- `gate_ids_match_the_declared_gates` exists precisely to stop that drift.
--
-- ## Promotion path, for whoever needs the ledger
--
-- To make individual refusals durable and reviewable:
--   1. `Retention::Recorded` on the spec in `src/gate_trust.rs`.
--   2. Widen the same vocabulary on `gate_decision_reviews.gate`, added by
--      migration 216. Do it in a new migration rather than editing 216.
--   3. Add a door to `gate_api::GATE_DOORS`, or the rows accrue with nobody
--      able to say whether any refusal was right — which `gate_review.rs`
--      names as the failure counters cannot detect.
-- Steps 2 and 3 are why this landed as `Counted`: an unreviewable ledger row
-- is not obviously better than a counter, and a half-promoted gate is worse
-- than either.

-- ## Why the DROP and the ADD are in one block
--
-- `scripts/lint-migrations.sh` refuses a bare DROP+ADD pair, and the first
-- draft of this migration was exactly that. Through PgBouncer each statement
-- can land in its own transaction: the DROP commits, the ADD fails, and
-- `run_migrations` logs the failure and continues. The net effect of a
-- migration whose purpose is to WIDEN a constraint would then be to DELETE it,
-- leaving the column with no CHECK at all and the vocabulary registration in
-- `seam_vocabulary.rs` describing a constraint that no longer exists.
--
-- Wrapped so the pair is atomic or neither happens.

DO $$
BEGIN
    ALTER TABLE public.gate_decisions
        DROP CONSTRAINT IF EXISTS gate_decisions_gate_check;

    ALTER TABLE public.gate_decisions
        ADD CONSTRAINT gate_decisions_gate_check
        CHECK (gate IN ('coherence', 'grounding', 'input_binding',
                        'admission', 'credit', 'rate_limit',
                        'attachment', 'output_schema'));

    COMMENT ON COLUMN public.gate_decisions.gate IS
        'Which gate decided. Closed vocabulary owned by gate_trust::GATE_IDS '
        'and registered in src/seam_vocabulary.rs. Widened for output_schema '
        'by migration 217.';
END $$;
