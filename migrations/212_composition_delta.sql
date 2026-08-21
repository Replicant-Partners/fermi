-- Migration 212: composition proposals carry a delta, not an absolute roster.
--
-- `composition_versions.member_agent_ids` is an absolute roster, and accepting
-- one runs:
--
--     DELETE FROM workspace_agents WHERE workspace_id = $1 AND agent_id <> ALL($2)
--
-- Loop 4's timescale is weeks to months. A roster computed when a proposal is
-- filed and applied six weeks later silently evicts everyone hired in the
-- interim and resurrects everyone dropped — and it does so under a button
-- labelled "accept", against a list the owner never saw change. The absolute
-- form is only safe when propose and accept are close together, which is
-- exactly what Loop 4 is not.
--
-- A delta says what the proposal is *for* rather than what the world should
-- look like afterwards, so the parts of the roster it has no opinion about stay
-- untouched however long it sits pending.
--
-- Shape:
--   {"remove": ["agent_name", ...], "add": ["agent_name", ...]}
--
-- By NAME, deliberately. The attribution deriver only ever knew names
-- (`AgentEvidence.agent_name`), and resolving to a UUID at proposal time would
-- have meant inventing a mapping and storing a stale answer. Names resolve at
-- accept time, scoped to the workspace's current membership: you can only
-- remove an agent that is on the team right now, and an entry that no longer
-- matches anyone is a reported no-op rather than a surprise.
--
-- `member_agent_ids` is retained. The HTTP propose route accepts an explicit
-- roster and a caller supplying one means it; the delta is the path for
-- proposals the platform derives on its own.

ALTER TABLE public.composition_versions
    ADD COLUMN IF NOT EXISTS member_delta JSONB;

COMMENT ON COLUMN public.composition_versions.member_delta IS
    'Roster change as {"remove":[agent_name],"add":[agent_name]}, resolved '
    'against current workspace membership at accept time. Preferred over '
    'member_agent_ids for derived proposals, which may sit pending for weeks.';
