-- Behavioural verification for mig-180 / SPEC_29.
-- Run against a THROWAWAY database that has all migrations applied.
--   psql ... -f scripts/spec29_verify.sql
\set ON_ERROR_STOP off

\echo '════ setup: users ════'
INSERT INTO users (user_id, email) VALUES
  ('mario', 'mario@example.com'),
  ('fermi-admin', 'admin@example.com')
ON CONFLICT DO NOTHING;

\echo ''
\echo '════ scenario: four agents covering every provenance case ════'
-- 1. curated specialist: contract from the boot seed, no request ever
-- 2. self-minted: contract via the old import bypass, no request  (the bug)
-- 3. properly approved: contract + an approved request
-- 4. contract but UNPUBLISHED: must not appear on the public roster
INSERT INTO agents (agent_id, agent_name, agent_type, executor_type, tier, status, visibility, user_id, fermi_contract, model, description)
VALUES
  ('11111111-1111-1111-1111-111111111111', 'macro_forecaster', 'research', 'llm', 'system',    'published', 'public', NULL,    '{"finding_labels":["a"]}', 'm', 'curated'),
  ('22222222-2222-2222-2222-222222222222', 'self_minted',      'research', 'llm', 'community', 'published', 'public', 'mario', '{"finding_labels":["b"]}', 'm', 'bypass'),
  ('33333333-3333-3333-3333-333333333333', 'efra_forensic',    'research', 'llm', 'community', 'published', 'public', 'mario', '{"finding_labels":["c"]}', 'm', 'approved'),
  ('44444444-4444-4444-4444-444444444444', 'draft_agent',      'research', 'llm', 'community', 'draft',     'public', 'mario', '{"finding_labels":["d"]}', 'm', 'unpublished')
ON CONFLICT (agent_id) DO NOTHING;

INSERT INTO orchestra_membership_requests
  (request_id, orchestra_name, agent_id, requested_by, proposed_contract, status, reviewed_by, reviewed_at)
VALUES
  ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', 'fermi',
   '33333333-3333-3333-3333-333333333333', 'mario', '{"finding_labels":["c"]}',
   'approved', 'fermi-admin', NOW())
ON CONFLICT DO NOTHING;

\echo ''
\echo '════ TEST 1 — pre-migration predicate would admit 3 published agents ════'
SELECT count(*) AS would_be_members_under_old_rule
  FROM agents WHERE fermi_contract IS NOT NULL AND status = 'published';

\echo ''
\echo '════ TEST 2 — roster is EMPTY until membership is stated ════'
\echo '(proves the contract column no longer grants membership)'
SELECT count(*) AS roster_before_any_grant FROM orchestra_fermi_members;

\echo ''
\echo '════ TEST 3 — CHECK blocks approved-without-a-receipt ════'
\echo '(expect: ERROR violating approved_has_request)'
INSERT INTO orchestra_members (orchestra_name, agent_id, source)
VALUES ('fermi', '22222222-2222-2222-2222-222222222222', 'approved');

\echo ''
\echo '════ TEST 4 — CHECK blocks an invented provenance ════'
\echo '(expect: ERROR violating orchestra_members_source_check)'
INSERT INTO orchestra_members (orchestra_name, agent_id, source)
VALUES ('fermi', '22222222-2222-2222-2222-222222222222', 'self_declared');

\echo ''
\echo '════ apply the mig-180 backfill to this scenario ════'
INSERT INTO orchestra_members (orchestra_name, agent_id, source, request_id, granted_by, granted_at)
SELECT 'fermi', a.agent_id,
       CASE WHEN r.request_id IS NOT NULL THEN 'approved' ELSE 'curated_seed' END,
       r.request_id, r.reviewed_by, COALESCE(r.reviewed_at, a.created_at)
  FROM agents a
  LEFT JOIN LATERAL (
       SELECT request_id, reviewed_by, reviewed_at
         FROM orchestra_membership_requests
        WHERE agent_id = a.agent_id AND orchestra_name = 'fermi' AND status = 'approved'
        ORDER BY reviewed_at DESC NULLS LAST LIMIT 1
  ) r ON TRUE
 WHERE a.fermi_contract IS NOT NULL AND a.status = 'published'
ON CONFLICT DO NOTHING;

\echo ''
\echo '════ TEST 5 — membership preserved exactly, provenance honest ════'
\echo '(expect 3 members: 1 approved + 2 curated_seed; draft_agent excluded)'
SELECT agent_name, membership_source, tier
  FROM orchestra_fermi_members ORDER BY agent_name;

\echo ''
\echo '════ TEST 6 — the audit query surfaces the self-minted agent ════'
\echo '(expect exactly one row: self_minted. macro_forecaster is tier=system'
\echo ' so it is expected debt; efra_forensic has a receipt.)'
SELECT m.agent_name, a.tier, a.user_id AS owner
  FROM orchestra_fermi_members m JOIN agents a USING (agent_id)
 WHERE a.tier <> 'system'
   AND NOT EXISTS (SELECT 1 FROM orchestra_membership_requests r
                    WHERE r.agent_id = m.agent_id AND r.orchestra_name='fermi'
                      AND r.status='approved');

\echo ''
\echo '════ TEST 7 — revoke removes membership but KEEPS the capability ════'
DELETE FROM orchestra_members
 WHERE orchestra_name='fermi' AND agent_id='22222222-2222-2222-2222-222222222222';
SELECT (SELECT count(*) FROM orchestra_fermi_members WHERE agent_name='self_minted') AS still_a_member,
       (SELECT fermi_contract IS NOT NULL FROM agents WHERE agent_name='self_minted') AS contract_intact;

\echo ''
\echo '════ TEST 8 — publishing a draft member activates it ════'
\echo '(grant first, then publish: roster reflects it without further action)'
INSERT INTO orchestra_members (orchestra_name, agent_id, source, request_id, granted_by)
VALUES ('fermi','44444444-4444-4444-4444-444444444444','admin_grant',NULL,'fermi-admin')
ON CONFLICT DO NOTHING;
SELECT count(*) AS roster_while_draft FROM orchestra_fermi_members WHERE agent_name='draft_agent';
UPDATE agents SET status='published' WHERE agent_name='draft_agent';
SELECT count(*) AS roster_after_publish FROM orchestra_fermi_members WHERE agent_name='draft_agent';

\echo ''
\echo '════ TEST 9 — deleting an agent cascades the grant away ════'
DELETE FROM agents WHERE agent_name='draft_agent';
SELECT count(*) AS orphan_grants FROM orchestra_members
 WHERE agent_id='44444444-4444-4444-4444-444444444444';
