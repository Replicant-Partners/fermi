-- ═══════════════════════════════════════════════════════════════════════
-- Integrity triage — READ ONLY follow-up to scripts/integrity_audit.sql
-- ═══════════════════════════════════════════════════════════════════════
--
-- The audit told us WHAT is wrong. This tells us WHICH ROWS and WHY, so the
-- reconciliation migration can be written precisely rather than broadly.
--
-- Answers four questions:
--
--   1. Which single ledger row broke the credit chain? (CREDIT-001/005)
--   2. Are the 13 "orphaned" ACL rows real orphans, or system principals /
--      agents that my users-only check mis-flagged? (SHARE-001, TEAM-002/003)
--   3. What exactly is the state of the abw-system principal? (SEC-001)
--   4. Is rbac_orphans_view genuinely absent? (PRESENCE-*)
--
--   ./scripts/psql_direct.sh -f scripts/integrity_triage.sql

\pset pager off

\echo ''
\echo '═══ 1a. Wallets whose balance disagrees with their ledger ═══'

SELECT w.wallet_id,
       w.owner_type,
       w.owner_id,
       w.balance                        AS wallet_balance,
       COALESCE(sum(l.amount), 0)       AS ledger_sum,
       w.balance - COALESCE(sum(l.amount), 0) AS drift,
       count(l.tx_id)                   AS ledger_rows,
       min(l.created_at)                AS first_tx,
       max(l.created_at)                AS last_tx
  FROM wallets w
  LEFT JOIN credit_ledger l ON l.wallet_id = w.wallet_id
 GROUP BY w.wallet_id, w.owner_type, w.owner_id, w.balance
HAVING w.balance <> COALESCE(sum(l.amount), 0)
 ORDER BY abs(w.balance - COALESCE(sum(l.amount), 0)) DESC;

\echo ''
\echo '═══ 1b. Chain breaks — ORDER-INDEPENDENT ═══'
\echo '(A ledger row is sound if some other row in the same wallet has'
\echo ' balance_after = this.balance_after - this.amount. This avoids the'
\echo ' same-timestamp tie-break noise that inflated CREDIT-005 to 154.)'

SELECT l.tx_id,
       l.wallet_id,
       l.tx_type,
       l.amount,
       l.balance_after,
       l.balance_after - l.amount AS implied_predecessor_balance,
       l.related_id,
       left(coalesce(l.description, ''), 44) AS description,
       l.created_at
  FROM credit_ledger l
 WHERE l.balance_after - l.amount <> 0            -- genesis row is legitimately unlinked
   AND NOT EXISTS (
        SELECT 1 FROM credit_ledger p
         WHERE p.wallet_id = l.wallet_id
           AND p.tx_id <> l.tx_id
           AND p.balance_after = l.balance_after - l.amount
   )
 ORDER BY l.wallet_id, l.created_at;

\echo ''
\echo '═══ 1c. Does each wallet''s newest ledger row match its balance? ═══'

WITH newest AS (
    SELECT DISTINCT ON (wallet_id)
           wallet_id, tx_id, balance_after, created_at
      FROM credit_ledger
     ORDER BY wallet_id, created_at DESC, balance_after DESC
)
SELECT w.wallet_id, w.owner_id, w.balance, n.balance_after AS newest_ledger_balance,
       w.balance - n.balance_after AS gap, n.created_at AS newest_tx_at
  FROM wallets w JOIN newest n ON n.wallet_id = w.wallet_id
 WHERE w.balance <> n.balance_after
 ORDER BY abs(w.balance - n.balance_after) DESC;

\echo ''
\echo '═══ 2a. SHARE-001 — who are the 8 non-user share targets? ═══'

SELECT s.object_type, s.share_target, s.permission, s.granted_by, s.created_at,
       CASE
         WHEN s.share_target = 'abw-system'                                    THEN 'SYSTEM PRINCIPAL'
         WHEN EXISTS (SELECT 1 FROM agents a WHERE a.agent_id::text = s.share_target) THEN 'IS AN AGENT (agent_id)'
         WHEN EXISTS (SELECT 1 FROM agents a WHERE a.agent_name = s.share_target)     THEN 'IS AN AGENT (agent_name)'
         WHEN EXISTS (SELECT 1 FROM teams  t WHERE t.id::text = s.share_target)       THEN 'IS A TEAM'
         WHEN s.share_target ~ '^0x[0-9a-fA-F]{40}$'                            THEN 'ETH ADDRESS (never registered?)'
         WHEN s.share_target = ''                                               THEN 'EMPTY STRING'
         ELSE 'UNKNOWN — likely a genuinely deleted user'
       END AS classification
  FROM object_shares s
 WHERE s.share_type = 'user'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = s.share_target)
 ORDER BY classification, s.created_at;

