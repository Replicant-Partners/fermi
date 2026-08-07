-- ═══════════════════════════════════════════════════════════════════════
-- ΞSYSTEM production integrity audit — READ ONLY
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT THIS IS
--
-- A ground-truth report on the STATE of the database, as opposed to its
-- SHAPE. `src/schema_trust.rs` answers "do the columns exist"; this answers
-- "is the data they hold actually consistent".
--
-- Those are different questions and they fail differently. A swallowed
-- `let _ = sqlx::query(...)` on a credit deposit leaves the schema perfectly
-- valid and the money gone.
--
-- WHY IT EXISTS
--
-- Three facts established on 2026-08-06 (see
-- docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md):
--
--   1. ~240 `let _ = sqlx::query(...)` sites swallow write failures, several
--      of them in credit and royalty paths.
--   2. `run_migrations` swallows migration errors and there is no ledger, so
--      nobody can say which migrations actually applied. Several *integrity*
--      migrations (163/165/171/174/176, and 136 which was never even wired)
--      are in the unverifiable set.
--   3. Only 12 database transactions exist in 223k LOC, so multi-write
--      operations — charge+ledger, charge+royalty, resolve+counterfactual —
--      are non-atomic by default.
--
-- Any of those can have already produced silent divergence. This tells you
-- whether they did.
--
-- THE ORDERING PRINCIPLE
--
-- You cannot enable an invariant you have not first reconciled to zero.
-- SCHEMA_STRICT=1 could not be turned on because of a false positive;
-- migration 136 cannot be applied because existing rows may violate it. Every
-- constraint you eventually want — FKs, NOT NULLs, rule gates — needs its
-- current violation count known first. That is what this produces.
--
-- SAFETY
--
--   * No INSERT/UPDATE/DELETE/DDL against any application table.
--   * Creates only `pg_temp` objects (session-local, vanish on disconnect).
--   * Every check is guarded on object existence, so it degrades to SKIPPED
--     rather than erroring on a database missing some of these tables.
--
-- USAGE
--
--   psql "$DIRECT_DATABASE_URL" -f scripts/integrity_audit.sql
--
-- ⚠ Use a DIRECT connection, not the PgBouncer pooler. In transaction-mode
--   pooling the temp table may land on a different backend than the final
--   SELECT and you will get "relation does not exist". On Neon this is the
--   connection string WITHOUT `-pooler` in the host.
--
-- Read the output bottom-up: the summary is printed last.
-- ═══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on
\timing off

CREATE TEMP TABLE integrity_findings (
    seq        serial,
    check_id   text,
    category   text,
    severity   text,
    status     text,      -- OK | VIOLATION | SKIPPED | ERROR
    violations bigint,
    detail     text
);

-- ── Object-existence guard ────────────────────────────────────────────
-- Accepts 'table' or 'table.column'. Returns NULL if all present, else a
-- comma-separated list of what is missing.
CREATE FUNCTION pg_temp.missing_objects(reqs text[]) RETURNS text AS $fn$
DECLARE
    r text; t text; c text; missing text[] := '{}';
BEGIN
    FOREACH r IN ARRAY reqs LOOP
        IF position('.' in r) > 0 THEN
            t := split_part(r, '.', 1);
            c := split_part(r, '.', 2);
            IF NOT EXISTS (
                SELECT 1
                  FROM pg_attribute a
                  JOIN pg_class cl     ON cl.oid = a.attrelid
                  JOIN pg_namespace n  ON n.oid = cl.relnamespace
                 WHERE n.nspname = 'public'
                   AND cl.relname = t
                   AND a.attname = c
                   AND a.attnum > 0
                   AND NOT a.attisdropped
            ) THEN
                missing := missing || r;
            END IF;
        ELSE
            IF to_regclass('public.' || r) IS NULL THEN
                missing := missing || r;
            END IF;
        END IF;
    END LOOP;

    IF array_length(missing, 1) IS NULL THEN
        RETURN NULL;
    END IF;
    RETURN array_to_string(missing, ', ');
END
$fn$ LANGUAGE plpgsql;

