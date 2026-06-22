-- ─────────────────────────────────────────────────────────────────────
-- 151 — forecast_invites: unified invite primitive
-- ─────────────────────────────────────────────────────────────────────
--
-- Spec 24 §3.1.1: one polymorphic invite row backs three collab flows
--   • share a forecast with a user/email
--   • share a portfolio with a user/email
--   • join a team (replaces the direct-add path's UX surface; the
--     legacy POST /api/teams/:id/members stays for tooling)
--
-- One table on purpose — the console's "Inbox" lists pending invites
-- across all three target types from a single GET /api/me/invites.
-- Per-target-type tables would have forced a UNION query for every
-- inbox render.
--
-- Decoupled from `object_shares` and `team_members`: an invite is the
-- pending request, those tables are the materialised grant. Accepting
-- an invite writes the row in the appropriate target table inside a
-- transaction and then sets status='accepted'. We never read the
-- invite when computing access — `fermi_auth::visibility::can_access`
-- knows nothing about this table.
--
-- ID conventions (verified against prod 2026-06-19):
--   • inviter_id, invitee_user_id, target_id — TEXT, mirroring the
--     `team_members.member_id` / `object_shares.share_target` /
--     `forecast_relationships.owner_id` convention used elsewhere on
--     the collab side. No FK to users — the column holds whatever
--     shape `principal.user_id()` yields (UUID-string today, but
--     Zitadel ids and ENS addresses are also valid shapes per
--     `fermi-auth/src/types.rs:62-76`).
--   • target_id for `target_type='forecast'` holds `fermi_forecasts.id`
--     (TEXT). For `'portfolio'` holds `fermi_portfolios.id` (TEXT).
--     For `'team'` holds `teams.id::text` (UUID-as-text).
--
-- Email-pending invites: invitee_user_id is NULL, invitee_email is set,
-- token is generated. When that email signs in for the first time the
-- OIDC/SIWE callback (Spec 24 §3.8.1) calls `claim_pending_for_email`
-- which UPDATEs `invitee_user_id` on every matching pending row. We do
-- NOT auto-accept — the user still chooses.

CREATE TABLE IF NOT EXISTS public.forecast_invites (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),

    -- What the invite grants access to.
    target_type         TEXT        NOT NULL
                                    CHECK (target_type IN
                                        ('forecast', 'portfolio', 'team')),
    target_id           TEXT        NOT NULL,

    -- Permission to grant on accept.
    --   • forecast/portfolio invites: 'view' | 'edit' | 'admin'
    --     (matches `object_shares.permission`)
    --   • team invites: 'owner' | 'admin' | 'member' | 'viewer'
    --     (matches `team_members.role`)
    -- The CHECK is loose because a single column has to hold both
    -- vocabularies. The accept handler enforces "right value for this
    -- target_type" — bad rows simply fail to materialise.
    permission          TEXT        NOT NULL
                                    CHECK (permission IN
                                        ('view', 'edit', 'admin',
                                         'owner', 'member', 'viewer')),

    -- Recipient. EXACTLY ONE of (invitee_user_id, invitee_email) must
    -- be non-null. invitee_user_id may be NULL initially (email-only
    -- invite) and populated later by claim_pending_for_email.
    invitee_user_id     TEXT,
    invitee_email       TEXT,

    -- For email invites and shareable links we generate a token.
    -- NULL for direct user-id invites that don't need a link (the
    -- recipient finds the invite in their Inbox by invitee_user_id).
    -- UNIQUE so /api/invites/by-token/:token can resolve in O(1).
    token               TEXT        UNIQUE,

    inviter_id          TEXT        NOT NULL,
    message             TEXT,

    status              TEXT        NOT NULL DEFAULT 'pending'
                                    CHECK (status IN
                                        ('pending', 'accepted', 'declined',
                                         'revoked', 'expired')),

    expires_at          TIMESTAMPTZ NOT NULL
                                    DEFAULT NOW() + INTERVAL '14 days',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    accepted_at         TIMESTAMPTZ,

    -- Exactly-one-of-two invariant. We use IS DISTINCT FROM rather than
    -- != so NULLs compare correctly.
    CONSTRAINT forecast_invites_recipient_exactly_one
        CHECK ((invitee_user_id IS NOT NULL AND invitee_email IS NULL)
            OR (invitee_user_id IS NULL     AND invitee_email IS NOT NULL))
);

-- ─── Indexes ─────────────────────────────────────────────────────────
--
-- Index access patterns (matched to spec §3.3 endpoint workload):
--
--   • GET /api/me/invites          → by invitee_user_id (partial, only
--                                    pending). Also by lowercased email
--                                    so the email-claim resolver can
--                                    UPDATE in one shot.
--   • GET /api/forecasts/:id/      → by (target_type, target_id)
--           invites                  partial on pending.
--   • by-token routes              → UNIQUE token already does it.

CREATE INDEX IF NOT EXISTS idx_invites_recipient_user
    ON public.forecast_invites(invitee_user_id)
    WHERE invitee_user_id IS NOT NULL AND status = 'pending';

CREATE INDEX IF NOT EXISTS idx_invites_recipient_email
    ON public.forecast_invites(LOWER(invitee_email))
    WHERE invitee_email IS NOT NULL AND status = 'pending';

CREATE INDEX IF NOT EXISTS idx_invites_target
    ON public.forecast_invites(target_type, target_id)
    WHERE status = 'pending';

-- ─── Comments ────────────────────────────────────────────────────────

COMMENT ON TABLE public.forecast_invites IS
    'Unified invite primitive for Spec 24 collaboration. One row per pending request; status transitions are terminal (pending → {accepted, declined, revoked, expired}). Materialised grants live in object_shares or team_members depending on target_type. The fermi_auth::visibility::can_access chain never consults this table.';

COMMENT ON COLUMN public.forecast_invites.target_id IS
    'TEXT-typed identifier of the target row. For target_type=''forecast'' or ''portfolio'' this is the row''s `id` (already TEXT in those schemas). For target_type=''team'' this is teams.id cast to text (UUID-as-text).';

COMMENT ON COLUMN public.forecast_invites.permission IS
    'The role to grant on accept. ''view''/''edit''/''admin'' for forecast/portfolio targets (matches object_shares.permission). ''owner''/''admin''/''member''/''viewer'' for team targets (matches team_members.role). The accept handler enforces type-permission consistency.';

COMMENT ON COLUMN public.forecast_invites.invitee_email IS
    'Lowercased for matching in idx_invites_recipient_email. The email-claim resolver in the OIDC/SIWE sign-in callback (Spec 24 §3.8.1) sets invitee_user_id when a matching account is created so the new user sees the invite in their Inbox.';
