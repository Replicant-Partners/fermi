-- Migration 182: Make Stripe checkout idempotency structural
--
-- THE BUG (src/handlers/billing.rs:255-267, pre-v0.11.9)
--
--     let existing = sqlx::query("SELECT tx_id FROM credit_ledger
--                                  WHERE stripe_session_id = $1 LIMIT 1")…
--     if let Ok(Some(_)) = existing { return; }
--
-- Two independent ways to double-credit real money:
--
--   1. `if let Ok(Some(_))` treats a DATABASE ERROR as "not yet processed".
--      A transient blip on that SELECT falls through and credits again.
--
--   2. The marker that makes the check work is written AFTER the deposit,
--      in a separate `let _ = UPDATE credit_ledger SET stripe_session_id…`
--      whose failure is swallowed. If it fails, the session looks unprocessed
--      forever and Stripe's retry credits again.
--
-- Read-then-write is not idempotency; it is a race with money in it. The
-- 2026-08-06 audit confirmed it has not fired yet (CREDIT-003 = 0), which is
-- luck, not design.
--
-- THE FIX
--
-- A claim table with the session id as PRIMARY KEY. The handler INSERTs a
-- claim BEFORE crediting; the database — not application logic — decides who
-- owns the session. Concurrent webhooks cannot both win, and a failed marker
-- write can no longer resurrect a processed session.
--
-- Single DO block => one statement => PgBouncer-safe, idempotent.

DO $$
DECLARE
    v_backfilled integer;
    v_dupes      integer;
BEGIN
    CREATE TABLE IF NOT EXISTS public.stripe_sessions_processed (
        session_id  TEXT PRIMARY KEY,
        user_id     TEXT,
        credits     INTEGER,
        tx_id       UUID,
        claimed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
        settled_at  TIMESTAMPTZ
    );

    COMMENT ON TABLE public.stripe_sessions_processed IS
        'Idempotency claims for Stripe checkout.session.completed webhooks. '
        'INSERT the claim BEFORE crediting; PK conflict means another delivery '
        'already owns this session. settled_at NULL = claimed but the deposit '
        'did not complete — safe to investigate and retry.';

    -- Backfill from history so sessions already credited are never re-credited
    -- once the handler switches to consulting this table.
    INSERT INTO public.stripe_sessions_processed (session_id, credits, tx_id, claimed_at, settled_at)
    SELECT DISTINCT ON (l.stripe_session_id)
           l.stripe_session_id, l.amount, l.tx_id, l.created_at, l.created_at
      FROM public.credit_ledger l
     WHERE l.stripe_session_id IS NOT NULL
       AND l.stripe_session_id <> ''
     ORDER BY l.stripe_session_id, l.created_at
    ON CONFLICT (session_id) DO NOTHING;

    GET DIAGNOSTICS v_backfilled = ROW_COUNT;
    IF v_backfilled > 0 THEN
        RAISE NOTICE '[mig 182] backfilled % processed Stripe session(s)', v_backfilled;
    END IF;

    -- Defence in depth: even if the handler regresses, the ledger itself
    -- refuses a second row for the same session.
    --
    -- Only create it if the data permits. The audit measured zero duplicates
    -- (CREDIT-003), so this succeeds today — but a future operator running
    -- this against a dirty database should get a warning, not a failed boot
    -- that the runner silently swallows.
    SELECT count(*) INTO v_dupes FROM (
        SELECT stripe_session_id FROM public.credit_ledger
         WHERE stripe_session_id IS NOT NULL AND stripe_session_id <> ''
         GROUP BY stripe_session_id HAVING count(*) > 1
    ) d;

    IF v_dupes > 0 THEN
        RAISE WARNING '[mig 182] % duplicate stripe_session_id value(s) — unique index NOT created. Reconcile first.', v_dupes;
    ELSE
        CREATE UNIQUE INDEX IF NOT EXISTS uq_credit_ledger_stripe_session
            ON public.credit_ledger (stripe_session_id)
         WHERE stripe_session_id IS NOT NULL AND stripe_session_id <> '';
    END IF;
END $$;
