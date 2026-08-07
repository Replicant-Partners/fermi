-- ═══════════════════════════════════════════════════════════════════════
-- Clean up orphaned ACL test data left in production
-- ═══════════════════════════════════════════════════════════════════════
--
-- DRY RUN BY DEFAULT. Nothing is deleted unless you pass -v apply=yes.
--
--   ./scripts/psql_direct.sh -f scripts/cleanup_test_orphans.sql              # preview
--   ./scripts/psql_direct.sh -v apply=yes -f scripts/cleanup_test_orphans.sql # execute
--
-- WHY THIS IS NOT A MIGRATION
--
-- run_migrations re-executes every file on every boot. A DELETE that runs
-- unattended, forever, against production is not something you want on a
-- boot path. Deleting rows is a deliberate, reviewed, once-off act.
--
-- WHAT IT REMOVES
--
-- The 2026-08-06 audit found 13 orphaned ACL rows. Triage showed they are
-- fixtures from tests/forecast_acl.rs that were executed against the
-- PRODUCTION database on 2026-06-22 and 2026-06-25 and never cleaned up:
--
--     acl-viewer-*, acl-member-*, acl-owner-*, share-target-*,
--     email-claim-accepter-*     and teams slugged acl-test-*
--
-- They are inert (they grant access to principals that cannot log in), but
-- they are noise in the visibility ladder and they mask real orphans.
--
-- ⚠ SEPARATELY: one row is NOT test data —
--     object_shares: portfolio shared with user 'a', 2026-08-06 12:17
-- That is a live bug: the share endpoint accepts a share_target that does
-- not exist in users. Fix the endpoint before deleting the row, or it will
-- simply come back. It is listed below but excluded from the delete.

-- Default `apply` to "no" unless the caller set it. `:{?var}` tests whether
-- a psql variable is defined; without this, an unset :'apply' would
-- interpolate as the literal text ":apply".
\if :{?apply}
\else
  \set apply no
\endif

\pset pager off

\echo ''
\echo '═══ Candidates: object_shares ═══'
SELECT id, object_type, object_id, share_target, permission, created_at
  FROM object_shares
 WHERE share_type = 'user'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = share_target)
   AND (share_target LIKE 'acl-%'
        OR share_target LIKE 'share-target-%'
        OR share_target LIKE 'email-claim-accepter-%')
 ORDER BY created_at;

\echo ''
\echo '═══ Candidates: team_members ═══'
SELECT team_id, member_id, role, joined_at
  FROM team_members
 WHERE member_type = 'user'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = member_id)
   AND member_id LIKE 'acl-%'
 ORDER BY joined_at;

\echo ''
\echo '═══ Candidates: teams ═══'
SELECT id, slug, name, owner_id, created_at
  FROM teams
 WHERE slug LIKE 'acl-test-%'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = owner_id)
 ORDER BY created_at;

\echo ''
\echo '═══ NOT DELETED — live bug, needs a code fix first ═══'
SELECT id, object_type, object_id, share_target, granted_by, created_at
  FROM object_shares
 WHERE share_type = 'user'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = share_target)
   AND share_target NOT LIKE 'acl-%'
   AND share_target NOT LIKE 'share-target-%'
   AND share_target NOT LIKE 'email-claim-accepter-%';

\echo ''

-- NOTE: the delete block below uses psql \if rather than a plpgsql DO block.
-- psql does NOT interpolate :'variables' inside dollar-quoted strings, so a
-- DO block cannot see the -v flag at all.
--
-- BEGIN/COMMIT is banned in migrations/ (PgBouncer transaction mode eats
-- them) but is correct and safe here: this is an operator script, run over
-- the DIRECT connection by scripts/psql_direct.sh. Three related deletes
-- should be all-or-nothing.

\if :apply

\echo '── APPLYING ──'
BEGIN;

-- Order matters: members before the teams they belong to.
DELETE FROM team_members
 WHERE member_type = 'user'
   AND member_id LIKE 'acl-%'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = member_id);

DELETE FROM object_shares
 WHERE share_type = 'user'
   AND (share_target LIKE 'acl-%'
        OR share_target LIKE 'share-target-%'
        OR share_target LIKE 'email-claim-accepter-%')
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = share_target);

-- Only teams left with no members at all, so a team that has picked up a
-- real member since the fixtures were created is preserved.
DELETE FROM teams
 WHERE slug LIKE 'acl-test-%'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = owner_id)
   AND NOT EXISTS (SELECT 1 FROM team_members m WHERE m.team_id = teams.id);

COMMIT;
\echo '── DONE. Re-run scripts/integrity_audit.sql to confirm. ──'

\else

\echo '── DRY RUN. Nothing deleted. ──'
\echo '── Re-run with: ./scripts/psql_direct.sh -v apply=yes -f scripts/cleanup_test_orphans.sql ──'

\endif
