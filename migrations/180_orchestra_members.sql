-- ═══════════════════════════════════════════════════════════════════
-- Migration 180 — Orchestra membership as governed state
--
-- Implements docs/specs/SPEC_29_ORCHESTRA_MEMBERSHIP_AS_GOVERNED_STATE.md
--
-- WHY
-- ───
-- mig-172 (v0.11.2) built the governance loop — request / approve /
-- reject, admin-gated — but kept the membership predicate it inherited:
--
--     orchestra_fermi_members := agents.status='published'
--                            AND agents.fermi_contract IS NOT NULL
--
-- Approval was therefore a *side effect that produces membership*, not
-- the state membership derives from. Two consequences:
--
--   1. Any other writer of `agents.fermi_contract` was indistinguishable
--      from an approval. `POST /api/agents/import` copied the column
--      straight out of a user-supplied card with no admin check, so any
--      authenticated user could mint themselves an "admin-approved"
--      Fermi member by pasting a contract and self-publishing.
--   2. `orchestra_membership_requests` — the entire governance record —
--      was write-only. No read path consulted it when deciding
--      membership, so an agent could read MEMBER while its only request
--      row said 'rejected'.
--
-- THE MODEL
-- ─────────
-- `fermi_contract` was doing two unrelated jobs. Split them:
--
--   * CAPABILITY  — "this agent can emit finding labels and multipliers
--     in Fermi's aggregation format". A property of the agent. Stays on
--     `agents.fermi_contract`, freely owner-editable; declaring a shape
--     is not a privilege.
--
--   * MEMBERSHIP  — "this agent is admitted to the Fermi orchestra". A
--     decision about the agent, made by someone else, at a point in
--     time. Moves here, with provenance.
--
-- After this migration `agents.fermi_contract` grants nothing, so the
-- writer set stops being a security surface.
--
-- Idempotent, PgBouncer-safe DO blocks, RAISE NOTICE observability.
-- Same pattern as mig-166 through mig-179.
-- ═══════════════════════════════════════════════════════════════════

-- ── Table: orchestra_members ───────────────────────────────────────
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = 'orchestra_members'
    ) THEN
        CREATE TABLE public.orchestra_members (
            orchestra_name TEXT NOT NULL,
            agent_id       UUID NOT NULL
                           REFERENCES public.agents(agent_id) ON DELETE CASCADE,

            -- How this membership came to exist. NEVER 'approved' unless
            -- an approval transaction actually ran. `curated_seed` is a
            -- real, auditable, NON-approval provenance for the platform's
            -- own boot-seeded specialists — deliberately not disguised as
            -- review output, because laundering them as 'approved' would
            -- destroy the audit trail this table exists to establish.
            source         TEXT NOT NULL
                           CHECK (source IN ('approved', 'curated_seed', 'admin_grant')),

            -- The request that authorised this, for source='approved'.
            request_id     UUID
                           REFERENCES public.orchestra_membership_requests(request_id)
                           ON DELETE SET NULL,

            granted_by     TEXT REFERENCES public.users(user_id) ON DELETE SET NULL,
            granted_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

            PRIMARY KEY (orchestra_name, agent_id)
        );

        -- Make approval-without-a-request IMPOSSIBLE TO INSERT, rather
        -- than merely absent by convention. The invariant is enforced by
        -- the database, not by reviewer diligence.
        ALTER TABLE public.orchestra_members
            ADD CONSTRAINT approved_has_request
            CHECK (source <> 'approved' OR request_id IS NOT NULL);

        RAISE NOTICE '[mig 180] created orchestra_members table';
    ELSE
        RAISE NOTICE '[mig 180] orchestra_members already exists — skipping CREATE';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 180] CREATE TABLE orchestra_members failed: %', SQLERRM;
END $$;

DO $$
BEGIN
    CREATE INDEX IF NOT EXISTS idx_orchestra_members_agent
        ON public.orchestra_members (agent_id);
    CREATE INDEX IF NOT EXISTS idx_orchestra_members_source
        ON public.orchestra_members (orchestra_name, source);
    RAISE NOTICE '[mig 180] indexed orchestra_members (agent, source)';
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 180] index creation failed: %', SQLERRM;
END $$;

COMMENT ON TABLE public.orchestra_members IS
    'Orchestra membership as stated governed state (SPEC_29). Membership is '
    'declared here, never inferred from a column''s presence. `source` records '
    'provenance honestly: approved (has request_id), curated_seed (platform '
    'boot seed), admin_grant (override, audited to admin_bypass_events).';

