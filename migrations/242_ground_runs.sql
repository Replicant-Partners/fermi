-- Grounding runs: an agent ABW does not host, grounded by ABW.
--
-- # What this is for
--
-- "Ground your agent" is a service. The owner of an agent running anywhere
-- opens a run with their API key, calls ABW tools during it with a run token,
-- and submits the output. ABW grades the output against the agent's own
-- declared contract, through the same boundary every hosted agent's output
-- crosses, and returns the enforced document.
--
-- # Why a run token and not the owner's API key
--
-- Grounding can only say "a tool returned this" if the tool call is tied to the
-- output it supports. A long-lived key would let tool calls happen outside any
-- run, and nothing would connect the evidence to a document. The token is
-- scoped to one episode and to the tools the agent's card declares, and it
-- stops working when the output is submitted or the run expires.
--
-- `run_id` is the episode id: the row is reserved when the run opens, so the
-- trace, the lineage and the gate ledger all key on the same value.
--
-- Only a SHA-256 of the token is stored. The token is 32 random bytes, so a
-- fast hash is enough and each tool call does not pay for Argon2.

CREATE TABLE IF NOT EXISTS public.ground_runs (
    run_id        UUID        PRIMARY KEY,
    agent_id      UUID        NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    -- The ABW user who opened the run, and whose wallet pays for its tools.
    user_id       TEXT        NOT NULL REFERENCES public.users(user_id) ON DELETE CASCADE,
    token_hash    TEXT        NOT NULL UNIQUE,
    -- The tools this run may call, fixed at open from the agent's card.
    allowed_tools TEXT[]      NOT NULL DEFAULT '{}',
    query         TEXT        NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at    TIMESTAMPTZ NOT NULL,
    -- Set when the output is submitted. A closed run's token is dead.
    closed_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_ground_runs_user ON public.ground_runs(user_id, created_at DESC);

-- Every tool call made under a run, in order. This is the run record grounding
-- and completeness read: without it, `tool_verified` is the agent's own claim.
CREATE TABLE IF NOT EXISTS public.ground_run_tool_calls (
    call_id      BIGSERIAL   PRIMARY KEY,
    run_id       UUID        NOT NULL REFERENCES public.ground_runs(run_id) ON DELETE CASCADE,
    tool_name    TEXT        NOT NULL,
    input        JSONB       NOT NULL DEFAULT '{}'::jsonb,
    -- Verbatim, like `episodes.response_text`: the evidence, not a digest.
    output       TEXT        NOT NULL DEFAULT '',
    ok           BOOLEAN     NOT NULL,
    duration_ms  BIGINT      NOT NULL DEFAULT 0,
    credits      INTEGER     NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_ground_run_tool_calls_run ON public.ground_run_tool_calls(run_id, call_id);
