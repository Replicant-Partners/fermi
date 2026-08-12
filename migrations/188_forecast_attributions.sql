-- ═══════════════════════════════════════════════════════════════════════
-- Migration 188 — forecast_attributions: persisted Shapley credit per agent
--
-- WHAT
-- ----
-- Three tables recording the output of the counterfactual attribution job
-- (src/handlers/attribution.rs, math in src/attribution/):
--
--   forecast_attributions          one header row per attributed forecast
--   forecast_agent_credit          one row per (forecast, agent) — the φ values
--   forecast_agent_interactions    one row per (forecast, agent pair) — synergy
--
-- WHY A HEADER TABLE
-- ------------------
-- A φ value is uninterpretable without its provenance, and two of those fields
-- are *validity gates* rather than metadata:
--
--   efficiency_residual  |Σφᵢ - (v(N)-v(∅))|. Exact Shapley makes this ~1e-12.
--                        A larger value means the value function was not
--                        deterministic between subset evaluations, so Monte
--                        Carlo noise has been silently redistributed as agent
--                        credit. This exact failure already happened once: the
--                        executor sampled drivers in HashMap order, so
--                        `with_seed` did not reproduce a run and a provably-
--                        worthless agent earned ~1e-4 of phantom credit
--                        (fixed by the BTreeMap change in src/executor.rs).
--                        The job refuses to write above a threshold; the column
--                        exists so a stored row can still be re-audited.
--
--   reconstruction_error |p_full - scored_probability|. The attribution is only
--                        about the REAL forecast if re-running the model with
--                        every agent's claim applied reproduces the probability
--                        that was actually scored. If it does not, we have the
--                        wrong claims, the wrong model snapshot, or the wrong
--                        params — and the φ values describe a forecast that
--                        never existed. This is the mechanism check for the
--                        attribution layer itself, and it is the reason the
--                        header carries scored_probability alongside p_full.
--
--   neutralisation       'identity' (credit vs silence) or 'reference' (credit
--                        vs an average replacement). These answer different
--                        questions and are NOT comparable, so the mode is part
--                        of the primary key semantics: a forecast may carry one
--                        attribution per mode.
--
--   seed                 Derived deterministically from the forecast id, so a
--                        recomputation months later reproduces the attribution
--                        byte-for-byte. Stored anyway, because a change in the
--                        derivation must not silently invalidate old rows.
--                        `stable_seed` masks to 63 bits precisely so this column
--                        can be a plain signed BIGINT: Postgres has no unsigned
--                        integer, and storing a full u64 as a wrapped negative
--                        would make the value unreadable and overflow any
--                        hand-written query against it.
--
-- IDEMPOTENCE
-- -----------
-- Keyed on (forecast_id, neutralisation) with ON CONFLICT DO UPDATE in the job,
-- so re-running attribution for a forecast overwrites rather than accumulating.
-- Attribution is a pure function of (model, claims, outcome, mode, seed); there
-- is no history worth keeping, unlike the claims themselves.
-- ═══════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.forecast_attributions (
    forecast_id           TEXT NOT NULL
        REFERENCES public.fermi_forecasts(id) ON DELETE CASCADE,

    -- 'identity' | 'reference' — see header. Part of the key.
    neutralisation        TEXT NOT NULL,

    -- Provenance of the computation.
    seed                  BIGINT NOT NULL,
    iterations            INTEGER NOT NULL,
    n_players             INTEGER NOT NULL,
    outcome               BOOLEAN NOT NULL,

    -- The counterfactual endpoints.
    p_baseline            REAL NOT NULL,   -- every agent neutralised: v(∅) anchor
    p_full                REAL NOT NULL,   -- every agent's claim applied
    scored_probability    REAL,            -- what resolution actually scored
    team_improvement      DOUBLE PRECISION NOT NULL, -- v(N) - v(∅)

    -- Validity gates. Both must be near zero for the row to mean anything.
    efficiency_residual   DOUBLE PRECISION NOT NULL,
    reconstruction_error  DOUBLE PRECISION,

    computed_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (forecast_id, neutralisation)
);

CREATE TABLE IF NOT EXISTS public.forecast_agent_credit (
    forecast_id     TEXT NOT NULL,
    neutralisation  TEXT NOT NULL,

    -- agent_id may be NULL if the claim ledger recorded a name that no longer
    -- resolves; agent_name is always present so credit survives a rename.
    agent_id        UUID REFERENCES public.agents(agent_id) ON DELETE SET NULL,
    agent_name      TEXT NOT NULL,

    -- The Shapley value. Positive = moved the forecast toward the outcome.
    -- Signed: an agent that dragged against the truth carries negative credit
    -- even when its team improved overall.
    shapley_value   DOUBLE PRECISION NOT NULL,

    PRIMARY KEY (forecast_id, neutralisation, agent_name),
    FOREIGN KEY (forecast_id, neutralisation)
        REFERENCES public.forecast_attributions(forecast_id, neutralisation)
        ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS public.forecast_agent_interactions (
    forecast_id       TEXT NOT NULL,
    neutralisation    TEXT NOT NULL,

    -- Stored with agent_a < agent_b so each unordered pair appears once.
    agent_a           TEXT NOT NULL,
    agent_b           TEXT NOT NULL,

    -- Shapley interaction index. > 0 synergy (keep both), < 0 redundancy
    -- (they substitute — consider dropping the cheaper). This is the Loop 4
    -- composition-evolution signal.
    interaction_index DOUBLE PRECISION NOT NULL,

    PRIMARY KEY (forecast_id, neutralisation, agent_a, agent_b),
    FOREIGN KEY (forecast_id, neutralisation)
        REFERENCES public.forecast_attributions(forecast_id, neutralisation)
        ON DELETE CASCADE,
    CONSTRAINT agent_pair_ordered CHECK (agent_a < agent_b)
);

-- Per-agent rollup: "what is this agent's mean credit across resolved
-- forecasts" — the Loop 5 read path that replaces the roster average.
CREATE INDEX IF NOT EXISTS idx_agent_credit_agent
    ON public.forecast_agent_credit (agent_id, neutralisation)
    WHERE agent_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_credit_name
    ON public.forecast_agent_credit (agent_name, neutralisation);

-- Pair rollup for Loop 4.
CREATE INDEX IF NOT EXISTS idx_agent_interactions_pair
    ON public.forecast_agent_interactions (agent_a, agent_b, neutralisation);

COMMENT ON TABLE public.forecast_attributions IS
  'Header for counterfactual Shapley attribution of a resolved forecast. efficiency_residual and reconstruction_error are validity gates, not metadata: a row with either far from zero describes a forecast that was not actually measured. See migration header and docs/architecture/COMBINATORIAL_CREDIT_ASSIGNMENT.md';
