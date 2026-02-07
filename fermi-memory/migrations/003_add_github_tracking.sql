-- Migration: Add GitHub tracking to ontology_snapshots
-- Date: 2026-02-07
-- Description: Adds github_url and pushed_to_remote fields to track GitHub repository URLs

-- Add github_url column
ALTER TABLE ontology_snapshots
ADD COLUMN github_url TEXT;

-- Add pushed_to_remote column
ALTER TABLE ontology_snapshots
ADD COLUMN pushed_to_remote BOOLEAN NOT NULL DEFAULT false;

-- Create index on github_url for faster lookups
CREATE INDEX idx_ontology_snapshots_github_url ON ontology_snapshots(github_url) WHERE github_url IS NOT NULL;

-- Add comment explaining the new fields
COMMENT ON COLUMN ontology_snapshots.github_url IS 'GitHub repository URL (e.g., https://github.com/Replicant-Partners/fermi-agent-market-research)';
COMMENT ON COLUMN ontology_snapshots.pushed_to_remote IS 'Whether this snapshot was successfully pushed to GitHub';
