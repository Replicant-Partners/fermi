-- Migration 237: Admit `price_bom` to workspace_action_log.
--
-- The fourth DPP Studio action, and the second that runs an agent. BOM
-- pricing previously went through the workspace message path, which writes no
-- action-log row at all — so the run could not appear in the Activity panel
-- after a reload, and the credits it spent landed in the ledger with nothing
-- to attribute them to.
--
-- Same reasoning as mig-234 for `evaluate_claims`: a priced bill of materials
-- is a commercial figure someone may later rely on, so the action row is the
-- audit anchor tying a stored price to who asked for it and when.
--
-- NOTE the list below is the UNION, including `calculate_carbon` from mig-236.
-- Each of these migrations DROPs and re-ADDs the whole constraint, so the last
-- one to run defines it entirely. Restating only the actions this migration
-- cares about would silently un-admit the one added immediately before it, and
-- the failure would surface as an unrelated handler's action-log INSERT
-- breaking long after this file stopped being the obvious suspect. Anything
-- added after this must extend the list, not replace it.

DO $$ BEGIN
    ALTER TABLE workspace_action_log
        DROP CONSTRAINT IF EXISTS workspace_action_log_action_type_check;

    ALTER TABLE workspace_action_log
        ADD CONSTRAINT workspace_action_log_action_type_check
        CHECK (action_type IN (
            -- Original set (mig-125)
            'mutate_document',
            'fork_state',
            'compare',
            'invoke_member',
            'annotate_schema',
            'annotate',
            -- Added via handlers without a migration update
            'log_observation',
            'identify',
            -- Adaptogen Lab DPP Studio (mig-232)
            'render_lens',
            'compare_lenses',
            'flag_divergence',
            -- Agent-driven evaluation (mig-234)
            'evaluate_claims',
            -- Agent-driven carbon accounting (mig-236)
            'calculate_carbon',
            -- Agent-driven BOM pricing (mig-237)
            'price_bom'
        ));
END $$;
