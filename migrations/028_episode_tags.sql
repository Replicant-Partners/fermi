-- Migration 028: Add tags array to episodes for auto-generated execution tags
ALTER TABLE public.episodes ADD COLUMN IF NOT EXISTS tags TEXT[] NOT NULL DEFAULT '{}';

-- Index for tag-based queries (GIN index for array containment)
CREATE INDEX IF NOT EXISTS idx_episodes_tags ON public.episodes USING GIN (tags);
