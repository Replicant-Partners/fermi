-- ═══════════════════════════════════════════════════════════════════
-- Migration 163 — rbac_orphans view: cross-table drift audit
--
-- Single query that surfaces every row in every resource table whose
-- owner reference doesn't resolve to a live `users.user_id`. If this
-- view returns zero rows, the RBAC invariant holds across ABW.
--
-- Powers:
--   * GET /api/admin/rbac/orphans   (returns this view, admin-only)
--   * Grafana panel  (SELECT COUNT(*) FROM rbac_orphans)
--   * CI + deploy health check       (fail deploy if count > 0 after
--     the substrate migration + repeated deploys)
--
-- Columns:
--   resource     TEXT   — the table name
--   row_id       TEXT   — the resource's primary key as text
--   owner_col    TEXT   — the column name that carries the drift
--   owner_ref    TEXT   — the drifted value (what's stored in the row)
--   label        TEXT   — human-readable identifier for the resource
--   created_at   TIMESTAMPTZ — when the resource was created (best-effort)
--
-- Extending: when a new tenant app adds a table with a user reference,
-- add one SELECT block below. That's the whole per-tenant tax.
--
-- Idempotent: CREATE OR REPLACE VIEW. Safe to re-run.
-- PgBouncer-safe: single DDL statement, no BEGIN/COMMIT.
-- ═══════════════════════════════════════════════════════════════════

CREATE OR REPLACE VIEW public.rbac_orphans AS
-- ── Fermi tenant
SELECT
    'agents'::text                     AS resource,
    agent_id::text                     AS row_id,
    'user_id'::text                    AS owner_col,
    user_id                            AS owner_ref,
    agent_name                         AS label,
    NULL::timestamptz                  AS created_at
FROM public.agents
WHERE user_id IS NOT NULL
  AND user_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'teams', id::text, 'owner_id', owner_id, name, created_at
FROM public.teams
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'apps', id::text, 'owner_user_id', owner_user_id, slug, created_at
FROM public.apps
WHERE owner_user_id IS NOT NULL
  AND owner_user_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

-- Fermi objects that already had FK enforcement (should be clean, but
-- included for completeness so a single view answers "any orphans anywhere?")
SELECT 'fermi_forecasts', id, 'owner_id', owner_id, question_text, created_at
FROM public.fermi_forecasts
WHERE owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'fermi_portfolios', id, 'owner_id', owner_id, title, created_at
FROM public.fermi_portfolios
WHERE owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'fermi_notebooks', id, 'owner_id', owner_id, title, created_at
FROM public.fermi_notebooks
WHERE owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

-- ── Rabble tenant
SELECT 'creatures', creature_id::text, 'owner_id', owner_id,
       COALESCE(specimen_name, scientific_name), created_at
FROM public.creatures
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'creature_collections', collection_id::text, 'owner_id', owner_id, name, created_at
FROM public.creature_collections
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'creature_flights', flight_id::text, 'owner_id', owner_id,
       COALESCE(location_name, h3_cell), started_at
FROM public.creature_flights
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'swarm_events', swarm_id::text, 'creator_id', creator_id,
       name, created_at
FROM public.swarm_events
WHERE creator_id IS NOT NULL
  AND creator_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

-- ── simOps / SOSA tenants
SELECT 'swarm_sessions', session_id::text, 'owner_id', owner_id,
       name, started_at
FROM public.swarm_sessions
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'sosa_platforms', platform_id::text, 'owner_id', owner_id,
       name, created_at
FROM public.sosa_platforms
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

SELECT 'observation_sessions', session_id::text, 'owner_id', owner_id,
       name, started_at
FROM public.observation_sessions
WHERE owner_id IS NOT NULL
  AND owner_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL)

UNION ALL

-- ── AR spatial tenant
SELECT 'ar_beacons', beacon_id::text, 'creator_id', creator_id,
       COALESCE(location_name, h3_cell), created_at
FROM public.ar_beacons
WHERE creator_id IS NOT NULL
  AND creator_id NOT IN (SELECT user_id FROM public.users WHERE user_id IS NOT NULL);

COMMENT ON VIEW public.rbac_orphans IS
    'Cross-table drift audit. Any row here means a resource points at '
    'a user_id not present in users.user_id. Zero rows = RBAC invariant '
    'holds. Add a SELECT block per new tenant table with an owner column. '
    'See migration 163 for the extension pattern.';
