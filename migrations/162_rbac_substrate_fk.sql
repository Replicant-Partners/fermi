-- ═══════════════════════════════════════════════════════════════════
-- Migration 162 — RBAC substrate: FK NOT VALID + orphan heal
--
-- v0.10.4 substrate fix. Establishes the platform-wide invariant that
-- every resource table storing a user reference is FK-enforced against
-- `users(user_id)`. Uses NOT VALID so existing pre-v0.10.3 drift
-- doesn't block the deploy — only *new* writes are enforced.
--
-- Rationale: pre-v0.10.3, session `principal.user_id()` could drift
-- (Some("") skipping the unwrap_or_else, or `None → id::text` fallback
-- that didn't match users.user_id). Tables that stored owner
-- references without FK silently accumulated orphans. Rabble +
-- simOps "worked" because their read paths are mostly public — no
-- ownership gate — but the drift is still there and any future
-- owner-gated feature would 404 for legacy owners.
--
-- This migration adds FK NOT VALID on every resource-owner column
-- across ABW so:
--   * New writes must resolve to a real user_id (no more silent drift).
--   * Existing orphans are surfaced by `rbac_orphans` view (mig 163)
--     without blocking deploy.
--   * A future `VALIDATE CONSTRAINT` migration can promote to enforced
--     once orphans are cleared via the admin reassign endpoint.
--
-- Also heals two recoverable drift classes on the affected tables:
--   1. owner_col = '' (empty string) → NULL — system-orphan for admin
--      reassignment.
--   2. owner_col = users.id::text → users.user_id — the mig 161 healed
--      the users side; this heal completes the substrate side for any
--      resources that captured the drifted id::text value before the
--      user-side heal ran (belt-and-suspenders; usually 0 rows).
--
-- Idempotent + PgBouncer-safe: every write is wrapped in DO … END $$.
-- ═══════════════════════════════════════════════════════════════════

-- ── Helper macro (via DO) that adds a NOT VALID FK if missing.
--
-- Wrapped in an EXCEPTION handler so a single failure (type mismatch,
-- table doesn't exist on this deploy, etc.) doesn't abort the whole
-- migration. We WANT to see each failure in the logs — but a partial
-- rollout across environments shouldn't block the healthy ones.

-- ── agents.user_id — Fermi + tenant apps
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'agents_user_id_fk') THEN
        BEGIN
            ALTER TABLE public.agents
                ADD CONSTRAINT agents_user_id_fk
                FOREIGN KEY (user_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added agents_user_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] agents_user_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── teams.owner_id — workspace / team primary owner
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'teams_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.teams
                ADD CONSTRAINT teams_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE RESTRICT
                NOT VALID;
            RAISE NOTICE '[mig 162] added teams_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] teams_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── apps.owner_user_id — App directory (rabble, silat, fermi_console, …)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'apps_owner_user_id_fk') THEN
        BEGIN
            ALTER TABLE public.apps
                ADD CONSTRAINT apps_owner_user_id_fk
                FOREIGN KEY (owner_user_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added apps_owner_user_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] apps_owner_user_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Rabble tenant: creatures, collections, flights, tethers, devices, goals
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creatures_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creatures
                ADD CONSTRAINT creatures_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creatures_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creatures_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creature_collections_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creature_collections
                ADD CONSTRAINT creature_collections_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creature_collections_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creature_collections_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creature_flights_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creature_flights
                ADD CONSTRAINT creature_flights_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creature_flights_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creature_flights_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creature_tethers_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creature_tethers
                ADD CONSTRAINT creature_tethers_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creature_tethers_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creature_tethers_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creature_devices_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creature_devices
                ADD CONSTRAINT creature_devices_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creature_devices_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creature_devices_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'creature_goals_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.creature_goals
                ADD CONSTRAINT creature_goals_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added creature_goals_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] creature_goals_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Swarm / rabble events
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'swarm_events_creator_id_fk') THEN
        BEGIN
            ALTER TABLE public.swarm_events
                ADD CONSTRAINT swarm_events_creator_id_fk
                FOREIGN KEY (creator_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added swarm_events_creator_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] swarm_events_creator_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'swarm_sessions_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.swarm_sessions
                ADD CONSTRAINT swarm_sessions_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added swarm_sessions_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] swarm_sessions_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'swarm_sub_flocks_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.swarm_sub_flocks
                ADD CONSTRAINT swarm_sub_flocks_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added swarm_sub_flocks_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] swarm_sub_flocks_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── SOSA / observation platforms (simOps + telemetry tenants)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'sosa_platforms_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.sosa_platforms
                ADD CONSTRAINT sosa_platforms_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added sosa_platforms_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] sosa_platforms_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'observation_sessions_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.observation_sessions
                ADD CONSTRAINT observation_sessions_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added observation_sessions_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] observation_sessions_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'forage_observations_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.forage_observations
                ADD CONSTRAINT forage_observations_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added forage_observations_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] forage_observations_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── AR beacons + grid maps (spatial tenant)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ar_beacons_creator_id_fk') THEN
        BEGIN
            ALTER TABLE public.ar_beacons
                ADD CONSTRAINT ar_beacons_creator_id_fk
                FOREIGN KEY (creator_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added ar_beacons_creator_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] ar_beacons_creator_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'ar_grid_maps_creator_id_fk') THEN
        BEGIN
            ALTER TABLE public.ar_grid_maps
                ADD CONSTRAINT ar_grid_maps_creator_id_fk
                FOREIGN KEY (creator_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added ar_grid_maps_creator_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] ar_grid_maps_creator_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Marketplace + rabble co-presence
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'shopping_profiles_user_id_fk') THEN
        BEGIN
            ALTER TABLE public.shopping_profiles
                ADD CONSTRAINT shopping_profiles_user_id_fk
                FOREIGN KEY (user_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added shopping_profiles_user_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] shopping_profiles_user_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'rabble_co_presence_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.rabble_co_presence
                ADD CONSTRAINT rabble_co_presence_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added rabble_co_presence_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] rabble_co_presence_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Forecast relationships (Fermi)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'forecast_relationships_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.forecast_relationships
                ADD CONSTRAINT forecast_relationships_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added forecast_relationships_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] forecast_relationships_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'forecast_relationship_groups_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.forecast_relationship_groups
                ADD CONSTRAINT forecast_relationship_groups_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added forecast_relationship_groups_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] forecast_relationship_groups_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'pending_cascades_owner_id_fk') THEN
        BEGIN
            ALTER TABLE public.pending_cascades
                ADD CONSTRAINT pending_cascades_owner_id_fk
                FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added pending_cascades_owner_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] pending_cascades_owner_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Fermi market observations (already FK'd for observer_id, but
--    kept here for shape parity — no-op via IF NOT EXISTS).
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'fermi_market_observations_observer_id_fk') THEN
        BEGIN
            ALTER TABLE public.fermi_market_observations
                ADD CONSTRAINT fermi_market_observations_observer_id_fk
                FOREIGN KEY (observer_id) REFERENCES public.users(user_id)
                ON DELETE SET NULL
                NOT VALID;
            RAISE NOTICE '[mig 162] added fermi_market_observations_observer_id_fk NOT VALID';
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] fermi_market_observations_observer_id_fk skipped: %', SQLERRM;
        END;
    END IF;
