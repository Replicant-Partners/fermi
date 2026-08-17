-- Migration 203: a semantic rule records how well-grounded its evidence was.
--
-- ## The gap
--
-- Loop 1's correction half runs like this: `ontologist` reads an agent's
-- episodes during a dream cycle and writes `semantic_rules` from them. Those
-- rules are then retrieved and injected into later prompts as context — they
-- become things the platform tells its own agents are true.
--
-- Nothing in that path records how well-grounded the episodes were. A rule
-- extracted from ten `tool_verified` genome lookups and a rule extracted from
-- ten paragraphs of model prose are stored identically: same table, same
-- `confidence_score` scale, same retrieval path, same injection. The second
-- kind is a fabrication with a citation, and it is *more* dangerous than a
-- bare hallucination because the citation is real — `source_episode_cluster`
-- genuinely points at episodes that genuinely said that.
--
-- That is a laundering path, and it runs in the direction the platform cares
-- about most: outward, into other agents' prompts.
--
-- ## What a floor is
--
-- Two rules, both in `grounding_trust`:
--
--   floor(sources)   = the WEAKEST verdict among the source episodes.
--   EXTRACTION_CEILING = model_inference.
--
-- and the value stored here is the minimum of the two.
--
-- The floor is a minimum because nine sourced episodes and one guess is a
-- guess; averaging would let volume launder a fabrication. The ceiling exists
-- because provenance is not transitive upward: reading ten tool-verified
-- episodes and writing down a generalisation about them is an act of
-- judgement, and judgement does not inherit retrieval. Without the ceiling
-- the ontologist would manufacture `tool_verified` facts out of nothing but
-- its own reading.
--
-- So the best value any extracted rule can ever hold is `model_inference`.
-- That is not a defect. It is the honest ceiling for the class of operation,
-- and a rule that claimed better would be lying about what extraction is.
--
-- ## NULL means unknown, and unknown is not clean
--
-- Every one of the 190 existing rules gets NULL, and NULL must never be read
-- as "fine". Two separate reasons it is unrecoverable:
--
--   * Before migration 199 `episodes` did not retain `response_text` at all.
--     The evidence those rules were extracted from no longer exists, so their
--     groundedness is not merely unrecorded, it is gone.
--   * `grounding_trust` has field contracts for a handful of agents. For the
--     rest, absence of a contract is absence of a verdict — not a pass.
--
-- Consumers must therefore treat NULL as a third state. The pattern to copy
-- is `extracted_by` from migration 201: exclude NULLs from the signal rather
-- than attributing them. A report that counts NULL as grounded would show the
-- corpus getting cleaner as coverage got worse, which is the failure mode
-- this whole layer exists to prevent.
--
-- ## Not the same thing as `provenance_trusted`
--
-- `semantic_rules` already carries `provenance_trusted` (migration 135) and
-- the names are close enough to invite a real mistake. They are different
-- axes and both are needed:
--
--   provenance_trusted — Spec 22, EMBEDDING provenance. "Is `source_text`
--       genuinely the string that produced `embedding`?" A mechanical
--       question about a vector's reconstructability.
--   provenance_floor   — EPISTEMIC provenance. "Could anything have known
--       that the content of this rule is true?"
--
-- A rule can be perfectly embedding-trusted and epistemically worthless: the
-- vector faithfully encodes a sentence nobody had grounds to write. Note also
-- that `provenance_trusted` is `NOT NULL DEFAULT true`, which is defensible
-- for its own question (the writer normally does know the source text) but is
-- exactly the shape this column must not take. A trust flag that defaults to
-- trusted answers a question nobody asked.
--
-- ## Why the basis column
--
-- A floor with no working shown is an assertion, and an assertion is the
-- thing being replaced. `provenance_floor_basis` records the per-source
-- verdicts, how many sources were seen, and whether the ceiling was the
-- binding constraint — enough for a reader to recompute the value and
-- disagree. It also makes the empty-cluster case *visible* rather than
-- inferable: `{"sources": [], "reason": "empty_cluster"}` is a finding.
--
-- ## Vocabulary
--
-- The CHECK enumerates `grounding_trust::PROVENANCE_VALUES`. The two are kept
-- in step by `the_migration_check_matches_the_runtime_vocabulary` in
-- `tests/grounding_contract.rs`, which parses this file. Drift between a
-- card's vocabulary and the runtime's has already happened once here
-- (`gbif_verified` for `tool_verified`), and a value the runtime emits but the
-- constraint rejects would surface as a write failure inside a dream cycle,
-- at 3am, in a worker whose errors are logged rather than raised.

-- One DO block, not BEGIN/COMMIT: Postgres is behind PgBouncer in
-- transaction-pooling mode, where an explicit transaction is not reliably
-- honoured across statements and can half-apply without erroring.
-- `scripts/lint-migrations.sh` rejects the pattern. Every statement here is
-- idempotent, so a partial apply re-runs safely.
DO $$
BEGIN
    ALTER TABLE public.semantic_rules
        ADD COLUMN IF NOT EXISTS provenance_floor TEXT,
        ADD COLUMN IF NOT EXISTS provenance_floor_basis JSONB;

    -- Dropped and re-added rather than guarded, so that widening
    -- PROVENANCE_VALUES is a one-line edit here and the parity test tells you
    -- to make it.
    ALTER TABLE public.semantic_rules
        DROP CONSTRAINT IF EXISTS semantic_rules_provenance_floor_check;
    ALTER TABLE public.semantic_rules
        ADD CONSTRAINT semantic_rules_provenance_floor_check
        CHECK (provenance_floor IS NULL OR provenance_floor IN (
            'tool_verified',
            'tool_no_match',
            'unavailable_no_tool_source',
            'model_inference',
            'platform_derived'
        ));

    COMMENT ON COLUMN public.semantic_rules.provenance_floor IS
        'Weakest provenance among the episodes this rule was extracted from, '
        'capped at `model_inference` because extraction is judgement and does '
        'not inherit retrieval. Computed by `grounding_trust::extracted_floor`. '
        'NULL means UNKNOWN — every rule written before migration 203, plus any '
        'rule whose subject agent has no field contract. NULL is not a pass: '
        'consumers must exclude it from grounded counts rather than assume it. '
        'Different axis from `provenance_trusted`, which is about whether '
        '`source_text` reconstructs `embedding` (Spec 22).';

    COMMENT ON COLUMN public.semantic_rules.provenance_floor_basis IS
        'Working shown for `provenance_floor`, so a reader can recompute and '
        'disagree: {"sources": [{"episode_id": .., "floor": ..}], '
        '"ceiling_applied": bool, "reason": ..}. `{"sources": []}` records an '
        'empty source cluster, which is a finding rather than a default.';

    -- Partial index on the two states worth acting on: rules that are known
    -- to be ungrounded (fix or retire), and rules whose grounding is unknown
    -- (the backlog). Deliberately NOT indexing `model_inference`, which is
    -- the expected steady state for every extracted rule and would be most
    -- of the table.
    CREATE INDEX IF NOT EXISTS idx_semantic_rules_weak_floor
        ON public.semantic_rules(agent_id, created_at DESC)
        WHERE is_active
          AND (provenance_floor IS NULL
               OR provenance_floor IN ('unavailable_no_tool_source', 'tool_no_match'));
END $$;
