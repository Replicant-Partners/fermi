-- ═══════════════════════════════════════════════════════════════════
-- Migration 189 — impersonation (view-as-user) audit substrate
--
-- Admin support tooling: a platform admin mints a short-lived,
-- READ-ONLY session that resolves to another user's identity, so they
-- can reproduce what that user actually sees. Without this, admins are
-- structurally unable to debug user-visible behaviour: `can_admin()`
-- short-circuits RBAC (fermi-auth/src/rbac.rs) and visibility, so an
-- admin can never observe the 404 the user is reporting.
--
-- Two tables, mirroring the `admin_bypass_events` philosophy from
-- mig-164: the platform's privileged escape hatches leave a paper
-- trail, always, and the trail is queryable by the affected user.
--
--   * impersonation_sessions — one row per "view as" session. Carries
--     the mandatory `reason` (requiring a written justification
--     measurably changes behaviour), the mode, and the lifecycle
--     timestamps. `ended_at IS NULL AND expires_at > NOW()` = live.
--
--   * impersonation_events — one row per request served under an
--     impersonated principal. This is the "what did they actually
--     look at" record. Mutations are blocked in read_only mode, but
--     we log the ATTEMPT (`blocked = true`) because a blocked write
--     is exactly the signal a security review wants to see.
--
-- Deliberately NOT modelled here:
--   * Token storage. The session token is a short-TTL HS256 JWT
--     carrying `imp.sid` = session_id; revocation is by looking up
--     this table, so there is no secret at rest to leak.
--
-- PgBouncer-safe (single statement per object), idempotent.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.impersonation_sessions (
    session_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- The real human. NOT the effective identity.
    admin_user_id   TEXT NOT NULL REFERENCES public.users(user_id) ON DELETE CASCADE,
    -- The account being viewed.
    target_user_id  TEXT NOT NULL REFERENCES public.users(user_id) ON DELETE CASCADE,
    -- Free-text justification, required at mint time (min length is
    -- enforced in the handler so the error message can be helpful).
    reason          TEXT NOT NULL,
    -- 'read_only' today. The column exists so that adding an
    -- explicitly-consented write mode later is a value change, not a
    -- schema migration.
    mode            TEXT NOT NULL DEFAULT 'read_only',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    -- Set when the admin explicitly exits. NULL + past expires_at
    -- means it lapsed rather than being closed.
    ended_at        TIMESTAMPTZ,
    -- 'exited' | 'expired' | 'revoked'
    end_reason      TEXT,
    ip_address      TEXT,
    user_agent      TEXT,
    CONSTRAINT impersonation_mode_check
        CHECK (mode IN ('read_only', 'assist')),
    -- An admin may never impersonate themselves: it produces audit
    -- records that look like impersonation but aren't, and it is
    -- always a bug in the caller.
    CONSTRAINT impersonation_no_self
        CHECK (admin_user_id <> target_user_id)
);

CREATE INDEX IF NOT EXISTS idx_impersonation_sessions_admin
    ON public.impersonation_sessions (admin_user_id, created_at DESC);

-- Powers the user-facing "who has viewed my account" surface.
CREATE INDEX IF NOT EXISTS idx_impersonation_sessions_target
    ON public.impersonation_sessions (target_user_id, created_at DESC);

-- Live-session lookup on every impersonated request: keep it cheap.
CREATE INDEX IF NOT EXISTS idx_impersonation_sessions_live
    ON public.impersonation_sessions (session_id)
    WHERE ended_at IS NULL;

CREATE TABLE IF NOT EXISTS public.impersonation_events (
    event_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id   UUID NOT NULL REFERENCES public.impersonation_sessions(session_id) ON DELETE CASCADE,
    method       TEXT NOT NULL,
    path         TEXT NOT NULL,
    status       INTEGER,
    -- TRUE when the read-only guard refused the request. The most
    -- security-relevant rows in the table.
    blocked      BOOLEAN NOT NULL DEFAULT FALSE,
    -- Why it was blocked ('mutation_in_read_only' | 'denied_path').
    block_reason TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_impersonation_events_session
    ON public.impersonation_events (session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_impersonation_events_blocked
    ON public.impersonation_events (created_at DESC)
    WHERE blocked;

COMMENT ON TABLE public.impersonation_sessions IS
    'Admin "view as user" sessions. Read-only by default. Every session '
    'requires a written reason and is visible to the impersonated user. '
    'See docs/specs/SPEC_33_IMPERSONATION.md.';

COMMENT ON TABLE public.impersonation_events IS
    'Per-request trail for impersonated sessions, including blocked '
    'mutation attempts (blocked = true).';
