-- ═══════════════════════════════════════════════════════════════════
-- Migration 186 — agents.taxonomy
--
-- Implements the open question left by SPEC_30 §6: taxonomy lived only in
-- the on-disk `agent_card.json`, so it existed for the 96 curated cards
-- and was *structurally unavailable* to every agent created through the
-- UI or the import endpoint.
--
-- That is the majority of third-party agents. All 13 efra agents, for
-- instance, have no card on disk and therefore could never be classified,
-- appear under `Incertae sedis` in the Ecology register forever, and were
-- invisible to any grouping the field guide offers. The taxonomy could
-- only ever describe the platform's own agents, which makes it an
-- in-house convenience rather than a property of the ecology.
--
-- Shape mirrors `valence` (mig-114) and `output_contract`: a nullable
-- JSONB column on `agents`, flat string->string:
--
--   { "kingdom":"Quantitativa", "phylum":"Instrumenta", "class":"Researchia",
--     "order":"Evidentiales", "family":"Investigatidae",
--     "genus":"Analyticus", "species":"macro_forecaster" }
--
-- JSONB rather than seven columns because the rank set is a modelling
-- decision that has already changed once (SPEC_30 reformed four of the
-- seven) and will change again. Seven columns would make the next reform a
-- migration; a JSONB blob makes it a code change.
--
-- NOT backfilled here. The backfill source is the on-disk cards, which
-- SQL cannot read; `seed_agents_to_database` writes card taxonomy through
-- on every boot, and agents created through the API get their derived
-- ranks from `fermi::taxonomy::derive` at creation time.
--
-- Idempotent. Safe to re-run on every boot.
-- ═══════════════════════════════════════════════════════════════════

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'agents'
           AND column_name = 'taxonomy'
    ) THEN
        ALTER TABLE public.agents ADD COLUMN taxonomy JSONB;
        RAISE NOTICE '[mig 186] added agents.taxonomy (nullable JSONB)';
    ELSE
        RAISE NOTICE '[mig 186] agents.taxonomy already exists — skipping';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 186] ADD COLUMN taxonomy failed: %', SQLERRM;
END $$;

-- Grouping the Ecology register by a rank is the hot path: one index per
-- editorial rank people actually browse by. Expression indexes on JSONB
-- keys, so no schema change is needed when the rank set evolves.
DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_agents_taxonomy_family
        ON public.agents ((taxonomy ->> 'family'));
    CREATE INDEX IF NOT EXISTS idx_agents_taxonomy_kingdom
        ON public.agents ((taxonomy ->> 'kingdom'));
    CREATE INDEX IF NOT EXISTS idx_agents_taxonomy_genus
        ON public.agents ((taxonomy ->> 'genus'));
    RAISE NOTICE '[mig 186] indexed taxonomy (family, kingdom, genus)';
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 186] taxonomy index creation failed: %', SQLERRM;
END $$;

COMMENT ON COLUMN public.agents.taxonomy IS
    'Seven-rank agent classification (SPEC_30). Flat string->string JSONB. '
    'phylum/class/order/species are DERIVED from the agent and written by '
    'fermi::taxonomy::derive; kingdom/family/genus are editorial, from '
    'agents/taxonomy_vocab.json. Null means undescribed — rendered as '
    '"Incertae sedis" rather than guessed at.';

-- ── Report ─────────────────────────────────────────────────────────
DO $$
DECLARE
    v_total    INTEGER;
    v_classed  INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_total FROM public.agents
     WHERE agent_name NOT LIKE 'test\_agent\_%';
    SELECT COUNT(*) INTO v_classed FROM public.agents
     WHERE taxonomy IS NOT NULL AND agent_name NOT LIKE 'test\_agent\_%';
    RAISE NOTICE '[mig 186] % of % agent(s) classified; the rest are populated at boot (curated) or on create (API-authored)',
        v_classed, v_total;
END $$;
