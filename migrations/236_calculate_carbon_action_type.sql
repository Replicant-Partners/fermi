-- Migration 236: Admit `calculate_carbon` to workspace_action_log.
--
-- mig-232 added the three DPP Studio actions that read stored rulesets.
-- mig-234 added `evaluate_claims`, the first of them to run an agent against a
-- live corpus and spend credits. This is the second, and it differs from
-- `evaluate_claims` in one way that makes the log row matter more rather than
-- less: the action does not only retrieve and judge, it CALCULATES. The handler
-- multiplies each bill-of-materials quantity by the emission factor the agent
-- retrieved, in Rust, and writes the product over whatever the model replied.
--
-- So the row is the audit anchor for a number somebody may put in a disclosure.
-- `apply_result` carries the coverage, the total, the unpriced lines and
-- `model_arithmetic_disagreements` — the count of lines where the model did the
-- multiplication anyway and got it wrong. That count is the only place the
-- platform's derivation does not hide the model's discipline, and it is
-- worthless if the row it lives on was never inserted.
--
-- Registered in `run_migrations` in the same commit as the handler, and the
-- reason is specific: `log_action` soft-fails so that a missing migration
-- cannot lose work that has already cost real searches. That softness means a
-- rejected CHECK produces a 200 response carrying a freshly-minted UUID that
-- exists in no table — and that UUID is embedded in the committed
-- dpp/carbon/statement.yaml. An audit anchor pointing at nothing is worse than
-- an absent one, because it looks discharged.
--
-- Note what is NOT widened here. `workspace_messages.message_type` has its own
-- CHECK (mig-077) and `calculate_carbon` is deliberately not added to it: the
-- handler dispatches as the already-admitted `agent_action`, so the agent's
-- reply reaches the workspace transcript instead of failing that constraint
-- inside a `let _ =`. Two tables, two vocabularies; widening one does not
-- widen the other, and `evaluate_claim` is silently losing its transcript
-- message today for exactly that reason.
--
-- MUST use a DO block — a bare DROP + ADD pair through PgBouncer commits the
-- DROP and loses the ADD, whose net effect is DELETING the constraint. See
-- tests/constraint_trust.rs::no_new_migration_declares_a_constraint_it_cannot_apply.

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
            'evaluate_claims',
            -- Adaptogen Lab DPP Studio, product carbon accounting (mig-236).
            -- Retrieval by the agent, arithmetic by the platform.
            'calculate_carbon'
        ));
END $$;
