-- ─────────────────────────────────────────────────────────────────────
-- Spec 26 SQL validation fixture
-- ─────────────────────────────────────────────────────────────────────
--
-- A minimal schema that mirrors the REAL column types of the tables the
-- Spec 26 queries touch, so every query in
-- `src/handlers/collab.rs`, `fermi-auth/src/visibility.rs` and the
-- extended list projections in `src/handlers/forecasts.rs` can be
-- planned (and, for the behavioural block at the bottom, executed)
-- without a production database.
--
-- Why a fixture rather than the real migrations: the real chain is 176
-- files deep with cross-app dependencies (rabble, simops, SOSA) that
-- have nothing to do with collaboration. This carries only what the
-- Spec 26 queries reference, at the types production actually has:
--
--   * fermi_forecasts.id / owner_id      TEXT  (mig 094 + mig 165 realign)
--   * fermi_forecasts.predicted_probability  REAL  ← the f32/f64 trap
--   * teams.id / team_members.team_id    UUID
--   * object_shares.object_id / share_target  TEXT  (UUID-as-text for teams)
--
-- The TEXT-vs-UUID split across the join keys is the single most
-- error-prone thing in this area (every `::text` cast in the queries is
-- load-bearing), which is exactly why it's worth checking offline.
--
-- Run with: scripts/spec26_sql_check.sh

BEGIN;

CREATE TABLE users (
    user_id      TEXT PRIMARY KEY,
    display_name TEXT,
    name         TEXT,
    email        TEXT,
    avatar_url   TEXT
);

CREATE TABLE teams (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name       TEXT NOT NULL,
    slug       TEXT NOT NULL UNIQUE,
    owner_id   TEXT NOT NULL,
    origin     TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE team_members (
    team_id     UUID NOT NULL,
    member_type TEXT NOT NULL DEFAULT 'user',
    member_id   TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member',
    invited_by  TEXT,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, member_id)
);

CREATE TABLE object_shares (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_type  TEXT NOT NULL,
    object_id    TEXT NOT NULL,
    share_type   TEXT NOT NULL,
    share_target TEXT NOT NULL,
    permission   TEXT NOT NULL DEFAULT 'view',
    granted_by   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (object_type, object_id, share_type, share_target)
);

CREATE TABLE fermi_portfolios (
    id          TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    title       TEXT NOT NULL,
    description TEXT,
    owner_id    TEXT NOT NULL,
    visibility  TEXT NOT NULL DEFAULT 'private',
    team_id     UUID,
    domain      TEXT,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE fermi_forecasts (
    id                       TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    owner_id                 TEXT NOT NULL,
    question_text            TEXT NOT NULL,
    domain                   TEXT,
    resolution_criteria      TEXT,
    target_date              TIMESTAMPTZ,
    predicted_probability    REAL NOT NULL,
    confidence_interval_low  REAL,
    confidence_interval_high REAL,
    status                   TEXT NOT NULL DEFAULT 'draft',
    actual_outcome           BOOLEAN,
    brier_score              REAL,
    resolved_at              TIMESTAMPTZ,
    resolved_by              TEXT,
    resolution_notes         TEXT,
    visibility               TEXT NOT NULL DEFAULT 'private',
    team_id                  UUID,
    tags                     TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE fermi_forecast_updates (
    id                   TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    forecast_id          TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    previous_probability REAL NOT NULL,
    new_probability      REAL NOT NULL,
    reason               TEXT,
    agent_id             TEXT,
    evidence_added       JSONB,
    revision_trigger     TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE fermi_portfolio_forecasts (
    portfolio_id TEXT NOT NULL REFERENCES fermi_portfolios(id) ON DELETE CASCADE,
    forecast_id  TEXT NOT NULL REFERENCES fermi_forecasts(id) ON DELETE CASCADE,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (portfolio_id, forecast_id)
);

CREATE TABLE forecast_invites (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    target_type     TEXT NOT NULL,
    target_id       TEXT NOT NULL,
    permission      TEXT NOT NULL,
    invitee_user_id TEXT,
    invitee_email   TEXT,
    token           TEXT UNIQUE,
    inviter_id      TEXT NOT NULL,
    message         TEXT,
    status          TEXT NOT NULL DEFAULT 'pending',
    expires_at      TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '14 days',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at     TIMESTAMPTZ
);

COMMIT;

-- ─── The migration under test, verbatim ──────────────────────────────
\echo '=== migration 176 ==='
\i migrations/176_collab_attribution.sql

-- Re-running must be a no-op (the runner executes every file on every
-- boot, so non-idempotent migrations break restarts).
\echo '=== migration 176 (idempotency re-run) ==='
\i migrations/176_collab_attribution.sql
