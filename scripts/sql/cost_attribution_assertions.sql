-- Assertions for migrations 194 + 197. Run AFTER both are applied to a
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

-- TWO DELEGATED CHILDREN of the claimed execution (mig-198). Neither carries a
-- claim of its own -- sub-agents do research, they rarely emit a multiplier --
-- so they can reach the forecast ONLY by descent from their parent. Before
-- mig-198 these rows did not exist at all: the delegation tools discarded the
-- child's AgentOutput, so a compound agent under-reported by its whole fan-out.
INSERT INTO public.episodes
    (episode_id, agent_id, tokens_used, cost_usd, cost_basis,
     provider_used, model_used, parent_episode_id)
VALUES
    ('e0000003-0000-0000-0000-000000000003',
     '22222222-2222-2222-2222-222222222222',
     10000, 0.010000, 'measured_split', 'deepseek', 'deepseek-chat',
     'e0000001-0000-0000-0000-000000000001'),
    ('e0000004-0000-0000-0000-000000000004',
     '22222222-2222-2222-2222-222222222222',
     20000, 0.020000, 'measured_split', 'deepseek', 'deepseek-chat',
     'e0000001-0000-0000-0000-000000000001');

-- A GRANDCHILD, two levels down. Proves the descent is recursive rather than
-- a single-level join -- delegation depth is capped at 2 in code today, but a
-- view that silently stops at one level would be wrong the moment that changes.
INSERT INTO public.episodes
    (episode_id, agent_id, tokens_used, cost_usd, cost_basis,
     provider_used, model_used, parent_episode_id)
VALUES
    ('e0000005-0000-0000-0000-000000000005',
     '22222222-2222-2222-2222-222222222222',
     5000, 0.005000, 'measured_split', 'deepseek', 'deepseek-chat',
     'e0000003-0000-0000-0000-000000000003');

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

    -- Trustworthy spend over the whole tree, each row counted ONCE:
    --   parent      0.090  (reached by 3 claims -- must not triple)
    --   child  x2   0.010 + 0.020
    --   grandchild  0.005
    --               = 0.125
    -- The parent alone tripling would give 0.185+; tripling the subtree too
    -- (the naive-join failure) gives 0.375.
    IF v_attributed IS DISTINCT FROM 0.125000 THEN
        RAISE EXCEPTION
            'DEDUP-001 FAILED: attributed_cost_usd = %, expected 0.125000. '
            '0.375000 means the one-claim-per-driver fan-out is multiplying '
            'the whole delegation subtree by driver count; 0.185000 means only '
            'the parent is being tripled.', v_attributed;
    END IF;

    -- 5 distinct executions: parent, unknown-model run, 2 children, grandchild.
    -- NOT the 4 claim rows, and not one row per (claim x descendant) pair.
    IF v_executions IS DISTINCT FROM 5 THEN
        RAISE EXCEPTION
            'DEDUP-001 FAILED: executions = %, expected 5 distinct episodes. '
            'The view must count episodes, never claims or join pairs.', v_executions;
    END IF;

    RAISE NOTICE 'DEDUP-001 ok: every execution costed once across 3 drivers and 2 tree levels';
END $$;

-- ─── TREE-001: delegated cost is included, recursively ─────────────────────

DO $$
DECLARE
    v_delegated BIGINT;
    v_attr      NUMERIC;
BEGIN
    SELECT delegated_executions, attributed_cost_usd
      INTO v_delegated, v_attr
      FROM public.forecast_cost_attribution
     WHERE forecast_id = 'fc_resolved';

    -- 2 children + 1 grandchild. A value of 2 means the descent stopped at one
    -- level; 0 means delegated work is invisible, the pre-mig-198 behaviour.
    IF v_delegated IS DISTINCT FROM 3 THEN
        RAISE EXCEPTION
            'TREE-001 FAILED: delegated_executions = %, expected 3 '
            '(2 children + 1 grandchild). 2 = descent is not recursive; '
            '0 = delegated executions are not reaching the forecast at all.',
            v_delegated;
    END IF;

    -- The parent''s own cost is 0.090; the tree adds 0.035 on top. If a reader
    -- ever sees attributed cost equal to the root''s own figure, compound spend
    -- is being dropped.
    IF v_attr <= 0.090000 THEN
        RAISE EXCEPTION
            'TREE-001 FAILED: attributed_cost_usd = % is not greater than the '
            'root execution''s own 0.090000, so delegated spend is being lost.',
            v_attr;
    END IF;

    RAISE NOTICE 'TREE-001 ok: % delegated executions included; tree total % vs root 0.090000',
        v_delegated, v_attr;
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

    -- 0.125 / 0.04 = 3.125. Must NOT be (0.125+0.25)/0.04 = 9.375, which would
    -- mean untrustworthy spend leaked into the cost-effectiveness metric.
    IF round(v_metric, 4) IS DISTINCT FROM 3.1250 THEN
        RAISE EXCEPTION
            'BRIER-001 FAILED: usd_per_brier_point = %, expected 3.1250 '
            '(attributed 0.125 over the tree / brier 0.04). 9.3750 means the '
            'unknown_model run leaked in; 2.2500 means delegated cost is '
            'missing.', v_metric;
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

