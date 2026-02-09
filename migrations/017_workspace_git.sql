-- Migration 017: Add git repo tracking columns to teams (workspaces)
-- Every workspace gets a local git repo for version control of context, outputs, and ontology

ALTER TABLE public.teams
  ADD COLUMN IF NOT EXISTS git_repo_path TEXT,
  ADD COLUMN IF NOT EXISTS git_latest_commit TEXT,
  ADD COLUMN IF NOT EXISTS git_commit_count INTEGER NOT NULL DEFAULT 0;
