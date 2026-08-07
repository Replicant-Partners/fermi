-- ═══════════════════════════════════════════════════════════════════
-- Delist the efra agents from the Fermi orchestra
--
-- WHY
-- ───
-- Nine `efra_*` agents held Fermi membership with no review behind it.
-- They were never approved — no membership request has ever been approved
-- on this platform at all. They became members under the pre-SPEC_29
-- predicate, where `agents.fermi_contract IS NOT NULL AND published` WAS
-- membership, and `POST /api/agents/import` copied that column straight
-- out of a user-supplied card with no admin gate.
--
-- mig-180 carried them forward honestly as `curated_seed` rather than
-- laundering them as `approved`, which is what made them visible. This
-- removes the grants so the owner can request membership through the
-- governance loop and a Fermi maintainer can actually review them.
--
-- WHAT THIS DOES NOT DO
-- ─────────────────────
-- It does not touch `agents.fermi_contract`. That column is the agent's
-- declared *capability* — which output shape it can emit — and SPEC_29
-- deliberately separated it from membership. Revoking a grant should not
-- destroy the agent's declared contract; the agents keep their shape and
-- can re-request immediately.
--
-- It does not touch the four `efra_*` drafts (efra_company, efra_gorilla,
-- efra_imagine, efra_thesis). They carry contracts but hold no grant, and
-- under SPEC_29 publishing no longer confers membership, so they will not
-- silently join later.
--
-- It does not touch `guidance_tracker`, which is the same owner with the
-- same unreviewed provenance but was out of scope for this instruction.
-- It remains an unreviewed member and will keep showing as flagged on
-- /ecology until someone decides.
--
-- Mirrors `revoke_orchestra_member_handler` exactly, including the
-- `orchestra_revoke` audit event, so the record is identical to what the
-- admin endpoint would have written.
--
-- Rollback: scripts/rollback_efra_delisting.sql
-- ═══════════════════════════════════════════════════════════════════

BEGIN;

-- Audit first, while the grants still exist to describe.
INSERT INTO public.admin_bypass_events
    (admin_user_id, target_type, target_id, action, details)
SELECT '2e644008-f5c7-47c5-854c-3801df9879cc',   -- fermi strategist owner
       'agent',
       a.agent_id::text,
       'orchestra_revoke',
       jsonb_build_object(
           'orchestra',        'fermi',
           'agent_name',       a.agent_name,
           'previous_source',  m.source,
           'reason',           'unreviewed membership acquired before SPEC_29; '
                            || 'owner to resubmit through the request/approve flow',
           'contract_retained', true
       )
  FROM public.orchestra_members m
  JOIN public.agents a USING (agent_id)
 WHERE m.orchestra_name = 'fermi'
   AND a.agent_name LIKE 'efra%'
   AND m.source <> 'approved';   -- never revoke a reviewed membership

DELETE FROM public.orchestra_members m
 USING public.agents a
 WHERE m.agent_id = a.agent_id
   AND m.orchestra_name = 'fermi'
   AND a.agent_name LIKE 'efra%'
   AND m.source <> 'approved';

-- ── Verify inside the transaction; abort if anything looks wrong ────
DO $$
DECLARE
    v_roster     INTEGER;
    v_efra       INTEGER;
    v_contracts  INTEGER;
    v_audit      INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_roster FROM public.orchestra_fermi_members;
    SELECT COUNT(*) INTO v_efra   FROM public.orchestra_fermi_members
     WHERE agent_name LIKE 'efra%';
    SELECT COUNT(*) INTO v_contracts FROM public.agents
     WHERE agent_name LIKE 'efra%' AND fermi_contract IS NOT NULL;
    SELECT COUNT(*) INTO v_audit FROM public.admin_bypass_events
     WHERE action = 'orchestra_revoke';

    RAISE NOTICE '[delist] fermi roster: % member(s); efra still in roster: %; '
                 'efra contracts retained: %; revoke audit events: %',
                 v_roster, v_efra, v_contracts, v_audit;

    IF v_efra <> 0 THEN
        RAISE EXCEPTION '[delist] ABORT: % efra agent(s) still hold membership', v_efra;
    END IF;
    IF v_contracts <> 13 THEN
        RAISE EXCEPTION '[delist] ABORT: expected 13 efra contracts intact, found % '
                        '-- capability must not be destroyed by revoking membership', v_contracts;
    END IF;
    IF v_roster <> 12 THEN
        RAISE EXCEPTION '[delist] ABORT: expected 12 remaining members (21 - 9), found %', v_roster;
    END IF;
END $$;

COMMIT;
