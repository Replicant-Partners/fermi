-- Assertions for migrations 194 + 195. Run AFTER both are applied to a
-- throwaway database seeded by cost_attribution_fixture.sql.
--
-- Every check RAISEs on failure, so a non-zero psql exit means a real
-- regression. The load-bearing one is DEDUP-001: the claim table fans out one
-- row per driver, and a naive join multiplies an execution's cost by its driver
-- count. That error inflates precisely the broad-coverage agents that cost most,
-- so it would bias the marketplace toward the wrong conclusion while looking
-- entirely plausible.

\set ON_ERROR_STOP on

-- ─── Seed ────────────────────────────────────────────────────────────────────

INSERT INTO public.agents (agent_id, agent_name, tier) VALUES
    ('11111111-1111-1111-1111-111111111111', 'efra_critical_factor', 'community'),
    ('22222222-2222-2222-2222-222222222222', 'macro_data_agent',     'curated');

INSERT INTO public.fermi_forecasts
    (id, question_text, status, brier_score, resolved_at, workspace_id) VALUES
    ('fc_resolved', 'Will X happen?', 'resolved', 0.04, NOW(),
     'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'),
    ('fc_open',     'Will Y happen?', 'active',   NULL, NULL,
     'bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb');

-- ONE execution costing exactly $0.090000, on a trustworthy basis.
INSERT INTO public.episodes
    (episode_id, agent_id, tokens_used, cost_usd, cost_basis, cost_rate_key,
     input_tokens, output_tokens, provider_used, model_used, context)
VALUES
    ('e0000001-0000-0000-0000-000000000001',
     '11111111-1111-1111-1111-111111111111',
     205424, 0.090000, 'measured_split', 'deepseek/deepseek-chat',
     164339, 41085, 'deepseek', 'deepseek-chat',
     '{"invocation":{"route_reason":"domain_specialist","driver":"socio"}}'::jsonb);

-- THREE claims from that ONE execution — the driver fan-out.
INSERT INTO public.forecast_agent_claims
    (workspace_id, agent_id, agent_name, driver, p50, episode_id) VALUES
    ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
     'efra_critical_factor', 'socio',         1.1, 'e0000001-0000-0000-0000-000000000001'),
    ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
     'efra_critical_factor', 'institutional', 1.2, 'e0000001-0000-0000-0000-000000000001'),
    ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
     'efra_critical_factor', 'dynamic',       0.9, 'e0000001-0000-0000-0000-000000000001');

-- An execution on an UNKNOWN model: real spend, untrustworthy figure.
INSERT INTO public.episodes
    (episode_id, agent_id, tokens_used, cost_usd, cost_basis, provider_used, model_used)
VALUES
    ('e0000002-0000-0000-0000-000000000002',
     '22222222-2222-2222-2222-222222222222',
     50000, 0.250000, 'unknown_model', 'someprovider', 'some-new-model');

INSERT INTO public.forecast_agent_claims
    (workspace_id, agent_id, agent_name, driver, p50, episode_id) VALUES
    ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '22222222-2222-2222-2222-222222222222',
     'macro_data_agent', 'fixture', 1.0, 'e0000002-0000-0000-0000-000000000002');

-- A pre-195 claim: NULL episode_id. Real spend that cannot be located.
INSERT INTO public.forecast_agent_claims
    (workspace_id, agent_id, agent_name, driver, p50, episode_id) VALUES
    ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '22222222-2222-2222-2222-222222222222',
     'macro_data_agent', 'legacy', 1.0, NULL);

-- ─── DEDUP-001: an execution is costed ONCE, not once per driver ─────────────

DO $$
DECLARE
    v_attributed  NUMERIC;
    v_executions  BIGINT;
BEGIN
    SELECT attributed_cost_usd, executions
      INTO v_attributed, v_executions
      FROM public.forecast_cost_attribution
     WHERE forecast_id = 'fc_resolved';

    -- $0.09 counted once. A naive join would give $0.27 (3 drivers).
    IF v_attributed IS DISTINCT FROM 0.090000 THEN
        RAISE EXCEPTION
            'DEDUP-001 FAILED: attributed_cost_usd = %, expected 0.090000. '
            'A value of 0.270000 means the one-claim-per-driver fan-out is '
            'multiplying execution cost by driver count.', v_attributed;
    END IF;

    -- 2 distinct executions (one trustworthy, one unknown_model), not 4 claims.
    IF v_executions IS DISTINCT FROM 2 THEN
        RAISE EXCEPTION
            'DEDUP-001 FAILED: executions = %, expected 2 distinct episodes '
            '(4 claim rows exist; the view must not count claims).', v_executions;
    END IF;

    RAISE NOTICE 'DEDUP-001 ok: one execution costed once across 3 drivers';