END $$;

-- ── Heal empty-string drift across all resource owner columns.
--
-- Empty string was the "Some("") skips unwrap_or_else" pre-v0.10.3
-- bug's fingerprint. We can't recover the intended owner, so we NULL
-- them out — the admin can then see them in `rbac_orphans` view and
-- reassign via POST /api/admin/rbac/reassign.
DO $$
DECLARE
    tbls TEXT[] := ARRAY[
        'agents:user_id',
        'teams:owner_id',
        'apps:owner_user_id',
        'creatures:owner_id',
        'creature_collections:owner_id',
        'creature_flights:owner_id',
        'creature_tethers:owner_id',
        'creature_devices:owner_id',
        'creature_goals:owner_id',
        'swarm_events:creator_id',
        'swarm_sessions:owner_id',
        'swarm_sub_flocks:owner_id',
        'sosa_platforms:owner_id',
        'observation_sessions:owner_id',
        'forage_observations:owner_id',
        'ar_beacons:creator_id',
        'ar_grid_maps:creator_id',
        'shopping_profiles:user_id',
        'rabble_co_presence:owner_id',
        'forecast_relationships:owner_id',
        'forecast_relationship_groups:owner_id',
        'pending_cascades:owner_id'
    ];
    entry TEXT;
    parts TEXT[];
    tbl TEXT;
    col TEXT;
    healed INTEGER;
