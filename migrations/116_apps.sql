-- Migration 116: App primitive
--
-- Adds the `apps` table — a registered platform artifact that ties together
-- a composition, a canonical document schema, a workspace template, and a
-- UI pointer. See docs/specs/01_APP_PRIMITIVE.md for the full design.
--
-- Workspaces continue to work unchanged. The link from a workspace to its
-- originating App is via teams.origin = apps.slug (already exists from
-- migration 112). No FK is enforced — workspaces with an origin that doesn't
-- match any App (e.g. 'bestiary_workspace', 'rabble_swarm') keep working.
--
-- PgBouncer-safe. Idempotent (IF NOT EXISTS everywhere).

CREATE TABLE IF NOT EXISTS public.apps (
    -- Identity
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug             TEXT NOT NULL UNIQUE
                         CHECK (slug ~ '^[a-z][a-z0-9_]{2,63}$'),
    name             TEXT NOT NULL,
    tagline          TEXT,

    -- Ownership
    owner_user_id    TEXT NOT NULL,
    owner_team_id    UUID REFERENCES public.teams(id) ON DELETE SET NULL,

    -- Surface
    homepage_url     TEXT,
    icon_url         TEXT,

    -- Composition reference (advisory — no FK; composition_patterns is not a
    -- DB table today. This is a free-text slug for documentation and Xaman Ek.)
    composition_slug TEXT,

    -- Canonical document schema for this app
    schema_slug      TEXT,
    schema_json      JSONB,

    -- Workspace provisioning template
    -- Shape: { initial_budget, auto_hire[], initial_files[], default_name_pattern, compositions[] }
    workspace_template JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- Economics (reserved, inert in v1)
    revenue_share    JSONB DEFAULT NULL,
    pricing_policy   TEXT NOT NULL DEFAULT 'platform_default'
                         CHECK (pricing_policy IN (
                             'platform_default', 'subscription', 'metered', 'free'
                         )),

    -- Lifecycle
    visibility       TEXT NOT NULL DEFAULT 'private'
                         CHECK (visibility IN ('private', 'unlisted', 'public')),
    published_at     TIMESTAMPTZ,
    archived_at      TIMESTAMPTZ,

    -- Bookkeeping
    description      TEXT,
    metadata         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_apps_owner_user   ON public.apps(owner_user_id);
CREATE INDEX IF NOT EXISTS idx_apps_visibility   ON public.apps(visibility)
    WHERE archived_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_apps_slug         ON public.apps(slug);

-- updated_at trigger
CREATE OR REPLACE FUNCTION public.touch_apps_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_apps_updated_at ON public.apps;
CREATE TRIGGER trg_apps_updated_at
    BEFORE UPDATE ON public.apps
    FOR EACH ROW EXECUTE FUNCTION public.touch_apps_updated_at();

-- Reserved origin tags that cannot be used as App slugs.
-- Enforced in the handler, not in the DB (easier to extend without migrations).
-- Reserved: bestiary_workspace, rabble_swarm, personal_workspace,
--           fermi_forecast, silat_workspace
