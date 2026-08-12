-- ═══════════════════════════════════════════════════════════════════════
-- Self-test fixture for scripts/loop5_brier_mechanical_check.sql
-- ═══════════════════════════════════════════════════════════════════════
--
-- Builds a throwaway schema carrying only the columns the probe reads, then
-- seeds one deliberate violation per MECHANISM check plus a block of clean
-- rows. Running the probe against this database must flag exactly the
-- seeded violations and nothing else — that is what makes the probe itself
-- trustworthy rather than merely plausible.
--
-- Two modes, because a probe has to be proven in both directions: it must
-- catch what it claims to catch, AND stay silent on a healthy database.
-- A probe that only ever fires is as useless as one that never does.
--
--   -v mode=violations  (default)  seed every violation class
--   -v mode=clean                  seed only correct data
--
-- Expected results:
--
--   mode=clean       → MECHANISM: 9 OK, 0 violations. Verdict SOUND.
--                      INFO L5-I03 = 3 (the World Cup block below is
--                      deliberately non-discriminating), L5-I04 = 0.
--
--   mode=violations  → M01=1 M02=1 M03=1 M04=1 M05=1 M06=1 M07=1 M08=1
--                      M09=2. Verdict BROKEN.
--
-- USAGE (throwaway cluster; never point this at a real database):
--
--   psql "$SCRATCH_URL" -f scripts/loop5_probe_selftest.sql -v mode=clean
--   psql "$SCRATCH_URL" -f scripts/loop5_brier_mechanical_check.sql
-- ══════════════════════════════════════════════════════════════════════

\set ON_ERROR_STOP on

\if :{?mode}
\else
  \set mode violations
\endif
SELECT (:'mode' = 'clean') AS clean_only \gset

DROP TABLE IF EXISTS eval_signals, fermi_forecasts, agents CASCADE;

CREATE TABLE agents (
    agent_id   UUID PRIMARY KEY,
    agent_name TEXT UNIQUE NOT NULL
);

CREATE TABLE fermi_forecasts (
    id                    TEXT PRIMARY KEY,
    status                TEXT NOT NULL,
    actual_outcome        BOOLEAN,
    predicted_probability REAL,
    scored_probability    REAL,
    brier_score           REAL,
    agents_used           JSONB NOT NULL DEFAULT '[]'::jsonb,
    resolved_at           TIMESTAMPTZ
);

CREATE TABLE eval_signals (
    signal_id      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id       UUID NOT NULL REFERENCES agents(agent_id),
    evaluator_name TEXT NOT NULL,
    dimension      TEXT NOT NULL,
    score          DOUBLE PRECISION NOT NULL,
    rationale      TEXT
);

-- ── Agents ────────────────────────────────────────────────────────────
INSERT INTO agents (agent_id, agent_name) VALUES
  ('aaaaaaaa-0000-0000-0000-000000000001', 'agent_alpha'),
  ('aaaaaaaa-0000-0000-0000-000000000002', 'agent_beta'),
  ('aaaaaaaa-0000-0000-0000-000000000003', 'agent_gamma'),
  ('bbbbbbbb-0000-0000-0000-000000000001', 'wc_factor_x1'),
  ('bbbbbbbb-0000-0000-0000-000000000002', 'wc_factor_x2'),
  ('bbbbbbbb-0000-0000-0000-000000000003', 'wc_factor_x3');

-- ── Helper: emit a correct signal for (agent, forecast) ───────────────
CREATE OR REPLACE FUNCTION emit(p_agent TEXT, p_forecast TEXT) RETURNS void AS $$
    INSERT INTO eval_signals (agent_id, evaluator_name, dimension, score, rationale)
    SELECT a.agent_id, 'brier_forecast_resolver', 'forecast_calibration',
           1.0 - least(greatest(f.brier_score::float8, 0.0), 1.0),
           format('forecast %s resolved (brier=%s)', f.id, round(f.brier_score::numeric, 4))
      FROM agents a, fermi_forecasts f
     WHERE a.agent_name = p_agent AND f.id = p_forecast;
