-- Migration: Add teams and object sharing
-- Date: 2026-02-08
-- Description: Teams, team membership (users + agents), and polymorphic object sharing

BEGIN;

-- ============================================================================
-- TEAMS
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.teams (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    slug        TEXT NOT NULL UNIQUE,
    description TEXT,
    owner_id    TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_teams_owner ON public.teams(owner_id);
CREATE INDEX idx_teams_slug  ON public.teams(slug);

CREATE TRIGGER update_teams_updated_at BEFORE UPDATE ON public.teams
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

COMMENT ON TABLE public.teams IS 'Teams for collaborative sharing of objects';
COMMENT ON COLUMN public.teams.owner_id IS 'Creator/owner - references users.user_id';
COMMENT ON COLUMN public.teams.slug IS 'URL-safe unique identifier for the team';

-- ============================================================================
-- TEAM MEMBERS (users and agents)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.team_members (
    team_id     UUID NOT NULL,
    member_type TEXT NOT NULL DEFAULT 'user'
                    CHECK (member_type IN ('user', 'agent')),
    member_id   TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'member'
                    CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    invited_by  TEXT,
    joined_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (team_id, member_id)
);

CREATE INDEX idx_team_members_member ON public.team_members(member_id);
CREATE INDEX idx_team_members_team   ON public.team_members(team_id);
CREATE INDEX idx_team_members_type   ON public.team_members(member_type);

COMMENT ON TABLE public.team_members IS 'Team membership - users and agents can both be members';
COMMENT ON COLUMN public.team_members.member_type IS 'user or agent';
COMMENT ON COLUMN public.team_members.member_id IS 'users.user_id or agent_id depending on member_type';
COMMENT ON COLUMN public.team_members.role IS 'owner/admin/member/viewer within this team';

-- ============================================================================
-- OBJECT SHARES (polymorphic)
-- ============================================================================

CREATE TABLE IF NOT EXISTS public.object_shares (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    object_type  TEXT NOT NULL
                     CHECK (object_type IN (
                         'agent', 'capability', 'forecast',
                         'index', 'repo', 'file'
                     )),
    object_id    TEXT NOT NULL,
    share_type   TEXT NOT NULL
                     CHECK (share_type IN ('team', 'user')),
    share_target TEXT NOT NULL,
    permission   TEXT NOT NULL DEFAULT 'view'
                     CHECK (permission IN ('view', 'edit', 'admin')),
    granted_by   TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (object_type, object_id, share_type, share_target)
);

CREATE INDEX idx_object_shares_object ON public.object_shares(object_type, object_id);
CREATE INDEX idx_object_shares_target ON public.object_shares(share_type, share_target);

COMMENT ON TABLE public.object_shares IS 'Polymorphic sharing: any object to any team or user';
COMMENT ON COLUMN public.object_shares.object_id IS 'ID of the shared object (UUID as text or string ID)';
COMMENT ON COLUMN public.object_shares.share_target IS 'teams.id (as text) or users.user_id';

-- ============================================================================
-- AUTO-ADD TEAM CREATOR AS OWNER
-- ============================================================================

CREATE OR REPLACE FUNCTION auto_add_team_owner()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO public.team_members (team_id, member_type, member_id, role, invited_by)
    VALUES (NEW.id, 'user', NEW.owner_id, 'owner', NEW.owner_id);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_auto_add_team_owner
    AFTER INSERT ON public.teams
    FOR EACH ROW EXECUTE FUNCTION auto_add_team_owner();

COMMIT;