END $$;

-- ─── BASIS-001: untrustworthy cost is reported, never silently included ──────

DO $$
DECLARE
    v_attr NUMERIC; v_unattr NUMERIC; v_untrusted BIGINT; v_unlinked BIGINT;
BEGIN
    SELECT attributed_cost_usd, unattributed_cost_usd,
           untrusted_cost_rows, unlinked_claims
      INTO v_attr, v_unattr, v_untrusted, v_unlinked
      FROM public.forecast_cost_attribution
     WHERE forecast_id = 'fc_resolved';

    IF v_unattr IS DISTINCT FROM 0.250000 THEN
        RAISE EXCEPTION
            'BASIS-001 FAILED: unattributed_cost_usd = %, expected 0.250000 '
            '(the unknown_model run). Untrustworthy spend must be reported, '
            'not folded into the attributed total nor dropped.', v_unattr;
    END IF;

    IF v_untrusted IS DISTINCT FROM 1 THEN
        RAISE EXCEPTION 'BASIS-001 FAILED: untrusted_cost_rows = %, expected 1', v_untrusted;
    END IF;

    -- The pre-195 claim is counted as unlocatable spend.
    IF v_unlinked IS DISTINCT FROM 1 THEN
        RAISE EXCEPTION
            'BASIS-001 FAILED: unlinked_claims = %, expected 1 (the NULL '
            'episode_id claim). Pre-195 spend must be visible as missing.', v_unlinked;
    END IF;

    RAISE NOTICE 'BASIS-001 ok: trusted % / untrusted % / unlinked % kept separate',
        v_attr, v_unattr, v_unlinked;
END $$;

-- ─── BRIER-001: usd_per_brier_point uses attributed spend only ───────────────

DO $$
DECLARE
    v_metric NUMERIC; v_open NUMERIC;
BEGIN
    SELECT usd_per_brier_point INTO v_metric
      FROM public.forecast_cost_attribution WHERE forecast_id = 'fc_resolved';

    -- 0.09 / 0.04 = 2.25. Must NOT be (0.09+0.25)/0.04 = 8.5.
    IF round(v_metric, 4) IS DISTINCT FROM 2.2500 THEN
        RAISE EXCEPTION
            'BRIER-001 FAILED: usd_per_brier_point = %, expected 2.2500 '
            '(attributed 0.09 / brier 0.04). 8.5 means untrustworthy spend '
            'leaked into the cost-effectiveness metric.', v_metric;
    END IF;

    -- Unresolved forecasts have no Brier, so no metric. Must be NULL, not 0.
    SELECT usd_per_brier_point INTO v_open
      FROM public.forecast_cost_attribution WHERE forecast_id = 'fc_open';
    IF v_open IS NOT NULL THEN
        RAISE EXCEPTION
            'BRIER-001 FAILED: unresolved forecast returned %, expected NULL', v_open;
    END IF;

    RAISE NOTICE 'BRIER-001 ok: usd_per_brier_point = % from attributed spend only', v_metric;
END $$;

-- ─── JOIN-001: route_outcomes reports how each row was joined ────────────────

DO $$
DECLARE
    v_exact BIGINT;
BEGIN
    SELECT COUNT(*) INTO v_exact
      FROM public.route_outcomes
     WHERE episode_id = 'e0000001-0000-0000-0000-000000000001'
       AND join_method = 'exact';

    -- 3 claims share this episode, so 3 exact rows — correct at claim grain.
    IF v_exact IS DISTINCT FROM 3 THEN
        RAISE EXCEPTION
            'JOIN-001 FAILED: % exact-joined rows, expected 3 (one per driver). '
            'route_outcomes is at CLAIM grain by design; only '
            'forecast_cost_attribution de-duplicates to execution grain.', v_exact;
    END IF;

    RAISE NOTICE 'JOIN-001 ok: exact join reported via join_method';
END $$;

-- ─── CHECK-001: the cost_basis vocabulary is enforced ───────────────────────

DO $$
BEGIN
    BEGIN
        INSERT INTO public.episodes (agent_id, cost_basis)
        VALUES ('11111111-1111-1111-1111-111111111111', 'not_a_real_basis');
        RAISE EXCEPTION
            'CHECK-001 FAILED: an invalid cost_basis was accepted. A typo''d '
            'basis reads as untrustworthy in every consumer, silently excluding '
            'the row from cost analysis instead of failing at write time.';
    EXCEPTION WHEN check_violation THEN
        RAISE NOTICE 'CHECK-001 ok: invalid cost_basis rejected';
    END;
END $$;

SELECT 'ALL COST-ATTRIBUTION ASSERTIONS PASSED' AS result;
