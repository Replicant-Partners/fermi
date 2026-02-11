-- Migration 038: Add prompt_template to agents
-- Structured prompt scaffold for agent @mention auto-fill
ALTER TABLE agents ADD COLUMN IF NOT EXISTS prompt_template TEXT;
