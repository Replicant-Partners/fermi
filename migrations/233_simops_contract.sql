-- ═══════════════════════════════════════════════════════════════════
-- Migration 233 — SimOps orchestra: simops_contract column + view
--
-- Adds `simops_contract JSONB` to `agents`, creating a typed capability
-- declaration for SimOps orchestra specialists. Follows the exact same
-- pattern as mig-105 (`fermi_contract`) and the capability/membership
-- split established by mig-180.
--
-- CAPABILITY  — "this agent can participate in a SimOps pipeline in
--               the declared role". A property of the agent. Stored on
--               `agents.simops_contract`, freely owner-editable.
--               Declaring a shape is not a privilege.
--
-- MEMBERSHIP  — "this agent is hired into a SimOps workspace". A
--               decision made at the workspace level via the kask_simops
--               app's `auto_hire` manifest. Not stored here.
--
-- `orchestra_simops_members` is informational: it shows which platform
-- agents have declared SimOps capability. It does NOT govern workspace
-- hiring. Contrast with `orchestra_fermi_members` which IS governed
-- (requires a grant in `orchestra_members`).
--
-- Idempotent, PgBouncer-safe DO blocks, RAISE NOTICE observability.
-- ═══════════════════════════════════════════════════════════════════

-- ── Column: agents.simops_contract ─────────────────────────────────
ALTER TABLE agents ADD COLUMN IF NOT EXISTS simops_contract JSONB;

CREATE INDEX IF NOT EXISTS idx_agents_simops_contract
    ON agents (agent_id)
    WHERE simops_contract IS NOT NULL;

-- ── View: orchestra_simops_members ──────────────────────────────────
--
-- All published agents that have declared a SimOps capability contract.
-- Used for admin visibility (Ecology lens) and by simops_companion via
-- list_workspace_agents (which adds the workspace filter on top).
-- Integration-test rows are hidden (agent_name pattern guard).
DROP VIEW IF EXISTS public.orchestra_simops_members CASCADE;
CREATE VIEW public.orchestra_simops_members AS
    SELECT a.agent_id,
           a.agent_name,
           a.agent_type,
           a.tier,
           a.description,
           a.tags,
           a.simops_contract,
           a.output_contract,
           a.user_id       AS owner_user_id,
           a.created_at,
           a.updated_at,
           -- Derived convenience columns from the contract JSON.
           a.simops_contract->>'role'           AS simops_role,
           a.simops_contract->>'extension_task' AS simops_extension_task,
           a.simops_contract->>'calibration_signal' AS calibration_signal
      FROM public.agents a
     WHERE a.simops_contract IS NOT NULL
       AND a.status = 'published'
       AND a.agent_name NOT LIKE 'test\_agent\_%'
     ORDER BY
       CASE a.simops_contract->>'role'
         WHEN 'cascade'   THEN 0
         WHEN 'predictor' THEN 1
         WHEN 'optimizer' THEN 2
         WHEN 'narrator'  THEN 3
         WHEN 'dynamics'  THEN 4
         WHEN 'advisor'   THEN 5
         WHEN 'extension' THEN 6
         ELSE 7
       END,
       a.agent_name;

COMMENT ON VIEW public.orchestra_simops_members IS
    'SimOps capability roster (mig-233). '
    'Membership = simops_contract IS NOT NULL AND status = published. '
    'Declaring a contract is a capability; workspace hiring is the admission decision. '
    'Integration-test rows are hidden.';

-- ── Post-migration verification ─────────────────────────────────────
DO $$
DECLARE
    v_col     boolean;
    v_view    boolean;
    v_members integer;
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = 'agents'
           AND column_name = 'simops_contract'
    ) INTO v_col;

    SELECT EXISTS (
        SELECT 1 FROM information_schema.views
         WHERE table_schema = 'public'
           AND table_name = 'orchestra_simops_members'
    ) INTO v_view;

    SELECT COUNT(*) INTO v_members FROM public.orchestra_simops_members;

    RAISE NOTICE '[mig 233] simops_contract column: %, orchestra_simops_members view: %, declared members: %',
        v_col, v_view, v_members;
END $$;
