-- ROLLBACK SCRIPT: Undo all authentication migrations
-- WARNING: This will drop auth tables and remove user_id columns
-- Use only if rolling back authentication system

BEGIN;

-- Step 1: Drop new tables (in reverse order of creation)
DROP TABLE IF EXISTS public.siwe_nonces CASCADE;
DROP TABLE IF EXISTS public.api_keys CASCADE;
DROP TABLE IF EXISTS public.users CASCADE;

-- Step 2: Remove columns from existing tables
ALTER TABLE public.agents DROP COLUMN IF EXISTS user_id CASCADE;
ALTER TABLE public.agents DROP COLUMN IF EXISTS is_public CASCADE;
ALTER TABLE public.agents DROP COLUMN IF EXISTS visibility CASCADE;

ALTER TABLE public.episodes DROP COLUMN IF EXISTS user_id CASCADE;
ALTER TABLE public.semantic_rules DROP COLUMN IF EXISTS user_id CASCADE;
ALTER TABLE public.entities DROP COLUMN IF EXISTS user_id CASCADE;
ALTER TABLE public.facts DROP COLUMN IF EXISTS user_id CASCADE;
ALTER TABLE public.relationships DROP COLUMN IF EXISTS user_id CASCADE;

-- Step 3: Drop helper functions
DROP FUNCTION IF EXISTS update_updated_at_column() CASCADE;
DROP FUNCTION IF EXISTS cleanup_expired_siwe_nonces() CASCADE;

-- Step 4: Drop any auth-related policies (if RLS was enabled)
DO $$
DECLARE
    pol record;
BEGIN
    FOR pol IN
        SELECT schemaname, tablename, policyname
        FROM pg_policies
        WHERE schemaname = 'public'
    LOOP
        EXECUTE format('DROP POLICY IF EXISTS %I ON %I.%I',
            pol.policyname, pol.schemaname, pol.tablename);
    END LOOP;
END $$;

-- Step 5: Disable RLS on all tables
ALTER TABLE IF EXISTS public.agents DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS public.episodes DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS public.semantic_rules DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS public.entities DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS public.facts DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS public.relationships DISABLE ROW LEVEL SECURITY;

COMMIT;

-- Verification queries
SELECT 'Rollback complete. Verifying...' AS status;

SELECT table_name FROM information_schema.tables
WHERE table_schema = 'public' AND table_name IN ('users', 'api_keys', 'siwe_nonces');
-- Should return 0 rows

SELECT column_name FROM information_schema.columns
WHERE table_schema = 'public' AND table_name = 'agents' AND column_name = 'user_id';
-- Should return 0 rows
