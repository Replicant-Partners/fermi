-- Migration 194: make episode cost a DERIVED, auditable quantity.
--
-- ## The problem
--
-- `episodes` stored `tokens_used` (one number) and `cost_usd` (a figure
-- computed at write time). Neither the input/output token split nor the
-- rate basis was recorded, which has two consequences:
--
--   1. **Cost could not be corrected.** `calculate_cost` matched on the
--      model string alone with `_ => 3.0`, so a DeepSeek agent
--      (`efra_critical_factor`) was priced at Anthropic Sonnet's rate:
--      two runs recorded at $0.616272 and $0.311628 against a real
--      ~$0.090 and ~$0.046 — roughly 6.9x overstated. Because the price
--      was baked into `cost_usd` and the split discarded, those rows
--      cannot be re-derived from what was persisted.
--   2. **Trust was inferred from a deploy date.** `economics.rs` carries
--      `RATE_CARD_WIRED_ON = '2026-08-12'` and treats everything before
--      it as flat-rated. A date boundary cannot express "this row's rate
--      was known but its split was assumed", which is the actual state of
--      most rows.
--
-- Storing the split plus the basis makes cost a function of persisted
-- inputs: fix a rate, re-derive history. That property is a precondition
-- for settling a marketplace on these numbers, and for dividing spend
-- into a resolved Brier score.
--
-- ## What this adds
--
--   * `input_tokens`  / `output_tokens` — the split, when the provider
--     reported it. NULL means "not reported", which pricing treats as
--     "assume a split", NOT as zero.
--   * `cost_basis` — how much to trust `cost_usd`, per row:
--       'measured_split' — known rate, real split. Trustworthy.
--       'assumed_split'  — known rate, split assumed at 20% output.
--       'unknown_model'  — no rate for this (provider, model). A DATA GAP,
--                          not a cost. Count these and fix the rate card.
--       'no_charge'      — local inference; zero asserted, not missing.
--     NULL = written before this migration; basis genuinely unknown.
--   * `cost_rate_key` — which rate-card row priced the run
--     (e.g. 'anthropic/claude-sonnet-4', 'openrouter:anthropic/claude-haiku-4').
--     Makes a mispricing traceable to the entry that caused it.
--
-- Additive only: every column is nullable with no default backfill, so
-- historical rows stay honestly unlabelled rather than being retconned
-- into a basis they never had. PgBouncer-safe (no BEGIN/COMMIT, no
-- CONCURRENTLY).

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS input_tokens  INTEGER,
    ADD COLUMN IF NOT EXISTS output_tokens INTEGER,
    ADD COLUMN IF NOT EXISTS cost_basis    TEXT,
    ADD COLUMN IF NOT EXISTS cost_rate_key TEXT;

-- Guard the vocabulary. Without this, a typo'd basis silently reads as
-- "not trustworthy" in every consumer and the row is quietly excluded
-- from cost analysis instead of failing loudly at write time.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'episodes_cost_basis_valid'
    ) THEN
        ALTER TABLE public.episodes
            ADD CONSTRAINT episodes_cost_basis_valid
            CHECK (cost_basis IS NULL OR cost_basis IN (
                'measured_split', 'assumed_split', 'unknown_model', 'no_charge'
            ));
    END IF;
END $$;

-- The two queries this table now has to answer cheaply:
--
--   "what fraction of spend is priced on a basis we trust?"
--   "which (provider, model) pairs are landing in the unknown bucket?"
--
-- Partial index because untrustworthy rows are the minority and the
-- interesting set — a full index would mostly store rows nobody queries.
CREATE INDEX IF NOT EXISTS idx_episodes_cost_basis_untrusted
    ON public.episodes(cost_basis, provider_used, model_used)
    WHERE cost_basis IN ('assumed_split', 'unknown_model');

COMMENT ON COLUMN public.episodes.input_tokens IS
    'Prompt tokens. NULL = provider did not report the split (assume, do not zero).';
COMMENT ON COLUMN public.episodes.output_tokens IS
    'Completion tokens. Priced 3-5x input, which is why the split is stored.';
COMMENT ON COLUMN public.episodes.cost_basis IS
    'Trust level of cost_usd: measured_split | assumed_split | unknown_model | no_charge. NULL = pre-migration-194.';
COMMENT ON COLUMN public.episodes.cost_rate_key IS
    'Rate-card row that priced this run, so a mispricing is traceable to its cause.';
