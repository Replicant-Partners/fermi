-- ═══════════════════════════════════════════════════════════════════════
-- Migration 190 — agent_evolution: the high-water mark behind the badge
-- ═══════════════════════════════════════════════════════════════════════
--
-- The evolution badge (src/handlers/evolution.rs) is computed live from
-- outcomes, so it needs no storage — except for one thing: regression.
--
-- "Don't let your agents regress" is only an incentive if losing ground is
-- visible, and that requires remembering the best the agent ever reached.
-- Everything else on the badge is derived; this table exists solely so
-- `regressed` can mean something.
--
-- `peak_level` is a ratchet: it only ever increases. A drop in the live level
-- is exactly the signal we want to surface, so it must not quietly reset the
-- benchmark it is being measured against.
--
-- `current_level` and `last_computed_at` are cached for fleet-wide queries
-- (leaderboards, "who regressed this week") that should not have to recompute
-- every agent's badge from scratch.
-- ═══════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.agent_evolution (
    agent_id          UUID PRIMARY KEY
        REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- Best rank ever achieved. Ratchets up, never down.
    peak_level        SMALLINT NOT NULL DEFAULT 0,
    peak_at           TIMESTAMPTZ,

    -- Most recent computation, cached for fleet views.
    current_level     SMALLINT NOT NULL DEFAULT 0,
    last_computed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Set when current_level < peak_level, so "who slipped?" is one indexed
    -- query rather than a scan-and-compare.
    regressed_since   TIMESTAMPTZ,

    CONSTRAINT agent_evolution_levels_sane
        CHECK (peak_level BETWEEN 0 AND 5 AND current_level BETWEEN 0 AND 5),
    CONSTRAINT agent_evolution_peak_is_a_ratchet
        CHECK (peak_level >= current_level)
);

-- "Who is regressing right now" and leaderboard ordering.
CREATE INDEX IF NOT EXISTS idx_agent_evolution_regressed
    ON public.agent_evolution (regressed_since)
    WHERE regressed_since IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_agent_evolution_level
    ON public.agent_evolution (current_level DESC, peak_level DESC);

COMMENT ON TABLE public.agent_evolution IS
  'High-water mark for the agent evolution badge. The badge itself is computed live from outcomes; only the peak is stored, because regression cannot be detected without remembering the best rank previously reached. peak_level is a ratchet.';
