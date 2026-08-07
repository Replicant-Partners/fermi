-- ═══════════════════════════════════════════════════════════════════
-- Migration 185 — Hide integration-test cruft from orchestra rosters
--
-- Policy: HIDE, don't delete.
--
-- Integration tests have been inserting `test_agent_<uuid>` rows into the
-- shared database for a long time (v0.10.20's audit found 565). Deleting
-- them is destructive and unnecessary — they are harmless where they sit.
-- What matters is that they never appear in a human-facing list.
--
-- Rust-side enumeration is handled at source by `NOT_TEST_CRUFT` in
-- `agent-bestiary/memory/src/store.rs`, which every `list_agents*` call
-- passes through. But the orchestra rosters are SQL views read directly,
-- so they need the same predicate applied here.
--
-- `orchestra_xaman_ek_members` is the acute case: its rule is literally
-- "every published agent", so any published test row lands in the
-- platform's top-level ontology — and in the roster block injected into
-- xaman_ek's system prompt.
--
-- The admin cleanup tool (`/api/admin/agents/cleanup-test-cruft`)
-- deliberately does NOT go through these views; it uses raw SQL, so
-- hiding here does not make the rows unreachable for an operator who
-- later decides to purge them.
--
-- Note the escaped underscores: `_` is a single-character wildcard in
-- SQL LIKE, so an unescaped `test_agent_%` would also match e.g.
-- `testXagentY...`. Escaped, it matches the literal prefix only.
--
-- Idempotent (CREATE OR REPLACE VIEW). Safe to re-run on every boot.
-- ═══════════════════════════════════════════════════════════════════

-- ── orchestra_fermi_members ────────────────────────────────────────
-- Membership comes from mig-180's orchestra_members grant table. A test
-- row would need an explicit grant to appear here, which is unlikely —
-- but the predicate costs nothing and keeps the two rosters consistent.
CREATE OR REPLACE VIEW public.orchestra_fermi_members AS
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
     WHERE a.status = 'published'
       AND a.agent_name NOT LIKE 'test\_agent\_%';

COMMENT ON VIEW public.orchestra_fermi_members IS
    'Fermi orchestra roster (SPEC_29). Membership = a row in '
    'orchestra_members, NOT the presence of agents.fermi_contract. '
    'Declaring a contract is a capability; being admitted is a decision. '
    'Integration-test rows are hidden (mig-185).';

-- ── orchestra_xaman_ek_members ─────────────────────────────────────
-- "Every published agent" — so this is where test rows actually leak.
CREATE OR REPLACE VIEW public.orchestra_xaman_ek_members AS
    SELECT a.agent_id,
           a.agent_name,
           a.agent_type,
           a.tier,
           a.description,
           a.tags,
           a.output_contract,
           a.fermi_contract,
           a.user_id       AS owner_user_id,
           a.created_at,
           a.updated_at
      FROM public.agents a
     WHERE a.status = 'published'
       AND a.agent_name NOT LIKE 'test\_agent\_%';

COMMENT ON VIEW public.orchestra_xaman_ek_members IS
    'xaman_ek ontology. Every published agent (v0.11.2), excluding '
    'integration-test rows (mig-185). No opt-in — publishing IS joining.';

-- ── Post-migration validation ─────────────────────────────────────
DO $$
DECLARE
    v_hidden    INTEGER;
    v_xaman     INTEGER;
    v_fermi     INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_hidden FROM public.agents
     WHERE agent_name LIKE 'test\_agent\_%';
    SELECT COUNT(*) INTO v_xaman FROM public.orchestra_xaman_ek_members;
    SELECT COUNT(*) INTO v_fermi FROM public.orchestra_fermi_members;

    RAISE NOTICE '[mig 185] % test row(s) hidden (not deleted); rosters now xaman_ek=%, fermi=%',
        v_hidden, v_xaman, v_fermi;
END $$;