-- ── Check runner ──────────────────────────────────────────────────────
-- p_count_sql must return exactly one bigint (the violation count).
-- p_sample_sql, if given, must return one text value; it runs only when
-- violations > 0, so an audit on a clean database stays cheap.
CREATE FUNCTION pg_temp.chk(
    p_id         text,
    p_category   text,
    p_severity   text,
    p_reqs       text[],
    p_count_sql  text,
    p_sample_sql text DEFAULT NULL,
    p_note       text DEFAULT NULL
) RETURNS void AS $fn$
DECLARE
    miss text;
    n    bigint;
    smp  text;
BEGIN
    miss := pg_temp.missing_objects(p_reqs);
    IF miss IS NOT NULL THEN
        INSERT INTO integrity_findings(check_id, category, severity, status, violations, detail)
        VALUES (p_id, p_category, p_severity, 'SKIPPED', NULL,
                'cannot check — missing: ' || miss);
        RETURN;
    END IF;

    BEGIN
        EXECUTE p_count_sql INTO n;
    EXCEPTION WHEN OTHERS THEN
        INSERT INTO integrity_findings(check_id, category, severity, status, violations, detail)
        VALUES (p_id, p_category, p_severity, 'ERROR', NULL, SQLERRM);
        RETURN;
    END;

    IF n > 0 AND p_sample_sql IS NOT NULL THEN
        BEGIN
            EXECUTE p_sample_sql INTO smp;
        EXCEPTION WHEN OTHERS THEN
            smp := '(sample failed: ' || SQLERRM || ')';
        END;
    END IF;

    INSERT INTO integrity_findings(check_id, category, severity, status, violations, detail)
    VALUES (p_id, p_category, p_severity,
            CASE WHEN n = 0 THEN 'OK' ELSE 'VIOLATION' END,
            n,
            COALESCE(NULLIF(concat_ws(' | ', p_note, smp), ''), p_note));
END
$fn$ LANGUAGE plpgsql;


-- ═══════════════════════════════════════════════════════════════════════
-- CREDITS — conservation laws
-- ═══════════════════════════════════════════════════════════════════════
--
-- `wallets.balance` is authoritative in code; `credit_ledger` is written
-- afterwards, in a separate statement, with no enclosing transaction
-- (fermi-auth/src/credits.rs:245 then :283). Nothing anywhere recomputes a
-- balance from the ledger. So the two CAN diverge, and no code path would
-- ever notice. These checks are the first time anyone has looked.

SELECT pg_temp.chk(
  'CREDIT-001', 'credits', 'CRITICAL',
  ARRAY['wallets.wallet_id','wallets.balance','credit_ledger.wallet_id','credit_ledger.amount'],
  $q$
    SELECT count(*) FROM (
      SELECT w.wallet_id
        FROM wallets w
        LEFT JOIN credit_ledger l ON l.wallet_id = w.wallet_id
       GROUP BY w.wallet_id, w.balance
      HAVING w.balance <> COALESCE(sum(l.amount), 0)
    ) x
  $q$,
  $q$
    SELECT string_agg(t, '; ') FROM (
      SELECT format('wallet %s: balance=%s ledger_sum=%s drift=%s',
                    w.wallet_id, w.balance, COALESCE(sum(l.amount),0),
                    w.balance - COALESCE(sum(l.amount),0)) AS t
        FROM wallets w
        LEFT JOIN credit_ledger l ON l.wallet_id = w.wallet_id
       GROUP BY w.wallet_id, w.balance
      HAVING w.balance <> COALESCE(sum(l.amount), 0)
       ORDER BY abs(w.balance - COALESCE(sum(l.amount),0)) DESC
       LIMIT 10
    ) s
  $q$,
  'wallets.balance must equal SUM(credit_ledger.amount). Divergence = a ledger write was swallowed, or a balance was mutated without a ledger row.'
);

SELECT pg_temp.chk(
  'CREDIT-002', 'credits', 'CRITICAL',
  ARRAY['wallets.balance'],
  $q$ SELECT count(*) FROM wallets WHERE balance < 0 $q$,
  $q$ SELECT string_agg(format('wallet %s: %s', wallet_id, balance), '; ')
        FROM (SELECT wallet_id, balance FROM wallets WHERE balance < 0 ORDER BY balance LIMIT 10) s $q$,
  'Negative balance. The charge guard is `WHERE balance >= $1`, so this should be unreachable.'
);

