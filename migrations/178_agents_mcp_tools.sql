-- ═══════════════════════════════════════════════════════════════════
-- Migration 178 — agents.mcp_tools: DB-backed published-tool list
--
-- PROBLEM
-- -------
-- `resolve_agent_card` prefers the filesystem agent-card registry and
-- falls back to `agent_card_from_db`, which hardcodes `mcp_tools: vec![]`.
-- So any agent without an `agent_card.json` on disk declares no tools.
--
-- That is the overwhelming majority of them: ~709 rows in `agents` against
-- 95 card files on disk.
--
-- The agent's OWN tool access is unaffected — `to_claude_tools_with_card`
-- starts from every builtin and only appends card entries, and
-- `ToolRegistry::execute` performs no per-agent authorization. What breaks
-- is the OUTBOUND MCP surface: `/mcp/agents/:agent_id` gates `tools/call`
-- on `card.capabilities.mcp_tools` (`handlers/mcp.rs`), and builds its
-- `tools/list` manifest from the same field. A DB-only agent therefore
-- publishes nothing and is reachable only through the catch-all `execute`
-- path — send prose, get prose. It cannot expose a typed tool to Claude
-- Desktop, Cursor, or Zed.
--
-- mig-177 made the DB authoritative for `mcp_servers` (the client
-- direction: which remote servers an agent may CALL). This is the
-- symmetric column for the server direction: which tools an agent
-- PUBLISHES.
--
-- SEMANTICS
-- ---------
-- `mcp_tools` is an EXPORT ALLOWLIST, not a capability grant. Every agent
-- already receives all platform builtins internally, so this column only
-- answers "which of them do I expose over MCP".
--
-- Same precedence as `mcp_servers` (see `interpret_db_column`):
--   NULL       -> inherit whatever the filesystem card declares
--   []  / {}   -> explicitly publish nothing (how the UI un-publishes a
--                 tool a file card declared)
--   non-empty  -> authoritative replacement
--
-- Stored as the same shape the card uses: an array of
-- `{name, description, input_schema}`. `name` is the load-bearing field —
-- it must resolve to a dispatch arm in `ToolRegistry::execute`, or to a
-- `server__tool` name belonging to a server the agent declares in
-- `mcp_servers`. Writes are validated against
-- `tools::invalid_tool_declarations`; unvalidated names would become
-- phantom tools (advertised, called, then `Unknown tool: X`).
--
-- NOTE ON NAMING
-- --------------
-- Do not confuse this with the pre-mig-177 bug where the agent-create path
-- wrote card `mcp_tools` into the `mcp_servers` column. That was the wrong
-- field for that column. This column is where those declarations always
-- belonged.
-- ═══════════════════════════════════════════════════════════════════

ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS mcp_tools JSONB;

COMMENT ON COLUMN agents.mcp_tools IS
    'Tools this agent PUBLISHES over /mcp/agents/:id (outbound server direction). Source of truth for agent config: when non-NULL it overrides capabilities.mcp_tools from the filesystem agent_card.json — see resolve_agent_card. NULL = inherit from the file card; [] = publish nothing; non-empty = authoritative. Shape: [{name, description, input_schema}]. An export allowlist, NOT a capability grant — every agent already receives all platform builtins internally. Each name must resolve to a dispatch arm in ToolRegistry::execute, or be a server__tool name from a server declared in agents.mcp_servers; otherwise it is a phantom tool. Companion to agents.mcp_servers (the inbound client direction).';
