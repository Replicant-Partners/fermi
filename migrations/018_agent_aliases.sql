-- Migration 018: Agent display aliases + avatar columns
-- display_alias: human-friendly name shown in UI (separate from system agent_name)
-- avatar_url: cached avatar image URL for agents, workspaces, and users

ALTER TABLE public.agents ADD COLUMN IF NOT EXISTS display_alias TEXT;
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS avatar_url TEXT;
ALTER TABLE public.users ADD COLUMN IF NOT EXISTS avatar_url TEXT;