SELECT pg_temp.chk(
  'CREDIT-003', 'credits', 'CRITICAL',
  ARRAY['credit_ledger.stripe_session_id'],
  $q$
    SELECT count(*) FROM (
      SELECT stripe_session_id FROM credit_ledger
       WHERE stripe_session_id IS NOT NULL AND stripe_session_id <> ''
       GROUP BY stripe_session_id HAVING count(*) > 1
    ) x
  $q$,
  $q$
    SELECT string_agg(format('%s x%s', stripe_session_id, c), '; ') FROM (
      SELECT stripe_session_id, count(*) c FROM credit_ledger
       WHERE stripe_session_id IS NOT NULL AND stripe_session_id <> ''
       GROUP BY stripe_session_id HAVING count(*) > 1 ORDER BY c DESC LIMIT 10
    ) s
  $q$,
  'DOUBLE-CREDITED Stripe sessions. src/handlers/billing.rs:255 guards with `if let Ok(Some(_))`, so a transient DB error falls through and credits again; the marker write at :282 is itself `let _ =`.'
);

SELECT pg_temp.chk(
  'CREDIT-004', 'credits', 'HIGH',
  ARRAY['credit_ledger.stripe_session_id','credit_ledger.description','credit_ledger.tx_type'],
  $q$
    SELECT count(*) FROM credit_ledger
     WHERE tx_type = 'deposit'
       AND (stripe_session_id IS NULL OR stripe_session_id = '')
       AND description ILIKE '%stripe%'
  $q$,
  $q$ SELECT string_agg(tx_id::text, '; ')
        FROM (SELECT tx_id FROM credit_ledger
               WHERE tx_type='deposit' AND (stripe_session_id IS NULL OR stripe_session_id='')
                 AND description ILIKE '%stripe%' ORDER BY created_at DESC LIMIT 10) s $q$,
  'Stripe-looking deposits with no session marker — the `let _ =` idempotency write failed. These are the rows a Stripe retry would double-credit.'
);

-- CHAIN INTEGRITY, measured order-independently.
--
-- The first version of this check computed a running total with
-- `sum(amount) OVER (PARTITION BY wallet_id ORDER BY created_at, tx_id)`
-- and reported 154 violations. That was the CHECK being wrong, not the data.
--
-- `created_at DEFAULT NOW()` is transaction START time in Postgres, not
-- commit time, so two overlapping transactions can commit in a different
-- order than they began. Ordering the ledger by created_at therefore does
-- not reproduce the order the balances were actually written in, and the
-- mismatch is an artifact rather than a defect. (Confirmed: zero rows in
-- this database share a created_at with a sibling, so it was never a
-- tie-break problem either.)
--
-- The real invariant needs no ordering at all: every row must have a
-- predecessor it chains to. Row R is sound if some other row in the same
-- wallet has balance_after = R.balance_after - R.amount. This found the one
-- genuine break (a ledger INSERT lost to a swallowed write, repaired by
-- migration 181) among 154 false positives.
SELECT pg_temp.chk(
  'CREDIT-005', 'credits', 'HIGH',
  ARRAY['credit_ledger.wallet_id','credit_ledger.amount','credit_ledger.balance_after'],
  $q$
    SELECT count(*) FROM credit_ledger l
     WHERE l.balance_after - l.amount <> 0          -- genesis row is legitimately unlinked
       AND NOT EXISTS (
            SELECT 1 FROM credit_ledger p
             WHERE p.wallet_id = l.wallet_id
               AND p.tx_id <> l.tx_id
               AND p.balance_after = l.balance_after - l.amount
       )
  $q$,
  $q$
    SELECT string_agg(format('tx %s (%s %s): balance_after=%s implies a predecessor at %s, which does not exist',
                             l.tx_id, l.tx_type, l.amount, l.balance_after, l.balance_after - l.amount), '; ')
      FROM (
        SELECT * FROM credit_ledger l
         WHERE l.balance_after - l.amount <> 0
           AND NOT EXISTS (SELECT 1 FROM credit_ledger p
                            WHERE p.wallet_id = l.wallet_id AND p.tx_id <> l.tx_id
                              AND p.balance_after = l.balance_after - l.amount)
         LIMIT 10
      ) l
  $q$,
  'Every ledger row must chain to a predecessor. A break means a ledger INSERT was lost while the wallet balance was still mutated.'
);