$$ LANGUAGE sql;

-- ═══════════════════════════════════════════════════════════════════════
-- CLEAN BASELINE — 4 fully correct forecasts, agent_id-stamped rosters.
-- These must produce zero violations.
-- ═══════════════════════════════════════════════════════════════════════
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at) VALUES
 ('11111111-1111-1111-1111-111111111101', 'resolved', true,  0.80, 0.80, 0.04,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001","name":"agent_alpha"}]', now()),
 ('11111111-1111-1111-1111-111111111102', 'resolved', false, 0.10, 0.10, 0.01,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001","name":"agent_alpha"}]', now()),
 ('11111111-1111-1111-1111-111111111103', 'resolved', true,  0.70, 0.70, 0.09,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001","name":"agent_alpha"}]', now()),
 ('11111111-1111-1111-1111-111111111104', 'resolved', false, 0.20, 0.20, 0.04,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001","name":"agent_alpha"}]', now());

SELECT emit('agent_alpha', '11111111-1111-1111-1111-111111111101');
SELECT emit('agent_alpha', '11111111-1111-1111-1111-111111111102');
SELECT emit('agent_alpha', '11111111-1111-1111-1111-111111111103');
SELECT emit('agent_alpha', '11111111-1111-1111-1111-111111111104');

-- ── An unresolved and a voided forecast: must be ignored entirely ─────
INSERT INTO fermi_forecasts (id, status, predicted_probability, agents_used) VALUES
 ('11111111-1111-1111-1111-111111111105', 'active', 0.50,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001"}]'),
 ('11111111-1111-1111-1111-111111111106', 'voided', 0.50,
  '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001"}]');

-- ── World Cup shape: mechanically perfect, informationally degenerate ──
--
-- Six forecasts, every one citing all three factor agents — the 6-factor
-- tournament model's actual attribution pattern. Emission is complete and
-- correct, so every MECHANISM check stays clean. But all three agents end up
-- with a byte-identical forecast set, so L5-I03 fires: Loop 5 can never rank
-- them against each other, and no amount of additional data will change that.
-- This is the distinction the probe exists to draw.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
SELECT format('44444444-0000-0000-0000-00000000w%s', g),
       'resolved',
       (g = 1),                                  -- exactly one winner
       CASE WHEN g = 1 THEN 0.30 ELSE 0.12 END,
       CASE WHEN g = 1 THEN 0.30 ELSE 0.12 END,
       CASE WHEN g = 1 THEN 0.49 ELSE 0.0144 END,
       '[{"agent_id":"bbbbbbbb-0000-0000-0000-000000000001","name":"wc_factor_x1"},
         {"agent_id":"bbbbbbbb-0000-0000-0000-000000000002","name":"wc_factor_x2"},
         {"agent_id":"bbbbbbbb-0000-0000-0000-000000000003","name":"wc_factor_x3"}]'::jsonb,
       now()
  FROM generate_series(1, 6) g;

INSERT INTO eval_signals (agent_id, evaluator_name, dimension, score, rationale)
SELECT a.agent_id, 'brier_forecast_resolver', 'forecast_calibration',
       1.0 - least(greatest(f.brier_score::float8, 0.0), 1.0),
       format('forecast %s resolved (brier=%s)', f.id, round(f.brier_score::numeric, 4))
  FROM fermi_forecasts f
  CROSS JOIN LATERAL jsonb_array_elements(f.agents_used) e
  JOIN agents a ON a.agent_id::text = e->>'agent_id'
 WHERE f.id LIKE '44444444-%';

-- ══════════════════════════════════════════════════════════════════════
-- SEEDED VIOLATIONS — one per MECHANISM check (skipped when mode=clean)
-- ══════════════════════════════════════════════════════════════════════

\if :clean_only
\echo 'mode=clean — skipping violation seeding'
\else

-- L5-M01: resolved with an outcome but never scored.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m1', 'resolved', true, 0.60,
        '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001"}]', now());

-- L5-M02: stored brier inconsistent with the frozen scored_probability.
-- (0.90 vs outcome true ⇒ 0.01, but 0.25 is stored.)
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m2', 'resolved', true, 0.90, 0.90, 0.25,
        '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000001"}]', now());
SELECT emit('agent_alpha', '22222222-0000-0000-0000-0000000000m2');

-- L5-M03: scored but attributable to nobody (empty roster).
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m3', 'resolved', false, 0.30, 0.30, 0.09,
        '[]', now());

-- L5-M04: roster carries one real agent and one that does not exist.
-- Also emits for the real agent so this does NOT trip M05.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m4', 'resolved', true, 0.75, 0.75, 0.0625,
        '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000002"},{"name":"agent_deleted"}]', now());
