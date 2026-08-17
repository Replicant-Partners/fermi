-- Migration 201: record WHICH agent extracted a semantic rule.
--
-- ## What was missing
--
-- Loop 1's correction half works like this: the consolidation worker asks the
-- `ontologist` to derive semantic rules from an agent's episodes, and the rules
-- are stored under the **subject** agent — `SemanticRule { agent_id, .. }` in
-- `agent-bestiary/memory/src/consolidation.rs`. That is correct: the knowledge
-- belongs to whoever will use it.
--
-- The consequence is that nothing records who *produced* it. The nearest thing
-- is `verification_method`, which holds `llm_extraction:<model>` — a model
-- string, not an agent. So the platform could not answer "how good is the
-- ontologist at extraction?", because it could not identify a single rule the
-- ontologist had written.
--
-- That is the missing half of a feedback loop, not a reporting gap. Loop 1 for
-- the extractor needs a signal, the only honest signal is whether the rules it
-- wrote turned out to be useful, and utility cannot be attributed to a producer
-- that is not recorded.
--
-- ## Why this and not a join
--
-- `source_episode_cluster` links a rule to the episodes it came from, and those
-- episodes belong to the subject. There is no path from either back to the
-- extractor: consolidation is handed a bare `LLMProvider`, so the extraction
-- call left no row anywhere until the dream ledger was added, and even that
-- records the cycle rather than the individual rule. A column is the only way
-- to attribute a rule to its author.
--
-- ## Scope: rules only, deliberately
--
-- `entities` and `facts` are extracted in the same cycles and are not stamped
-- here. The ontologist's declared output is `semantic-rules` — it is resolved
-- by that label in `dream_member(state, "semantic-rules", "ontologist")` — so
-- rules are the unit its contract is written in and the unit it should be
-- judged on. Extending this to entities and facts is a later decision, and
-- should be made because someone wants to score those separately, not because
-- the column pattern was easy to copy.
--
-- ## The utility counters were already here
--
-- `semantic_rules` has carried `application_count`, `successful_applications`,
-- `failed_applications` and `last_validated_at` since migration 010. All four
-- had ZERO non-test references in the codebase — declared, never written, never
-- read. The schema anticipated exactly this signal and nothing populated it.
-- This migration adds only the missing provenance; the counters it feeds were
-- waiting.
--
-- Read-only for existing rows: `extracted_by` is NULL for every rule written
-- before this deploy, and NULL means "author unrecorded", not "no author". The
-- signal emitter must therefore exclude NULLs rather than treating them as
-- belonging to anyone — see `extraction_utility` in
-- `src/handlers/consolidation.rs`.

-- ## Why a DO block and not BEGIN/COMMIT
--
-- Postgres is reached through PgBouncer in transaction-pooling mode, where an
-- explicit BEGIN/COMMIT pair does not reliably wrap the statements between it:
-- the pooler may hand them to different backends, so a "transaction" can be
-- half-applied without erroring. `scripts/lint-migrations.sh` rejects the
-- pattern outright for that reason (memory/MEMORY.md → PgBouncer Pitfalls).
--
-- A single DO block is one statement to the pooler and therefore genuinely
-- atomic — the same shape migration 200 uses for its CHECK swap. Every
-- statement inside is idempotent, so an earlier partial apply re-runs safely.
DO $$
BEGIN
    ALTER TABLE public.semantic_rules
        ADD COLUMN IF NOT EXISTS extracted_by UUID
            REFERENCES public.agents(agent_id) ON DELETE SET NULL;

    COMMENT ON COLUMN public.semantic_rules.extracted_by IS
        'Agent that produced this rule (the dream_coordinator EXTRACT member, '
        'normally `ontologist`). Distinct from `agent_id`, which is the agent the '
        'rule is FOR. NULL means the author was not recorded — every rule written '
        'before migration 201. Consumers must exclude NULL rather than attribute it.';

    -- Partial: the signal query only ever asks for rules with a known author,
    -- and pre-201 rows are permanently NULL, so indexing them wastes space on a
    -- selectivity the planner cannot use.
    CREATE INDEX IF NOT EXISTS idx_semantic_rules_extracted_by
        ON public.semantic_rules(extracted_by)
        WHERE extracted_by IS NOT NULL;

    -- Supports the utility query directly: "rules this agent extracted, how
    -- old, and were they ever retrieved". Ordering by created_at because the
    -- resolution rule is age-based — a rule too young to have been retrieved is
    -- not yet evidence either way.
    CREATE INDEX IF NOT EXISTS idx_semantic_rules_extractor_utility
        ON public.semantic_rules(extracted_by, created_at)
        WHERE extracted_by IS NOT NULL AND is_active;
END $$;