-- Claimed but never credited. With the claim-then-credit flow (mig 182 +
-- billing.rs), settled_at IS NULL means a webhook took ownership of a Stripe
-- session and then failed before the deposit landed — i.e. a customer paid
-- and may be owed credits. The handler releases its claim on the failure
-- paths it can see, so anything lingering here needs a human.
SELECT pg_temp.chk(
  'CREDIT-008', 'credits', 'CRITICAL',
  ARRAY['stripe_sessions_processed.settled_at','stripe_sessions_processed.claimed_at'],
  $q$
    SELECT count(*) FROM stripe_sessions_processed
     WHERE settled_at IS NULL AND claimed_at < NOW() - INTERVAL '1 hour'
  $q$,
  $q$ SELECT string_agg(format('session %s user=%s credits=%s claimed=%s',
                               session_id, coalesce(user_id,'?'), coalesce(credits,0), claimed_at), '; ')
        FROM (SELECT * FROM stripe_sessions_processed
               WHERE settled_at IS NULL AND claimed_at < NOW() - INTERVAL '1 hour'
               ORDER BY claimed_at LIMIT 10) s $q$,
  'Stripe session claimed but never settled — a customer may have paid without receiving credits.'
);

SELECT pg_temp.chk(
  'CREDIT-006', 'credits', 'HIGH',
  ARRAY['wallets.owner_type','wallets.owner_id','users.user_id'],
  $q$
    SELECT count(*) FROM wallets w
     WHERE w.owner_type = 'user'
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = w.owner_id)
  $q$,
  $q$ SELECT string_agg(format('wallet %s owner=%s balance=%s', wallet_id, owner_id, balance), '; ')
        FROM (SELECT wallet_id, owner_id, balance FROM wallets w
               WHERE owner_type='user'
                 AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = w.owner_id)
               ORDER BY balance DESC LIMIT 10) s $q$,
  'ORPHANED WALLETS — credits held for a user that no longer exists.'
);

SELECT pg_temp.chk(
  'CREDIT-007', 'credits', 'MEDIUM',
  ARRAY['wallets.total_spent','wallets.total_deposited','credit_ledger.amount'],
  $q$
    SELECT count(*) FROM (
      SELECT w.wallet_id
        FROM wallets w
        LEFT JOIN credit_ledger l ON l.wallet_id = w.wallet_id
       GROUP BY w.wallet_id, w.total_deposited
      HAVING w.total_deposited <> COALESCE(sum(l.amount) FILTER (WHERE l.amount > 0), 0)
    ) x
  $q$,
  NULL,
  'total_deposited disagrees with summed positive ledger entries (denormalised counter drift).'
);


-- ═══════════════════════════════════════════════════════════════════════
-- SECRETS / CREDENTIALS
-- ═══════════════════════════════════════════════════════════════════════
--
-- Note: migration 171 (agent_credentials) is one of the migrations that
-- cannot be verified to have applied. If these come back SKIPPED, the
-- credential store is not installed on this database at all.

SELECT pg_temp.chk(
  'SEC-001', 'secrets', 'CRITICAL',
  ARRAY['agent_credentials.principal_id','users.user_id'],
  $q$
    SELECT count(*) FROM agent_credentials c
     WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = c.principal_id)
  $q$,
  $q$ SELECT string_agg(format('cred %s principal=%s provider=%s', credential_id, principal_id, provider), '; ')
        FROM (SELECT credential_id, principal_id, provider FROM agent_credentials c
               WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = c.principal_id)
               LIMIT 10) s $q$,
  'ORPHANED SECRETS — encrypted provider keys owned by a principal that no longer exists. These are live billable credentials with no accountable owner.'
);

