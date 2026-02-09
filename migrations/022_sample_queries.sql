-- Add sample_queries column to agents table
ALTER TABLE agents ADD COLUMN IF NOT EXISTS sample_queries TEXT[] DEFAULT '{}';