-- The failure mode migration 193 documented about its own heuristic: it
-- "can MISS when an agent is invoked twice for the same driver inside the
-- window (the two claims are indistinguishable)". Reproduce exactly that, and
-- prove the episode_id join separates what the window cannot.
DO $$
DECLARE
    v_run_a BIGINT;
    v_run_b BIGINT;
    v_cross BIGINT;
BEGIN
    -- Two runs of the SAME agent on the SAME driver, 30 seconds apart: well
    -- inside migration 193's -2min/+10min window, so the heuristic would match
    -- each episode against BOTH claims.
    INSERT INTO public.episodes
        (episode_id, agent_id, tokens_used, cost_usd, cost_basis,
         provider_used, model_used, created_at, context)
    VALUES
        ('eaaa0001-0000-0000-0000-00000000000a',
         '11111111-1111-1111-1111-111111111111', 1000, 0.001, 'measured_split',
         'deepseek', 'deepseek-chat', NOW(),
         '{"invocation":{"route_reason":"domain_specialist","driver":"tactical"}}'::jsonb),
        ('eaaa0002-0000-0000-0000-00000000000b',
         '11111111-1111-1111-1111-111111111111', 1000, 0.001, 'measured_split',
         'deepseek', 'deepseek-chat', NOW() + INTERVAL '30 seconds',
         '{"invocation":{"route_reason":"domain_specialist","driver":"tactical"}}'::jsonb);

    INSERT INTO public.forecast_agent_claims
        (workspace_id, agent_id, agent_name, driver, p50, episode_id, claimed_at)
    VALUES
        ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
         'efra_critical_factor', 'tactical', 1.4,
         'eaaa0001-0000-0000-0000-00000000000a', NOW()),
        ('aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa', '11111111-1111-1111-1111-111111111111',
         'efra_critical_factor', 'tactical', 1.9,
         'eaaa0002-0000-0000-0000-00000000000b', NOW() + INTERVAL '30 seconds');

    SELECT COUNT(*) INTO v_run_a FROM public.route_outcomes
     WHERE episode_id = 'eaaa0001-0000-0000-0000-00000000000a';
    SELECT COUNT(*) INTO v_run_b FROM public.route_outcomes
     WHERE episode_id = 'eaaa0002-0000-0000-0000-00000000000b';

    -- Exactly one claim each. Under the old window join both would be 2.
    IF v_run_a IS DISTINCT FROM 1 OR v_run_b IS DISTINCT FROM 1 THEN
        RAISE EXCEPTION
            'JOIN-001 FAILED: run A matched % claims, run B matched % — expected '
            '1 each. 2 each means the time-window heuristic is still being used '
            'and two runs of the same agent on the same driver are being '
            'conflated, which is what migration 197 exists to fix.',
            v_run_a, v_run_b;
    END IF;

    -- And each run is matched to ITS OWN claim, not the other one (p50 1.4 vs 1.9).
    SELECT COUNT(*) INTO v_cross FROM public.route_outcomes
     WHERE episode_id = 'eaaa0001-0000-0000-0000-00000000000a'
       AND claimed_multiplier <> 1.4::real;
    IF v_cross <> 0 THEN
        RAISE EXCEPTION
            'JOIN-001 FAILED: run A is attributed a claim value that is not its '
            'own, so claims are crossed between runs.';
    END IF;

    RAISE NOTICE 'JOIN-001 ok: two same-agent same-driver runs 30s apart stay distinct';
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
