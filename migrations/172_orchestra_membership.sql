-- ═══════════════════════════════════════════════════════════════════
-- Migration 172 — Orchestra registry: membership requests + views
--
-- v0.11.2. Makes orchestra membership a first-class substrate. Prior
-- to this release, an agent was "in Fermi" iff `agents.fermi_contract
-- IS NOT NULL` — a hidden column condition, opaque to Mario, with no
-- governance loop and no visible list.
--
-- This migration adds the substrate for a request/approve flow:
--
--   * `orchestra_membership_requests` — audit trail of every proposed
--     addition to any orchestra. Preserves rejections and the
--     rationale so we don't lose the governance decisions over time.
--
--   * View `orchestra_fermi_members` — the current Fermi roster.
--     Derived from `agents.fermi_contract IS NOT NULL AND status =
--     'published'`. Adding a member = setting that column; leaving =
--     clearing it. No writes here; membership is derived.
--
--   * View `orchestra_xaman_ek_members` — every published agent. The
--     top-level Bestiary ontology. Existence-in-catalogue IS the
--     registration.
--
--   * Column `fermi_forecasts.counterfactual_brier` — reserved for the
--     manager-effect metric (Team Brier − Counterfactual Brier).
--     Nullable; not populated by this release. Placeholder so future
--     features can compute the manager-skill delta without a schema
--     change.
--
-- Idempotent, PgBouncer-safe DO blocks, RAISE NOTICE observability.
-- Same pattern as mig-166 through mig-169.
-- ═══════════════════════════════════════════════════════════════════

-- ── Table: orchestra_membership_requests ───────────────────────────
--
-- Governance audit trail. Every request lands here whether approved,
-- rejected, or pending. Preserves the full context (who requested,
-- what contract was proposed, who reviewed, why) for six-months-later
-- readability.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public'
           AND table_name = 'orchestra_membership_requests'
    ) THEN
        CREATE TABLE public.orchestra_membership_requests (
            request_id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),

            -- Which orchestra the agent is requesting to join.
            -- Currently: 'fermi'. Extensible without schema change.
            orchestra_name       TEXT NOT NULL,

            -- The candidate agent.
            agent_id             UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,

            -- Who submitted the request. FK realigned to users(user_id)
            -- per mig-165 pattern — the TEXT identity, not the UUID PK.
            requested_by         TEXT NOT NULL REFERENCES public.users(user_id) ON DELETE CASCADE,

            -- Proposed contract shape. For Fermi:
            --   { finding_labels: [...], multiplier_range: [min,max],
            --     kg_fact_categories: [...] }
            -- Stored as JSONB so the shape can evolve per-orchestra
            -- without schema changes.
            proposed_contract    JSONB NOT NULL DEFAULT '{}'::jsonb,

            -- Optional free-form rationale from the requester.
            rationale            TEXT,

            -- Governance state.
            status               TEXT NOT NULL DEFAULT 'pending'
                                 CHECK (status IN ('pending', 'approved', 'rejected', 'withdrawn')),

            -- Reviewer (Fermi admin) — populated on approve/reject.
            reviewed_by          TEXT REFERENCES public.users(user_id) ON DELETE SET NULL,
            reviewed_at          TIMESTAMPTZ,

            -- Rejection note (required when status = 'rejected') or
            -- approval note (optional when status = 'approved').
            review_note          TEXT,

            created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        RAISE NOTICE '[mig 172] created orchestra_membership_requests table';
    ELSE
        RAISE NOTICE '[mig 172] orchestra_membership_requests already exists — skipping CREATE';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 172] CREATE TABLE orchestra_membership_requests failed: %', SQLERRM;
END $$;

-- Indexes: fast lookup by (orchestra, status) for admin inbox; by
-- agent for "which orchestras is this agent requesting to join".
DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_orchestra_requests_pending
        ON public.orchestra_membership_requests (orchestra_name, created_at DESC)
        WHERE status = 'pending';
    CREATE INDEX IF NOT EXISTS idx_orchestra_requests_agent
        ON public.orchestra_membership_requests (agent_id, orchestra_name, status);
    CREATE INDEX IF NOT EXISTS idx_orchestra_requests_requester
        ON public.orchestra_membership_requests (requested_by, created_at DESC);
    RAISE NOTICE '[mig 172] indexed orchestra_membership_requests (pending, agent, requester)';
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 172] index creation failed: %', SQLERRM;
END $$;

