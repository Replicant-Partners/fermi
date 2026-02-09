-- Migration 027: Eval framework tables
-- Test cases per agent + eval run results with per-case scoring

BEGIN;

-- Test cases: enriched versions of sample_queries with optional rubrics
CREATE TABLE IF NOT EXISTS public.eval_test_cases (
    test_case_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id      UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    query         TEXT NOT NULL,
    expected_output TEXT,
    rubric        TEXT,
    tags          TEXT[] DEFAULT '{}',
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_eval_test_cases_agent
    ON public.eval_test_cases(agent_id) WHERE is_active;

-- Eval runs: one row per "run all test cases" invocation
CREATE TABLE IF NOT EXISTS public.eval_runs (
    run_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id      UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    triggered_by  TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'running'
                  CHECK (status IN ('running', 'completed', 'failed')),
    judge_enabled BOOLEAN NOT NULL DEFAULT FALSE,
    total_cases   INTEGER NOT NULL DEFAULT 0,
    passed        INTEGER NOT NULL DEFAULT 0,
    failed        INTEGER NOT NULL DEFAULT 0,
    avg_latency_ms BIGINT,
    avg_tokens    INTEGER,
    avg_judge_score DOUBLE PRECISION,
    total_cost_credits INTEGER NOT NULL DEFAULT 0,
    case_results  JSONB NOT NULL DEFAULT '[]',
    regression_detected BOOLEAN NOT NULL DEFAULT FALSE,
    regression_details  JSONB,
    started_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at  TIMESTAMPTZ,
    duration_ms   BIGINT
);

CREATE INDEX IF NOT EXISTS idx_eval_runs_agent
    ON public.eval_runs(agent_id, started_at DESC);

-- Extend credit_ledger CHECK to include eval_fee
ALTER TABLE credit_ledger DROP CONSTRAINT IF EXISTS credit_ledger_tx_type_check;
ALTER TABLE credit_ledger ADD CONSTRAINT credit_ledger_tx_type_check
    CHECK (tx_type IN (
        'deposit', 'withdrawal',
        'execution_fee', 'gas_fee',
        'education_alloc', 'education_spend',
        'transfer_out', 'transfer_in',
        'grant', 'refund',
        'fork_royalty', 'fork_fee', 'publish_fee',
        'eval_fee'
    ));

COMMIT;
