-- ═══════════════════════════════════════════════════════════════════════
-- 218 — intention provenance: whose plan is this, and who said so
-- ═══════════════════════════════════════════════════════════════════════
--
-- WHAT WAS MISSING
-- ----------------
-- `workspace_intentions` (mig-210) records `agent_id` — the agent whose next
-- action the row describes — and nothing about who put the row there. Every
-- intention on the platform is written by one caller: the coordination
-- strategist, running Stage 0, reading a 20-message transcript and declaring
-- on each member's behalf what it *supposes* that member is about to do.
--
-- So a row the member itself declared and a row the coordinator guessed are
-- byte-identical, and every reader downstream treats them the same way.
--
-- WHY THAT IS NOT A COSMETIC GAP
-- ------------------------------
-- The whole conflict checker is built on the premise that two rows are two
-- agents' plans. When both rows were written by the same coordinator from the
-- same transcript, an OVERLAP_WARNING between them is not evidence that two
-- agents are about to duplicate work — it is evidence that the coordinator
-- described the same work twice, in two paraphrases, which is exactly the
-- condition a cosine threshold of 0.82 is tuned to fire on.
--
-- `suggest_differentiation` then tells two agents to split work neither of
-- them said they were doing. The platform acts on its own guess and reports
-- the result as coordination.
--
-- ReMALIS (arXiv:2407.12532 §3.1) is explicit about the distinction this
-- column restores. Agent i holds a *private* intention
-- I_i = (goal, sub-goals, next-sub-goal distribution, teammate assignment).
-- What another agent may hold is a *belief* b_i(I_j | m_ij), inferred from a
-- message m_ij that j actually sent. These are different objects. §4.4 Table 3
-- measures the difference: sub-task alignment runs 31%/23%/17% (easy/medium/
-- hard) with no communication and 91%/71%/62% with full intention sharing.
-- Collapsing I_j into a coordinator's guess is the no-communication row wearing
-- the vocabulary of the full-sharing one.
--
-- THE THREE SOURCES
-- -----------------
--   self         — the agent declared its own intention. I_i, first-hand.
--   solicited    — the platform asked the agent for its plan and recorded the
--                  answer. Still the agent's own words; `solicit_agent_plan`
--                  is the propagation channel, and the platform vouches for
--                  the round trip having happened.
--   inferred     — a third party (in practice the strategist) wrote this from
--                  observation. A belief, not an intention. Usable, and never
--                  to be mistaken for the agent's own statement.
--   unattributed — pre-existing rows. See the backfill note.
--
-- WHY THE BACKFILL DOES NOT GUESS
-- -------------------------------
-- Rows written before this migration carry no record of their author. The
-- overwhelmingly likely answer is `inferred`, because the strategist prompt is
-- the only caller — but "overwhelmingly likely" is how a denormalised counter
-- starts drifting from the truth. `unattributed` says what is actually known,
-- and lets a reader tell an old row from a new claim. It is accepted by the
-- CHECK and excluded from grounding counts.
-- ═══════════════════════════════════════════════════════════════════════

DO $$
BEGIN
    EXECUTE $ddl$
        ALTER TABLE public.workspace_intentions
            ADD COLUMN IF NOT EXISTS declared_by UUID
                REFERENCES public.agents(agent_id) ON DELETE SET NULL;
    $ddl$;

    EXECUTE $ddl$
        ALTER TABLE public.workspace_intentions
            ADD COLUMN IF NOT EXISTS source TEXT NOT NULL DEFAULT 'unattributed';
    $ddl$;

    -- Added separately and idempotently: ALTER TABLE ... ADD CONSTRAINT has no
    -- IF NOT EXISTS, and a re-run must not fail the whole migration file.
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint
         WHERE conname = 'workspace_intentions_source_check'
    ) THEN
        EXECUTE $ddl$
            ALTER TABLE public.workspace_intentions
                ADD CONSTRAINT workspace_intentions_source_check
                CHECK (source IN ('self','solicited','inferred','unattributed'));
        $ddl$;
    END IF;

    EXECUTE $ddl$
        COMMENT ON COLUMN public.workspace_intentions.declared_by IS
            'The agent that wrote this row, which is not necessarily the agent '
            'the row is about. NULL for rows predating mig-218.';
    $ddl$;

    EXECUTE $ddl$
        COMMENT ON COLUMN public.workspace_intentions.source IS
            'self | solicited | inferred | unattributed. Whether this is the '
            'agent''s own stated plan (self/solicited) or a third party''s '
            'belief about it (inferred). Conflict detection between two '
            'inferred rows is the coordinator agreeing with itself and is '
            'suppressed — see fermi::intentions.';
    $ddl$;

    -- The grounding query: "how much of this map is first-hand?" runs on every
    -- get_intention_map and check_conflicts.
    EXECUTE $ddl$
        CREATE INDEX IF NOT EXISTS idx_intentions_source
            ON public.workspace_intentions(workspace_id, source)
            WHERE status = 'active';
    $ddl$;
END $$;
