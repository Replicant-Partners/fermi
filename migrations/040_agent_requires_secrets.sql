-- Migration 040: Agent secret requirements
-- Agents declare what credentials they need to function
ALTER TABLE agents ADD COLUMN IF NOT EXISTS requires_secrets JSONB DEFAULT '[]';
