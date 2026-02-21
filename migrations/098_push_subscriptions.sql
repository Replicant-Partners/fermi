-- Migration 098: Web Push notification subscriptions
--
-- Stores browser/device push subscription endpoints so the backend can
-- send notifications when the app is closed or in the background.
--
-- Uses the Web Push API (VAPID authentication, RFC 8030).
-- Each user can have multiple subscriptions (multiple devices/browsers).

CREATE TABLE IF NOT EXISTS push_subscriptions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         TEXT NOT NULL,
    endpoint        TEXT NOT NULL,               -- the push service URL
    p256dh_key      TEXT NOT NULL,               -- client public key (base64url)
    auth_key        TEXT NOT NULL,               -- client auth secret (base64url)
    user_agent      TEXT,                        -- browser/device info for debugging
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ,                 -- updated on successful push
    failed_count    INTEGER NOT NULL DEFAULT 0,  -- consecutive failures (for cleanup)
    active          BOOLEAN NOT NULL DEFAULT true,
    UNIQUE(user_id, endpoint)
);

-- Fast lookup: which subscriptions does this user have?
CREATE INDEX IF NOT EXISTS idx_push_subs_user
    ON push_subscriptions(user_id) WHERE active = true;

-- Cleanup: find stale/failed subscriptions
CREATE INDEX IF NOT EXISTS idx_push_subs_failed
    ON push_subscriptions(failed_count) WHERE failed_count > 5;

-- Dedup: prevent duplicate endpoints
CREATE INDEX IF NOT EXISTS idx_push_subs_endpoint
    ON push_subscriptions(endpoint);

-- VAPID keys stored as a singleton config row.
-- Generated once, shared across all push operations.
CREATE TABLE IF NOT EXISTS push_config (
    id              INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),  -- singleton
    vapid_public_key  TEXT NOT NULL,
    vapid_private_key TEXT NOT NULL,
    vapid_subject     TEXT NOT NULL DEFAULT 'mailto:hello@rabble.world',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