-- ── Backfill: classify every CURRENT member, don't blanket-approve ──
--
-- An agent with an approved request row is genuinely 'approved' and keeps
-- its receipt. Everything else is 'curated_seed' — which is accurate for
-- the platform's own specialists and is ALSO the bucket that exposes any
-- self-minted membership from the import bypass.
--
-- Run scripts/audit_orchestra_membership.sql BEFORE this migration. Any
-- third-party (non-system) agent landing in the curated_seed bucket
-- should be triaged by a Fermi maintainer first — approve it properly or
-- clear its contract. Do not let this backfill grandfather in a bypass.
DO $$
DECLARE
    n_backfilled INTEGER;
    n_suspect    INTEGER;
BEGIN
    INSERT INTO public.orchestra_members
        (orchestra_name, agent_id, source, request_id, granted_by, granted_at)
    SELECT 'fermi',
           a.agent_id,
           CASE WHEN r.request_id IS NOT NULL THEN 'approved' ELSE 'curated_seed' END,
           r.request_id,
           r.reviewed_by,
           COALESCE(r.reviewed_at, a.created_at)
      FROM public.agents a
      LEFT JOIN LATERAL (
           SELECT request_id, reviewed_by, reviewed_at
             FROM public.orchestra_membership_requests
            WHERE agent_id = a.agent_id
              AND orchestra_name = 'fermi'
              AND status = 'approved'
            ORDER BY reviewed_at DESC NULLS LAST
            LIMIT 1
      ) r ON TRUE
     WHERE a.fermi_contract IS NOT NULL
       AND a.status = 'published'
    ON CONFLICT (orchestra_name, agent_id) DO NOTHING;

    SELECT COUNT(*) INTO n_backfilled FROM public.orchestra_members
     WHERE orchestra_name = 'fermi';

    -- Non-system members with no approval receipt. Expected > 0 (the
    -- curated specialists). Any THIRD-PARTY agent in here is a
    -- self-minted membership and wants review.
    SELECT COUNT(*) INTO n_suspect
      FROM public.orchestra_members m
      JOIN public.agents a USING (agent_id)
     WHERE m.orchestra_name = 'fermi'
       AND m.source = 'curated_seed'
       AND a.tier <> 'system';

    RAISE NOTICE '[mig 180] backfilled % fermi member(s); % non-system without an approval receipt (review these)',
        n_backfilled, n_suspect;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 180] backfill failed: %', SQLERRM;
END $$;

-- ── Redefined view: membership is STATED, not inferred ─────────────
--
-- `status='published'` is retained as a visibility gate (an unpublished
-- member isn't on the public roster), matching prior behaviour. The
-- change is that `fermi_contract IS NOT NULL` no longer confers
-- membership — the join to orchestra_members does.
--
-- `source` and `granted_at` are exposed so every consumer (roster API,
-- agent Manage page, Fermi's own injected roster block) reads provenance
-- from one place instead of re-deriving it.
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
           a.updated_at,
           m.source        AS membership_source,
           m.granted_at    AS membership_granted_at,
           m.granted_by    AS membership_granted_by
      FROM public.agents a
      JOIN public.orchestra_members m
        ON m.agent_id = a.agent_id
       AND m.orchestra_name = 'fermi'
     WHERE a.status = 'published';

COMMENT ON VIEW public.orchestra_fermi_members IS
    'Fermi orchestra roster (SPEC_29). Membership = a row in '
    'orchestra_members, NOT the presence of agents.fermi_contract. '
    'Declaring a contract is a capability; being admitted is a decision.';

-- ── Post-migration validation ─────────────────────────────────────
--
-- The count must not change: this migration re-expresses existing
-- membership, it does not grant or revoke any. A mismatch means the
-- backfill missed rows — investigate before shipping.
DO $$
DECLARE
    v_members    INTEGER;
    v_contracts  INTEGER;
    v_approved   INTEGER;
    v_seeded     INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_members FROM public.orchestra_fermi_members;
    SELECT COUNT(*) INTO v_contracts FROM public.agents
     WHERE fermi_contract IS NOT NULL AND status = 'published';
    SELECT COUNT(*) INTO v_approved FROM public.orchestra_members
     WHERE orchestra_name = 'fermi' AND source = 'approved';
    SELECT COUNT(*) INTO v_seeded FROM public.orchestra_members
     WHERE orchestra_name = 'fermi' AND source = 'curated_seed';

    RAISE NOTICE '[mig 180] roster: % member(s) (% approved, % curated_seed); % published agent(s) carry a contract',
        v_members, v_approved, v_seeded, v_contracts;

    IF v_members <> v_contracts THEN
        RAISE WARNING '[mig 180] MEMBERSHIP COUNT CHANGED (% -> %). This migration should be behaviour-preserving; investigate the backfill.',
            v_contracts, v_members;
    ELSE
        RAISE NOTICE '[mig 180] membership preserved exactly (% agents)', v_members;
    END IF;
END $$;
