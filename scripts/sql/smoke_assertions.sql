-- ═══════════════════════════════════════════════════════════════════
-- Assertions for scripts/smoke_economics.sh
--
-- The three economics queries are loaded as PREPARE'd statements by the
-- runner (which is itself a check: PREPARE fails if the SQL doesn't
-- parse or the $n types don't resolve). Here we EXECUTE them and assert
-- exact values against scripts/sql/smoke_fixture_data.sql.
--
-- Any mismatch RAISEs, which makes psql exit non-zero.
-- ═══════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on

-- ─── 1. by_principal, 30-day window, no filter ──────────────────────
CREATE TEMP TABLE r_principal AS EXECUTE by_principal('30', NULL);

DO $$
DECLARE r RECORD; n INT;
BEGIN
    SELECT COUNT(*) INTO n FROM r_principal;
    IF n <> 3 THEN
        RAISE EXCEPTION 'by_principal: expected 3 principals, got %', n;
    END IF;

    -- abw-system: 2 in-window episodes (opus $15 + sonnet $3), the
    -- 400-day-old one excluded.
    SELECT * INTO r FROM r_principal WHERE funding_principal = 'abw-system';
    IF r.executions <> 2 THEN
        RAISE EXCEPTION 'abw-system executions: expected 2, got % (400-day-old episode leaking in?)', r.executions;
    END IF;
    IF r.tokens <> 2000000 THEN
        RAISE EXCEPTION 'abw-system tokens: expected 2000000, got %', r.tokens;
    END IF;
    IF r.cost_usd <> 18.0 THEN
        RAISE EXCEPTION 'abw-system cost: expected 18.00, got %', r.cost_usd;
    END IF;

    SELECT * INTO r FROM r_principal WHERE funding_principal = 'mario';
    IF r.cost_usd <> 0.25 THEN
        RAISE EXCEPTION 'mario cost: expected 0.25, got %', r.cost_usd;
    END IF;

    -- The pre-SPEC_28 episode: bucketed, not dropped; NULLs coalesced;
    -- and counted as missing cost so the gap is visible.
    SELECT * INTO r FROM r_principal WHERE funding_principal = 'unattributed';
    IF r.executions <> 1 THEN
        RAISE EXCEPTION 'unattributed executions: expected 1, got %', r.executions;
    END IF;
    IF r.cost_usd <> 0 OR r.tokens <> 0 THEN
        RAISE EXCEPTION 'unattributed NULLs not coalesced: cost=% tokens=%', r.cost_usd, r.tokens;
    END IF;
    IF r.missing_cost <> 1 THEN
        RAISE EXCEPTION 'unattributed missing_cost: expected 1, got %', r.missing_cost;
    END IF;

    RAISE NOTICE '  ok  by_principal: 3 buckets, abw-system $18.00 / 2 runs, unattributed surfaced';
END $$;

-- ─── 2. by_principal with the principal filter ──────────────────────
CREATE TEMP TABLE r_filtered AS EXECUTE by_principal('30', 'abw-system');

DO $$
DECLARE n INT;
BEGIN
    SELECT COUNT(*) INTO n FROM r_filtered;
    IF n <> 1 THEN
        RAISE EXCEPTION 'filtered by_principal: expected 1 row, got %', n;
    END IF;
    RAISE NOTICE '  ok  by_principal filter isolates a single principal';
END $$;

-- ─── 3. by_agent, joined to revenue ─────────────────────────────────
CREATE TEMP TABLE r_agent AS EXECUTE by_agent('30', NULL);

DO $$
DECLARE r RECORD; n INT;
BEGIN
    -- xaman_ek appears TWICE: once funded by abw-system, once
    -- 'unattributed' (the empty-context episode). That is the grouping
    -- by funding_principal doing its job.
    SELECT COUNT(*) INTO n FROM r_agent;
    IF n <> 4 THEN
        RAISE EXCEPTION 'by_agent: expected 4 (agent, principal) groups, got %', n;
    END IF;

    SELECT * INTO r FROM r_agent
     WHERE agent_name = 'xaman_ek' AND funding_principal = 'abw-system';
    IF r.cost_usd <> 15.0 THEN
        RAISE EXCEPTION 'xaman_ek cost: expected 15.00, got %', r.cost_usd;
    END IF;
    -- Sign convention: ledger stores -50, query must report +50.
    -- Also guards the fee CTE's window filter: a backdated -777 fee
    -- points at this same (in-window) episode, so if that filter is
    -- dropped this reads 827 instead of 50.
    IF r.fee_credits <> 50 THEN
        RAISE EXCEPTION 'xaman_ek revenue: expected 50, got % (ledger sign flipped, or fee window filter dropped?)', r.fee_credits;
    END IF;
    IF r.model <> 'claude-opus-4-6' THEN
        RAISE EXCEPTION 'xaman_ek model: expected claude-opus-4-6, got %', r.model;
    END IF;

    SELECT * INTO r FROM r_agent WHERE agent_name = 'cohere_and_coordinate';
    IF r.fee_credits <> 100 THEN
        RAISE EXCEPTION 'cohere revenue: expected 100, got %', r.fee_credits;
    END IF;
    IF r.tier <> 'curated' OR r.owner_id <> 'ivan' THEN
        RAISE EXCEPTION 'cohere tier/owner wrong: % / %', r.tier, r.owner_id;
    END IF;

    -- No fee row for this episode: LEFT JOIN must yield 0, not drop it.
    SELECT * INTO r FROM r_agent WHERE agent_name = 'football_analyst';
    IF r.fee_credits <> 0 THEN
        RAISE EXCEPTION 'football_analyst revenue: expected 0, got %', r.fee_credits;
    END IF;
    IF r.executions <> 1 THEN
        RAISE EXCEPTION 'football_analyst dropped by LEFT JOIN: executions=%', r.executions;
    END IF;

    -- The out-of-window -999 fee must not appear anywhere.
    SELECT COALESCE(SUM(fee_credits), 0) INTO n FROM r_agent;
    IF n <> 150 THEN
        RAISE EXCEPTION 'total revenue: expected 150, got % (out-of-window fee leaking?)', n;
    END IF;

    RAISE NOTICE '  ok  by_agent: 4 groups, revenue 150cr, sign + window + LEFT JOIN correct';
END $$;

-- ─── 4. royalties ───────────────────────────────────────────────────
CREATE TEMP TABLE r_royalty AS EXECUTE royalties('30');

DO $$
DECLARE r RECORD; n INT;
BEGIN
    SELECT COUNT(*) INTO n FROM r_royalty;
    IF n <> 1 THEN
        RAISE EXCEPTION 'royalties: expected 1 recipient, got %', n;
    END IF;
    SELECT * INTO r FROM r_royalty;
    IF r.owner_id <> 'ivan' OR r.royalty_credits <> 85 THEN
        RAISE EXCEPTION 'royalties: expected ivan/85, got %/%', r.owner_id, r.royalty_credits;
    END IF;
    RAISE NOTICE '  ok  royalties: ivan 85cr';
END $$;

-- ─── 5. Migration 189 — impersonation audit substrate ───────────────
DO $$
DECLARE sid UUID; n INT;
BEGIN
    -- Tables and the partial index exist.
    IF to_regclass('public.impersonation_sessions') IS NULL THEN
        RAISE EXCEPTION 'mig189: impersonation_sessions missing';
    END IF;
    IF to_regclass('public.impersonation_events') IS NULL THEN
        RAISE EXCEPTION 'mig189: impersonation_events missing';
    END IF;

    -- Happy path: the insert the mint handler performs, including the
    -- `NOW() + (text || ' seconds')::interval` expression.
    INSERT INTO impersonation_sessions
        (admin_user_id, target_user_id, reason, mode, expires_at)
    VALUES ('ivan', 'mario', 'debugging a reported 404 on forecasts',
            'read_only', NOW() + ('1800' || ' seconds')::interval)
    RETURNING session_id INTO sid;

    -- The liveness query the guard middleware runs on every request.
    IF NOT EXISTS (SELECT 1 FROM impersonation_sessions
                    WHERE session_id = sid AND ended_at IS NULL AND expires_at > NOW()) THEN
        RAISE EXCEPTION 'mig189: freshly-minted session is not live';
    END IF;

    INSERT INTO impersonation_events (session_id, method, path, status, blocked, block_reason)
    VALUES (sid, 'GET',  '/api/forecasts', 200, FALSE, NULL),
           (sid, 'POST', '/api/forecasts', NULL, TRUE, 'mutation_in_read_only');

    SELECT COUNT(*) INTO n FROM impersonation_events WHERE session_id = sid AND blocked;
    IF n <> 1 THEN
        RAISE EXCEPTION 'mig189: expected 1 blocked event, got %', n;
    END IF;

    -- Ending it must make the liveness check fail.
    UPDATE impersonation_sessions
       SET ended_at = NOW(), end_reason = 'exited' WHERE session_id = sid;
    IF EXISTS (SELECT 1 FROM impersonation_sessions
                WHERE session_id = sid AND ended_at IS NULL AND expires_at > NOW()) THEN
        RAISE EXCEPTION 'mig189: ended session still reads as live — revocation broken';
    END IF;

    RAISE NOTICE '  ok  mig189: mint → live → log → end → revoked';
END $$;

-- Self-impersonation must be rejected by the CHECK constraint.
DO $$
BEGIN
    BEGIN
        INSERT INTO impersonation_sessions
            (admin_user_id, target_user_id, reason, expires_at)
        VALUES ('ivan', 'ivan', 'should not be allowed at all', NOW() + INTERVAL '1 hour');
        RAISE EXCEPTION 'mig189: self-impersonation was ACCEPTED (CHECK missing)';
    EXCEPTION WHEN check_violation THEN
        RAISE NOTICE '  ok  mig189: self-impersonation rejected by CHECK';
    END;
END $$;

-- Unknown modes must be rejected too.
DO $$
BEGIN
    BEGIN
        INSERT INTO impersonation_sessions
            (admin_user_id, target_user_id, reason, mode, expires_at)
        VALUES ('ivan', 'mario', 'mode should be constrained', 'root', NOW() + INTERVAL '1 hour');
        RAISE EXCEPTION 'mig189: mode=root was ACCEPTED (CHECK missing)';
    EXCEPTION WHEN check_violation THEN
        RAISE NOTICE '  ok  mig189: unknown mode rejected by CHECK';
    END;
END $$;

-- Cascade: deleting the session must take its events with it.
DO $$
DECLARE sid UUID; n INT;
BEGIN
    INSERT INTO impersonation_sessions
        (admin_user_id, target_user_id, reason, expires_at)
    VALUES ('ivan', 'mario', 'cascade behaviour check', NOW() + INTERVAL '1 hour')
    RETURNING session_id INTO sid;
    INSERT INTO impersonation_events (session_id, method, path) VALUES (sid, 'GET', '/x');
    DELETE FROM impersonation_sessions WHERE session_id = sid;
    SELECT COUNT(*) INTO n FROM impersonation_events WHERE session_id = sid;
    IF n <> 0 THEN
        RAISE EXCEPTION 'mig189: % orphaned event(s) after session delete', n;
    END IF;
    RAISE NOTICE '  ok  mig189: events cascade with the session';
END $$;
