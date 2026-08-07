-- ═══════════════════════════════════════════════════════════════════
-- Audit: Fermi orchestra membership vs. approval record
--
-- Why this exists
-- ───────────────
-- `orchestra_fermi_members` (mig-172) derives membership from
-- `agents.status = 'published' AND agents.fermi_contract IS NOT NULL`.
-- The governance loop in `handlers::orchestras` writes that column on
-- approve, but it is not the *only* writer, so membership and approval
-- can drift apart. Until the view is tightened to require an approved
-- request (needs a backfill decision — see §3), this script is how you
-- find the drift.
--
-- Run:  psql "$DATABASE_URL" -f scripts/audit_orchestra_membership.sql
-- ═══════════════════════════════════════════════════════════════════


-- ── 1. Every current Fermi member, with its approval provenance ─────
--
-- provenance:
--   approved   — an admin approved it; there's a receipt.
--   unreviewed — satisfies the membership predicate with no approval on
--                record. Expected for the curated boot-seed specialists
--                (macro_forecaster, equity_analyst, …) and for admin
--                imports. NOT expected for third-party community agents.
--   stale      — the latest decision was reject/withdraw, yet it is
--                still a member. Always a bug; investigate.
\echo '── Fermi members and their approval provenance ──'
-- NB: the owner handle is `agents.user_id` (the Rust `Agent.owner_id`
-- field maps to that column; there is no `agents.owner_id`).
SELECT m.agent_name,
       a.tier,
       a.user_id AS owner_id,
       CASE
           WHEN r.status = 'approved'               THEN 'approved'
           WHEN r.status IN ('rejected','withdrawn') THEN 'stale'
           ELSE 'unreviewed'
       END                                   AS provenance,
       r.reviewed_by,
       r.reviewed_at,
       a.created_at                          AS agent_created_at
  FROM public.orchestra_fermi_members m
  JOIN public.agents a USING (agent_id)
  LEFT JOIN LATERAL (
       SELECT status, reviewed_by, reviewed_at
         FROM public.orchestra_membership_requests
        WHERE agent_id = m.agent_id
          AND orchestra_name = 'fermi'
          AND status IN ('approved','rejected','withdrawn')
        ORDER BY reviewed_at DESC NULLS LAST
        LIMIT 1
  ) r ON TRUE
 ORDER BY provenance, m.agent_name;


-- ── 2. The actual red flags ─────────────────────────────────────────
--
-- Community/third-party agents that are members without an approval.
-- Before the import fix, any authenticated user could produce one of
-- these by pasting a card carrying `capabilities.fermi_contract` and
-- then self-publishing. Each row here should be either approved
-- retroactively or have its contract cleared.
\echo ''
\echo '── UNAPPROVED non-system members (review these) ──'
SELECT m.agent_name, a.tier, a.user_id AS owner_id, a.created_at
  FROM public.orchestra_fermi_members m
  JOIN public.agents a USING (agent_id)
 WHERE a.tier <> 'system'
   AND NOT EXISTS (
       SELECT 1 FROM public.orchestra_membership_requests r
        WHERE r.agent_id = m.agent_id
          AND r.orchestra_name = 'fermi'
          AND r.status = 'approved'
   )
 ORDER BY a.created_at DESC;

-- Corroboration: an approval always writes an audit row. No row here
-- for an agent above confirms no approval transaction ever ran.
\echo ''
\echo '── Recorded orchestra_approve audit events ──'
SELECT * FROM public.admin_bypass_events
 WHERE action = 'orchestra_approve'
 ORDER BY created_at DESC
 LIMIT 50;


-- ── 3. Remediation ─────────────────────────────────────────────────
--
-- To revoke a self-minted membership (agent stays published, it just
-- leaves the orchestra and can request properly):
--
--   UPDATE public.agents SET fermi_contract = NULL, updated_at = NOW()
--    WHERE agent_name = '<agent_name>';
--
-- Do NOT bulk-insert synthetic 'approved' rows to make §2 empty — that
-- launders the bypass instead of reviewing it.
--
-- Tightening the view to make approval the membership predicate:
--
--   WHERE a.status = 'published'
--     AND a.fermi_contract IS NOT NULL
--     AND EXISTS (SELECT 1 FROM orchestra_membership_requests r
--                  WHERE r.agent_id = a.agent_id
--                    AND r.orchestra_name = 'fermi'
--                    AND r.status = 'approved')
--
-- This is the correct end state, but it silently empties the roster of
-- every curated boot-seed specialist (they get their contract from the
-- filesystem registry at startup, never through a request). Ship it
-- only together with a deliberate backfill for those specific agents.
