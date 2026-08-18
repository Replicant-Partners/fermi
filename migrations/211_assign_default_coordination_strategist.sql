-- ═══════════════════════════════════════════════════════════════════════
-- 211 — assign the default coordination strategist to existing workspaces
-- ═══════════════════════════════════════════════════════════════════════
--
-- THE COLUMN NOBODY WROTE
-- ----------------------
-- `teams.coordination_strategist_id` is read in 40 places: the
-- composition-dreaming handler, the Loop 4 accept path, and
-- `record_coordination_observation`'s authorisation gate. It was written by
-- none of them, and by nothing else either — no endpoint, no creation path, no
-- migration.
--
-- Measured before this migration: **249 workspaces, 1 with a strategist.** The
-- one exception looks manual.
--
-- The consequence is that Loop 3's coordination half and the whole of Loop 4
-- were unreachable *by construction rather than by defect*. Both look up the
-- workspace's strategist and find NULL. A correctly implemented, correctly
-- gated coordination tool refuses in 248 of 249 workspaces, and a composition
-- tension audit has nobody to attribute itself to. The mechanisms were built,
-- tested, and aimed at a column nobody populated.
--
-- `cohere_and_coordinate`'s own card opens: "You are Cohere & Coordinate — the
-- default coordination strategist for every workspace on the Agent Bestiary
-- platform." It held that role for 0.4% of them.
--
-- WHAT THIS DOES
-- --------------
-- Sets `coordination_strategist_id` to `cohere_and_coordinate` for every
-- non-archived workspace that has none, and stamps `strategist_assigned_at`.
--
-- The forward fix is in code — `fermi_auth::teams::assign_default_strategist`,
-- called from both workspace creation paths (`create_team` and the
-- forecast-repo path in `handlers/forecast_git.rs`, which bypasses it and
-- accounts for 149 of the 249). This is the one-off repair.
--
-- WHY ALL WORKSPACES, INCLUDING EMPTY ONES
-- ----------------------------------------
-- 160 of the 249 have no messages, and a handful are smoke-test artefacts.
-- Assigning to those is deliberate: the column does no work by existing. It
-- costs nothing on an unused workspace and means coordination is available the
-- moment one is used, rather than requiring someone to remember a step that has
-- never once been taken in the platform's history.
--
-- Archived workspaces are skipped — there is no point coordinating them.
--
-- Idempotent: the `IS NULL` guard means re-running is a no-op, and an explicit
-- assignment made later is never clobbered.
-- ═══════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    EXECUTE $upd$
        UPDATE teams t
           SET coordination_strategist_id = a.agent_id,
               strategist_assigned_at     = NOW()
          FROM agents a
         WHERE a.agent_name = 'cohere_and_coordinate'
           AND t.coordination_strategist_id IS NULL
           AND COALESCE(t.origin, '') NOT LIKE 'archived%'
    $upd$;
END $$;