SELECT emit('agent_beta', '22222222-0000-0000-0000-0000000000m4');

-- L5-M05: PARTIAL emission — two agents on the roster, only one got a signal.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m5', 'resolved', true, 0.65, 0.65, 0.1225,
        '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000002"},{"agent_id":"aaaaaaaa-0000-0000-0000-000000000003"}]', now());
SELECT emit('agent_beta', '22222222-0000-0000-0000-0000000000m5');   -- gamma deliberately skipped

-- L5-M06: signal score does not equal 1 - brier.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('22222222-0000-0000-0000-0000000000m6', 'resolved', true, 0.85, 0.85, 0.0225,
        '[{"agent_id":"aaaaaaaa-0000-0000-0000-000000000002"}]', now());
INSERT INTO eval_signals (agent_id, evaluator_name, dimension, score, rationale)
VALUES ('aaaaaaaa-0000-0000-0000-000000000002', 'brier_forecast_resolver',
        'forecast_calibration', 0.42,   -- wrong: should be 0.9775
        'forecast 22222222-0000-0000-0000-0000000000m6 resolved (brier=0.0225)');

-- L5-M07: signal citing a forecast that is not resolved-and-scored.
-- Uses the 'active' forecast seeded above.
INSERT INTO eval_signals (agent_id, evaluator_name, dimension, score, rationale)
VALUES ('aaaaaaaa-0000-0000-0000-000000000003', 'brier_forecast_resolver',
        'forecast_calibration', 0.5,
        'forecast 11111111-1111-1111-1111-111111111105 resolved (brier=0.5000)');

-- L5-M08: duplicate (agent, forecast) signal.
SELECT emit('agent_alpha', '11111111-1111-1111-1111-111111111101');

-- L5-M09: two forecasts reachable only by name (no agent_id in the roster) —
-- the mig-170 staleness case. Emitted correctly so they trip only M09.
INSERT INTO fermi_forecasts (id, status, actual_outcome, predicted_probability,
                             scored_probability, brier_score, agents_used, resolved_at)
VALUES ('33333333-0000-0000-0000-0000000000n1', 'resolved', true, 0.55, 0.55, 0.2025,
        '[{"name":"agent_gamma"}]', now()),
       ('33333333-0000-0000-0000-0000000000n2', 'resolved', false, 0.45, 0.45, 0.2025,
        '[{"name":"agent_gamma"}]', now());
SELECT emit('agent_gamma', '33333333-0000-0000-0000-0000000000n1');
SELECT emit('agent_gamma', '33333333-0000-0000-0000-0000000000n2');

\endif

\echo ''
\echo 'Fixture built.'
\echo '  mode=clean      → expect MECHANISM 9 OK / 0 violations, L5-I03 = 3'
\echo '  mode=violations → expect M01=1 M02=1 M03=1 M04=1 M05=1 M06=1 M07=1 M08=1 M09=2'
\echo ''
