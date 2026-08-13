-- ═══════════════════════════════════════════════════════════════════
-- Fixture data for scripts/smoke_economics.sh
--
-- Numbers are chosen so every expected result is exact and hand-checkable:
-- every episode uses 1,000,000 tokens, so cost == the model's per-Mtok rate.
--
--   xaman_ek   / opus   → $15.00   (funded by abw-system)
--   cohere…    / sonnet → $3.00    (funded by abw-system)
--   football…  / haiku  → $0.25    (funded by mario)
--
-- Plus two rows that exist purely to catch classes of bug:
--   * an episode 400 days old  → must be excluded by the window filter
--   * an episode with no funding_principal and NULL cost
--                              → must land in 'unattributed' and be
--                                counted by missing_cost, not dropped
-- ═══════════════════════════════════════════════════════════════════

INSERT INTO users (user_id, email, role, auth_provider) VALUES
    ('abw-system', 'system@abw.local',  'admin',     'legacy'),
    ('ivan',       'ivan@example.test', 'admin',     'email'),
    ('mario',      'mario@example.test','developer', 'email');

INSERT INTO agents (agent_id, agent_name, tier, user_id) VALUES
    ('11111111-1111-1111-1111-111111111111', 'xaman_ek',              'system',    'abw-system'),
    ('22222222-2222-2222-2222-222222222222', 'cohere_and_coordinate', 'curated',   'ivan'),
    ('33333333-3333-3333-3333-333333333333', 'football_analyst',      'community', 'mario');

INSERT INTO episodes (episode_id, agent_id, context, tokens_used, cost_usd, created_at) VALUES
    -- In-window, attributed to abw-system.
    ('aaaaaaaa-0000-0000-0000-000000000001',
     '11111111-1111-1111-1111-111111111111',
     '{"funding_principal":"abw-system","provider":"anthropic","model_used":"claude-opus-4-6"}',
     1000000, 15.000000, NOW() - INTERVAL '1 day'),

    ('aaaaaaaa-0000-0000-0000-000000000002',
     '22222222-2222-2222-2222-222222222222',
     '{"funding_principal":"abw-system","provider":"anthropic","model_used":"claude-sonnet-4-6"}',
     1000000,  3.000000, NOW() - INTERVAL '2 days'),

    -- In-window, funded by its own owner.
    ('aaaaaaaa-0000-0000-0000-000000000003',
     '33333333-3333-3333-3333-333333333333',
     '{"funding_principal":"mario","provider":"anthropic","model_used":"claude-haiku-4-5-20251001"}',
     1000000,  0.250000, NOW() - INTERVAL '3 days'),

    -- OUT of a 30-day window. If this shows up, the interval filter is broken.
    ('aaaaaaaa-0000-0000-0000-000000000004',
     '11111111-1111-1111-1111-111111111111',
     '{"funding_principal":"abw-system","provider":"anthropic","model_used":"claude-opus-4-6"}',
     9000000, 99.000000, NOW() - INTERVAL '400 days'),

    -- Pre-SPEC_28 shape: no funding_principal, no token accounting.
    ('aaaaaaaa-0000-0000-0000-000000000005',
     '11111111-1111-1111-1111-111111111111',
     '{}', NULL, NULL, NOW() - INTERVAL '4 days');

INSERT INTO wallets (wallet_id, owner_type, owner_id, balance) VALUES
    ('bbbbbbbb-0000-0000-0000-000000000001', 'user', 'mario', 1000),
    ('bbbbbbbb-0000-0000-0000-000000000002', 'user', 'ivan',    85);

-- execution_fee rows are stored NEGATIVE (debit on the caller's wallet)
-- — see fermi-auth/src/credits.rs binding `-amount`. The query flips the
-- sign; if that convention ever changes, revenue silently inverts and
-- the assertions below catch it.
INSERT INTO credit_ledger (wallet_id, amount, tx_type, related_id, created_at) VALUES
    ('bbbbbbbb-0000-0000-0000-000000000001',  -50, 'execution_fee',
     'aaaaaaaa-0000-0000-0000-000000000001', NOW() - INTERVAL '1 day'),
    ('bbbbbbbb-0000-0000-0000-000000000001', -100, 'execution_fee',
     'aaaaaaaa-0000-0000-0000-000000000002', NOW() - INTERVAL '2 days'),

    -- Out of window, pointing at an out-of-window episode. Excluded
    -- twice over (both the episode filter and the fee filter).
    ('bbbbbbbb-0000-0000-0000-000000000001', -999, 'execution_fee',
     'aaaaaaaa-0000-0000-0000-000000000004', NOW() - INTERVAL '400 days'),

    -- Out of window, but pointing at an IN-window episode. This row is
    -- the *only* thing the fee CTE's own window filter catches: the
    -- episode filter alone would let it through the join. Without it a
    -- backdated or replayed ledger row would inflate current revenue.
    -- Mutation-tested: deleting that filter must turn this red.
    ('bbbbbbbb-0000-0000-0000-000000000001', -777, 'execution_fee',
     'aaaaaaaa-0000-0000-0000-000000000001', NOW() - INTERVAL '400 days'),

    -- Royalty deposit (positive) to the agent owner.
    ('bbbbbbbb-0000-0000-0000-000000000002',   85, 'agent_royalty_in',
     '22222222-2222-2222-2222-222222222222', NOW() - INTERVAL '2 days');
