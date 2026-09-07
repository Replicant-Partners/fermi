-- Migration 232: Extend workspace_action_log action_type_check to include
-- Adaptogen Lab DPP Studio action types and other actions added since mig-125.
--
-- The original constraint (mig-125) defined 6 action types. Handlers for
-- `log_observation` and `identify` were added later without a migration.
-- This migration formalises the full current set and adds the three DPP
-- Studio action types: render_lens, compare_lenses, flag_divergence.

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
            'flag_divergence'
        ));
END $$;