-- ── Placeholder: fermi_forecasts.counterfactual_brier ──────────────
--
-- Reserved for the manager-effect metric. Fermi's skill relative to
-- naive-average aggregation of member outputs = Team Brier −
-- Counterfactual Brier. Nullable. Populated by nothing in this
-- release — the substrate is ready for when the counterfactual
-- computation ships (v0.11.3+ candidate).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public'
           AND table_name = 'fermi_forecasts'
           AND column_name = 'counterfactual_brier'
    ) THEN
        ALTER TABLE public.fermi_forecasts
            ADD COLUMN counterfactual_brier REAL;
        RAISE NOTICE '[mig 172] added fermi_forecasts.counterfactual_brier (nullable placeholder)';
    ELSE
        RAISE NOTICE '[mig 172] fermi_forecasts.counterfactual_brier already exists';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 172] ADD COLUMN counterfactual_brier failed: %', SQLERRM;
END $$;

-- ── View: orchestra_fermi_members ─────────────────────────────────
--
-- The current Fermi roster. Membership rule = published + has a
-- fermi_contract declared. No membership table because the contract
-- IS the membership (single source of truth on the agent row).
--
-- Approval flow: admin sets agents.fermi_contract from the request →
-- agent appears here automatically.
DROP VIEW IF EXISTS public.orchestra_fermi_members CASCADE;
CREATE VIEW public.orchestra_fermi_members AS
    SELECT a.agent_id,
           a.agent_name,
           a.agent_type,
           a.tier,
           a.description,
           a.tags,
           a.fermi_contract,
           a.output_contract,
           a.user_id       AS owner_user_id,
           a.created_at,
           a.updated_at
      FROM public.agents a
     WHERE a.status = 'published'
       AND a.fermi_contract IS NOT NULL;

COMMENT ON VIEW public.orchestra_fermi_members IS
    'Fermi orchestra roster. Membership = published + fermi_contract declared. '
    'Set via approved orchestra_membership_requests (v0.11.2).';

-- ── View: orchestra_xaman_ek_members ──────────────────────────────
--
-- Top-level Bestiary ontology. Every published agent is discoverable
-- by xaman_ek — the platform navigator. Registration is implicit:
-- publishing IS joining. No request/approve flow.
DROP VIEW IF EXISTS public.orchestra_xaman_ek_members CASCADE;
CREATE VIEW public.orchestra_xaman_ek_members AS
    SELECT a.agent_id,
           a.agent_name,
           a.agent_type,
           a.tier,
           a.description,
           a.tags,
           a.output_contract,
           a.fermi_contract,   -- surfaced so xaman_ek can tell which agents are also in Fermi
           a.user_id       AS owner_user_id,
           a.created_at,
           a.updated_at
      FROM public.agents a
     WHERE a.status = 'published';

COMMENT ON VIEW public.orchestra_xaman_ek_members IS
    'xaman_ek ontology. Every published agent (v0.11.2). No opt-in — '
    'publishing IS joining. Includes fermi_contract so xaman_ek can '
    'surface sub-orchestra membership.';

-- ── Post-migration validation ─────────────────────────────────────
DO $$
DECLARE
    n_requests     INTEGER;
    fermi_count    INTEGER;
    xaman_ek_count INTEGER;
    has_counterfact BOOLEAN;
BEGIN
    SELECT COUNT(*) INTO n_requests FROM public.orchestra_membership_requests;
    SELECT COUNT(*) INTO fermi_count FROM public.orchestra_fermi_members;
    SELECT COUNT(*) INTO xaman_ek_count FROM public.orchestra_xaman_ek_members;
    SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema='public' AND table_name='fermi_forecasts'
           AND column_name='counterfactual_brier'
    ) INTO has_counterfact;

    RAISE NOTICE '[mig 172] post-migration — requests: %, fermi members: %, xaman_ek members: %, counterfactual_brier: %',
        n_requests, fermi_count, xaman_ek_count, has_counterfact;
END $$;
