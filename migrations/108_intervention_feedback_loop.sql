-- Migration 108 — Phase 5: Intervention feedback loop
--
-- Adds the `two_reviewer_requests` table for the two-reviewer consensus
-- path required for `agent_wide` scope interventions (D26 / OQ-4).
--
-- Per decision D26: `agent_wide` interventions require a second reviewer
-- before the write proceeds. Phase 4 surfaces the "Intervene" button as
-- disabled; Phase 5 enables it and routes `agent_wide` through this table.
--
-- Workflow:
--   1. First reviewer calls POST /api/observatory/hitl/:id/action
--      {action: "intervene", scope: "agent_wide", ...}
--   2. Handler creates a pending `two_reviewer_requests` row instead of
--      immediately executing the two-write pattern.
--   3. Second reviewer (must be different user) calls the same endpoint
--      with the request_id in the body.
--   4. Handler checks both reviewers differ, marks the row fulfilled, and
--      proceeds with the coherence gate + two-write memory pattern.
--
-- Idempotent — safe to re-run. PgBouncer-safe (no prepared statements,
-- no multi-statement DDL within a transaction that PgBouncer would split).

-- ── 1. two_reviewer_requests ─────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS two_reviewer_requests (
    request_id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    anomaly_event_id    UUID NOT NULL REFERENCES anomaly_events(event_id),
    agent_id            UUID NOT NULL,

    -- Encoded intervention payload (from InterventionEncoder).
    -- Stored as JSONB so the second reviewer sees exactly what the first
    -- reviewer submitted.
    encoded_intervention JSONB NOT NULL,

    -- First reviewer (the one who initiated the agent_wide intervention).
    first_reviewer_id   TEXT NOT NULL,
    first_reviewed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Second reviewer (filled in when they confirm).
    second_reviewer_id  TEXT,
    second_reviewed_at  TIMESTAMPTZ,

    -- Whether the second reviewer approved or rejected.
    -- NULL = awaiting second review.
    -- TRUE = both approved → proceed with two-write pattern.
    -- FALSE = second reviewer rejected → request is cancelled.
    second_approved     BOOLEAN,

    -- Final outcome.
    -- 'pending'   = awaiting second review
    -- 'approved'  = both reviewers approved, writes have been executed
    -- 'rejected'  = second reviewer rejected
    -- 'expired'   = not confirmed within TTL (future enforcement)
    status              TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),

    -- Populated after the two-write pattern executes successfully.
    correction_id           UUID,
    synthetic_episode_id    UUID,

    notes               TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Prevent two second-reviewers by adding a unique partial index:
-- only one 'pending' request per anomaly.
CREATE UNIQUE INDEX IF NOT EXISTS idx_two_reviewer_requests_pending_anomaly
    ON two_reviewer_requests (anomaly_event_id)
    WHERE status = 'pending';

-- Fast lookup: find pending request for a given anomaly.
CREATE INDEX IF NOT EXISTS idx_two_reviewer_requests_anomaly
    ON two_reviewer_requests (anomaly_event_id, status);

CREATE INDEX IF NOT EXISTS idx_two_reviewer_requests_agent
    ON two_reviewer_requests (agent_id, status);

-- ── 2. Immutability trigger (second-reviewer writes must be to second_* cols) ─
--
-- We do NOT add an UPDATE-blocking trigger here because the workflow
-- genuinely needs to UPDATE the row when the second reviewer acts.
-- Instead we log changes through the append-only hitl_actions table
-- (already in Phase 4) and keep two_reviewer_requests mutable.

-- ── 3. updated_at trigger ───────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION public.touch_two_reviewer_requests()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_touch_two_reviewer_requests
    ON two_reviewer_requests;

CREATE TRIGGER trg_touch_two_reviewer_requests
    BEFORE UPDATE ON two_reviewer_requests
    FOR EACH ROW EXECUTE FUNCTION public.touch_two_reviewer_requests();
