-- Migration 214: the gate decision ledger.
--
-- `docs/AUDIT_loops_and_gates.md` §2.2, the precondition the audit called the
-- one that everything else waits on:
--
--     The platform has a record of every request it served and none of any it
--     refused.
--
-- That is how the Γ arithmetic bug survived. The coherence gate rejected 100%
-- of agent-wide interventions for reasons unrelated to their content, and
-- because the two-reviewer consensus path sits downstream of it, Loop 2's only
-- structural control was unreachable. Nobody could see it, because there was no
-- record the gate had ever been asked.
--
-- `src/gate_trust.rs` has counted decisions in memory since that audit, and
-- `Retention::Recorded` has promised a row in THIS table for the two gates
-- whose individual decisions are governance events. No migration created it, so
-- the promise had no referent and the counters vanished on every deploy.
--
-- ## Why only some gates land here
--
-- Two tiers, declared in `gate_trust::GATES` rather than inferred from volume:
--
--   * Counted   — every gate, in memory, free, cannot fail.
--   * Recorded  — additionally a row here. Currently `coherence` and
--                 `admission`: a blocked agent-wide correction and a refused
--                 publish are both governance events that must survive a
--                 restart.
--
-- A rate-limit tick is deliberately NOT here. One row per refused request turns
-- a control into a load generator, and the counter already answers the only
-- question anyone asks of it.
--
-- ## The two closed vocabularies
--
-- `decision` and `gate` both carry CHECKs, and both are registered in
-- `src/seam_vocabulary.rs` against the Rust declarations that own them
-- (`gate_trust::DECISIONS`, `gate_trust::GATE_IDS`). That registration is not
-- optional: an unregistered CHECK on a closed set is the `severity = 'L1'`
-- setup exactly — Postgres holding one opinion, a Rust string literal holding
-- another, nothing comparing them, and the rejected write swallowed in a
-- spawned task.
--
-- `undetermined` is a first-class decision, not a missing one. A gate that
-- cannot form an opinion has neither approved nor refused, and folding it into
-- either is how "the check could not run" becomes indistinguishable from a
-- verdict.

CREATE TABLE IF NOT EXISTS public.gate_decisions (
    id           BIGSERIAL PRIMARY KEY,

    -- Which gate. Constrained to `gate_trust::GATE_IDS`.
    gate         TEXT        NOT NULL
                 CHECK (gate IN ('coherence', 'grounding', 'input_binding',
                                 'admission', 'credit', 'rate_limit',
                                 'attachment')),

    -- Constrained to `gate_trust::DECISIONS`.
    decision     TEXT        NOT NULL
                 CHECK (decision IN ('approved', 'refused', 'undetermined')),

    -- Why it refused, truncated at the writer. NULL for approvals: recording a
    -- reason for every pass would make the table mostly noise, and the question
    -- this ledger exists to answer is what was refused.
    reason       TEXT,

    -- What was being decided about — an agent name, a workspace id, a
    -- principal. Free text and nullable on purpose: the gates do not share a
    -- subject type, and inventing one would mean a join that lies for five of
    -- the seven.
    subject      TEXT,

    -- When the gate decided, not when the row landed. The recorder batches, so
    -- these differ, and the decision time is the one an audit needs.
    decided_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    recorded_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- The two reads this table exists to serve: "what has this gate been refusing"
-- and "what was refused recently, across all gates".
CREATE INDEX IF NOT EXISTS gate_decisions_gate_decided_idx
    ON public.gate_decisions (gate, decided_at DESC);

CREATE INDEX IF NOT EXISTS gate_decisions_refused_idx
    ON public.gate_decisions (decided_at DESC)
    WHERE decision = 'refused';

COMMENT ON TABLE public.gate_decisions IS
    'One row per decision by a Retention::Recorded gate (gate_trust::GATES). '
    'The record of what the platform refused, which it did not have before '
    'migration 214. Counted-tier gates are in-memory only and are not here.';

COMMENT ON COLUMN public.gate_decisions.decision IS
    'approved | refused | undetermined. Owned by gate_trust::DECISIONS and '
    'registered in seam_vocabulary. `undetermined` means the gate could not '
    'form an opinion, which is not a pass.';

COMMENT ON COLUMN public.gate_decisions.decided_at IS
    'When the gate decided. The recorder batches, so this precedes '
    'recorded_at; an audit wants this one.';
