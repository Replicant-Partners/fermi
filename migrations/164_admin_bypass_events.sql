-- ═══════════════════════════════════════════════════════════════════
-- Migration 164 — admin_bypass_events audit table
--
-- v0.10.5. Records every time a platform admin bypasses a workflow
-- gate on behalf of another user. The prototypical case is
-- `POST /api/agents/:id/publish?force=true` — admin publishes an
-- agent whose publish-readiness checks failed, on behalf of the
-- owner.
--
-- Separating this from RBAC (v0.10.4): RBAC gates "who can do this?".
-- Workflow gates ("is this thing ready?") are a different axis. Admin
-- bypass on ownership is always OK (that's what platform admin means).
-- Admin bypass on workflow quality gates needs a paper trail — hence
-- this table.
--
-- Schema:
--   * event_id       — UUID PK
--   * admin_user_id  — the admin who performed the bypass (FK to users)
--   * target_type    — resource type ('agent', 'team', 'forecast', …).
--                      Uses ObjectType::as_str() values from
--                      fermi_auth::types::ObjectType so callers can
--                      round-trip through the same enum.
--   * target_id      — resource primary key as text
--   * action         — the specific gate that was bypassed
--                      (e.g. 'force_publish', 'reassign_owner').
--   * details        — JSONB blob for gate-specific context (e.g. the
--                      failing checks, the reason the admin gave)
--   * created_at     — event timestamp
--
-- Indexes:
--   * (target_type, target_id) — "show me every bypass on this agent"
--   * (admin_user_id, created_at DESC) — "what has this admin been up to"
--
-- PgBouncer-safe, idempotent.
-- ═══════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.admin_bypass_events (
    event_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    admin_user_id  TEXT NOT NULL REFERENCES public.users(user_id) ON DELETE SET NULL,
    target_type    TEXT NOT NULL,
    target_id      TEXT NOT NULL,
    action         TEXT NOT NULL,
    details        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_admin_bypass_events_target
    ON public.admin_bypass_events (target_type, target_id);

CREATE INDEX IF NOT EXISTS idx_admin_bypass_events_admin
    ON public.admin_bypass_events (admin_user_id, created_at DESC);

COMMENT ON TABLE public.admin_bypass_events IS
    'Audit trail for platform-admin bypasses of workflow gates '
    '(force-publish, reassign, etc.). RBAC-level ownership bypass is '
    'implicit in the platform-admin role and NOT logged here; only '
    'quality-gate overrides land in this table. See v0.10.5.';