SELECT pg_temp.chk(
  'SEC-002', 'secrets', 'HIGH',
  ARRAY['agent_credentials.scope','agents.agent_name'],
  $q$
    SELECT count(*) FROM agent_credentials c
     WHERE c.scope <> '*'
       AND NOT EXISTS (SELECT 1 FROM agents a WHERE a.agent_name = c.scope)
  $q$,
  $q$ SELECT string_agg(format('cred %s scope=%s', credential_id, scope), '; ')
        FROM (SELECT credential_id, scope FROM agent_credentials c
               WHERE c.scope <> '*' AND NOT EXISTS (SELECT 1 FROM agents a WHERE a.agent_name = c.scope)
               LIMIT 10) s $q$,
  'Agent-scoped credential naming an agent that does not exist — the scope can never match, so the credential silently never resolves.'
);

SELECT pg_temp.chk(
  'SEC-003', 'secrets', 'MEDIUM',
  ARRAY['secret_access_log'],
  $q$ SELECT 0::bigint $q$,
  NULL,
  'secret_access_log present (presence check only).'
);


-- ═══════════════════════════════════════════════════════════════════════
-- USERS, AGENTS, SHARING — referential orphans
-- ═══════════════════════════════════════════════════════════════════════
--
-- These are TEXT-keyed references. Many predate the v0.10.4 FK substrate
-- (migration 162+), so the database does not enforce them; the only thing
-- standing between you and an orphan is application code that, per the
-- audit, frequently swallows its own errors.

SELECT pg_temp.chk(
  'USER-001', 'users', 'HIGH',
  ARRAY['users.user_id'],
  $q$ SELECT count(*) FROM users WHERE user_id IS NULL OR btrim(user_id) = '' $q$,
  NULL,
  'Users with empty primary identifier. src/handlers/invites.rs:768 references a legacy empty-string user_id path.'
);

SELECT pg_temp.chk(
  'USER-002', 'users', 'MEDIUM',
  ARRAY['users.email'],
  $q$ SELECT count(*) FROM (
        SELECT lower(btrim(email)) e FROM users
         WHERE email IS NOT NULL AND btrim(email) <> ''
         GROUP BY 1 HAVING count(*) > 1) x $q$,
  $q$ SELECT string_agg(format('%s x%s', e, c), '; ') FROM (
        SELECT lower(btrim(email)) e, count(*) c FROM users
         WHERE email IS NOT NULL AND btrim(email) <> '' GROUP BY 1 HAVING count(*) > 1
         ORDER BY c DESC LIMIT 10) s $q$,
  'Duplicate emails — two identities for one human; credits and ownership split across them.'
);

-- users.id: referenced by fermi-auth/src/api_keys.rs:96 and by the
-- schema_trust contract, but created by NO migration in the repository.
-- If this reports SKIPPED, API-key auth is joining a non-existent column.
SELECT pg_temp.chk(
  'USER-003', 'users', 'CRITICAL',
  ARRAY['users.id'],
  $q$ SELECT count(*) FROM users WHERE id IS NULL $q$,
  NULL,
  'users.id exists here but is created by no migration — production-only artifact. SKIPPED means api_keys.rs:96 JOIN users u ON ak.user_id = u.id cannot resolve.'
);

SELECT pg_temp.chk(
  'AGENT-001', 'agents', 'HIGH',
  ARRAY['agents.user_id','users.user_id'],
  $q$
    SELECT count(*) FROM agents a
     WHERE a.user_id IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = a.user_id)
  $q$,
  $q$ SELECT string_agg(format('agent %s owner=%s', agent_name, user_id), '; ')
        FROM (SELECT agent_name, user_id FROM agents a
               WHERE a.user_id IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = a.user_id)
               LIMIT 10) s $q$,
  'Agents owned by a non-existent user. (NULL user_id is legitimate — curated/system agents.)'
);

