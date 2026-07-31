-- ═══════════════════════════════════════════════════════════════════
-- Migration 165 — realign fermi_forecasts / _portfolios / _notebooks
--                 owner_id FK from users(id) → users(user_id)
--
-- v0.10.9 hotfix. Root cause of "only admin can save forecasts"
-- reported by every non-legacy user (Ilabra, Mo, …):
--
-- On this deploy, three FKs point at the WRONG column of users:
--
--     ALTER TABLE fermi_forecasts / _portfolios / _notebooks
--       ADD CONSTRAINT ..._owner_id_fkey
--         FOREIGN KEY (owner_id) REFERENCES users(id) …  ← drifted
--
-- vs. what migration 094 declared:
--
--         FOREIGN KEY (owner_id) REFERENCES users(user_id) …  ← intended
--
-- (Someone reworked the FK at some point — the "UUID-drifted
-- deployment" v0.9.1's release notes hinted at without diagnosing.)
--
-- Every handler across the codebase writes `owner_id = principal.user_id()`,
-- which resolves to `users.user_id` (a TEXT column). But the FK checks
-- against `users.id` (UUID PK). For legacy users mig 004b backfilled
-- `user_id = id::text`, so both columns hold the same UUID and the
-- constraint passes coincidentally. For every user created by
-- `sync_user_from_app`'s INSERT branch (which mints a fresh
-- `Uuid::new_v4()` for `user_id`, distinct from the row's PK `id`),
-- the values diverge and every save trips the FK.
--
-- Confirmed empirically before this migration:
--
--     SELECT conname, pg_get_constraintdef(oid)
--     FROM pg_constraint
--     WHERE conrelid IN (
--         'fermi_forecasts'::regclass,
--         'fermi_portfolios'::regclass,
--         'fermi_notebooks'::regclass)
--       AND conname LIKE '%owner_id%fkey%';
--
--     → all three: FOREIGN KEY (owner_id) REFERENCES users(id) ON DELETE CASCADE
--
-- Fix (per table):
--   1. DROP the drifted FK (currently → users.id).
--   2. Convert owner_id column from UUID → TEXT so it can reference
--      users.user_id (TEXT). Values are preserved (UUID → text is
--      lossless).
--   3. Rebase every row's owner_id from `users.id::text` to
--      `users.user_id` via a JOIN. For legacy users this is a no-op
--      (id::text == user_id); for OAuth users it's the actual heal.
--   4. Re-add the FK targeting users(user_id) — matching mig 094's
--      original intent.
--
-- Sequenced so there's never a moment where the FK exists AND the
-- data doesn't satisfy it: DROP → convert type → rebase values → ADD.
--
-- Idempotent: every step guarded by IF EXISTS / IF NOT EXISTS or
-- catalog probes. Safe to re-run. Individual failures are logged as
-- WARNINGs but don't abort the migration.
--
-- PgBouncer-safe: no BEGIN/COMMIT; each DDL in its own DO block.
-- ═══════════════════════════════════════════════════════════════════

-- ── Pre-migration diagnostics ──────────────────────────────────────
DO $$
DECLARE
    n_forecasts INTEGER;
    n_portfolios INTEGER;
    n_notebooks INTEGER;
BEGIN
    SELECT COUNT(*) INTO n_forecasts FROM public.fermi_forecasts;
    SELECT COUNT(*) INTO n_portfolios FROM public.fermi_portfolios;
    SELECT COUNT(*) INTO n_notebooks FROM public.fermi_notebooks;
    RAISE NOTICE '[mig 165] pre-migration counts — forecasts: %, portfolios: %, notebooks: %',
        n_forecasts, n_portfolios, n_notebooks;
END $$;

-- ═══════════════════════════════════════════════════════════════════
-- fermi_forecasts
-- ═══════════════════════════════════════════════════════════════════

DO $$
BEGIN
    -- Step 1: drop the drifted FK (→ users.id).
    IF EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_forecasts_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_forecasts
            DROP CONSTRAINT fermi_forecasts_owner_id_fkey;
        RAISE NOTICE '[mig 165] dropped fermi_forecasts_owner_id_fkey (was → users.id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] DROP fermi_forecasts_owner_id_fkey skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    current_type TEXT;
BEGIN
    -- Step 2: convert column type UUID → TEXT if needed.
    SELECT data_type INTO current_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_forecasts'
       AND column_name  = 'owner_id';

    IF current_type = 'uuid' THEN
        ALTER TABLE public.fermi_forecasts
            ALTER COLUMN owner_id TYPE TEXT USING owner_id::text;
        RAISE NOTICE '[mig 165] fermi_forecasts.owner_id: UUID → TEXT';
    ELSE
        RAISE NOTICE '[mig 165] fermi_forecasts.owner_id already %', current_type;
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ALTER fermi_forecasts.owner_id TYPE skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    n_rebased INTEGER;
BEGIN
    -- Step 3: rebase every owner_id from users.id::text to users.user_id.
    -- For legacy users (mig 004b backfill: user_id = id::text) this
    -- is a no-op — the value doesn't change. For OAuth users the
    -- rewrite is the actual fix.
    UPDATE public.fermi_forecasts f
       SET owner_id = u.user_id
      FROM public.users u
     WHERE f.owner_id = u.id::text
       AND f.owner_id <> u.user_id;
    GET DIAGNOSTICS n_rebased = ROW_COUNT;
    RAISE NOTICE '[mig 165] fermi_forecasts: rebased % rows (id::text → user_id)', n_rebased;
END $$;

DO $$
BEGIN
    -- Step 4: add the correct FK (→ users.user_id) per mig 094 intent.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_forecasts_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_forecasts
            ADD CONSTRAINT fermi_forecasts_owner_id_fkey
            FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
            ON DELETE CASCADE;
        RAISE NOTICE '[mig 165] added fermi_forecasts_owner_id_fkey → users(user_id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ADD fermi_forecasts_owner_id_fkey failed: % — likely orphan rows remain; check rbac_orphans view', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════
-- fermi_portfolios
-- ═══════════════════════════════════════════════════════════════════

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_portfolios_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_portfolios
            DROP CONSTRAINT fermi_portfolios_owner_id_fkey;
        RAISE NOTICE '[mig 165] dropped fermi_portfolios_owner_id_fkey (was → users.id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] DROP fermi_portfolios_owner_id_fkey skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    current_type TEXT;
BEGIN
    SELECT data_type INTO current_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_portfolios'
       AND column_name  = 'owner_id';

    IF current_type = 'uuid' THEN
        ALTER TABLE public.fermi_portfolios
            ALTER COLUMN owner_id TYPE TEXT USING owner_id::text;
        RAISE NOTICE '[mig 165] fermi_portfolios.owner_id: UUID → TEXT';
    ELSE
        RAISE NOTICE '[mig 165] fermi_portfolios.owner_id already %', current_type;
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ALTER fermi_portfolios.owner_id TYPE skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    n_rebased INTEGER;
BEGIN
    UPDATE public.fermi_portfolios p
       SET owner_id = u.user_id
      FROM public.users u
     WHERE p.owner_id = u.id::text
       AND p.owner_id <> u.user_id;
    GET DIAGNOSTICS n_rebased = ROW_COUNT;
    RAISE NOTICE '[mig 165] fermi_portfolios: rebased % rows (id::text → user_id)', n_rebased;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_portfolios_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_portfolios
            ADD CONSTRAINT fermi_portfolios_owner_id_fkey
            FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
            ON DELETE CASCADE;
        RAISE NOTICE '[mig 165] added fermi_portfolios_owner_id_fkey → users(user_id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ADD fermi_portfolios_owner_id_fkey failed: %', SQLERRM;
END $$;

-- ═══════════════════════════════════════════════════════════════════
-- fermi_notebooks
-- ═══════════════════════════════════════════════════════════════════

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_notebooks_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_notebooks
            DROP CONSTRAINT fermi_notebooks_owner_id_fkey;
        RAISE NOTICE '[mig 165] dropped fermi_notebooks_owner_id_fkey (was → users.id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] DROP fermi_notebooks_owner_id_fkey skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    current_type TEXT;
BEGIN
    SELECT data_type INTO current_type
      FROM information_schema.columns
     WHERE table_schema = 'public'
       AND table_name   = 'fermi_notebooks'
       AND column_name  = 'owner_id';

    IF current_type = 'uuid' THEN
        ALTER TABLE public.fermi_notebooks
            ALTER COLUMN owner_id TYPE TEXT USING owner_id::text;
        RAISE NOTICE '[mig 165] fermi_notebooks.owner_id: UUID → TEXT';
    ELSE
        RAISE NOTICE '[mig 165] fermi_notebooks.owner_id already %', current_type;
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ALTER fermi_notebooks.owner_id TYPE skipped: %', SQLERRM;
END $$;

DO $$
DECLARE
    n_rebased INTEGER;
BEGIN
    UPDATE public.fermi_notebooks n
       SET owner_id = u.user_id
      FROM public.users u
     WHERE n.owner_id = u.id::text
       AND n.owner_id <> u.user_id;
    GET DIAGNOSTICS n_rebased = ROW_COUNT;
    RAISE NOTICE '[mig 165] fermi_notebooks: rebased % rows (id::text → user_id)', n_rebased;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'fermi_notebooks_owner_id_fkey'
    ) THEN
        ALTER TABLE public.fermi_notebooks
            ADD CONSTRAINT fermi_notebooks_owner_id_fkey
            FOREIGN KEY (owner_id) REFERENCES public.users(user_id)
            ON DELETE CASCADE;
        RAISE NOTICE '[mig 165] added fermi_notebooks_owner_id_fkey → users(user_id)';
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE WARNING '[mig 165] ADD fermi_notebooks_owner_id_fkey failed: %', SQLERRM;
END $$;

-- ── Post-migration validation ──────────────────────────────────────
DO $$
DECLARE
    fk_target_forecasts TEXT;
    fk_target_portfolios TEXT;
    fk_target_notebooks TEXT;
BEGIN
    SELECT pg_get_constraintdef(oid) INTO fk_target_forecasts
      FROM pg_constraint WHERE conname = 'fermi_forecasts_owner_id_fkey';
    SELECT pg_get_constraintdef(oid) INTO fk_target_portfolios
      FROM pg_constraint WHERE conname = 'fermi_portfolios_owner_id_fkey';
    SELECT pg_get_constraintdef(oid) INTO fk_target_notebooks
      FROM pg_constraint WHERE conname = 'fermi_notebooks_owner_id_fkey';

    RAISE NOTICE '[mig 165] post-migration FK definitions:';
    RAISE NOTICE '           fermi_forecasts:  %', COALESCE(fk_target_forecasts,   '(missing)');
    RAISE NOTICE '           fermi_portfolios: %', COALESCE(fk_target_portfolios,  '(missing)');
    RAISE NOTICE '           fermi_notebooks:  %', COALESCE(fk_target_notebooks,   '(missing)');
END $$;