BEGIN
    FOREACH entry IN ARRAY tbls LOOP
        parts := string_to_array(entry, ':');
        tbl := parts[1];
        col := parts[2];
        BEGIN
            EXECUTE format(
                'UPDATE public.%I SET %I = NULL WHERE %I = ''''',
                tbl, col, col
            );
            GET DIAGNOSTICS healed = ROW_COUNT;
            IF healed > 0 THEN
                RAISE NOTICE '[mig 162] healed % empty-string %.% rows -> NULL',
                    healed, tbl, col;
            END IF;
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] empty-string heal on %.% skipped: %',
                tbl, col, SQLERRM;
        END;
    END LOOP;
END $$;

-- ── Heal id::text drift across the same set. Rare after mig 161
--    already normalised users.user_id, but belt-and-suspenders for
--    rows whose owner_col was captured from a session that resolved
--    via the None → id::text fallback path AFTER mig 161 ran (i.e.
--    another user_id NULL user showing up post-heal).
--
-- Rewrites owner_col to users.user_id where owner_col = users.id::text
-- AND owner_col is not already in users.user_id (i.e. is an orphan).
DO $$
DECLARE
    tbls TEXT[] := ARRAY[
        'agents:user_id',
        'teams:owner_id',
        'apps:owner_user_id',
        'creatures:owner_id',
        'creature_collections:owner_id',
        'creature_flights:owner_id',
        'creature_tethers:owner_id',
        'creature_devices:owner_id',
        'creature_goals:owner_id',
        'swarm_events:creator_id',
        'swarm_sessions:owner_id',
        'swarm_sub_flocks:owner_id',
        'sosa_platforms:owner_id',
        'observation_sessions:owner_id',
        'forage_observations:owner_id',
        'ar_beacons:creator_id',
        'ar_grid_maps:creator_id',
        'shopping_profiles:user_id',
        'rabble_co_presence:owner_id',
        'forecast_relationships:owner_id',
        'forecast_relationship_groups:owner_id',
        'pending_cascades:owner_id'
    ];
    entry TEXT;
    parts TEXT[];
    tbl TEXT;
    col TEXT;
    healed INTEGER;
BEGIN
    FOREACH entry IN ARRAY tbls LOOP
        parts := string_to_array(entry, ':');
        tbl := parts[1];
        col := parts[2];
        BEGIN
            EXECUTE format(
                'UPDATE public.%I t SET %I = u.user_id ' ||
                'FROM public.users u ' ||
                'WHERE t.%I = u.id::text ' ||
                '  AND t.%I <> u.user_id ' ||
                '  AND NOT EXISTS (SELECT 1 FROM public.users u2 WHERE u2.user_id = t.%I)',
                tbl, col, col, col, col
            );
            GET DIAGNOSTICS healed = ROW_COUNT;
            IF healed > 0 THEN
                RAISE NOTICE '[mig 162] healed % id::text-drift %.% rows via users.id join',
                    healed, tbl, col;
            END IF;
        EXCEPTION WHEN OTHERS THEN
            RAISE WARNING '[mig 162] id::text heal on %.% skipped: %',
                tbl, col, SQLERRM;
        END;
    END LOOP;
END $$;