\echo ''
\echo '═══ 2b. TEAM-002 — who are the 3 non-user team members? ═══'

SELECT m.team_id, m.member_id, m.role, m.invited_by, m.joined_at,
       CASE
         WHEN m.member_id = 'abw-system'                                        THEN 'SYSTEM PRINCIPAL'
         WHEN EXISTS (SELECT 1 FROM agents a WHERE a.agent_id::text = m.member_id) THEN 'IS AN AGENT (mislabelled member_type)'
         WHEN EXISTS (SELECT 1 FROM agents a WHERE a.agent_name = m.member_id)     THEN 'IS AN AGENT (by name)'
         WHEN m.member_id = ''                                                  THEN 'EMPTY STRING'
         ELSE 'UNKNOWN — likely a genuinely deleted user'
       END AS classification,
       EXISTS (SELECT 1 FROM teams t WHERE t.id = m.team_id) AS team_still_exists
  FROM team_members m
 WHERE m.member_type = 'user'
   AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = m.member_id)
 ORDER BY classification;

\echo ''
\echo '═══ 2c. TEAM-003 — the 2 teams with non-user owners ═══'

SELECT t.id, t.slug, t.name, t.owner_id, t.created_at,
       CASE
         WHEN t.owner_id = 'abw-system'                                        THEN 'SYSTEM PRINCIPAL'
         WHEN EXISTS (SELECT 1 FROM agents a WHERE a.agent_id::text = t.owner_id) THEN 'IS AN AGENT'
         WHEN t.owner_id = ''                                                  THEN 'EMPTY STRING'
         ELSE 'UNKNOWN — likely a genuinely deleted user'
       END AS classification,
       (SELECT count(*) FROM team_members m WHERE m.team_id = t.id) AS members
  FROM teams t
 WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = t.owner_id)
 ORDER BY t.created_at;

\echo ''
\echo '═══ 3. abw-system principal state ═══'

SELECT 'users row for abw-system' AS item,
       CASE WHEN EXISTS (SELECT 1 FROM users WHERE user_id = 'abw-system')
            THEN 'PRESENT' ELSE 'ABSENT' END AS state;

SELECT credential_id, provider, scope, label, created_at, updated_at,
       octet_length(encrypted_value) AS ciphertext_bytes
  FROM agent_credentials
 WHERE principal_id = 'abw-system'
 ORDER BY provider, scope;

\echo ''
\echo '-- users NOT NULL columns without defaults (the migration must supply these) --'
SELECT a.attname AS column_name,
       pg_catalog.format_type(a.atttypid, a.atttypmod) AS type
  FROM pg_attribute a
  JOIN pg_class c    ON c.oid = a.attrelid
  JOIN pg_namespace n ON n.oid = c.relnamespace
 WHERE n.nspname = 'public' AND c.relname = 'users'
   AND a.attnum > 0 AND NOT a.attisdropped
   AND a.attnotnull
   AND NOT EXISTS (SELECT 1 FROM pg_attrdef d WHERE d.adrelid = c.oid AND d.adnum = a.attnum)
 ORDER BY a.attnum;

\echo ''
\echo '-- CHECK constraints on users (so the seed does not violate one) --'
SELECT conname, pg_get_constraintdef(oid) AS definition
  FROM pg_constraint
 WHERE conrelid = 'public.users'::regclass AND contype = 'c'
 ORDER BY conname;

\echo ''
\echo '═══ 4. rbac_orphans_view (migration 163) ═══'

SELECT COALESCE(
    (SELECT c.relkind::text || ' — present'
       FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
      WHERE n.nspname = 'public' AND c.relname = 'rbac_orphans_view'),
    'ABSENT — migration 163 did not apply'
) AS rbac_orphans_view;

\echo ''
\echo '-- does users.location_name exist? (163 failed on this column) --'
SELECT CASE WHEN EXISTS (
    SELECT 1 FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid
      JOIN pg_namespace n ON n.oid = c.relnamespace
     WHERE n.nspname='public' AND c.relname='users' AND a.attname='location_name'
       AND a.attnum > 0 AND NOT a.attisdropped
) THEN 'present' ELSE 'ABSENT — this is why 163 aborted' END AS users_location_name;

\echo ''
\echo '═══ 5. users.id — the column no migration creates ═══'

SELECT a.attname, pg_catalog.format_type(a.atttypid, a.atttypmod) AS type,
       a.attnotnull AS not_null,
       pg_get_expr(d.adbin, d.adrelid) AS default_expr
  FROM pg_attribute a
  JOIN pg_class c     ON c.oid = a.attrelid
  JOIN pg_namespace n ON n.oid = c.relnamespace
  LEFT JOIN pg_attrdef d ON d.adrelid = c.oid AND d.adnum = a.attnum
 WHERE n.nspname='public' AND c.relname='users' AND a.attname='id';

\echo ''
