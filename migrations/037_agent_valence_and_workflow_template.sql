-- Agent valence: accepts/produces free-form string tags
ALTER TABLE agents ADD COLUMN IF NOT EXISTS accepts TEXT[] DEFAULT '{}';
ALTER TABLE agents ADD COLUMN IF NOT EXISTS produces TEXT[] DEFAULT '{}';

-- Workflow template: static mermaid + meta for compound agents
ALTER TABLE agents ADD COLUMN IF NOT EXISTS workflow_template JSONB;
