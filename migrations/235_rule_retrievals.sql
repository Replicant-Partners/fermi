-- 235 — rule_retrievals: which rule went into which run.
--
-- WHY THIS TABLE EXISTS
--
-- Loop 1 claims episodes become semantic rules and those rules are retrieved
-- into the next prompt. Retrieval is now counted (`semantic_rules.
-- application_count`), so the loop closes as far as "was it used".
--
-- It does not close on "was it any good". `semantic_rules.verification_status`
-- has four readers and no production writer: measured 2026-09-10, all 264 real
-- rules on this deployment sit at `pending`, and the consolidation worker
-- reports `rules_verified: 0` unconditionally. Nothing promotes a rule.
--
-- Three cheap proxies were measured before adding a table, and none holds:
--   * near-duplicate rules to reject      0 pairs at cosine >= 0.95
--   * corroboration, independent cluster  6 of 265 rules
--   * human corrections to adjudicate     0 episodes
-- Rules are well grounded (265/265 carry a source cluster, avg 12.2 episodes)
-- and are not near-copies. There is simply no evidence in the stored data that
-- adjudicates them.
--
-- The specific missing evidence: `record_rule_retrievals` increments a counter
-- and discards WHICH run the rule was injected into, so no rule can be set
-- against the outcome of a run that used it. This records that pairing. It
-- adjudicates nothing by itself — it is the evidence a verifier would need,
-- and it has to start accruing before any verifier can be honest.
--
-- WHY query_sha AND NOT episode_id
--
-- Ordering. `enrich_with_kg_context` runs BEFORE the model does; the episode is
-- written after it returns. At the moment a retrieval happens there is no
-- episode row and no episode id to reference.
--
-- Threading one through would mean widening the enrich signature across ~10
-- execution call sites. `query_sha` correlates without touching any of them:
-- both sides already hold the query text. It is `artifact_hash::of_text`, i.e.
-- `'sha256:' || encode(sha256(convert_to(query,'UTF8')),'hex')`, so the join is
-- reproducible in SQL:
--
--   SELECT rr.rule_id, e.execution_status
--     FROM rule_retrievals rr
--     JOIN LATERAL (
--       SELECT execution_status
--         FROM episodes e
--        WHERE e.agent_id = rr.agent_id
--          AND 'sha256:' || encode(sha256(convert_to(e.query,'UTF8')),'hex')
--              = rr.query_sha
--          AND e.timestamp_created >= rr.created_at
--        ORDER BY e.timestamp_created
--        LIMIT 1
--     ) e ON true;
--
-- The correlation is not a key. An agent asked the same question twice has two
-- candidate episodes and the join takes the nearest subsequent one, which is a
-- guess in that case. `episode_id` is therefore present and nullable so a later
-- pass can record the pairing exactly, once the plumbing allows, without a
-- second migration. It is NULL today and that is the honest state.

CREATE TABLE IF NOT EXISTS public.rule_retrievals (
    retrieval_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- The rule that reached a prompt.
    rule_id      UUID NOT NULL
                 REFERENCES public.semantic_rules(rule_id) ON DELETE CASCADE,

    -- The agent whose prompt it reached. Denormalised from semantic_rules so
    -- the outcome join does not have to go through the rule to find the agent,
    -- and so a retrieval survives its rule being deleted in analysis windows
    -- that have already been aggregated.
    agent_id     UUID NOT NULL
                 REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- `artifact_hash::of_text(query)` — the correlation handle described above.
    query_sha    TEXT NOT NULL,

    -- Cosine similarity that admitted this rule, where the retrieval path knows
    -- it. NULL on the ANN path, which returns rows already filtered by the
    -- database and does not surface per-row scores. NULL means "not reported",
    -- never "zero".
    similarity   REAL,

    -- Reserved for exact attribution. See the header: NULL until the enrich
    -- signature can carry an episode id.
    episode_id   UUID REFERENCES public.episodes(episode_id) ON DELETE SET NULL,

    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The outcome join: by agent, in time order.
CREATE INDEX IF NOT EXISTS idx_rule_retrievals_agent_time
    ON public.rule_retrievals(agent_id, created_at DESC);

-- Per-rule adjudication: every run that used this rule.
CREATE INDEX IF NOT EXISTS idx_rule_retrievals_rule
    ON public.rule_retrievals(rule_id);

-- The correlation handle.
CREATE INDEX IF NOT EXISTS idx_rule_retrievals_query
    ON public.rule_retrievals(agent_id, query_sha);

COMMENT ON TABLE public.rule_retrievals IS
    'One row per (rule, prompt it was injected into). The evidence base for '
    'adjudicating semantic_rules.verification_status, which no code writes '
    'today. Recorded by agent_backend::kg_context::record_rule_retrievals.';
