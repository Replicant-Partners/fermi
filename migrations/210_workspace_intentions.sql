-- ═══════════════════════════════════════════════════════════════════════
-- 210 — workspace_intentions: Loop 3 Stage 0, prospective coordination
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT WAS MISSING
-- ----------------
-- `intention_coordinator` is a curated agent whose entire purpose is to catch
-- coordination failures *before* they happen: two agents about to research the
-- same thing, one about to write a file another is mid-way through, one whose
-- plan depends on an output nobody has produced.
--
-- All six of its tools were phantom. `declare_intention`, `check_conflicts`,
-- `get_intention_map`, `clear_intention`, `suggest_differentiation` and
-- `emit_coherence_signal` were declared on the card, advertised to the model,
-- and had no dispatch arm — so every call returned `Unknown tool`. The agent
-- has never once functioned, and Loop 3's Stage 0 has never run.
--
-- The card told the agent to persist the map to
-- `_coordination/intention_map.json` via `write_workspace_file`. That cannot
-- work for this purpose: a JSON file in workspace git has no concurrency story,
-- and the whole point is several agents declaring intentions at once. A table
-- with row-level visibility is the right substrate.
--
-- WHY AN EMBEDDING COLUMN
-- ----------------------
-- Duplication is the most valuable conflict to catch and it is *semantic*: "two
-- agents planning to research the same topic" is not string equality.
-- `research UK CPI trend` and `investigate British inflation data` are the same
-- work under different words, and only a vector comparison sees that.
--
-- The embedding is nullable and the conflict checker degrades to exact-match
-- signals without it, because an embedding outage must not take prospective
-- coordination offline — but it is populated on the write path, not deferred to
-- some later worker. This codebase has three separate defects that were all
-- "the consolidation worker will embed this later"; it never did.
--
-- STATUS LIFECYCLE
-- ----------------
--   active     — declared, not yet resolved; participates in conflict checks
--   completed  — the agent did the thing
--   cancelled  — the agent decided not to
--   superseded — replaced by a newer declaration from the same agent
--
-- Only `active` rows are compared, so a stale intention cannot generate a
-- phantom conflict forever. `declare_intention` supersedes an agent's own prior
-- active row, which keeps the map to at most one live intention per agent.
-- ═══════════════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS public.workspace_intentions (
    intention_id  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,
    agent_id      UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,

    -- What the agent is about to do.
    action_type   TEXT NOT NULL
        CHECK (action_type IN ('tool_call','research','synthesis','writing','review','idle')),
    tool          TEXT,
    description   TEXT NOT NULL,

    -- Resources this action will consume or write. Overlap between two active
    -- intentions is a resource conflict — the cheapest and most certain of the
    -- four conflict classes, because it needs no semantics at all.
    targets       TEXT[] NOT NULL DEFAULT '{}',

    -- Named outputs this action needs before it can run. An entry naming
    -- something no completed intention produced is a dependency conflict.
    depends_on    TEXT[] NOT NULL DEFAULT '{}',

    status        TEXT NOT NULL DEFAULT 'active'
        CHECK (status IN ('active','completed','cancelled','superseded')),

    -- Semantic duplication detection. Nullable: see header.
    embedding     vector(1024),

    declared_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at   TIMESTAMPTZ
);

-- The conflict check reads active intentions for one workspace, which is the
-- only hot query on this table.
CREATE INDEX IF NOT EXISTS idx_intentions_active
    ON public.workspace_intentions(workspace_id, status)
    WHERE status = 'active';

CREATE INDEX IF NOT EXISTS idx_intentions_agent
    ON public.workspace_intentions(agent_id, declared_at DESC);

COMMENT ON TABLE public.workspace_intentions IS
    'Loop 3 Stage 0 — prospective coordination. Agents declare planned actions '
    'before acting so duplication, resource contention and unmet dependencies '
    'are caught ahead of the work rather than diagnosed after it. Only `active` '
    'rows participate in conflict checks.';

-- ── Emitted coherence signals ──────────────────────────────────────────
--
-- `emit_coherence_signal` records an IntentionAligns / IntentionConflicts
-- relation. These are also posted into the workspace conversation, which is
-- what `ConversationObserver::observe` reads when building the TEC graph — so
-- the signal genuinely reaches coherence rather than being filed somewhere for
-- a future consumer. This table is the durable record and the audit trail.
CREATE TABLE IF NOT EXISTS public.workspace_intention_signals (
    signal_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id  UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('IntentionAligns','IntentionConflicts')),
    agent_a       UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    agent_b       UUID NOT NULL REFERENCES public.agents(agent_id) ON DELETE CASCADE,
    strength      DOUBLE PRECISION NOT NULL DEFAULT 0.5
        CHECK (strength >= 0.0 AND strength <= 1.0),
    rationale     TEXT,
    emitted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_intention_signals_workspace
    ON public.workspace_intention_signals(workspace_id, emitted_at DESC);
