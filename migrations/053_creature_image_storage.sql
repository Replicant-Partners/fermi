-- Store generated creature art in database for persistence across deploys.
-- Railway filesystem is ephemeral; images written to static/creatures/ are lost on redeploy.

CREATE TABLE IF NOT EXISTS creature_images (
    creature_id UUID PRIMARY KEY REFERENCES creatures(creature_id),
    image_bytes BYTEA NOT NULL,
    mime_type TEXT NOT NULL DEFAULT 'image/png',
    file_size INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_creature_images_updated ON creature_images(updated_at);

-- Migrate existing asset_paths from /static/creatures/<uuid>.ext to /api/creatures/<uuid>/image
UPDATE creatures
SET asset_path = '/api/creatures/' || creature_id || '/image'
WHERE asset_path LIKE '/static/creatures/%'
  AND asset_path NOT LIKE '%placeholder%';
