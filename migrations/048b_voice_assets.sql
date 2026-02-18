-- Voice Assets Table
-- Tracks generated audio from TTS synthesis

CREATE TABLE IF NOT EXISTS voice_assets (
    asset_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_type TEXT NOT NULL,     -- 'episode', 'message', 'creature', 'synopsis'
    object_id TEXT NOT NULL,        -- UUID of the related object
    provider TEXT NOT NULL,         -- 'cartesia', 'elevenlabs', etc.
    voice_id TEXT,                  -- Provider-specific voice identifier
    duration_ms INTEGER,            -- Audio duration in milliseconds
    character_count INTEGER NOT NULL, -- Text length for cost tracking
    storage_url TEXT NOT NULL,      -- URL to audio file (R2/S3)
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_voice_assets_object ON voice_assets(object_type, object_id);
CREATE INDEX IF NOT EXISTS idx_voice_assets_created ON voice_assets(created_at DESC);

-- Add audio_url columns to existing tables
ALTER TABLE episodes ADD COLUMN IF NOT EXISTS audio_url TEXT;
ALTER TABLE workspace_messages ADD COLUMN IF NOT EXISTS audio_url TEXT;
ALTER TABLE ontology_snapshots ADD COLUMN IF NOT EXISTS audio_url TEXT;

-- Comments
COMMENT ON TABLE voice_assets IS 'Generated audio assets from text-to-speech synthesis';
COMMENT ON COLUMN voice_assets.object_type IS 'Type of object that owns this audio (episode, message, etc.)';
COMMENT ON COLUMN voice_assets.storage_url IS 'URL to audio file storage (Cloudflare R2 or S3)';
