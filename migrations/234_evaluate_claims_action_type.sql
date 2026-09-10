-- Migration 234: Admit `evaluate_claims` to workspace_action_log.
--
-- mig-232 added the three DPP Studio actions that read stored rulesets
-- (render_lens, compare_lenses, flag_divergence). All three are deterministic
-- cache reads: free, instant, no agent involved.
--
-- `evaluate_claims` is the other half and a different kind of action. It runs
-- `regulatory_lens_translator` against the live regulatory corpus via
-- web_search, spends credits, and writes the enforced result back to
-- regulatory-lens/ontology/evaluated/{market}/{claim_id}.yaml. It is the only
-- one of the four that can produce a verdict for a claim nobody pre-authored,
-- which is every claim a user writes.
--
-- Logged for the same reason the others are, and one more: an evaluation is a
-- regulatory judgement that a human may later endorse, so the action row is
-- the audit anchor tying a stored verdict to who asked for it and when.

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
            -- Adaptogen Lab DPP Studio, agent-driven evaluation (mig-234)
            'evaluate_claims'
        ));
END $$;
