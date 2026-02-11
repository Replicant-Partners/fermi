-- Workflow visualization: mermaid sequence diagram + companion metadata
ALTER TABLE teams ADD COLUMN IF NOT EXISTS workflow_mermaid TEXT;
ALTER TABLE teams ADD COLUMN IF NOT EXISTS workflow_meta JSONB;
