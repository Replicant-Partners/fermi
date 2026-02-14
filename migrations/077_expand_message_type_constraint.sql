-- Expand workspace_messages message_type CHECK to include agent action types.
-- Old constraint only allowed: chat, execution_result, coherence_update, system_event, agent_invocation
-- Agent dispatch uses action_type as message_type (fly, flock_tick, navigate, etc.)
-- MUST use DO block — PgBouncer drops second statement in multi-statement migrations.
DO $$ BEGIN
    ALTER TABLE workspace_messages DROP CONSTRAINT IF EXISTS workspace_messages_message_type_check;
    ALTER TABLE workspace_messages ADD CONSTRAINT workspace_messages_message_type_check
        CHECK (message_type IN (
            'chat', 'execution_result', 'coherence_update', 'system_event', 'agent_invocation',
            'fly', 'flight_plan', 'flock_tick', 'navigate', 'narrate', 'anchor', 'lifecycle',
            'agent_action'
        ));
END $$;
