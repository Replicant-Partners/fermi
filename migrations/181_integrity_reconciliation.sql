-- Migration 181: Integrity reconciliation (audit of 2026-08-06)
--
-- Repairs the four structural findings from scripts/integrity_audit.sql.
-- See docs/SCHEMA_AND_RULE_INTEGRITY_RECONCILIATION.md Part 8.
--
-- Deliberately NOT included here: deletion of the 13 orphaned ACL test rows.
-- Destroying production rows should be an explicit, once-off, reviewed act —
-- not something that re-executes on every boot. That lives in
-- scripts/cleanup_test_orphans.sql.
--
-- Single DO block => one statement => PgBouncer-safe. Fully idempotent:
-- run_migrations re-executes every file on every boot, so each step is
-- guarded and a second run is a no-op.

DO $$
DECLARE
    v_wallet    CONSTANT uuid := 'a2ea920a-2123-4a68-91de-0837a0d87151';
    v_marker    CONSTANT text := 'mig-181 reconciliation: reconstruct ledger row lost to unlogged write';
    v_rows      integer;
BEGIN
    -- ═══════════════════════════════════════════════════════════════
    -- 1. Seed the abw-system principal
    -- ═══════════════════════════════════════════════════════════════
    --
    -- FINDING (SEC-001, CRITICAL): agent_credentials holds the platform's
    -- Anthropic and OpenAI keys under principal_id = 'abw-system', but no
    -- such row exists in users. Every join from a credential to its owning
    -- principal — budget attribution, RBAC, admin listing — silently
    -- returns nothing.
    --
    -- ROOT CAUSE (found 2026-08-06 when the first draft of this migration
    -- hit `duplicate key value violates unique constraint users_email_key`):
    --
    -- The row was never deleted. Its IDENTIFIER was overwritten.
    --
    --   * mig 171 inserts abw-system with auth_provider deliberately NULL
    --     (its comment: "left NULL to avoid CHECK-constraint drift").
    --   * mig 004b runs `UPDATE users SET auth_provider='legacy',
    --     user_id = id::text, display_name = name WHERE auth_provider IS NULL`.
    --   * run_migrations re-executes EVERY file on EVERY boot. So on the
    --     next boot 004b matched that row and rewrote user_id from
    --     'abw-system' to its UUID.
    --
    -- Confirmed in production: email=system@abw.local now carries
    -- user_id = id = fe81c651-fef1-457d-bcd4-07196d270d4e, auth_provider
    -- = 'legacy'. Nothing references it (verified across every owner
    -- column), so it is inert — and the credentials pointing at
    -- 'abw-system' resolve to nobody.
    --
    -- Worse: since then, mig 171 has FAILED ON EVERY BOOT. It sees no
    -- 'abw-system' user, tries to INSERT, and hits the email unique
    -- constraint — aborting its whole DO block. run_migrations swallows
    -- the error. Restoring the identifier below also repairs 171.
    --
    -- The `auth_provider` write is the load-bearing part: leaving it NULL
    -- is what let 004b clobber the row, and would let it happen again on
    -- the next boot.
    --
    -- Column set verified against production 2026-08-06:
    --   NOT NULL without default: email, password_hash, password_salt
    --   UNIQUE: users_email_key, users_user_id_unique
    --   CHECK users_role_check: role IN ('admin','developer','viewer')
    IF NOT EXISTS (SELECT 1 FROM users WHERE user_id = 'abw-system') THEN

        IF EXISTS (SELECT 1 FROM users WHERE email = 'system@abw.local') THEN
            -- Restore the clobbered identifier rather than inserting a
            -- duplicate. Safe: no row anywhere references the UUID form.
            UPDATE users
               SET user_id       = 'abw-system',
                   auth_provider = COALESCE(auth_provider, 'legacy'),
                   role          = COALESCE(role, 'admin'),
                   display_name  = COALESCE(NULLIF(display_name, ''), 'ABW System')
             WHERE email = 'system@abw.local';
            RAISE NOTICE '[mig 181] restored abw-system user_id (was clobbered by mig 004b)';
        ELSE
            -- Fresh database. Set auth_provider explicitly so 004b never
            -- matches this row on a later boot.
            INSERT INTO users (user_id, email, password_hash, password_salt,
                               role, display_name, auth_provider)
            VALUES ('abw-system', 'system@abw.local', '', '', 'admin',
                    'ABW System', 'legacy');
            RAISE NOTICE '[mig 181] seeded abw-system principal into users';
        END IF;
    END IF;

    -- Immunise against 004b regardless of which path ran above, including
    -- the case where mig 171 recreated the row with a NULL auth_provider
    -- earlier in this same boot.
    UPDATE users SET auth_provider = 'legacy'
     WHERE user_id = 'abw-system' AND auth_provider IS NULL;

    -- Landmine check. 004b is a destructive UPDATE that re-runs forever;
    -- any row with a NULL auth_provider silently has its primary key
    -- rewritten on the next boot. Currently zero in production, but the
    -- mechanism is still armed until migrations stop re-running (Phase 2.1).
    SELECT count(*) INTO v_rows FROM users WHERE auth_provider IS NULL;
    IF v_rows > 0 THEN
        RAISE WARNING '[mig 181] % user(s) have auth_provider IS NULL — mig 004b will rewrite their user_id on the next boot', v_rows;
    END IF;

    -- ═══════════════════════════════════════════════════════════════
    -- 2. Declare users.id
    -- ═══════════════════════════════════════════════════════════════
    --
    -- FINDING: users.id exists in production (uuid NOT NULL DEFAULT
    -- gen_random_uuid()) and is load-bearing — fermi-auth/src/api_keys.rs:96
    -- and :166 authenticate via `JOIN users u ON ak.user_id = u.id`. But NO
    -- migration creates it. Migration 004 declares user_id as the sole
    -- primary key and no later file adds id.
    --
    -- The only explanation is that migration 004 was edited in place after
    -- it had already been applied. Consequence: a database rebuilt from
    -- migrations has no users.id, so API-key authentication breaks — and
    -- migrations 004b/161/165 abort against a fresh database for the same
    -- reason.
    --
    -- This is a no-op in production and the thing that makes a rebuild
    -- faithful. It is the single most important line in this file.
    ALTER TABLE public.users
        ADD COLUMN IF NOT EXISTS id uuid NOT NULL DEFAULT gen_random_uuid();

    -- ═══════════════════════════════════════════════════════════════
    -- 3. Unblock migration 163 (rbac_orphans view)
    -- ═══════════════════════════════════════════════════════════════
    --
    -- FINDING: the rbac_orphans view does not exist in production. Migration
    -- 163 aborts at its ar_beacons block on `column "location_name" does not
    -- exist`, because migration 089 — which adds the spatial columns — fails
    -- first on `PostGIS extension not found`.
    --
    -- So your own cross-table orphan detector has never existed. The audit
    -- had to reimplement it.
    --
    -- Fix the cause rather than the symptom, and do NOT edit 163 in place:
    -- editing an already-applied migration is precisely what produced the
    -- users.id ghost above. Supplying the missing column here lets 163
    -- succeed unchanged on the next boot.
    IF to_regclass('public.ar_beacons') IS NOT NULL THEN
        ALTER TABLE public.ar_beacons ADD COLUMN IF NOT EXISTS location_name TEXT;
    END IF;

    -- ═══════════════════════════════════════════════════════════════
    -- 4. Reconstruct the lost credit_ledger row
    -- ═══════════════════════════════════════════════════════════════
    --
    -- FINDING (CREDIT-001 CRITICAL / CREDIT-005 HIGH): workspace wallet
    -- a2ea920a has balance 1015 but its ledger sums to 1016.
    --
    -- Order-independent chain analysis found exactly ONE break, not the 154
    -- the naive ordering suggested: tx 7219b9cd (file_write, -1,
    -- balance_after 1233) implies a predecessor at balance_after 1234, and
    -- no such row exists. Every wallet's newest ledger row matches its
    -- balance, so the wallet is authoritative and correct.
    --
    -- Diagnosis: a 1-credit debit was applied to wallets.balance and its
    -- ledger INSERT was lost — the `let _ = sqlx::query(...)` signature.
    -- No credits were miscounted against the user; the audit trail has a
    -- hole. This is the benign version of that bug, and proof it is live.
    --
    -- We know the missing row exactly: amount -1, balance_after 1234. We
    -- post an adjusting entry rather than rewriting history — standard
    -- ledger practice, and it keeps the row identifiable as reconstructed.
    -- Timestamp is 1µs before its successor to preserve ordering.
    IF EXISTS (SELECT 1 FROM wallets WHERE wallet_id = v_wallet)
       AND NOT EXISTS (
           SELECT 1 FROM credit_ledger
            WHERE wallet_id = v_wallet AND description = v_marker
       )
       -- Only if the gap is still there. If someone already reconciled it,
       -- or the wallet has moved on, do nothing.
       AND NOT EXISTS (
           SELECT 1 FROM credit_ledger WHERE wallet_id = v_wallet AND balance_after = 1234
       )
    THEN
        INSERT INTO credit_ledger (wallet_id, amount, balance_after, tx_type, description, created_at)
        VALUES (v_wallet, -1, 1234, 'reconciliation', v_marker,
                TIMESTAMPTZ '2026-05-31 12:49:23.275020+00');
        RAISE NOTICE '[mig 181] reconstructed 1 lost credit_ledger row for wallet %', v_wallet;
    END IF;

    -- ═══════════════════════════════════════════════════════════════
    -- Report
    -- ═══════════════════════════════════════════════════════════════
    SELECT count(*) INTO v_rows
      FROM wallets w
      LEFT JOIN credit_ledger l ON l.wallet_id = w.wallet_id
     GROUP BY w.wallet_id, w.balance
    HAVING w.balance <> COALESCE(sum(l.amount), 0);

    IF COALESCE(v_rows, 0) > 0 THEN
        RAISE WARNING '[mig 181] % wallet(s) still diverge from their ledger', v_rows;
    ELSE
        RAISE NOTICE '[mig 181] credit conservation holds across all wallets';
    END IF;
END $$;
