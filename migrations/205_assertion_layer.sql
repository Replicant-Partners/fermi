-- Migration 205: record what an agent quantified, whether or not there is a
-- forecast to bind it to.
--
-- ## What was being thrown away
--
-- `forecast_agent_claims` (migration 187) has `workspace_id NOT NULL` and
-- `driver NOT NULL`. That is correct for a *claim*: a claim is an adjustment
-- applied to a driver, it is neutralisable at 1.0, and neutralisability is what
-- lets the Shapley engine synthesise the forecast for any SUBSET of agents and
-- so compute exact credit from a single real run. None of that means anything
-- without a driver.
--
-- The problem is that it was the only place an agent's quantified output could
-- go. So `execution.rs` gates the write on `if let Some(ws_id) = ws_id_opt`,
-- and an agent evaluated outside a workspace has its judgement discarded.
--
-- Measured, before this migration: **14 quantified judgements, 14 of them
-- produced outside any workspace, 0 rows in `forecast_agent_claims`.** Agents
-- are mostly exercised by standalone evaluation, which is exactly the mode in
-- which the platform kept nothing — so no agent could accumulate a track record,
-- and the recommendation problem ("which agents suit this decomposition?") had
-- no data underneath it.
--
-- ## Two objects, not one
--
--   assertion — what the agent quantified. Exists whenever the agent ran.
--   claim     — that assertion BOUND to a driver in a workspace. 0..n per
--               assertion.
--
-- The one-to-many matters. `football_analyst` is asked for three factors (X3
-- dynamic, X4 squad, X5 tactical) and the output format can carry one number,
-- so `agent_params_hook` copies a single multiplier into three claim rows. The
-- comment says so outright: "same multiplier applies to all three". That records
-- three judgements where one was made. One assertion with three bindings records
-- the truth, and makes visible that the agent is not actually differentiating
-- the factors.
--
-- ## Why `episodes.assertions` is flat and verification is not
--
-- Split along the immutable/mutable line, because the two halves have opposite
-- natures:
--
--   * An assertion is what the agent said at a moment. It never changes.
--     Written once with the episode, in the log, no join to read it. The
--     agent-owner observability read ("what has this agent asserted, how
--     grounded, how often rejected") is the high-frequency query and it stays
--     flat.
--   * A verification is a decision made later, by a tool or a person, and it
--     transitions. A mutable `verification_status` column would destroy the
--     previous verdict on every transition, so a rejected-then-reverified
--     assertion would read as plain "verified". That is the mistake migration
--     202 recorded under `superseded_profile` and that `Violation.removed`
--     avoids: **retain what you supersede.** So verifications are an
--     append-only log and the current state is DERIVED from the latest row,
--     never stored.
--
-- Net effect: everything here is append-only, which is more so than either a
-- mutable column on `episodes` or a status column on a side table.
--
-- ## The cost of flat, and the guard for it
--
-- Identity. Array position is not an identity — reorder the array and every
-- verification points at the wrong element — so each assertion carries a
-- generated `assertion_id` inside the JSON. No foreign key can enforce that a
-- verification points at an assertion that exists.
--
-- That is a dangling citation, and this codebase already has one: a semantic
-- rule citing three episodes with no rows behind them. Flat trades *enforced*
-- referential integrity for *checked*, so the check ships with the schema
-- rather than after it — `liveness_trust` declares it, and an unresolvable
-- verification is a finding rather than a skipped row.
--
-- ## NULL is not empty
--
-- `episodes.assertions` NULL means the episode predates this layer, and 3,352
-- of them do. `'[]'::jsonb` means the agent ran and quantified nothing. Those
-- must never collapse: counting NULL as "asserted nothing" would show agents
-- getting quieter as coverage improved, the same inversion the provenance floor
-- guards against.

-- One DO block: PgBouncer runs in transaction-pooling mode, where top-level
-- statements get separate implicit transactions with no rollback between them.
-- `credit_ledger_tx_type_check` was declared by seventeen migrations and applied
-- by none for exactly that reason.
DO $$
BEGIN
    -- ── the immutable half ──────────────────────────────────────────────
    ALTER TABLE public.episodes
        ADD COLUMN IF NOT EXISTS assertions JSONB;

    COMMENT ON COLUMN public.episodes.assertions IS
        'What the agent quantified during this run, as an array of '
        '{assertion_id, kind, value, provenance, basis, target_hint}. Written '
        'once with the episode and NEVER updated - it is what the agent said at '
        'that moment. NULL means the episode predates migration 205; '
        '''[]''::jsonb means the agent ran and quantified nothing. Do not '
        'collapse those: counting NULL as "asserted nothing" would show agents '
        'getting quieter as coverage improved. Verification state is NOT here; '
        'it lives in assertion_verifications because it transitions, and a '
        'mutable status column would destroy the previous verdict.';

    -- GIN so the observability read can ask "every assertion by this agent"
    -- without unnesting the whole table. Partial: 3,352 pre-205 episodes are
    -- permanently NULL and indexing them buys a selectivity the planner cannot
    -- use.
    CREATE INDEX IF NOT EXISTS idx_episodes_assertions
        ON public.episodes USING GIN (assertions)
        WHERE assertions IS NOT NULL;

    -- ── the append-only half ────────────────────────────────────────────
    CREATE TABLE IF NOT EXISTS public.assertion_verifications (
        verification_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        -- The assertion_id minted inside episodes.assertions. Not a foreign
        -- key: the target lives inside a JSONB array and Postgres cannot
        -- reference it. Checked instead, by liveness_trust.
        assertion_id     UUID NOT NULL,
        -- Carried so the check has somewhere to start without scanning every
        -- episode, and so a deleted episode is detectable rather than merely
        -- unfindable.
        episode_id       UUID NOT NULL REFERENCES public.episodes(episode_id)
                              ON DELETE CASCADE,
        verdict          TEXT NOT NULL,
        -- What the verdict was checked against. REQUIRED for human_sourced,
        -- see the CHECK below.
        source_citation  TEXT,
        -- Who or what decided. `actor_kind` distinguishes a tool call from a
        -- person, because "verified" with no actor is how a queue becomes a
        -- rubber stamp.
        actor            TEXT NOT NULL,
        actor_kind       TEXT NOT NULL,
        -- The tool response or checker output, retained so a verdict can be
        -- re-examined rather than merely trusted.
        evidence         JSONB,
        created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
    );

    COMMENT ON TABLE public.assertion_verifications IS
        'Append-only log of verification decisions about agent assertions. '
        'NEVER updated and never deleted: current state is the latest row per '
        'assertion_id, derived rather than stored, so a '
        'rejected-then-reverified assertion reads as exactly that instead of as '
        '"verified". Same reasoning as migration 202''s superseded_profile.';

    -- The load-bearing constraint. A one-click "verified" button with no
    -- citation is a laundering UI: it turns a guess into a fact at zero cost
    -- and puts a person's name on it. `human_sourced` scores as high as
    -- `tool_verified` in `grounding_trust::strength` precisely BECAUSE someone
    -- else can follow the citation to the same source, so the citation is what
    -- earns the score and must be enforced rather than encouraged.
    --
    -- `human_endorsed` is the honest uncited verdict and is deliberately
    -- available, at the strength of a model inference. Requiring a citation for
    -- every judgement would push reviewers to paste a plausible URL, which is
    -- worse than an admitted opinion.
    ALTER TABLE public.assertion_verifications
        DROP CONSTRAINT IF EXISTS assertion_verifications_citation_check;
    ALTER TABLE public.assertion_verifications
        ADD CONSTRAINT assertion_verifications_citation_check
        CHECK (verdict <> 'human_sourced'
               OR (source_citation IS NOT NULL AND length(trim(source_citation)) > 0));

    ALTER TABLE public.assertion_verifications
        DROP CONSTRAINT IF EXISTS assertion_verifications_actor_kind_check;
    ALTER TABLE public.assertion_verifications
        ADD CONSTRAINT assertion_verifications_actor_kind_check
        CHECK (actor_kind IN ('tool', 'human', 'platform'));

    CREATE INDEX IF NOT EXISTS idx_assertion_verifications_assertion
        ON public.assertion_verifications(assertion_id, created_at DESC);
    CREATE INDEX IF NOT EXISTS idx_assertion_verifications_episode
        ON public.assertion_verifications(episode_id);
    -- The queue view: what still needs a decision, newest first.
    CREATE INDEX IF NOT EXISTS idx_assertion_verifications_verdict
        ON public.assertion_verifications(verdict, created_at DESC);

    -- ── bind a claim to the assertion it came from ──────────────────────
    ALTER TABLE public.forecast_agent_claims
        ADD COLUMN IF NOT EXISTS assertion_id UUID;

    COMMENT ON COLUMN public.forecast_agent_claims.assertion_id IS
        'The assertion this claim binds to a driver, in episodes.assertions. A '
        'claim is an assertion plus a binding, so one assertion may produce '
        'several claims - which is how football_analyst''s single multiplier '
        'across three factor drivers becomes one recorded judgement with three '
        'bindings instead of three fabricated judgements. NULL for claims '
        'written before migration 205, and there are none: the table is empty.';

    CREATE INDEX IF NOT EXISTS idx_agent_claims_assertion
        ON public.forecast_agent_claims(assertion_id)
        WHERE assertion_id IS NOT NULL;

    -- ── widen the provenance vocabulary ─────────────────────────────────
    --
    -- Supersedes migration 203's CHECK, which predates the pending tier.
    -- Redefined here rather than edited in place because 203 may already have
    -- deployed by the time this lands, and a DROP+ADD inside a DO block is
    -- idempotent and atomic either way. Kept in step with
    -- `grounding_trust::PROVENANCE_VALUES` by a test that parses this file.
    ALTER TABLE public.semantic_rules
        DROP CONSTRAINT IF EXISTS semantic_rules_provenance_floor_check;
    ALTER TABLE public.semantic_rules
        ADD CONSTRAINT semantic_rules_provenance_floor_check
        CHECK (provenance_floor IS NULL OR provenance_floor IN (
            'tool_verified',
            'tool_no_match',
            'unavailable_no_tool_source',
            'model_inference',
            'platform_derived',
            'pending_tool_check',
            'pending_human_check',
            'human_sourced',
            'human_endorsed',
            'rejected'
        ));

    ALTER TABLE public.assertion_verifications
        DROP CONSTRAINT IF EXISTS assertion_verifications_verdict_check;
    ALTER TABLE public.assertion_verifications
        ADD CONSTRAINT assertion_verifications_verdict_check
        CHECK (verdict IN (
            'tool_verified',
            'tool_no_match',
            'unavailable_no_tool_source',
            'model_inference',
            'platform_derived',
            'pending_tool_check',
            'pending_human_check',
            'human_sourced',
            'human_endorsed',
            'rejected'
        ));
END $$;
