-- Migration 029: Fix workspace message type constraint + ensure profile columns
-- Adds 'agent_invocation' to allowed message types.
-- Ensures bio column exists on users table.

-- Drop the old check constraint and add an expanded one
ALTER TABLE workspace_messages DROP CONSTRAINT IF EXISTS workspace_messages_message_type_check;
ALTER TABLE workspace_messages ADD CONSTRAINT workspace_messages_message_type_check
    CHECK (message_type IN ('chat', 'execution_result', 'coherence_update', 'system_event', 'agent_invocation'));

-- Ensure bio column exists (may have been missed if migration 020 partially failed)
ALTER TABLE public.users ADD COLUMN IF NOT EXISTS bio TEXT;
