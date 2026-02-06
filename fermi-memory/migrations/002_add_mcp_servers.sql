-- Migration: Add mcp_servers field to agents table
-- Date: 2026-02-06
-- Description: Each agent can have its own MCP server configuration

ALTER TABLE agents
ADD COLUMN mcp_servers JSONB;

COMMENT ON COLUMN agents.mcp_servers IS 'Array of MCP server configurations specific to this agent. Example: [{"name": "yahoo_finance", "command": "node", "args": ["server.js"], "env": {...}}]';

-- Example data structure for mcp_servers:
-- [
--   {
--     "name": "yahoo_finance",
--     "command": "node",
--     "args": ["/path/to/yahoo-finance-server.js"],
--     "env": {
--       "API_KEY": "..."
--     }
--   },
--   {
--     "name": "sec_api",
--     "command": "python",
--     "args": ["-m", "sec_api_server"],
--     "env": {
--       "SEC_API_KEY": "..."
--     }
--   }
-- ]
