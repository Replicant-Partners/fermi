-- Migration 125: Workspace Action Protocol
--
-- Adds the infrastructure for the generalised App action protocol:
-- every structured mutation an agent (or CLI, or MCP client) makes to
-- a workspace's canonical document is recorded as a typed action event
-- and optionally requires human confirmation before being applied.
--
-- Two new tables:
--
--   workspace_action_log  — append-only record of every action emitted,
--                           its confirmation state, and who/what applied it.
--                           This is the ground truth for Loop 2 (HITL),
--                           calibration (Loop 5), and audit.
--
--   workspace_annotations — typed observations attached to document fragments
--                           (stages, process-level, variation-level).
--                           Replaces ad-hoc metadata JSONb on messages.
--
-- Action types (generalised from SimOps action grammar):
--   mutate_document    — patch the App's canonical document (simops: edit_process)
--   fork_state         — create a named variant (simops: fork_variation)
--   compare            — run member agents across variants (simops: compare_variations)
--   invoke_member      — call a fleet member with structured input (simops: invoke_agent)
--   annotate_schema    — attach a SOSA contract / schema metadata to a field
--   annotate           — record a typed observation about the document
--
-- PgBouncer-safe: each DDL is a single statement.

-- ─── workspace_action_log ─────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.workspace_action_log (
    action_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,

    -- Who emitted this action
    emitted_by_type  TEXT NOT NULL CHECK (emitted_by_type IN ('agent', 'user', 'cli', 'mcp')),
    emitted_by_id    TEXT NOT NULL,  -- agent_name, user_id, or 'abw-cli'

    -- The action
    action_type      TEXT NOT NULL CHECK (action_type IN (
        'mutate_document',
        'fork_state',
        'compare',
        'invoke_member',
        'annotate_schema',
        'annotate'
    )),

    -- App-specific domain tag (e.g. 'kask_simops', 'efrain_ai').
    -- NULL for generic workspace actions not tied to an App schema.
    app_schema       TEXT,

    -- The full action payload as emitted (verbatim from the action block
    -- or API request body). Immutable after insert.
    payload          JSONB NOT NULL DEFAULT '{}',

    -- Confirmation lifecycle
    confirmation     TEXT NOT NULL DEFAULT 'ask'
                         CHECK (confirmation IN ('auto', 'ask', 'pending', 'accepted', 'rejected')),
    confirmed_by     TEXT,       -- user_id of the human who accepted/rejected
    confirmed_at     TIMESTAMPTZ,
    rejection_note   TEXT,

    -- Outcome — populated when the action is applied
    applied          BOOLEAN NOT NULL DEFAULT FALSE,
    applied_at       TIMESTAMPTZ,
    apply_result     JSONB,  -- e.g. { sha, path } for mutate_document

    -- Calibration hook: links back to the workspace message that triggered this action
    source_message_id UUID REFERENCES public.workspace_messages(message_id) ON DELETE SET NULL,

    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wsal_workspace
    ON public.workspace_action_log(workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_wsal_pending
    ON public.workspace_action_log(workspace_id, action_type)
    WHERE confirmation = 'pending';

CREATE INDEX IF NOT EXISTS idx_wsal_source_message
    ON public.workspace_action_log(source_message_id)
    WHERE source_message_id IS NOT NULL;

COMMENT ON TABLE public.workspace_action_log IS
    'Append-only record of every structured action emitted against a workspace '
    '(by agents via action blocks, by the CLI, or by MCP tool calls). '
    'Generalised from the SimOps action grammar — see docs/specs/04_APP_CLI_EXTENSION.md.';

-- ─── workspace_annotations ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS public.workspace_annotations (
    annotation_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id     UUID NOT NULL REFERENCES public.teams(id) ON DELETE CASCADE,

    -- What kind of annotation
    kind             TEXT NOT NULL CHECK (kind IN ('critique', 'insight', 'risk', 'decision')),

    -- What it applies to — free-form target string (e.g. "stage:fermentation",
    -- "process", "variation:co2-capture-75", "field:efficiency")
    target           TEXT NOT NULL DEFAULT 'process',

    -- The annotation body
    body             TEXT NOT NULL,
    severity         TEXT NOT NULL DEFAULT 'info'
                         CHECK (severity IN ('info', 'warn', 'block')),

    -- App-specific domain tag
    app_schema       TEXT,

    -- Who wrote it
    author_type      TEXT NOT NULL DEFAULT 'agent'
                         CHECK (author_type IN ('agent', 'user', 'cli', 'mcp')),
    author_id        TEXT NOT NULL,

    -- Lifecycle
    resolved         BOOLEAN NOT NULL DEFAULT FALSE,
    resolved_by      TEXT,
    resolved_at      TIMESTAMPTZ,

    -- Links
    action_id        UUID REFERENCES public.workspace_action_log(action_id) ON DELETE SET NULL,
    source_message_id UUID REFERENCES public.workspace_messages(message_id) ON DELETE SET NULL,

    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_wa_workspace
    ON public.workspace_annotations(workspace_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_wa_target
    ON public.workspace_annotations(workspace_id, target)
    WHERE NOT resolved;

COMMENT ON TABLE public.workspace_annotations IS
    'Typed observations attached to document fragments within a workspace. '
    'Written by agents via annotate action blocks, by users inline, or by CLI. '
    'Replaces ad-hoc metadata JSONb on workspace_messages for structured notes.';
