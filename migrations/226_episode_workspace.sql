-- Which workspace a pulse happened in.
--
-- ─── The question that could not be asked ─────────────────────────────
--
-- `/api/stream` has been saying so in its own contract string for months:
--
--   > There is no workspace filter because `episodes` carries no workspace
--   > column.
--
-- A workspace is the place a team of agents works, and an app instantiates as a
-- workspace. So "what has happened in this workspace" and "what has this app
-- done" are the same query with a different filter — and neither could be run.
-- Every workspace surface has therefore been built from `workspace_messages`,
-- which is a transcript: it says what was *said*, not what was *executed*.
--
-- ─── Measured before writing this, because a backfill was the plan ────
--
-- There is no link between a pulse and a workspace today, in either direction:
--
--   episodes.workspace_id                   did not exist
--   workspace_messages.episode_id           10 of 5,657 rows  (mig-222)
--   episodes.context -> 'workspace_id'      70 of 3,688 rows, and the values
--                                           include 'wild', 'current' and
--                                           'Efrain AI' — names, not ids
--
-- So this migration deliberately backfills **nothing**. Ten rows is not a
-- history and the JSON is prose. `workspace_messages.episode_id` stays as the
-- seam it was built for; this column is the one that makes the question a WHERE
-- clause instead of a join through a transcript.
--
-- ─── The consequence a surface must state ─────────────────────────────
--
-- The column starts empty and fills from here. That means an empty pulse list
-- for a workspace has two causes with opposite meanings — nothing ran, or
-- nothing was attributed yet — and the surface has to say which. Absent must
-- look different from unrecorded, and a workspace that predates this migration
-- is unrecorded rather than idle.
--
-- Not `NOT NULL`, and not defaulted. A pulse invoked directly by a person
-- belongs to no workspace, and that is a real answer rather than a gap: `NULL`
-- means "not workspace work", which is most of the fleet's history and much of
-- its present.
--
-- ─── No foreign key, on purpose ───────────────────────────────────────
--
-- `teams` is the workspace table and workspaces are deletable. A pulse is an
-- immutable record of something that happened; cascading a delete would erase
-- the audit trail of work done in a workspace someone later tidied up, and
-- refusing the delete would make the audit trail block housekeeping. The id is
-- retained either way and resolves to a name when the workspace still exists.

ALTER TABLE public.episodes
    ADD COLUMN IF NOT EXISTS workspace_id uuid;

-- Partial, because the column is null for every pulse that was not workspace
-- work — which is the large majority — and an index over those nulls would be
-- mostly dead weight on every episode write.
CREATE INDEX IF NOT EXISTS idx_episodes_workspace
    ON public.episodes (workspace_id, created_at DESC)
    WHERE workspace_id IS NOT NULL;

COMMENT ON COLUMN public.episodes.workspace_id IS
  'The workspace this pulse happened in, or NULL for one invoked outside a '
  'workspace — a person or a script calling an agent directly, which is not a '
  'gap. Written at the execute boundary (src/episode_boundary.rs, Write.workspace) '
  'and never inferred. NOT backfilled: at mig-226 the only existing links were '
  '10 of 5,657 workspace_messages.episode_id and 70 context blobs whose values '
  'included workspace NAMES, so an empty result for a workspace older than this '
  'migration means unattributed, not idle, and a surface must say so.';
