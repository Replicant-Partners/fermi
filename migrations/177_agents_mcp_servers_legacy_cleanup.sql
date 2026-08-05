-- ═══════════════════════════════════════════════════════════════════
-- Migration 177 — clear legacy tool data out of agents.mcp_servers
--
-- PROBLEM
-- -------
-- `agents.mcp_servers` (JSONB, mig-010) was intended for remote MCP
-- *server* configs. The agent-create path wrote the card's **`mcp_tools`**
-- into it instead:
--
--   mcp_servers: caps.and_then(|c| c.get("mcp_tools")).cloned()
--                                       ^^^^^^^^^^ wrong field
--
-- Nothing ever read the column, so the bug was invisible for its whole
-- life. Result: many rows hold *tool* declarations, e.g.
--
--   [{"name": "execute_agent", "description": "...", "inputSchema": {...}}]
--
-- v0.11.4 makes the DB the source of truth for agent config, and this
-- column is now read by `resolve_agent_card`. That turns the dormant bug
-- into an active one: those rows parse as `RemoteMcpServer` (name has a
-- default, endpoint is optional), producing phantom servers named after
-- tools which would override — and therefore erase — the real servers an
-- agent's filesystem card declares.
--
-- Two independent defences, because either alone is thin:
--   1. Code: `mcp_client::interpret_db_column` ignores any column whose
--      entries carry no `endpoint`/`url`/`streamable_url`/`command`.
--      This protects rows created before this migration runs, and any
--      new spill from a caller we haven't found.
--   2. This migration: null out the legacy rows so the column means what
--      it says and operators reading the table aren't misled.
--
-- WHAT COUNTS AS LEGACY
-- ---------------------
-- An entry is a server declaration only if it has one of `endpoint`,
-- `url`, `streamable_url`, `streamableUrl`, `endpoint_url`, or `command`.
-- A row is cleared only when NO entry qualifies, so a genuine config is
-- never touched — including a hand-authored map-form entry like
-- biotech_analyst's.
--
-- NULL (not `[]`) is the correct reset: NULL means "inherit from the
-- filesystem card", whereas `[]` would assert "this agent explicitly has
-- no servers" and suppress its real ones.
-- ═══════════════════════════════════════════════════════════════════

UPDATE agents
   SET mcp_servers = NULL
 WHERE mcp_servers IS NOT NULL
   AND jsonb_typeof(mcp_servers) IN ('array', 'object')
   AND NOT EXISTS (
       -- Any entry that looks like a real server declaration?
       SELECT 1
         FROM jsonb_array_elements(
                CASE
                  WHEN jsonb_typeof(mcp_servers) = 'array' THEN mcp_servers
                  -- Map form: {"name": {...}} — inspect the values.
                  ELSE (SELECT jsonb_agg(value) FROM jsonb_each(mcp_servers))
                END
              ) AS entry
        WHERE entry ? 'endpoint'
           OR entry ? 'url'
           OR entry ? 'streamable_url'
           OR entry ? 'streamableUrl'
           OR entry ? 'endpoint_url'
           OR entry ? 'command'
   );

COMMENT ON COLUMN agents.mcp_servers IS
    'Remote MCP servers this agent may call (outbound client direction). Source of truth for agent config: when non-NULL it overrides capabilities.mcp_servers from the filesystem agent_card.json — see resolve_agent_card. NULL = inherit from the file card; [] = explicitly no servers (how the UI removes a file-declared server); non-empty = authoritative. Accepts the ecosystem map form {"name": {...}} or a sequence. NOT for platform tool declarations — those are capabilities.mcp_tools and live only on the card; writing them here was a long-standing bug cleaned up by mig-177.';
