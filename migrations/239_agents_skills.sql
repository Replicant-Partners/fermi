-- agents.skills — the declaration a composing agent reads, made writable
--
-- # What was wrong
--
-- `capabilities.skills` on an agent card is one of exactly FOUR fields the
-- fleet index hands a navigator (`execute_list_agents` in
-- tools/domains/platform.rs):
--
--     { id, type, description, skills }
--
-- So it is half of what `xaman_ek` and any strategist have to go on when
-- deciding who belongs in a composition. And it lived ONLY in
-- `agents/curated/<name>/agent_card.json`, a file loaded into an in-memory
-- registry at boot. Deployed filesystems are read-only, `AgentUpdate` had no
-- `skills` member, and no column existed — so **no owner could change what
-- their agent is discoverable as, by any route.** The one lever on
-- composability was nailed shut.
--
-- # Why nullable with no default
--
-- Deliberately NOT `DEFAULT '{}'`, which is what `accepts` and `produces` do.
-- Those columns are the whole truth about a port. This one overrides a file
-- card, so it needs three states, exactly as `mcp_servers` (mig-176) and
-- `mcp_tools` (mig-178) do:
--
--   NULL        inherit whatever the agent card file declares. The default for
--               every existing row and every newly created agent, so this
--               migration changes no agent's behaviour on the way in.
--   '{}'        explicitly none, even though the card declares some. This is
--               the only way an owner can REMOVE a file-declared skill, and a
--               column defaulting to '{}' would have meant every one of the
--               743 existing rows silently stripped its agent on first read.
--   non-empty   authoritative replacement.
--
-- `resolve_agent_card` applies that precedence; see its comment for the table.
--
-- # Why TEXT[] and not JSONB
--
-- `AgentCapabilities.skills` is `Vec<String>` and always has been. JSONB would
-- admit shapes the struct cannot hold and defer the failure to deserialisation
-- at execution time, which is the pattern that produced the legacy `mcp_tools`
-- spill this project is still cleaning up. TEXT[] makes the wrong shape
-- unstorable and gives the GIN index below for free.

ALTER TABLE agents ADD COLUMN IF NOT EXISTS skills TEXT[];

COMMENT ON COLUMN agents.skills IS
  'Declarative capability labels. NULL = inherit the filesystem agent card; '
  '''{}'' = explicitly none; non-empty = authoritative. Two kinds share this '
  'column by design: names registered in SkillRegistry, which the executor can '
  'invoke directly, and free-text taxonomy labels read by xaman_ek for '
  'discovery. validate_card_skills draws the line at runtime.';

-- Discovery is the entire point of the column, and the query composition needs
-- is "who declares this skill" — a containment test over an array, which is
-- what GIN answers without a sequential scan of every agent.
--
-- Not a correctness requirement at 743 rows. Added now because the index is
-- cheap, the access pattern is known rather than guessed, and the alternative
-- is discovering it during the first composition that filters the fleet.
CREATE INDEX IF NOT EXISTS idx_agents_skills ON agents USING GIN (skills);
