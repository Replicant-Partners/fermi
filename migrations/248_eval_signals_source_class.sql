-- 248: what kind of claim an eval signal is (LLD G12, directive rule 8).
--
-- `interpretive`: a language model's judgement (LLM judge, the LLM modes of
-- WildGuard, Sotopia and CharacterEval). Never evidence, never a source in a
-- provenance or Grounding walk.
-- `computed`: deterministic code over recorded data (Brier, projection
-- scoring, pattern matching, schema conformance).
--
-- Nullable, NULL meaning "recorded before this column existed". Rows are not
-- backfilled: an old row's evaluator config (LLM or heuristic) was never
-- recorded, so any backfilled value would be a guess.

ALTER TABLE public.eval_signals
    ADD COLUMN IF NOT EXISTS source_class TEXT;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'eval_signals_source_class_check'
    ) THEN
        ALTER TABLE public.eval_signals
            ADD CONSTRAINT eval_signals_source_class_check
            CHECK (source_class IS NULL OR source_class IN ('interpretive', 'computed'));
    END IF;
END $$;
