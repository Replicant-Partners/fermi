-- 248: what kind of claim an eval signal is (LLD G12, directive rule 8).
--
-- `interpretive`: a language model's judgement (LLM judge, the LLM modes of
-- WildGuard, Sotopia and CharacterEval). Never evidence, never a source in a
-- provenance or Grounding walk.
-- `computed`: deterministic code over recorded data (Brier, projection
-- scoring, pattern matching, schema conformance).
--
-- NULL only on rows recorded before this column existed. They are not
-- backfilled: an old row's evaluator config (LLM or heuristic) was never
-- recorded, so any backfilled value would be a guess. New rows must carry a
-- class: the NOT NULL check is NOT VALID, so it binds every new or changed
-- row without asserting anything about the old ones.

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

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'eval_signals_source_class_required'
    ) THEN
        ALTER TABLE public.eval_signals
            ADD CONSTRAINT eval_signals_source_class_required
            CHECK (source_class IS NOT NULL) NOT VALID;
    END IF;
END $$;