SELECT pg_temp.chk(
  'AGENT-002', 'agents', 'MEDIUM',
  ARRAY['agent_versions.agent_id','agents.agent_id'],
  $q$ SELECT count(*) FROM agent_versions v
       WHERE NOT EXISTS (SELECT 1 FROM agents a WHERE a.agent_id = v.agent_id) $q$,
  NULL,
  'Version history for deleted agents. There is an ON DELETE CASCADE FK, so >0 means the FK is absent on this database.'
);

SELECT pg_temp.chk(
  'SHARE-001', 'sharing', 'HIGH',
  ARRAY['object_shares.share_type','object_shares.share_target','users.user_id'],
  $q$
    SELECT count(*) FROM object_shares s
     WHERE s.share_type = 'user'
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = s.share_target)
  $q$,
  NULL,
  'Shares granted to non-existent users. Dead ACL entries — noise in the visibility ladder.'
);

SELECT pg_temp.chk(
  'SHARE-002', 'sharing', 'HIGH',
  ARRAY['object_shares.share_type','object_shares.share_target','teams.id'],
  $q$
    SELECT count(*) FROM object_shares s
     WHERE s.share_type = 'team'
       AND NOT EXISTS (SELECT 1 FROM teams t WHERE t.id::text = s.share_target)
  $q$,
  NULL,
  'Shares granted to non-existent teams.'
);

SELECT pg_temp.chk(
  'TEAM-001', 'teams', 'HIGH',
  ARRAY['team_members.team_id','teams.id'],
  $q$ SELECT count(*) FROM team_members m
       WHERE NOT EXISTS (SELECT 1 FROM teams t WHERE t.id = m.team_id) $q$,
  NULL,
  'Memberships in deleted teams. These still grant access via the team-share branch of the ACL ladder.'
);

SELECT pg_temp.chk(
  'TEAM-002', 'teams', 'HIGH',
  ARRAY['team_members.member_type','team_members.member_id','users.user_id'],
  $q$
    SELECT count(*) FROM team_members m
     WHERE m.member_type = 'user'
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = m.member_id)
  $q$,
  NULL,
  'Team members that are not users.'
);

SELECT pg_temp.chk(
  'TEAM-003', 'teams', 'MEDIUM',
  ARRAY['teams.owner_id','users.user_id'],
  $q$ SELECT count(*) FROM teams t
       WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = t.owner_id) $q$,
  NULL,
  'Teams owned by non-existent users.'
);

SELECT pg_temp.chk(
  'FC-001', 'forecasts', 'HIGH',
  ARRAY['fermi_forecasts.owner_id','users.user_id'],
  $q$
    SELECT count(*) FROM fermi_forecasts f
     WHERE f.owner_id IS NOT NULL
       AND NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = f.owner_id)
  $q$,
  NULL,
  'Forecasts owned by non-existent users. Migration 165 was supposed to realign this FK and is in the unverifiable set.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- FORECAST / BRIER INTEGRITY
-- ═══════════════════════════════════════════════════════════════════════
--
-- Migration 174 installs the Brier-integrity trigger, which coerces edits to
-- resolved rows with RAISE WARNING (not EXCEPTION) — the UPDATE appears to
-- succeed and is silently discarded. Meanwhile
-- src/handlers/relationships/recompose.rs:160 issues an UPDATE with no
-- `status` filter. If both are live, resolved forecasts have been written to
-- and reverted, and the caller was told it worked.

SELECT pg_temp.chk(
  'FC-002', 'forecasts', 'HIGH',
  ARRAY['fermi_forecasts.status','fermi_forecasts.brier_score','fermi_forecasts.actual_outcome'],
  $q$
    SELECT count(*) FROM fermi_forecasts
     WHERE status = 'resolved' AND actual_outcome IS NOT NULL AND brier_score IS NULL
  $q$,
  NULL,
  'Resolved forecasts with no Brier score — the scoring path did not complete.'
);

