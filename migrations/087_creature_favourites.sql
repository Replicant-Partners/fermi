-- Migration 087: Creature favourites + feed performance indexes
-- IMPORTANT: No BEGIN/COMMIT — PgBouncer transaction mode.

-- Creature favourites (star/follow a creature)
CREATE TABLE IF NOT EXISTS creature_favourites (
    user_id TEXT NOT NULL,
    creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, creature_id)
);
CREATE INDEX IF NOT EXISTS idx_cf_user ON creature_favourites(user_id);

-- Feed query performance indexes on creature_versions
CREATE INDEX IF NOT EXISTS idx_cv_valid_from_desc ON creature_versions(valid_from DESC);
CREATE INDEX IF NOT EXISTS idx_cv_h3_valid ON creature_versions(h3_cell, valid_from DESC)
    WHERE h3_cell IS NOT NULL;
