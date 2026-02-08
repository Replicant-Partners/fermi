-- Migration 013: Workspace budget fields on teams
-- Every team IS a workspace. No separate table.
-- Budget fields track shared credit pool for computational cost awareness.

BEGIN;

ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS workspace_budget INTEGER NOT NULL DEFAULT 0;
ALTER TABLE public.teams ADD COLUMN IF NOT EXISTS workspace_spent INTEGER NOT NULL DEFAULT 0;

COMMIT;