SELECT pg_temp.chk(
  'FC-003', 'forecasts', 'HIGH',
  ARRAY['fermi_forecasts.brier_score','fermi_forecasts.scored_probability','fermi_forecasts.actual_outcome'],
  $q$
    SELECT count(*) FROM fermi_forecasts
     WHERE status = 'resolved'
       AND brier_score IS NOT NULL
       AND scored_probability IS NOT NULL
       AND actual_outcome IS NOT NULL
       AND abs(brier_score::numeric
               - power(scored_probability::numeric
                       - CASE WHEN actual_outcome THEN 1 ELSE 0 END, 2)) > 0.0001
  $q$,
  $q$ SELECT string_agg(format('forecast %s: brier=%s expected=%s', id, brier_score,
                               round(power(scored_probability::numeric - CASE WHEN actual_outcome THEN 1 ELSE 0 END, 2), 6)), '; ')
        FROM (SELECT id, brier_score, scored_probability, actual_outcome FROM fermi_forecasts
               WHERE status='resolved' AND brier_score IS NOT NULL AND scored_probability IS NOT NULL
                 AND actual_outcome IS NOT NULL
                 AND abs(brier_score::numeric - power(scored_probability::numeric - CASE WHEN actual_outcome THEN 1 ELSE 0 END, 2)) > 0.0001
               LIMIT 10) s $q$,
  'brier_score disagrees with (scored_probability - outcome)^2. Direct evidence of post-resolution mutation or a scoring bug.'
);

