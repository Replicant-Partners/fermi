-- Migration 195: declare the schema objects production has and no migration creates
--
-- Runs EARLY — immediately after 004, before 004b — despite its number. The
-- runner order is the list in `api_server::run_migrations()`, not the filename
-- order, and this file's whole purpose is to exist before the migrations that
-- depend on what it declares. Numbered 195 so it reads as "added late" in the
-- directory, positioned second so it is useful.
--
-- ─── Why this file exists ──────────────────────────────────────────────
--
-- The migration set cannot rebuild the schema from empty. 26 of ~196 files
-- failed against a fresh database, which is why `SCHEMA_STRICT=1` has never
-- been enablable and why the CI ratchet froze the number instead of gating on
-- zero. See docs/plans/CI_MIGRATION_RATCHET.md.
--
-- The causes are all one shape, repeated four times: **`IF NOT EXISTS`
-- silently skipping a schema change, and nobody noticing because
-- `run_migrations` swallows the error.**
--
--   1. `users.id` — migration 004 was edited in place after being applied, so
--      nothing in the repo creates it, yet fermi-auth JOINs on it. 181 already
--      declares it; it just runs 170 files too late to help 004b or 005.
--   2. `users.password_hash` / `password_salt` — referenced by 004b, 171 and
--      181, created by none.
--   3. `users_auth_provider_check` — 004 creates it WITHOUT 'legacy'; 004b
--      tries to widen it by re-declaring the column with
--      `ADD COLUMN IF NOT EXISTS`, which is a no-op once the column exists. A
--      CHECK cannot be widened by adding a column that is already there.
--   4. `fermi_forecasts` — 048 creates it with 13 columns; 094 does
--      `CREATE TABLE IF NOT EXISTS` with 28 and is therefore skipped entirely,
--      so its 15 extra columns never appear. 094 then aborts on
--      `CREATE INDEX ... (status)` and never reaches its own
--      `fermi_forecast_updates`, which takes 140, 149, 150, 156, 174 and 176
--      down with it, and 140's failure takes 175.
--
-- This file handles (1)-(3), the `users` ghosts. (4) needs to run between 048
-- and 094 rather than here, so it lives in 196.
--
-- Every statement here is additive and guarded, so this is a **no-op against
-- production** and the thing that makes a rebuild faithful. It deliberately
-- does not fix the migrations themselves: editing an already-applied migration
-- in place is what produced (1), and doing it again to fix (1) would be a poor
-- joke. Declaring the ghosts lets those files succeed unchanged.

DO $$
DECLARE
    v_rows integer;
BEGIN
    -- ═══════════════════════════════════════════════════════════════
    -- 1. users — the columns nothing creates
    -- ═══════════════════════════════════════════════════════════════
    --
    -- Shapes verified against production by the 2026-08-06 integrity audit:
    -- `id uuid NOT NULL DEFAULT gen_random_uuid()`, and email/password_hash/
    -- password_salt all NOT NULL without default.
    --
    -- 181 declares `users.id` identically. Both are `IF NOT EXISTS`, so
    -- whichever runs first wins and the other is a no-op; the duplication is
    -- the price of 181 remaining truthful about what it repairs in production.
    IF to_regclass('public.users') IS NOT NULL THEN
        ALTER TABLE public.users
            ADD COLUMN IF NOT EXISTS id uuid NOT NULL DEFAULT gen_random_uuid();

        -- DEFAULT '' only so the ADD can satisfy NOT NULL against rows an
        -- earlier boot inserted; dropped immediately so a rebuilt schema
        -- matches production's "NOT NULL without default" rather than
        -- acquiring a default production does not have.
        ALTER TABLE public.users
            ADD COLUMN IF NOT EXISTS password_hash text NOT NULL DEFAULT '';
        ALTER TABLE public.users
            ADD COLUMN IF NOT EXISTS password_salt text NOT NULL DEFAULT '';
        ALTER TABLE public.users ALTER COLUMN password_hash DROP DEFAULT;
        ALTER TABLE public.users ALTER COLUMN password_salt DROP DEFAULT;

        -- 004b reads `name` to seed `display_name`
        -- (`UPDATE users SET display_name = name ...`). Another column from
        -- the pre-edit 004 that no migration in the repo creates.
        ALTER TABLE public.users ADD COLUMN IF NOT EXISTS name text;

        -- 005 declares `user_id UUID NOT NULL REFERENCES public.users(id)`,
        -- commenting it as "Reference existing PK" — so production's `id` is
        -- unique, but a rebuilt `users` has it as an ordinary column and the
        -- FK is rejected with "no unique constraint matching given keys".
        -- UNIQUE rather than PRIMARY KEY: `user_id` is the declared primary
        -- key in 004 and moving that is not this file's business.
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint WHERE conname = 'users_id_unique'
        ) THEN
            ALTER TABLE public.users ADD CONSTRAINT users_id_unique UNIQUE (id);
        END IF;

        -- Widen the auth_provider CHECK to admit 'legacy'. Production admits
        -- it — the audit found system@abw.local carrying it, and 004b's UPDATE
        -- and 181's immunisation write both depend on it. Guarded: if a row
        -- already carries a value outside the declared set, warn and leave the
        -- constraint alone rather than aborting on a shape we have not seen.
        SELECT count(*) INTO v_rows
          FROM users
         WHERE auth_provider IS NOT NULL
           AND auth_provider NOT IN ('email','github','google','ethereum','legacy');

        IF v_rows > 0 THEN
            RAISE WARNING '[mig 195] % user(s) carry an auth_provider outside the declared set; leaving users_auth_provider_check untouched', v_rows;
        ELSE
            ALTER TABLE public.users DROP CONSTRAINT IF EXISTS users_auth_provider_check;
            ALTER TABLE public.users
                ADD CONSTRAINT users_auth_provider_check
                CHECK (auth_provider IN ('email','github','google','ethereum','legacy'));
        END IF;
    END IF;

    -- ══════════════════════════════════════════════════════════
    -- 4. migrations_log — written by two migrations, created by none
    -- ══════════════════════════════════════════════════════════
    --
    -- 089 and 090 both end with `INSERT INTO migrations_log (migration_id,
    -- applied_at, description)`. Nothing creates the table, so 090 aborts on
    -- its last statement having already done all its real work — which is the
    -- most misleading failure in the set, because the schema 090 builds is
    -- fine and only the bookkeeping is broken.
    --
    -- Not a substitute for a real migration ledger: `run_migrations` re-runs
    -- every file every boot and consults nothing, so this table records
    -- attempts, not state. It exists because two files write to it.
    -- `migration_id` is the primary key because 090 ends with
    -- `ON CONFLICT (migration_id) DO NOTHING`, which needs a unique index to
    -- infer — and one row per migration is the right model for a log that is
    -- re-executed on every boot.
    CREATE TABLE IF NOT EXISTS public.migrations_log (
        migration_id text PRIMARY KEY,
        applied_at   timestamptz NOT NULL DEFAULT NOW(),
        description  text
    );
END $$;
