-- Migration 222: join a workspace hop to the artifact it carried.
--
-- ## The defect
--
-- Every arrow in a workspace's workflow diagram is an artifact crossing a seam,
-- and every seam should pass through the gates. It cannot, because the same
-- interaction is recorded twice and joined never:
--
--   * as a MESSAGE PAIR -- `agent_invocation` then `execution_result`, which is
--     what the workflow sequence diagram is generated from;
--   * as an EPISODE -- which is what the gates, the verification ladder, the
--     grades and `gate_decisions` all act on.
--
-- Measured on workspace `fcc3bff3-ff10-4011-a74c-2eb3d35cc142`: 14
-- `agent_invocation` rows, 14 `execution_result` rows carrying structured typed
-- payloads, and 496 episodes from agents in that workspace. Nothing connects
-- them.
--
-- The consequence is exact, and it is why coordination is ungoverned: the
-- workflow tab can show WHAT HAPPENED, the trace can show WHETHER IT WAS
-- VERIFIED, and nothing can show both about the same hop. The gates do not
-- protect coordination -- they protect a parallel record of it.
--
-- ## Not a foreign key, for migration 220's reason
--
-- 220 established and tested the precedent for `gate_decisions.episode_id`, and
-- both halves of that argument apply here:
--
--   * the message is written on a different clock from the episode, and an
--     `execution_result` can land before the episode row it names;
--   * a bad reference must not reject unrelated rows in the same statement.
--
-- So the reference is unenforced and checked instead. An unresolvable
-- `episode_id` is a finding, not a rejected write.
--
-- ## Nullable, and permanently so for some rows
--
-- `NULL` is correct and final wherever no episode exists to name:
--
--   * `chat` messages -- a human talking is not an artifact crossing a seam;
--   * `agent_invocation` sent by a human -- the caller has no episode. The
--     CALLEE's episode belongs on the `execution_result`, not here;
--   * any hop whose agent failed before persisting an episode.
--
-- A NOT NULL here would make the human-initiated hop unrecordable, which is
-- most of the traffic on the platform today (all 14 hops in the workspace above
-- originate from a person).
--
-- ## The obligation this creates
--
-- An `execution_result` whose agent DID persist an episode must carry the id.
-- If it may legitimately be absent, the reason has to be a token rather than a
-- silence -- otherwise "this hop was never verified" and "this hop's join was
-- never written" render identically, which is the collapse the whole trust
-- surface exists to prevent.

ALTER TABLE public.workspace_messages
    ADD COLUMN IF NOT EXISTS episode_id UUID;

-- The read this exists for: "show me the verification for this arrow". Partial,
-- because `chat` dominates the table (919 of 947 rows in the sample workspace)
-- and will never carry an id.
CREATE INDEX IF NOT EXISTS workspace_messages_episode_idx
    ON public.workspace_messages (episode_id)
    WHERE episode_id IS NOT NULL;

COMMENT ON COLUMN public.workspace_messages.episode_id IS
    'The artifact this hop carried. Lets a workflow arrow be joined to the '
    'episode the gates acted on, so a refused artifact is visibly refused where '
    'the team sees it. NULL is correct and final for chat, and for an '
    'agent_invocation sent by a human (the callee episode belongs on the '
    'execution_result). Deliberately NOT a foreign key: the message and the '
    'episode are written on different clocks and a batched insert must not have '
    'one bad reference reject unrelated rows - the precedent and reasoning are '
    'in migration 220.';

-- The blackboard, same reason. `workspace_outputs` is how an artifact reaches a
-- teammate without a message at all -- 425 rows, keyed (workspace_id, key,
-- version) -- and it has the same gap: `updated_by` is a mix of user uuids and
-- agent slugs, and nothing says which run produced the value.
ALTER TABLE public.workspace_outputs
    ADD COLUMN IF NOT EXISTS episode_id UUID;

COMMENT ON COLUMN public.workspace_outputs.episode_id IS
    'The run that produced this value. NULL when a human wrote it directly. '
    'Same non-foreign-key reasoning as workspace_messages.episode_id.';