SELECT pg_temp.chk(
  'FC-004', 'forecasts', 'MEDIUM',
  ARRAY['fermi_forecasts.status','fermi_forecasts.resolved_at'],
  $q$ SELECT count(*) FROM fermi_forecasts WHERE status = 'resolved' AND resolved_at IS NULL $q$,
  NULL,
  'Resolved with no resolution timestamp — partial write of the resolve path.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- EMBEDDING PROVENANCE (Spec 22)
-- ═══════════════════════════════════════════════════════════════════════
--
-- Migration 136 turns this into a hard DB constraint and was NEVER WIRED into
-- run_migrations. The count below is exactly the number of rows that would
-- make applying it fail — i.e. the reconciliation backlog for that invariant.

SELECT pg_temp.chk(
  'PROV-001', 'provenance', 'HIGH',
  ARRAY['episodes.embedding','episodes.embedding_model_id'],
  $q$ SELECT count(*) FROM episodes WHERE embedding IS NOT NULL AND embedding_model_id IS NULL $q$,
  NULL,
  'Episodes with an embedding but no provenance. This is the blocker count for migration 136.'
);

SELECT pg_temp.chk(
  'PROV-002', 'provenance', 'MEDIUM',
  ARRAY['semantic_rules.embedding','semantic_rules.embedding_model_id'],
  $q$ SELECT count(*) FROM semantic_rules WHERE embedding IS NOT NULL AND embedding_model_id IS NULL $q$,
  NULL,
  'Semantic rules with an embedding but no provenance.'
);


-- ═══════════════════════════════════════════════════════════════════════
-- SCHEMA-OBJECT PRESENCE — did the unverifiable migrations actually apply?
-- ═══════════════════════════════════════════════════════════════════════
--
-- `run_migrations` swallows errors and there is no ledger, so the only way to
-- know whether a migration applied is to look for what it should have
-- created. Each of these corresponds to a migration that fails against an
-- empty database and therefore cannot be assumed present.

INSERT INTO integrity_findings(check_id, category, severity, status, violations, detail)
SELECT 'PRESENCE-' || upper(replace(obj, '.', '_')),
       'schema-presence',
       sev,
       CASE WHEN pg_temp.missing_objects(ARRAY[obj]) IS NULL THEN 'OK' ELSE 'VIOLATION' END,
       CASE WHEN pg_temp.missing_objects(ARRAY[obj]) IS NULL THEN 0 ELSE 1 END,
       note
  FROM (VALUES
    ('agent_credentials',                'CRITICAL', 'mig 171 — the credential store itself'),
    ('stripe_sessions_processed',        'CRITICAL', 'mig 182 — billing.rs fails closed without it, halting all credit purchases'),
    ('users.id',                         'CRITICAL', 'no migration creates it pre-181; api_keys.rs:96 JOINs on it'),
    ('users.password_hash',              'MEDIUM',   'mig 171 prerequisite; its absence is why 171 fails on a fresh DB'),
    -- The view migration 163 creates is named `rbac_orphans`. An earlier
    -- revision of this file checked for `rbac_orphans_view` — wrong name,
    -- right answer by luck, since neither exists. Verify your verifiers.
    ('rbac_orphans',                     'HIGH',     'mig 163 — your own orphan-detection view'),
    ('ar_beacons.location_name',         'HIGH',     'mig 089 (PostGIS) — its absence is why mig 163 aborts'),
    ('fermi_forecast_updates',           'HIGH',     'mig 094 — cascade root for 140/149/150/156/176'),
    ('forecast_spacetime',               'MEDIUM',   'mig 175'),
    ('composition_versions',             'MEDIUM',   'mig 113 (has a SQL syntax error on fresh apply)'),
    ('embedding_provenance',             'HIGH',     'mig 135 — the one DB-enforced append-only table'),
    ('agents.updated_at',                'MEDIUM',   'mig 166 — publish path depends on it'),
    ('credit_ledger.stripe_session_id',  'HIGH',     'mig 020 — required for Stripe idempotency'),
    ('api_keys',                         'HIGH',     'mig 005 — fails on fresh DB (users.id)')
  ) AS t(obj, sev, note);


-- ═══════════════════════════════════════════════════════════════════════
-- REPORT
-- ═══════════════════════════════════════════════════════════════════════

\echo ''
\echo '════════════════════════════════════════════════════════════════'
\echo ' FINDINGS — anything not OK'
\echo '════════════════════════════════════════════════════════════════'

SELECT check_id, category, severity, status, violations, detail
  FROM integrity_findings
 WHERE status <> 'OK'
 ORDER BY CASE severity WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1
                        WHEN 'MEDIUM' THEN 2 ELSE 3 END,
          CASE status WHEN 'VIOLATION' THEN 0 WHEN 'ERROR' THEN 1 ELSE 2 END,
          seq;

\echo ''
\echo '════════════════════════════════════════════════════════════════'
\echo ' PASSED'
\echo '════════════════════════════════════════════════════════════════'

SELECT check_id, category, severity FROM integrity_findings
 WHERE status = 'OK' ORDER BY seq;

\echo ''
\echo '════════════════════════════════════════════════════════════════'
\echo ' SUMMARY'
\echo '════════════════════════════════════════════════════════════════'

SELECT status,
       count(*)                                              AS checks,
       count(*) FILTER (WHERE severity = 'CRITICAL')          AS critical,
       count(*) FILTER (WHERE severity = 'HIGH')              AS high,
       COALESCE(sum(violations), 0)                           AS total_violating_rows
  FROM integrity_findings
 GROUP BY status
 ORDER BY CASE status WHEN 'VIOLATION' THEN 0 WHEN 'ERROR' THEN 1
                      WHEN 'SKIPPED' THEN 2 ELSE 3 END;

\echo ''
\echo 'Note: SKIPPED means the check could not run because an object is absent.'
\echo 'Treat SKIPPED as UNKNOWN, never as passing — a check that cannot fail is'
\echo 'not a check. (That is the exact defect that made SCHEMA_STRICT=1'
\echo 'un-enablable for eight releases.)'
\echo ''

-- ── Release gate ──────────────────────────────────────────────────────
-- Raises, which with ON_ERROR_STOP makes psql exit non-zero. Everything
-- above has already been printed, so the report survives the failure.
DO $gate$
DECLARE
    crit    int;
    missing int;
    errored int;
BEGIN
    SELECT count(*) INTO crit
      FROM integrity_findings WHERE status = 'VIOLATION' AND severity = 'CRITICAL';
    SELECT count(*) INTO missing
      FROM integrity_findings WHERE category = 'schema-presence' AND status = 'VIOLATION';
    SELECT count(*) INTO errored
      FROM integrity_findings WHERE status = 'ERROR';

    IF crit > 0 OR missing > 0 OR errored > 0 THEN
        RAISE EXCEPTION
            'INTEGRITY GATE FAILED — % critical violation(s), % missing schema object(s), % check error(s). Not releasable.',
            crit, missing, errored;
    END IF;

    RAISE NOTICE 'INTEGRITY GATE PASSED — no critical violations, no missing schema objects.';
END
$gate$;
